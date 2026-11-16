//! What the block editor (gpui-blocks' fork of gpui-kit-markdown-editor) needs from the paragraph
//! layer to draw and edit its text with it: carets and hit tests that agree, rows for ↑ ↓ Home
//! and End, inline boxes (pictures, mention chips) on the baseline, marks that don't move text,
//! aligned table cells, and nothing that overflows or panics on hostile text.
//!
//! Written by the gpui-blocks session against `feat/core-textview` (5aefe4a). The API it uses
//! beyond that branch is a proposal; rename freely, keep what the tests ask:
//!
//! - `Affinity { Upstream, Downstream }` and `ParagraphLayout::caret(offset, affinity) ->
//!   Bounds`: at a soft wrap, Upstream is the end of the upper row and Downstream the start of
//!   the lower one (the fork's own `Affinity`). The caret is the row's full height.
//! - `ParagraphLayout::rows() -> &[Row]` with `Row { range, top, height, baseline }`: the rows
//!   in order, covering the text, a hard break ending its row.
//! - `Paragraph::push_box(InlineBox { width, height, baseline })`: one U+FFFC in the text that
//!   lays out as a box of that size sitting on the row's baseline; `ParagraphLayout::boxes() ->
//!   Vec<(usize, Bounds)>` (the box's offset and where it's drawn, for the host to draw an
//!   element there); `ParagraphLayout::box_at(point) -> Option<usize>`.
//! - `Marks { underline, highlight: Option<Hsla>, color: Option<Hsla> }` as more marks.
//! - `ParagraphStyle::align: TextAlign` (Left by default), which position and hit testing follow.
//!
//! Offsets are UTF-8 byte offsets, as in `Paragraph::text`.

#![cfg(target_os = "macos")]

use std::sync::Arc;

use gpui::{
    Bounds, Pixels, TestAppContext, TestDispatcher, TextAlign, VisualTestContext, Window, div,
    hsla, point, px,
};
use gpui_core::text::paragraph::{
    Affinity, InlineBox, Marks, Paragraph, ParagraphLayout, ParagraphStyle,
};

fn app() -> TestAppContext {
    TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        Arc::new(gpui_apple::AppleTextSystem::new()),
    )
}

fn style() -> ParagraphStyle {
    ParagraphStyle {
        font: gpui::font("Helvetica"),
        size: px(16.),
        line_height: px(24.),
        color: hsla(0., 0., 0.9, 1.),
        muted: hsla(0., 0., 0.6, 1.),
        link: hsla(0.6, 0.8, 0.6, 1.),
        code_family: "Menlo".into(),
        code_background: hsla(0., 0., 0.2, 1.),
        align: TextAlign::Left,
    }
}

/// Lays `p` out at `width` in a test window.
fn layout(p: &Paragraph, style: &ParagraphStyle, width: f32) -> ParagraphLayout {
    let mut cx = app();
    let (_, vcx) = cx.add_window_view(|_, _| Empty);
    let vcx: &mut VisualTestContext = vcx;
    vcx.update(|window: &mut Window, _| ParagraphLayout::new(p, style, px(width), window))
}

struct Empty;
impl gpui::Render for Empty {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
        div()
    }
}

fn boundaries(text: &str) -> Vec<usize> {
    text.char_indices()
        .map(|(i, _)| i)
        .chain([text.len()])
        .collect()
}

const MIXED: &str = "Plain text, 漢字かな, emoji 👩‍👩‍👧 and e\u{301} combined, long enough to wrap twice or more at a narrow width.";

#[test]
fn every_offset_round_trips_through_its_point() {
    let p = Paragraph::plain(MIXED);
    let l = layout(&p, &style(), 180.);
    assert!(l.rows().len() >= 3, "it wraps");
    for offset in boundaries(MIXED) {
        let caret = l.caret(offset, Affinity::Downstream);
        let at = l.index_for(point(caret.left(), caret.center().y));
        // Inside a grapheme (the family emoji, the combining accent) the caret can only go to
        // the grapheme's start or end: round trips are for grapheme boundaries.
        use unicode_segmentation::UnicodeSegmentation as _;
        let grapheme = !MIXED
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .chain([MIXED.len()])
            .any(|i| i == offset);
        if !grapheme {
            assert_eq!(at, offset, "offset {offset}");
        }
    }
}

#[test]
fn a_hit_test_never_lands_inside_a_character() {
    let p = Paragraph::plain(MIXED);
    let l = layout(&p, &style(), 180.);
    let mut y = px(-20.);
    while y < l.height + px(20.) {
        let mut x = px(-20.);
        while x < px(200.) {
            let i = l.index_for(point(x, y));
            assert!(MIXED.is_char_boundary(i), "{i} at {x:?},{y:?}");
            x += px(3.);
        }
        y += px(4.);
    }
}

#[test]
fn points_outside_the_text_clamp_to_its_rows() {
    let text = "first row of text that wraps onto a second row";
    let l = layout(&Paragraph::plain(text), &style(), 200.);
    let rows = l.rows();
    assert_eq!(l.index_for(point(px(-50.), px(-50.))), 0, "above and left");
    assert_eq!(
        l.index_for(point(px(900.), l.height + px(50.))),
        text.len(),
        "below and right"
    );
    let first = &rows[0];
    // Right of a row's end: that row's end, never the next row's start.
    let end = l.index_for(point(px(900.), first.top + first.height / 2.));
    assert_eq!(end, first.range.end);
    assert_eq!(
        l.index_for(point(px(-5.), first.top + first.height / 2.)),
        0
    );
}

#[test]
fn a_soft_wrap_has_two_carets() {
    let text = "a sentence long enough to wrap at this width for sure";
    let l = layout(&Paragraph::plain(text), &style(), 150.);
    let rows = l.rows();
    let wrap = rows[0].range.end;
    assert_eq!(rows[1].range.start, wrap, "rows are contiguous");
    let up = l.caret(wrap, Affinity::Upstream);
    let down = l.caret(wrap, Affinity::Downstream);
    assert!(up.top() < down.top(), "upstream on the upper row");
    assert!(up.left() > px(0.));
    assert_eq!(down.left(), px(0.), "downstream at the lower row's start");
    assert_eq!(
        down.size.height, rows[1].height,
        "a caret is its row's height"
    );
}

#[test]
fn rows_cover_the_text_in_order() {
    let text = "one\ntwo two two two two two two two two two two\n\nfour";
    let l = layout(&Paragraph::plain(text), &style(), 160.);
    let rows = l.rows();
    let mut at = 0;
    let mut top = px(0.);
    for row in rows {
        // A hard break ends its row and belongs to no row's range.
        if text[at..].starts_with('\n') && row.range.start == at + 1 {
            at += 1;
        }
        assert_eq!(row.range.start, at);
        assert_eq!(row.top, top);
        at = row.range.end;
        top += row.height;
    }
    assert_eq!(at, text.len());
    assert_eq!(top, l.height);
    // The empty third line is a row of its own, with a caret.
    let empty = rows
        .iter()
        .find(|r| r.range.is_empty())
        .expect("an empty row");
    let caret = l.caret(empty.range.start, Affinity::Downstream);
    assert_eq!(caret.top(), empty.top);
}

#[test]
fn an_empty_paragraph_is_one_row_with_a_caret() {
    let l = layout(&Paragraph::default(), &style(), 300.);
    assert_eq!(l.rows().len(), 1);
    assert_eq!(l.height, px(24.));
    assert_eq!(l.caret(0, Affinity::Downstream).size.height, px(24.));
    assert_eq!(l.index_for(point(px(100.), px(10.))), 0);
    assert!(l.bands(0..0).is_empty());
}

#[test]
fn a_trailing_line_break_starts_an_empty_last_row() {
    let l = layout(&Paragraph::plain("ab\n"), &style(), 300.);
    assert_eq!(l.rows().len(), 2);
    let caret = l.caret(3, Affinity::Downstream);
    assert_eq!(caret.top(), px(24.));
    assert_eq!(caret.left(), px(0.));
}

#[test]
fn a_selection_across_an_empty_line_shows_on_it() {
    let text = "ab\n\ncd";
    let l = layout(&Paragraph::plain(text), &style(), 300.);
    let bands = l.bands(0..text.len());
    assert_eq!(bands.len(), 3, "one band per row, the empty row included");
    assert!(
        bands[1].size.width > px(0.),
        "the empty row's band can be seen"
    );
}

#[test]
fn an_inline_box_sits_on_the_baseline_and_takes_its_width() {
    let mut p = Paragraph::plain("ab");
    p.push_box(InlineBox {
        width: px(40.),
        height: px(30.),
        baseline: px(24.),
    });
    p.push("cd", Marks::default());
    let l = layout(&p, &style(), 400.);
    let at = 2; // the box's U+FFFC
    assert_eq!(&p.text[at..at + 3], "\u{fffc}");
    let before = l.caret(at, Affinity::Downstream).left();
    let after = l.caret(at + 3, Affinity::Downstream).left();
    assert!(
        (after - before - px(40.)).abs() < px(0.5),
        "the box's width is reserved"
    );
    let boxes = l.boxes();
    assert_eq!(boxes.len(), 1);
    let (offset, bounds) = boxes[0];
    assert_eq!(offset, at);
    assert_eq!(bounds.size.height, px(30.));
    let row = &l.rows()[0];
    assert!(row.height >= px(30.), "a tall box makes its row taller");
    assert!(
        (bounds.top() + px(24.) - (row.top + row.baseline)).abs() < px(0.5),
        "the box's baseline is the row's"
    );
    // The caret beside a box spans the row, box included.
    assert_eq!(l.caret(at, Affinity::Downstream).size.height, row.height);
}

#[test]
fn a_press_on_a_box_is_the_box() {
    let mut p = Paragraph::plain("ab");
    p.push_box(InlineBox {
        width: px(40.),
        height: px(20.),
        baseline: px(15.),
    });
    p.push("cd", Marks::default());
    let l = layout(&p, &style(), 400.);
    let (_, bounds) = l.boxes()[0];
    assert_eq!(l.box_at(bounds.center()), Some(2));
    assert_eq!(l.box_at(point(px(1.), bounds.center().y)), None);
    // Nearest edge: the left half is before the box, the right half after it.
    let left = point(bounds.left() + px(5.), bounds.center().y);
    let right = point(bounds.right() - px(5.), bounds.center().y);
    assert_eq!(l.index_for(left), 2);
    assert_eq!(l.index_for(right), 5);
}

#[test]
fn a_box_wraps_whole_and_stays_inside() {
    let mut p = Paragraph::plain("words before the box ");
    p.push_box(InlineBox {
        width: px(120.),
        height: px(20.),
        baseline: px(15.),
    });
    let width = 160.;
    let l = layout(&p, &style(), width);
    let (_, bounds) = l.boxes()[0];
    assert!(bounds.right() <= px(width), "{bounds:?}");
    assert!(bounds.left() >= px(0.));
}

#[test]
fn marks_that_draw_dont_move_text() {
    let text = "some marked words in a row that wraps at this width";
    let plain = Paragraph::plain(text);
    let mut marked = Paragraph::default();
    marked.push("some ", Marks::default());
    marked.push(
        "marked words",
        Marks {
            underline: true,
            highlight: Some(hsla(0.15, 0.9, 0.5, 0.4)),
            color: Some(hsla(0., 0.8, 0.6, 1.)),
            ..Default::default()
        },
    );
    marked.push(" in a row that wraps at this width", Marks::default());
    let (a, b) = (
        layout(&plain, &style(), 180.),
        layout(&marked, &style(), 180.),
    );
    for offset in boundaries(text) {
        assert_eq!(
            a.caret(offset, Affinity::Downstream),
            b.caret(offset, Affinity::Downstream),
            "offset {offset}"
        );
    }
}

#[test]
fn aligned_text_is_hit_where_it_is_drawn() {
    // Table cells: centred and right-aligned columns.
    for align in [TextAlign::Center, TextAlign::Right] {
        let text = "cell";
        let mut s = style();
        s.align = align;
        let l = layout(&Paragraph::plain(text), &s, 300.);
        let start = l.caret(0, Affinity::Downstream).left();
        let end = l.caret(text.len(), Affinity::Downstream).left();
        match align {
            TextAlign::Right => assert!((end - px(300.)).abs() < px(1.)),
            _ => assert!(((start + end) / 2. - px(150.)).abs() < px(1.)),
        }
        let mid = l.rows()[0].height / 2.;
        assert_eq!(l.index_for(point(start + px(1.), mid)), 0);
        assert_eq!(l.index_for(point(end - px(1.), mid)), text.len());
        assert_eq!(
            l.index_for(point(px(2.), mid)),
            0,
            "left of centred text is its start"
        );
    }
}

#[test]
fn nothing_overflows_however_narrow() {
    let unbreakable = "https://example.com/a/very/long/path/with/no/spaces/at/all/anywhere";
    for width in [1., 8., 40., 120.] {
        for text in [unbreakable, MIXED, "", "\n\n\n"] {
            let l = layout(&Paragraph::plain(text), &style(), width);
            for row in l.rows() {
                if row.range.len() > 4 {
                    // A row of more than one grapheme fits the width.
                    let right = l.caret(row.range.end, Affinity::Upstream).left();
                    assert!(right <= px(width) + px(0.5), "{text:?} at {width}");
                }
            }
            assert!(l.height >= l.line_height);
        }
    }
}

#[test]
fn bands_follow_rows_of_different_heights() {
    let mut p = Paragraph::plain("before ");
    p.push_box(InlineBox {
        width: px(20.),
        height: px(40.),
        baseline: px(30.),
    });
    p.push(
        " and text after that wraps onto another row",
        Marks::default(),
    );
    let l = layout(&p, &style(), 200.);
    let rows = l.rows();
    let bands: Vec<Bounds<Pixels>> = l.bands(0..p.text.len());
    assert_eq!(bands.len(), rows.len());
    for (band, row) in bands.iter().zip(rows) {
        assert_eq!(band.top(), row.top);
        assert_eq!(band.size.height, row.height);
    }
}
