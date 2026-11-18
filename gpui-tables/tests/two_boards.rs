//! Two boards side by side on one page, each with its own drag-and-drop provider and none round
//! the window (as a Docs page holds two), and the same for two lists. A card dragged to another
//! column of its board is one batch for that board's host alone; a card dropped on the other
//! board goes back, nothing emitted by either; a carried card has one picture; and two lists keep
//! their drags apart the same way.

#![cfg(all(target_os = "macos", feature = "gpui"))]

mod common;

use std::rc::Rc;
use std::time::Duration;

use common::ids::*;
use common::*;
use gpui::*;
use gpui_tables::Edit;
use gpui_tables::view::{TableHost, TableView};
use gpui_tables::*;
// The built-in #[test], not GPUI's, which gpui::* brings in.
use std::prelude::v1::test;

/// People's "By stage" view, drawn as a list in the list tests.
const BY_STAGE: &str = "29bdd0f4-6c47-4d20-8928-78be9f6ae20b";

/// People with the Pipeline board (or By stage as a list) in the manual order; the second's rows
/// have ids of their own, so the two views' cards tell apart.
fn people(prefix: &str, list: bool) -> (Doc, Vec<Doc>) {
    let (mut db, mut rows) = load("people");
    for v in db.database.as_mut().unwrap().views.iter_mut() {
        if v.id == PIPELINE {
            v.sort.clear();
        }
        if list && v.id == BY_STAGE {
            v.kind = ViewType::List;
        }
    }
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
            .child(div().w(px(800.)).h_full().child(self.a.clone()))
            .child(div().w(px(800.)).h_full().child(self.b.clone()))
    }
}

type Views = (Entity<TableView>, Entity<TableView>);

struct Two<'a> {
    a: Entity<TableView>,
    b: Entity<TableView>,
    host_a: Rc<TestHost>,
    host_b: Rc<TestHost>,
    cx: &'a mut VisualTestContext,
}

fn open(cx: &mut TestAppContext, list: bool) -> Two<'_> {
    setup(cx);
    let (host_a, host_b) = (TestHost::new(), TestHost::new());
    let (ha, hb): (Rc<dyn TableHost>, Rc<dyn TableHost>) = (host_a.clone(), host_b.clone());
    let made: Rc<std::cell::RefCell<Option<Views>>> = Rc::default();
    let view = if list { BY_STAGE } else { PIPELINE };
    let (_, vcx) = cx.add_window_view({
        let made = made.clone();
        move |window, cx| {
            let (da, ra) = people("", list);
            let (db, rb) = people("b-", list);
            let a = cx.new(|cx| TableView::new(ha, da, ra, cx).with_view(view));
            let b = cx.new(|cx| TableView::new(hb, db, rb, cx).with_view(view));
            *made.borrow_mut() = Some((a.clone(), b.clone()));
            let pair = cx.new(|_| Pair { a, b });
            gpui_core::Root::new(pair, window, cx)
        }
    });
    let (a, b) = made.borrow().clone().unwrap();
    *host_a.table.borrow_mut() = Some(a.downgrade());
    *host_b.table.borrow_mut() = Some(b.downgrade());
    vcx.simulate_resize(size(px(1600.), px(900.)));
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

/// A view's lanes (a board's columns, a list's groups): each key with its rows' ids.
fn lanes(view: &Entity<TableView>, cx: &mut VisualTestContext) -> Vec<(String, Vec<String>)> {
    view.update(cx, |t, _| {
        let lanes = if t.is_board() {
            t.board_columns().to_vec()
        } else {
            t.list_groups().to_vec()
        };
        lanes
            .iter()
            .map(|c| {
                (
                    c.key_string(),
                    c.rows.iter().map(|&r| t.rows()[r].id.clone()).collect(),
                )
            })
            .collect()
    })
}

fn batches(host: &TestHost) -> Vec<Vec<Edit>> {
    host.events
        .borrow()
        .iter()
        .filter_map(|e| match e {
            TableEvent::Edits { edits } => Some(edits.clone()),
            _ => None,
        })
        .collect()
}

/// Where a card or line is drawn.
fn grip(cx: &mut VisualTestContext, list: bool, id: &str, key: &str) -> Bounds<Pixels> {
    rect(
        cx,
        format!("{}:{id}:{key}", if list { "line" } else { "card" }),
    )
}

/// Presses at `from`, moves past the threshold, then to `to`, without letting go.
fn carry(cx: &mut VisualTestContext, from: Point<Pixels>, to: Point<Pixels>) {
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
}

fn let_go(cx: &mut VisualTestContext, at: Point<Pixels>) {
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    draw(cx);
}

/// The first card of the first lane with one, and the top of another lane's first card.
fn first_and_elsewhere(
    t: &mut Two,
    list: bool,
    from: &Entity<TableView>,
    to: &Entity<TableView>,
) -> (String, Point<Pixels>, Point<Pixels>) {
    let src = lanes(from, t.cx);
    let (key, ids) = src.iter().find(|(_, ids)| !ids.is_empty()).unwrap().clone();
    let id = ids[0].clone();
    let at = grip(t.cx, list, &id, &key).center();
    let dst = lanes(to, t.cx);
    let (dkey, dids) = dst
        .iter()
        .find(|(k, ids)| *k != key && !ids.is_empty())
        .unwrap()
        .clone();
    let b = grip(t.cx, list, &dids[0], &dkey);
    (id, at, point(b.center().x, b.top() + px(3.)))
}

fn a_drag_is_heard_by_its_own_view_alone(cx: &mut TestAppContext, list: bool) {
    let mut t = open(cx, list);
    let (a, b) = (t.a.clone(), t.b.clone());
    let (id, from, to) = first_and_elsewhere(&mut t, list, &a, &a);
    carry(t.cx, from, to);
    assert!(t.a.read_with(t.cx, |v, _| v.is_dragging_card()));
    assert!(!t.b.read_with(t.cx, |v, _| v.is_dragging_card()));
    let_go(t.cx, to);
    let got = batches(&t.host_a);
    assert_eq!(got.len(), 1, "one batch: {got:?}");
    assert!(
        got[0].iter().any(
            |e| matches!(e, Edit::RowsMoved { row_ids, .. } if *row_ids == [id.clone()])
                || matches!(e, Edit::RowValueChanged { row_id, .. } if *row_id == id)
        ),
        "{got:?}"
    );
    assert!(
        batches(&t.host_b).is_empty(),
        "the other view heard nothing"
    );
    // The other view drags its own the same way.
    let (id, from, to) = first_and_elsewhere(&mut t, list, &b, &b);
    carry(t.cx, from, to);
    let_go(t.cx, to);
    let got = batches(&t.host_b);
    assert_eq!(got.len(), 1, "one batch: {got:?}");
    assert!(id.starts_with("b-"));
    assert_eq!(
        batches(&t.host_a).len(),
        1,
        "and the first heard nothing more"
    );
}

fn a_drop_on_the_other_view_goes_back(cx: &mut TestAppContext, list: bool) {
    let mut t = open(cx, list);
    let (a, b) = (t.a.clone(), t.b.clone());
    let (_, from, to) = first_and_elsewhere(&mut t, list, &a, &b);
    carry(t.cx, from, to);
    let_go(t.cx, to);
    t.cx.executor().advance_clock(Duration::from_millis(300));
    draw(t.cx);
    assert!(batches(&t.host_a).is_empty(), "{:?}", batches(&t.host_a));
    assert!(batches(&t.host_b).is_empty(), "{:?}", batches(&t.host_b));
    assert!(!t.a.read_with(t.cx, |v, _| v.is_dragging_card()));
    assert!(!t.b.read_with(t.cx, |v, _| v.is_dragging_card()));
}

fn a_carried_card_has_one_picture(cx: &mut TestAppContext, list: bool) {
    let mut t = open(cx, list);
    let a = t.a.clone();
    let (_, from, to) = first_and_elsewhere(&mut t, list, &a, &a);
    carry(t.cx, from, to);
    // The picture's fill, painted once: a second provider drawing it too would paint it again.
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
    assert_eq!(fills, 1, "one picture of the card");
    keys(t.cx, "escape");
}

#[gpui::test]
fn a_drag_on_one_board_is_heard_by_its_host_alone(cx: &mut TestAppContext) {
    a_drag_is_heard_by_its_own_view_alone(cx, false);
}

#[gpui::test]
fn a_card_dropped_on_the_other_board_goes_back(cx: &mut TestAppContext) {
    a_drop_on_the_other_view_goes_back(cx, false);
}

#[gpui::test]
fn a_carried_card_has_one_picture_on_two_boards(cx: &mut TestAppContext) {
    a_carried_card_has_one_picture(cx, false);
}

#[gpui::test]
fn a_drag_in_one_list_is_heard_by_its_host_alone(cx: &mut TestAppContext) {
    a_drag_is_heard_by_its_own_view_alone(cx, true);
}

#[gpui::test]
fn a_line_dropped_on_the_other_list_goes_back(cx: &mut TestAppContext) {
    a_drop_on_the_other_view_goes_back(cx, true);
}

#[gpui::test]
fn a_carried_line_has_one_picture_on_two_lists(cx: &mut TestAppContext) {
    a_carried_card_has_one_picture(cx, true);
}
