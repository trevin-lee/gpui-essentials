//! Table: a thousand rows in a table with a height draw only the rows in view (the host is asked
//! for no others), a short table without one draws them all; the columns line up in every row
//! at their fixed widths and fractions, aligned as told; a click asks to select a row, the
//! selected row and the one under the pointer are lit; the keys ask for rows and scroll them
//! into view; the footer and the caption go under the body; cells cut their text short in a
//! narrow table (laid out with Core Text, as on screen); and nothing redraws while idle.

#![cfg(target_os = "macos")]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    AppContext as _, Bounds, Context, Entity, FocusHandle, Hsla, InteractiveElement as _,
    IntoElement, KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, ParentElement as _, Pixels,
    Render, ScrollDelta, ScrollEvent, Styled as _, TestAppContext, TestDispatcher,
    VisualTestContext, Window, div, point, px,
};
use gpui_core::{ColumnAlign, Press, Root, Table, TableColumn, TableRow, Theme};

const LONG: &str = "Pneumonoultramicroscopicsilicovolcanoconiosis-Antidisestablishmentarianism \
                    職務経歴書と志望動機を来週の月曜日までに提出してください";

struct Host {
    count: usize,
    height: Option<f32>,
    width: f32,
    long: bool,
    selected: Option<usize>,
    selectable: bool,
    /// The largest range of rows the table asked for.
    asked: Rc<Cell<usize>>,
    told: Rc<RefCell<Vec<usize>>>,
    draws: Rc<Cell<usize>>,
    before: FocusHandle,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let asked = self.asked.clone();
        let long = self.long;
        let told = self.told.clone();
        let mut table = Table::new("t")
            .column(TableColumn::new("Invoice").fixed(px(100.)))
            .column(TableColumn::new("Status").fraction(1.))
            .column(
                TableColumn::new("Amount")
                    .fraction(2.)
                    .align(ColumnAlign::End),
            )
            .rows(self.count, move |range, _, _| {
                asked.set(asked.get().max(range.len()));
                range
                    .map(|i| {
                        TableRow::new()
                            .cell(if long {
                                LONG.to_string()
                            } else {
                                format!("INV{i:03}")
                            })
                            .cell(if long { LONG } else { "Paid" })
                            .cell(
                                div()
                                    .debug_selector(move || format!("amount-{i}"))
                                    .child("$250.00"),
                            )
                    })
                    .collect()
            })
            .footer(TableRow::new().cell("Total").cell("").cell("$2,500.00"))
            .caption("A list of your recent invoices.")
            .selected(self.selected);
        if self.selectable {
            table = table.on_select(cx.listener(move |this, row: &usize, _, cx| {
                told.borrow_mut().push(*row);
                this.selected = Some(*row);
                cx.notify();
            }));
        }
        if let Some(h) = self.height {
            table = table.h(px(h));
        }
        div()
            .p(px(20.))
            .child(
                Press::new("before")
                    .track_focus(&self.before)
                    .child("Before"),
            )
            .child(
                div()
                    .w(px(self.width))
                    .debug_selector(|| "box".into())
                    .child(table),
            )
    }
}

struct Opened<'a> {
    host: Entity<Host>,
    asked: Rc<Cell<usize>>,
    told: Rc<RefCell<Vec<usize>>>,
    draws: Rc<Cell<usize>>,
    vcx: &'a mut VisualTestContext,
}

fn app() -> TestAppContext {
    TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        Arc::new(gpui_apple::AppleTextSystem::new()),
    )
}

fn open(cx: &mut TestAppContext, setup: impl FnOnce(&mut Host)) -> Opened<'_> {
    cx.update(gpui_core::init);
    let asked = Rc::new(Cell::new(0));
    let told = Rc::new(RefCell::new(Vec::new()));
    let draws = Rc::new(Cell::new(0));
    let (a, t, d) = (asked.clone(), told.clone(), draws.clone());
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let host = cx.new(|cx| {
            let mut h = Host {
                count: 1000,
                height: Some(300.),
                width: 500.,
                long: false,
                selected: None,
                selectable: true,
                asked: a,
                told: t,
                draws: d,
                before: cx.focus_handle(),
            };
            setup(&mut h);
            h
        });
        *m.borrow_mut() = Some(host.clone());
        Root::new(host, window, cx)
    });
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let host = made.borrow().clone().unwrap();
    Opened {
        host,
        asked,
        told,
        draws,
        vcx,
    }
}

fn bounds(vcx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    vcx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} drawn"))
}

/// A selector made at run time, as `debug_bounds` wants one.
fn sel(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn drawn(vcx: &mut VisualTestContext, row: usize) -> bool {
    vcx.debug_bounds(sel(format!("table-row-{row}"))).is_some()
}

#[test]
fn a_thousand_rows_draw_only_those_in_view() {
    let mut cx = app();
    let o = open(&mut cx, |_| {});
    assert!(drawn(o.vcx, 0));
    assert!(!drawn(o.vcx, 20), "a row out of view");
    assert!(!drawn(o.vcx, 999));
    let asked = o.asked.get();
    assert!(
        (1..=12).contains(&asked),
        "the host was asked for {asked} rows at once"
    );
    let table = bounds(o.vcx, "table");
    assert!((table.size.height - px(300.)).abs() < px(1.), "{table:?}");
    // The footer and the caption sit under the body, inside the table.
    let body = bounds(o.vcx, "table-body");
    let footer = bounds(o.vcx, "table-footer");
    let caption = bounds(o.vcx, "table-caption");
    assert!(
        footer.top() >= body.bottom() - px(0.5),
        "{footer:?} {body:?}"
    );
    assert!(caption.top() > footer.bottom());
    assert!(caption.bottom() <= table.bottom() + px(0.5));
}

#[test]
fn a_short_table_without_a_height_draws_every_row() {
    let mut cx = app();
    let o = open(&mut cx, |h| {
        h.count = 5;
        h.height = None;
    });
    for i in 0..5 {
        assert!(drawn(o.vcx, i), "row {i}");
    }
    let body = bounds(o.vcx, "table-body");
    let last = bounds(o.vcx, "table-row-4");
    assert!(
        (body.bottom() - last.bottom()).abs() < px(0.5),
        "{body:?} {last:?}"
    );
}

#[test]
fn the_columns_line_up_at_their_widths() {
    let mut cx = app();
    let o = open(&mut cx, |_| {});
    let t = bounds(o.vcx, "table");
    for (j, i) in [(0, 0), (1, 3), (2, 2)] {
        let head = bounds(o.vcx, sel(format!("table-head-{j}")));
        let cell = bounds(o.vcx, sel(format!("table-cell-{i}-{j}")));
        assert_eq!(head.left(), cell.left(), "column {j}");
        assert_eq!(head.size.width, cell.size.width, "column {j}");
    }
    let fixed = bounds(o.vcx, "table-head-0").size.width;
    assert_eq!(fixed, px(100.));
    let one = bounds(o.vcx, "table-head-1").size.width;
    let two = bounds(o.vcx, "table-head-2").size.width;
    assert!((two - one * 2.).abs() < px(1.), "{one:?} {two:?}");
    assert!((fixed + one + two - t.size.width).abs() < px(1.));
    // Amounts sit at the end of their cells, inside the padding.
    let cell = bounds(o.vcx, "table-cell-0-2");
    let amount = bounds(o.vcx, "amount-0");
    assert!(
        (cell.right() - amount.right() - px(8. * 15. / 15.)).abs() < px(1.),
        "{amount:?} in {cell:?}"
    );
}

fn fills(vcx: &mut VisualTestContext) -> Vec<Hsla> {
    vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        window
            .rendered_scene()
            .quads
            .iter()
            .filter_map(|q| q.background.as_solid())
            .collect()
    })
}

#[test]
fn a_click_selects_a_row_and_the_rows_are_lit() {
    let t = Theme::dark().colors;
    let mut cx = app();
    let o = open(&mut cx, |_| {});
    assert!(!fills(o.vcx).contains(&t.muted));
    let at = bounds(o.vcx, "table-row-2").center();
    o.vcx.simulate_click(at, Modifiers::none());
    o.vcx.run_until_parked();
    assert_eq!(*o.told.borrow(), [2]);
    assert!(fills(o.vcx).contains(&t.muted), "the selected row");
    let hover = t.muted.opacity(0.5);
    let lit = |f: Vec<Hsla>| f.iter().filter(|c| **c == hover).count();
    assert_eq!(lit(fills(o.vcx)), 1, "the footer alone");
    let at = bounds(o.vcx, "table-row-4").center();
    o.vcx.simulate_mouse_move(at, None, Modifiers::none());
    o.vcx.run_until_parked();
    assert_eq!(
        lit(fills(o.vcx)),
        2,
        "the row under the pointer and the footer"
    );
}

fn keys(vcx: &mut VisualTestContext, keys: &'static str) {
    for k in keys.split(' ') {
        let keystroke = Keystroke::parse(k).unwrap();
        vcx.simulate_event(KeyDownEvent {
            keystroke: keystroke.clone(),
            is_held: false,
            prefer_character_input: false,
        });
        vcx.simulate_event(KeyUpEvent { keystroke });
        vcx.run_until_parked();
    }
}

#[test]
fn the_keys_select_rows_and_scroll_them_into_view() {
    let mut cx = app();
    let o = open(&mut cx, |_| {});
    let before = o.host.read_with(o.vcx, |h, _| h.before.clone());
    o.vcx.update(|window, cx| window.focus(&before, cx));
    keys(o.vcx, "tab down down");
    assert_eq!(*o.told.borrow(), [0, 1]);
    keys(o.vcx, "end");
    assert_eq!(o.told.borrow().last(), Some(&999));
    assert!(drawn(o.vcx, 999), "scrolled to the last row");
    assert!(!drawn(o.vcx, 0));
    keys(o.vcx, "up");
    assert_eq!(o.told.borrow().last(), Some(&998));
    keys(o.vcx, "home");
    assert_eq!(o.told.borrow().last(), Some(&0));
    assert!(drawn(o.vcx, 0));
    keys(o.vcx, "pagedown");
    let page = *o.told.borrow().last().unwrap();
    // A page: the rows in view but one (the body holds four or five here).
    assert!((3..12).contains(&page), "a page down: {page}");
    assert!(drawn(o.vcx, page));
}

#[test]
fn without_on_select_the_body_is_no_tab_stop() {
    let mut cx = app();
    let o = open(&mut cx, |h| h.selectable = false);
    let before = o.host.read_with(o.vcx, |h, _| h.before.clone());
    o.vcx.update(|window, cx| window.focus(&before, cx));
    keys(o.vcx, "tab down");
    assert!(o.told.borrow().is_empty());
    let at = bounds(o.vcx, "table-row-2").center();
    o.vcx.simulate_click(at, Modifiers::none());
    o.vcx.run_until_parked();
    assert!(o.told.borrow().is_empty());
}

/// Every glyph drawn outside `inside`, or cut off at what clips it (but for a slot narrower
/// than one glyph, which cuts its ellipsis).
fn glyphs_out(vcx: &mut VisualTestContext, inside: Bounds<Pixels>) -> Vec<String> {
    vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let k = window.scale_factor();
        let scene = window.rendered_scene();
        scene
            .monochrome_sprites
            .iter()
            .map(|s| (s.bounds, s.content_mask.bounds))
            .chain(
                scene
                    .subpixel_sprites
                    .iter()
                    .map(|s| (s.bounds, s.content_mask.bounds)),
            )
            .filter_map(|(b, clip)| {
                let (l, r) = (
                    b.origin.x.as_f32() / k,
                    (b.origin.x + b.size.width).as_f32() / k,
                );
                let (cl, cr) = (
                    clip.origin.x.as_f32() / k,
                    (clip.origin.x + clip.size.width).as_f32() / k,
                );
                if r <= cl || l >= cr {
                    return None;
                }
                let slack = 2.;
                let squeezed = cr - cl < r - l;
                let cut = !squeezed && (l < cl - slack || r > cr + slack);
                let out = l < f32::from(inside.left()) - slack
                    || r.min(cr) > f32::from(inside.right()) + slack;
                (cut || out).then(|| format!("glyph {l:.0}–{r:.0} clip {cl:.0}–{cr:.0}"))
            })
            .collect()
    })
}

#[test]
fn cells_cut_their_text_short_in_a_narrow_table() {
    for width in [60., 160., 320.] {
        let mut cx = app();
        let o = open(&mut cx, |h| {
            h.width = width;
            h.long = true;
        });
        let table = bounds(o.vcx, "table");
        let b = bounds(o.vcx, "box");
        assert!(table.right() <= b.right() + px(0.5), "{width}: {table:?}");
        let bad = glyphs_out(o.vcx, table);
        assert!(bad.is_empty(), "{width}: {bad:?}");
        let row = bounds(o.vcx, "table-row-0");
        assert!(row.size.height < px(40.), "{width}: one line: {row:?}");
    }
}

/// Delivers one display frame and says how many things asked for it.
fn frame(vcx: &mut VisualTestContext) -> usize {
    vcx.executor().advance_clock(Duration::from_millis(16));
    let asked = vcx.update(|window, cx| window.simulate_next_frame(cx));
    vcx.run_until_parked();
    asked
}

#[test]
fn nothing_redraws_while_idle() {
    let mut cx = app();
    let o = open(&mut cx, |_| {});
    let at = bounds(o.vcx, "table-body").center();
    o.vcx.simulate_event(ScrollEvent {
        position: at,
        delta: ScrollDelta::Pixels(point(px(0.), px(-400.))),
        ..Default::default()
    });
    o.vcx.run_until_parked();
    assert!(!drawn(o.vcx, 0), "scrolled");
    frame(o.vcx);
    let settled = o.draws.get();
    for _ in 0..30 {
        assert_eq!(frame(o.vcx), 0, "asked for a frame while idle");
    }
    assert_eq!(o.draws.get(), settled);
}
