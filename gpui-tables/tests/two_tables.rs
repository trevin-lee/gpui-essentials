//! Two tables side by side on one page, each with its own drag-and-drop provider and none round
//! the window (as a page can hold two). A row dragged within one table is one `rowsMoved`, heard
//! by that table's host alone; a row dropped on the other table goes back, nothing emitted by
//! either; and the other table can then drag its own rows.

// The Mac's dev-dependencies switch GPUI's test support on; elsewhere, the test-support feature.
#![cfg(all(feature = "gpui", any(target_os = "macos", feature = "test-support")))]

mod common;

use std::rc::Rc;
use std::time::Duration;

use common::*;
use gpui::*;
use gpui_tables::Edit;
use gpui_tables::view::{TableHost, TableView};
use gpui_tables::*;
// The built-in #[test], not GPUI's, which gpui::* brings in.
use std::prelude::v1::test;

/// People's "By stage" list, ungrouped, as a table: in the manual order.
const BY_STAGE: &str = "29bdd0f4-6c47-4d20-8928-78be9f6ae20b";

/// People in the manual order; the second table's rows have ids of their own, so the two
/// tables' rows tell apart.
fn people(prefix: &str) -> (Doc, Vec<Doc>) {
    let (db, mut rows) = load("people");
    let mut db = as_table(db, BY_STAGE);
    db.database
        .as_mut()
        .unwrap()
        .views
        .iter_mut()
        .find(|v| v.id == BY_STAGE)
        .unwrap()
        .group_by = None;
    for r in &mut rows {
        r.id = format!("{prefix}{}", r.id);
    }
    (db, rows)
}

struct Pair {
    a: Entity<TableView>,
    b: Entity<TableView>,
}

impl Render for Pair {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .child(div().w(px(700.)).h_full().child(self.a.clone()))
            .child(div().w(px(700.)).h_full().child(self.b.clone()))
    }
}

type Tables = (Entity<TableView>, Entity<TableView>);

struct Two<'a> {
    a: Entity<TableView>,
    b: Entity<TableView>,
    host_a: Rc<TestHost>,
    host_b: Rc<TestHost>,
    cx: &'a mut VisualTestContext,
}

fn open(cx: &mut TestAppContext) -> Two<'_> {
    setup(cx);
    let (host_a, host_b) = (TestHost::new(), TestHost::new());
    let (ha, hb): (Rc<dyn TableHost>, Rc<dyn TableHost>) = (host_a.clone(), host_b.clone());
    let made: Rc<std::cell::RefCell<Option<Tables>>> = Rc::default();
    let (_, vcx) = cx.add_window_view({
        let made = made.clone();
        move |window, cx| {
            let (da, ra) = people("");
            let (db, rb) = people("b-");
            let a = cx.new(|cx| TableView::new(ha, da, ra, cx).with_view(BY_STAGE));
            let b = cx.new(|cx| TableView::new(hb, db, rb, cx).with_view(BY_STAGE));
            *made.borrow_mut() = Some((a.clone(), b.clone()));
            let pair = cx.new(|_| Pair { a, b });
            gpui_core::Root::new(pair, window, cx)
        }
    });
    let (a, b) = made.borrow().clone().unwrap();
    *host_a.table.borrow_mut() = Some(a.downgrade());
    *host_b.table.borrow_mut() = Some(b.downgrade());
    vcx.simulate_resize(size(px(1400.), px(900.)));
    draw(vcx);
    draw(vcx);
    Two {
        a,
        b,
        host_a,
        host_b,
        cx: vcx,
    }
}

fn id_of(table: &Entity<TableView>, cx: &mut VisualTestContext, title: &str) -> String {
    table.read_with(cx, |t, _| {
        t.rows()
            .iter()
            .find(|r| r.title == title)
            .unwrap()
            .id
            .clone()
    })
}

fn moves(host: &TestHost) -> Vec<Vec<Edit>> {
    host.events
        .borrow()
        .iter()
        .filter_map(|e| match e {
            TableEvent::Edits { edits } => Some(edits.clone()),
            _ => None,
        })
        .collect()
}

/// Carries row `id`'s handle to `to` and lets go there.
fn drag_row(cx: &mut VisualTestContext, id: &str, to: Point<Pixels>) {
    let r = rect(cx, format!("row:{id}"));
    cx.simulate_mouse_move(
        point(r.left() + px(200.), r.center().y),
        None,
        Modifiers::none(),
    );
    draw(cx);
    let from = rect(cx, format!("handle:{id}")).center();
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    draw(cx);
    cx.simulate_mouse_move(
        from + point(px(0.), px(6.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    draw(cx);
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    draw(cx);
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    draw(cx);
}

fn above(cx: &mut VisualTestContext, id: &str) -> Point<Pixels> {
    let b = rect(cx, format!("row:{id}"));
    point(b.left() + px(200.), b.top() + px(4.))
}

#[gpui::test]
fn a_drag_in_one_table_is_one_rows_moved_for_its_host_alone(cx: &mut TestAppContext) {
    let t = open(cx);
    let (rowan, sami) = (
        id_of(&t.a, t.cx, "Rowan Pike"),
        id_of(&t.a, t.cx, "Sami Halloran"),
    );
    let to = above(t.cx, &sami);
    drag_row(t.cx, &rowan, to);
    let a = moves(&t.host_a);
    assert_eq!(a.len(), 1, "one batch: {a:?}");
    let [Edit::RowsMoved { row_ids, .. }] = a[0].as_slice() else {
        panic!("{a:?}")
    };
    assert_eq!(row_ids, &[rowan]);
    assert!(moves(&t.host_b).is_empty(), "the other table heard nothing");
    // The other table drags its own rows the same way.
    let (b_rowan, b_sami) = (
        id_of(&t.b, t.cx, "Rowan Pike"),
        id_of(&t.b, t.cx, "Sami Halloran"),
    );
    let to = above(t.cx, &b_sami);
    drag_row(t.cx, &b_rowan, to);
    let b = moves(&t.host_b);
    assert_eq!(b.len(), 1, "one batch: {b:?}");
    assert!(matches!(b[0].as_slice(), [Edit::RowsMoved { row_ids, .. }] if *row_ids == [b_rowan]));
    assert_eq!(
        moves(&t.host_a).len(),
        1,
        "and the first heard nothing more"
    );
}

#[gpui::test]
fn a_row_dropped_on_the_other_table_goes_back(cx: &mut TestAppContext) {
    let t = open(cx);
    let (rowan, b_sami) = (
        id_of(&t.a, t.cx, "Rowan Pike"),
        id_of(&t.b, t.cx, "Sami Halloran"),
    );
    let to = above(t.cx, &b_sami);
    drag_row(t.cx, &rowan, to);
    t.cx.executor().advance_clock(Duration::from_millis(300));
    draw(t.cx);
    assert!(moves(&t.host_a).is_empty(), "{:?}", moves(&t.host_a));
    assert!(moves(&t.host_b).is_empty(), "{:?}", moves(&t.host_b));
    assert!(!t.a.read_with(t.cx, |t, _| t.is_dragging_card()));
    assert!(!t.b.read_with(t.cx, |t, _| t.is_dragging_card()));
}

#[gpui::test]
fn a_carried_row_has_one_picture(cx: &mut TestAppContext) {
    let t = open(cx);
    let (rowan, sami) = (
        id_of(&t.a, t.cx, "Rowan Pike"),
        id_of(&t.a, t.cx, "Sami Halloran"),
    );
    let r = rect(t.cx, format!("row:{rowan}"));
    t.cx.simulate_mouse_move(
        point(r.left() + px(200.), r.center().y),
        None,
        Modifiers::none(),
    );
    draw(t.cx);
    let from = rect(t.cx, format!("handle:{rowan}")).center();
    t.cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    draw(t.cx);
    let to = above(t.cx, &sami);
    for at in [from + point(px(0.), px(6.)), to] {
        t.cx.simulate_mouse_move(at, MouseButton::Left, Modifiers::none());
        draw(t.cx);
    }
    // The picture's fill, painted once: a second provider drawing the picture too would paint
    // it again at the same place.
    let picture = rect(t.cx, "dnd-preview");
    let fills = t.cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let picture = picture.scale(window.scale_factor());
        window
            .painted_quads()
            .iter()
            .filter(|q| {
                q.bounds == picture
                    && q.background.as_solid().is_some_and(|c| c.a > 0.)
                    && q.border_widths.top == ScaledPixels(0.)
            })
            .count()
    });
    assert_eq!(fills, 1, "one picture of the row");
    keys(t.cx, "escape");
}
