//! TextView's options for matching the web's Markdown: list markers set against their text,
//! heading line heights and margins by level, inline code's fill height, and a table's corners
//! and header fill. Laid out with Core Text, as on screen.

#![cfg(target_os = "macos")]

use std::sync::Arc;

use gpui::{
    AppContext as _, Context, Hsla, InteractiveElement as _, IntoElement, ParentElement as _,
    Pixels, Render, Styled as _, TestAppContext, TestDispatcher, Window, div, px,
};
use gpui_core::text::{InlineCodeStyle, TextViewColors};
use gpui_core::{Root, TextView, TextViewStyle};

const RED: Hsla = Hsla {
    h: 0.,
    s: 0.9,
    l: 0.5,
    a: 1.,
};
const BLUE: Hsla = Hsla {
    h: 0.6,
    s: 0.9,
    l: 0.5,
    a: 1.,
};

fn near(a: Hsla, b: Hsla) -> bool {
    (a.h - b.h).abs() < 0.02 && (a.l - b.l).abs() < 0.05 && a.s > 0.5
}

struct Doc(TextViewStyle, &'static str);

impl Render for Doc {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().p(px(20.)).child(
            div()
                .w(px(400.))
                .debug_selector(|| "doc".into())
                .child(TextView::markdown("d", self.1).style(self.0.clone())),
        )
    }
}

/// What a document drew: its height, its glyphs (left, right, colour) and its quads (bounds,
/// fill, top-left and bottom-left corner), in window pixels.
struct Drawn {
    left: f32,
    height: f32,
    glyphs: Vec<(f32, f32, Hsla)>,
    quads: Vec<(gpui::Bounds<f32>, Hsla, f32, f32)>,
}

fn draw(style: TextViewStyle, source: &'static str) -> Drawn {
    let mut cx = TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        Arc::new(gpui_apple::AppleTextSystem::new()),
    );
    cx.update(gpui_core::init);
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let v = cx.new(|_| Doc(style, source));
        Root::new(v, window, cx)
    });
    vcx.run_until_parked();
    let doc = vcx.debug_bounds("doc").unwrap();
    let (glyphs, quads) = vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let k = window.scale_factor();
        let scene = window.rendered_scene();
        let glyphs = scene
            .monochrome_sprites
            .iter()
            .map(|g| (g.bounds, g.color))
            .chain(scene.subpixel_sprites.iter().map(|g| (g.bounds, g.color)))
            .map(|(b, c)| (b.origin.x.0 / k, (b.origin.x.0 + b.size.width.0) / k, c))
            .collect();
        let quads = scene
            .quads
            .iter()
            .filter_map(|q| {
                let b = gpui::Bounds::new(
                    gpui::point(q.bounds.origin.x.0 / k, q.bounds.origin.y.0 / k),
                    gpui::size(q.bounds.size.width.0 / k, q.bounds.size.height.0 / k),
                );
                Some((
                    b,
                    q.background.as_solid()?,
                    q.corner_radii.top_left.0 / k,
                    q.corner_radii.bottom_left.0 / k,
                ))
            })
            .collect();
        (glyphs, quads)
    });
    Drawn {
        left: f32::from(doc.origin.x),
        height: f32::from(doc.size.height),
        glyphs,
        quads,
    }
}

fn marked(gap: Option<Pixels>, source: &'static str) -> Drawn {
    draw(
        TextViewStyle {
            list_marker_gap: gap,
            colors: TextViewColors {
                list_marker: Some(RED),
                ..Default::default()
            },
            ..Default::default()
        },
        source,
    )
}

/// The red glyphs' left and right ink edges.
fn marker_ink(d: &Drawn) -> (f32, f32) {
    let red: Vec<_> = d.glyphs.iter().filter(|g| near(g.2, RED)).collect();
    assert!(!red.is_empty(), "a marker drawn");
    (
        red.iter().map(|g| g.0).fold(f32::MAX, f32::min),
        red.iter().map(|g| g.1).fold(f32::MIN, f32::max),
    )
}

#[test]
fn a_marker_ends_its_gap_before_the_text() {
    let tight = marker_ink(&marked(Some(px(2.)), "- item"));
    let loose = marker_ink(&marked(Some(px(10.)), "- item"));
    assert!(
        ((tight.0 - loose.0) - 8.).abs() < 0.6,
        "{tight:?} {loose:?}"
    );
    // By default it sits by its text, not at the list's edge.
    let d = marked(None, "- item");
    let text_left = d
        .glyphs
        .iter()
        .filter(|g| !near(g.2, RED))
        .map(|g| g.0)
        .fold(f32::MAX, f32::min);
    let (_, right) = marker_ink(&d);
    assert!(right < text_left, "{right} {text_left}");
    assert!(
        right > d.left + (text_left - d.left) * 0.4,
        "{right} {text_left}"
    );
}

#[test]
fn a_marker_too_wide_for_its_room_starts_at_the_lists_edge() {
    let d = marked(Some(px(200.)), "10. ten\n11. eleven");
    let (left, _) = marker_ink(&d);
    assert!(left >= d.left - 0.5, "{left} {}", d.left);
}

#[test]
fn a_headings_line_height_is_the_hosts() {
    let h = |r: f32| {
        draw(
            TextViewStyle {
                heading_line_height: Some(r),
                heading_base_font_size: Some(px(10.)),
                ..Default::default()
            },
            "# Heading",
        )
        .height
    };
    // An h1 is 1.6 times the base: 16px, so one more line height is 16px more.
    assert!(((h(2.) - h(1.)) - 16.).abs() < 0.6, "{} {}", h(1.), h(2.));
}

#[test]
fn heading_margins_go_by_level_and_collapse_between_headings() {
    let h = |m: [(f32, f32); 2], source: &'static str| {
        draw(
            TextViewStyle {
                heading_margins: Some(Arc::new(move |level, _| {
                    let (t, b) = m[usize::from(level.clamp(1, 2)) - 1];
                    (px(t), px(b))
                })),
                ..Default::default()
            },
            source,
        )
        .height
    };
    // The space above an h1 is its own.
    let doc = "Intro\n\n# One\n\nText";
    assert!((h([(30., 0.), (0., 0.)], doc) - h([(10., 0.), (0., 0.)], doc) - 20.).abs() < 0.6);
    // Between an h1 and an h2, the larger of the h1's bottom and the h2's top.
    let two = "# One\n\n## Two";
    let base = h([(0., 20.), (5., 0.)], two);
    assert!((h([(0., 20.), (0., 0.)], two) - base).abs() < 0.6);
    assert!((h([(0., 5.), (20., 0.)], two) - base).abs() < 0.6);
    // A heading first has no space above it.
    assert!((h([(40., 0.), (0., 0.)], "# One") - h([(0., 0.), (0., 0.)], "# One")).abs() < 0.6);
}

fn code_fill_height(pad_y: Option<Pixels>) -> f32 {
    let d = draw(
        TextViewStyle {
            inline_code: InlineCodeStyle {
                background: Some(BLUE),
                padding: Some(px(4.)),
                padding_y: pad_y,
                ..Default::default()
            },
            ..Default::default()
        },
        "Some `code` here",
    );
    let q = d
        .quads
        .iter()
        .find(|q| near(q.1, BLUE))
        .expect("code's fill");
    q.0.size.height
}

#[test]
fn inline_codes_fill_takes_its_own_vertical_padding() {
    let none = code_fill_height(Some(px(0.)));
    assert!((code_fill_height(Some(px(-2.))) - (none - 4.)).abs() < 0.6);
    // Half the side padding unless given.
    assert!((code_fill_height(None) - (none + 4.)).abs() < 0.6);
}

#[test]
fn a_tables_corners_and_header_fill_are_the_hosts() {
    let d = draw(
        TextViewStyle {
            table_radius: Some(px(7.)),
            colors: TextViewColors {
                table_header: Some(BLUE),
                ..Default::default()
            },
            ..Default::default()
        },
        "| A | B |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |",
    );
    let header = d
        .quads
        .iter()
        .find(|q| near(q.1, BLUE))
        .expect("header fill");
    assert!((header.2 - 7.).abs() < 0.1, "rounded top: {header:?}");
    assert!(header.3 < 0.1, "square bottom: {header:?}");
    // The header row only: shorter than half the table's three rows.
    let table_h = d.quads.iter().map(|q| q.0.size.height).fold(0., f32::max);
    assert!(header.0.size.height < table_h * 0.5, "{header:?} {table_h}");
    // A line between each two rows, and none over the border at the bottom.
    let lines = d
        .quads
        .iter()
        .filter(|q| q.0.size.height <= 1.01 && q.0.size.width > 300.)
        .count();
    assert_eq!(lines, 2);
    // Without a header colour there's no fill.
    let plain = draw(TextViewStyle::default(), "| A | B |\n|---|---|\n| 1 | 2 |");
    assert!(!plain.quads.iter().any(|q| near(q.1, BLUE)));
}
