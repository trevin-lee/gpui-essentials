//! A table's rows as wholes, driven as a person drives them, on the CRM fixtures: a row's
//! handle dragged within and across groups (one batch), sorted tables refusing to reorder,
//! selected rows dragged and deleted together, the rows' menu (open, duplicate with its
//! relations written back, move up and down, delete), deleting one row at once and several
//! after asking, the keys (Delete emptying a cell but deleting selected rows), a finger's long
//! press on the handle against a pan, the handle's 44 points on touch, and no frames while idle.

// The Mac's dev-dependencies switch GPUI's test support on; elsewhere, the test-support feature.
#![cfg(all(feature = "gpui", any(target_os = "macos", feature = "test-support")))]

mod common;

use std::rc::Rc;
use std::time::Duration;

use common::ids::*;
use common::*;
use gpui::*;
use gpui_tables::engine::relation;
use gpui_tables::view::{Select, TableView};
use gpui_tables::*;
use gpui_tables::{Edit, Sort, Value};
// The built-in #[test], not GPUI's, which gpui::* brings in.
use std::prelude::v1::test;

const LENA: &str = "4fd58dbe-7bdc-468b-bafb-2c68774b15d7";
const SAMI: &str = "bfeaa155-1a28-47b3-a4e4-e25a15fc899e";
const JONAH: &str = "842e7fc2-2954-4a6e-b12a-a1f6d42fddbb";
/// People's "By stage" list (grouped by Stage, in the manual order), drawn as a table here.
const BY_STAGE: &str = "29bdd0f4-6c47-4d20-8928-78be9f6ae20b";
/// Companies' People: the other side of People's Company.
const COMPANY_PEOPLE: &str = "5790f82e-c1d3-4cff-aa3a-f4d46b0a18e8";

fn handle(row: &str) -> String {
    format!("handle:{row}")
}

fn row(row: &str) -> String {
    format!("row:{row}")
}

fn wide() -> Size<Pixels> {
    size(px(1400.), px(900.))
}

/// People's "By stage" view as a table, changed by `f`: grouped by Stage, in the manual order.
fn by_stage(f: impl FnOnce(&mut gpui_tables::View)) -> (Doc, Vec<Doc>) {
    let (db, rows) = load("people");
    let mut db = as_table(db, BY_STAGE);
    f(db.database
        .as_mut()
        .unwrap()
        .views
        .iter_mut()
        .find(|v| v.id == BY_STAGE)
        .unwrap());
    (db, rows)
}

/// People in the manual order, not grouped.
fn plain() -> (Doc, Vec<Doc>) {
    by_stage(|v| v.group_by = None)
}

fn open<'a>(
    cx: &'a mut TestAppContext,
    host: &Rc<TestHost>,
    (db, rows): (Doc, Vec<Doc>),
    size: Size<Pixels>,
) -> (Entity<TableView>, &'a mut VisualTestContext) {
    open_with(cx, host, db, rows, Some(BY_STAGE), size)
}

/// The titles of the rows the table shows, in its order.
fn titles(table: &Entity<TableView>, cx: &mut VisualTestContext) -> Vec<String> {
    table.update(cx, |t, _| {
        let rows = t.shown_rows().to_vec();
        rows.iter().map(|&r| t.rows()[r].title.clone()).collect()
    })
}

/// The titles of a group's rows, by its key.
fn group(table: &Entity<TableView>, cx: &mut VisualTestContext, key: &str) -> Vec<String> {
    table.update(cx, |t, _| {
        let lane = t
            .row_lanes()
            .iter()
            .find(|l| l.key_string() == key)
            .unwrap_or_else(|| panic!("no group {key}"))
            .clone();
        lane.rows
            .iter()
            .map(|&r| t.rows()[r].title.clone())
            .collect()
    })
}

fn opt(key: &str) -> String {
    format!("option:{key}")
}

/// A point over the upper part of a row (where a row dropped lands above it), or with `below`,
/// its lower part.
fn over(cx: &mut VisualTestContext, id: &str, below: bool) -> Point<Pixels> {
    let b = rect(cx, row(id));
    let x = b.left() + px(200.);
    if below {
        point(x, b.bottom() - px(4.))
    } else {
        point(x, b.top() + px(4.))
    }
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

fn selected(table: &Entity<TableView>, cx: &mut VisualTestContext) -> Vec<String> {
    table.read_with(cx, |t, _| t.selected_rows().to_vec())
}

fn deleted(host: &TestHost) -> Vec<Vec<String>> {
    host.edits()
        .into_iter()
        .filter_map(|e| match e {
            Edit::RowsDeleted { row_ids } => Some(row_ids),
            _ => None,
        })
        .collect()
}

/// A row's handle, the pointer moved over the row first so it's there.
fn grip(cx: &mut VisualTestContext, id: &str) -> Bounds<Pixels> {
    let r = rect(cx, row(id));
    cx.simulate_mouse_move(
        point(r.left() + px(200.), r.center().y),
        None,
        Modifiers::none(),
    );
    draw(cx);
    rect(cx, handle(id))
}

/// A press on a row's handle.
fn press(cx: &mut VisualTestContext, id: &str) {
    press_with(cx, id, Modifiers::none());
}

fn press_with(cx: &mut VisualTestContext, id: &str, modifiers: Modifiers) {
    let at = grip(cx, id).center();
    cx.simulate_click(at, modifiers);
    draw(cx);
}

/// Starts a mouse drag on a row's handle and carries it to `to`, without letting go.
fn carry(cx: &mut VisualTestContext, id: &str, to: Point<Pixels>) {
    let from = grip(cx, id).center();
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

// --- Dragging ----------------------------------------------------------------------------------

#[gpui::test]
fn a_row_dragged_by_its_handle_moves_in_the_manual_order(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open(cx, &host, plain(), wide());
    assert_eq!(
        titles(&table, cx)[..3],
        ["Rowan Pike", "Imani Castell", "Theo Brandt"]
    );
    // Rowan (a0) carried to between Lena (a3) and Sami (a4): a line shows where, and nothing's
    // emitted until it's let go.
    let to = over(cx, SAMI, false);
    carry(cx, ROWAN, to);
    assert!(table.read_with(cx, |t, _| t.is_dragging_card()));
    assert!(drawn(cx, "landing"), "a line where it lands");
    assert!(host.edits().is_empty());
    let_go(cx, to);
    let b = batches(&host);
    assert_eq!(b.len(), 1, "{b:?}");
    let [Edit::RowsMoved { row_ids, orders }] = b[0].as_slice() else {
        panic!("{b:?}")
    };
    assert_eq!(row_ids, &[ROWAN]);
    assert!("a3" < orders[0].as_str() && orders[0].as_str() < "a4");
    assert_eq!(
        titles(&table, cx)[..5],
        [
            "Imani Castell",
            "Theo Brandt",
            "Lena Varga",
            "Rowan Pike",
            "Sami Halloran"
        ]
    );
    assert!(!table.read_with(cx, |t, _| t.is_dragging_card()));
    assert!(!drawn(cx, "landing"));
    // Let go where it was: nothing.
    host.clear();
    let at = over(cx, ROWAN, false);
    carry(cx, ROWAN, at);
    let_go(cx, at);
    assert!(host.edits().is_empty(), "{:?}", host.edits());
    // Escape puts a carried row back.
    let to = over(cx, IMANI, false);
    carry(cx, LENA, to);
    keys(cx, "escape");
    assert!(!table.read_with(cx, |t, _| t.is_dragging_card()));
    let_go(cx, to);
    assert!(host.edits().is_empty(), "{:?}", host.edits());
}

#[gpui::test]
fn keys_that_cant_be_put_between_renumber_the_order(cx: &mut TestAppContext) {
    let host = TestHost::new();
    // Lena and Sami have the same key: a row dropped between them renumbers every row.
    let (db, mut rows) = plain();
    rows.iter_mut().find(|r| r.id == SAMI).unwrap().order = "a3".into();
    let (table, cx) = open(cx, &host, (db, rows), wide());
    let to = over(cx, SAMI, false);
    carry(cx, ROWAN, to);
    let_go(cx, to);
    let b = batches(&host);
    let [Edit::RowsMoved { row_ids, orders }] = b[0].as_slice() else {
        panic!("{b:?}")
    };
    assert!(row_ids.len() > 2, "renumbered: {row_ids:?}");
    assert!(orders.iter().all(|o| order::is_valid(o)));
    let shown = titles(&table, cx);
    let at = |name: &str| shown.iter().position(|t| t == name).unwrap();
    assert_eq!(at("Rowan Pike") + 1, at("Sami Halloran"), "{shown:?}");
    assert_eq!(at("Lena Varga") + 1, at("Rowan Pike"), "{shown:?}");
}

#[gpui::test]
fn a_row_dragged_into_another_group_changes_its_value_in_one_batch(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open(cx, &host, by_stage(|_| {}), wide());
    let (lead, warm) = (opt(LEAD), opt(WARM));
    assert_eq!(group(&table, cx, &warm), ["Imani Castell", "Sami Halloran"]);
    // Jonah (a6) from Lead to between Imani (a1) and Sami (a4) in Warm.
    let to = over(cx, SAMI, false);
    carry(cx, JONAH, to);
    assert!(drawn(cx, "landing"));
    let_go(cx, to);
    let b = batches(&host);
    assert_eq!(b.len(), 1, "one batch: {b:?}");
    let [
        Edit::RowsMoved { row_ids, orders },
        Edit::RowValueChanged {
            row_id,
            property_id,
            value,
        },
    ] = b[0].as_slice()
    else {
        panic!("{b:?}")
    };
    assert_eq!(row_ids, &[JONAH]);
    assert!("a1" < orders[0].as_str() && orders[0].as_str() < "a4");
    assert_eq!((row_id.as_str(), property_id.as_str()), (JONAH, STAGE));
    assert_eq!(value, &Some(Value::text(WARM)));
    assert_eq!(
        group(&table, cx, &warm),
        ["Imani Castell", "Jonah Whitfield-Arce", "Sami Halloran"]
    );
    assert_eq!(group(&table, cx, &lead), ["Theo Brandt"]);
    // The focus went with it.
    assert_eq!(
        table
            .update(cx, |t, _| t.focused_row_cell())
            .map(|c| c.0)
            .as_deref(),
        Some(JONAH)
    );
    // Onto a group's header: the top of its group.
    host.clear();
    let to = rect(cx, format!("group:{}", opt(ACTIVE))).center();
    carry(cx, THEO, to);
    let_go(cx, to);
    assert_eq!(group(&table, cx, &opt(ACTIVE))[0], "Theo Brandt");
    assert!(host.edits().iter().any(|e| matches!(e,
        Edit::RowValueChanged { row_id, value, .. } if row_id == THEO && *value == Some(Value::text(ACTIVE)))));
    // Onto the empty group's New row: its value goes, and there's nothing to order it by.
    host.clear();
    let to = rect(cx, "new:empty").center();
    carry(cx, SAMI, to);
    let_go(cx, to);
    assert_eq!(
        batches(&host),
        vec![vec![Edit::RowValueChanged {
            row_id: SAMI.into(),
            property_id: STAGE.into(),
            value: None,
        }]]
    );
    assert_eq!(group(&table, cx, "empty"), ["Sami Halloran"]);
}

#[gpui::test]
fn a_sorted_table_refuses_to_reorder_and_says_why(cx: &mut TestAppContext) {
    let host = TestHost::new();
    // All people is sorted by name: a row carried down it lands nowhere, and the table says
    // the sort places rows.
    let (table, cx) = open_with(cx, &host, load("people").0, load("people").1, None, wide());
    let to = over(cx, THEO, true);
    carry(cx, ANA, to);
    assert!(drawn(cx, "drag-note"), "the sort is named");
    assert!(!drawn(cx, "landing"), "nowhere to land");
    let_go(cx, to);
    assert!(host.edits().is_empty(), "{:?}", host.edits());
    assert!(!drawn(cx, "drag-note"));
    // Its menu can't move rows either: Move up and down are off, and it says why.
    press(cx, LENA);
    assert!(drawn(cx, "menu-item-Move up"));
    assert!(rect(cx, "menu-note").size.width > px(0.));
    click(cx, "menu-item-Move down");
    keys(cx, "secondary-shift-down");
    assert!(host.edits().is_empty(), "{:?}", host.edits());
    keys(cx, "escape");
    let _ = table;
}

#[gpui::test]
fn a_sorted_grouped_table_changes_only_the_group(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let data = by_stage(|v| v.sort = vec![Sort::ascending(NAME)]);
    let (table, cx) = open(cx, &host, data, wide());
    // Within its group: nothing.
    let to = over(cx, ANA, true);
    carry(cx, ROWAN, to);
    let_go(cx, to);
    assert!(host.edits().is_empty(), "{:?}", host.edits());
    // Into another: the value alone, the sort placing it.
    let to = over(cx, IMANI, false);
    carry(cx, ANA, to);
    assert!(
        !drawn(cx, "landing"),
        "the group is marked, not a place in it"
    );
    let_go(cx, to);
    assert_eq!(
        batches(&host),
        vec![vec![Edit::RowValueChanged {
            row_id: ANA.into(),
            property_id: STAGE.into(),
            value: Some(Value::text(WARM)),
        }]]
    );
    assert_eq!(
        group(&table, cx, &opt(WARM)),
        ["Ana Sofía Durán", "Imani Castell", "Sami Halloran"]
    );
}

#[gpui::test]
fn the_table_scrolls_while_a_row_is_carried_to_its_edge(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open(cx, &host, crowded(), size(px(900.), px(400.)));
    let before = rect(cx, row(ANA));
    let to = over(cx, IMANI, false);
    carry(cx, ROWAN, to);
    // Each move near the bottom scrolls a step.
    let window = cx.update(|window, _| window.viewport_size());
    for i in 0..6 {
        let at = point(px(300.), window.height - px(10. + i as f32));
        cx.simulate_mouse_move(at, MouseButton::Left, Modifiers::none());
        draw(cx);
    }
    let after = rect(cx, row(ANA));
    assert!(
        after.top() < before.top() - px(40.),
        "{after:?} from {before:?}"
    );
    keys(cx, "escape");
    let_go(cx, point(px(300.), px(200.)));
    assert!(host.edits().is_empty());
    let _ = table;
}

// --- Selecting ---------------------------------------------------------------------------------

#[gpui::test]
fn selected_rows_drag_together(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open(cx, &host, plain(), wide());
    // ⌘-presses on handles select rows without a menu.
    press_with(cx, ROWAN, Modifiers::command());
    press_with(cx, LENA, Modifiers::command());
    assert_eq!(selected(&table, cx), [ROWAN, LENA]);
    assert!(!table.read_with(cx, |t, _| t.has_popover()));
    // Carried by Lena's handle to the end, after Ana: both go, in their order.
    let to = over(cx, ANA, true);
    carry(cx, LENA, to);
    let_go(cx, to);
    let b = batches(&host);
    assert_eq!(b.len(), 1, "{b:?}");
    let [Edit::RowsMoved { row_ids, orders }] = b[0].as_slice() else {
        panic!("{b:?}")
    };
    assert_eq!(row_ids, &[ROWAN, LENA]);
    assert!("a7" < orders[0].as_str() && orders[0] < orders[1]);
    let shown = titles(&table, cx);
    assert_eq!(shown[shown.len() - 2..], ["Rowan Pike", "Lena Varga"]);
    // Carrying a row that isn't selected lets go of the others.
    host.clear();
    let to = over(cx, THEO, false);
    carry(cx, SAMI, to);
    let_go(cx, to);
    assert!(selected(&table, cx).is_empty());
    let Edit::RowsMoved { row_ids, .. } = host.last_edit() else {
        panic!("{:?}", host.edits())
    };
    assert_eq!(row_ids, [SAMI]);
}

#[gpui::test]
fn selected_rows_from_two_groups_join_a_third_in_one_batch(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open(cx, &host, by_stage(|_| {}), wide());
    press_with(cx, THEO, Modifiers::command());
    press_with(cx, IMANI, Modifiers::command());
    // To the top of Active, above Rowan (a0).
    let to = over(cx, ROWAN, false);
    carry(cx, THEO, to);
    let_go(cx, to);
    let b = batches(&host);
    assert_eq!(b.len(), 1, "{b:?}");
    let [Edit::RowsMoved { row_ids, orders }, values @ ..] = b[0].as_slice() else {
        panic!("{b:?}")
    };
    // In the view's order (the manual one): Imani (a1), then Theo (a2).
    assert_eq!(row_ids, &[IMANI, THEO]);
    assert!(orders[0] < orders[1] && orders[1].as_str() < "a0");
    assert_eq!(values.len(), 2, "{values:?}");
    for id in [IMANI, THEO] {
        assert!(values.iter().any(|e| matches!(e,
            Edit::RowValueChanged { row_id, value, .. } if row_id == id && *value == Some(Value::text(ACTIVE)))));
    }
    assert_eq!(
        group(&table, cx, &opt(ACTIVE))[..3],
        ["Imani Castell", "Theo Brandt", "Rowan Pike"]
    );
}

#[gpui::test]
fn shift_and_space_select_rows(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open(cx, &host, plain(), wide());
    // A press on the handle selects the row alone (and opens its menu); Shift runs to another.
    press(cx, IMANI);
    keys(cx, "escape");
    press_with(cx, LENA, Modifiers::shift());
    assert_eq!(selected(&table, cx), [IMANI, THEO, LENA]);
    // Down carries the selection to the next row; Shift-Down runs it on.
    keys(cx, "down");
    assert_eq!(selected(&table, cx), [SAMI]);
    keys(cx, "shift-down shift-down");
    assert_eq!(selected(&table, cx).len(), 3);
    // Escape lets go of them, then of the cell.
    keys(cx, "escape");
    assert!(selected(&table, cx).is_empty());
    assert!(table.read_with(cx, |t, _| t.focused_cell().is_some()));
    keys(cx, "escape");
    assert!(table.read_with(cx, |t, _| t.focused_cell().is_none()));
    // Space selects the focused row, and again lets go of it.
    table.update(cx, |t, cx| t.focus_cell(THEO, ROLE, cx));
    keys(cx, "space");
    assert_eq!(selected(&table, cx), [THEO]);
    keys(cx, "space");
    assert!(selected(&table, cx).is_empty());
    // A press on a cell lets go of selected rows.
    keys(cx, "space");
    click(cx, cell(ANA, ROLE));
    assert!(selected(&table, cx).is_empty());
    // The API does what the presses do.
    table.update(cx, |t, cx| {
        t.select_row(ROWAN, Select::Only, cx);
        t.select_row(LENA, Select::Toggle, cx);
    });
    assert_eq!(selected(&table, cx), [ROWAN, LENA]);
    assert!(drawn(cx, handle(ROWAN)));
}

// --- The menu ------------------------------------------------------------------------------------

#[gpui::test]
fn the_rows_menu_opens_from_the_handle_a_right_click_and_keys(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open(cx, &host, plain(), wide());
    // The handle: Open opens the row.
    press(cx, THEO);
    assert_eq!(selected(&table, cx), [THEO]);
    for item in ["Open", "Duplicate", "Move up", "Move down", "Delete"] {
        assert!(drawn(cx, format!("menu-item-{item}")), "{item}");
    }
    let menu = rect(cx, "menu");
    let h = grip(cx, THEO);
    assert!(menu.top() >= h.bottom() - px(1.) || menu.bottom() <= h.top() + px(1.));
    click(cx, "menu-item-Open");
    assert!(
        matches!(host.events.borrow().last(), Some(TableEvent::OpenRow { row_id }) if row_id == THEO)
    );
    assert!(!table.read_with(cx, |t, _| t.has_popover()));
    // A right-click on any of a row's cells, at the pointer.
    let at = rect(cx, cell(LENA, ROLE)).center();
    cx.simulate_mouse_down(at, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Right, Modifiers::none());
    draw(cx);
    assert!(drawn(cx, "menu-item-Duplicate"));
    assert_eq!(selected(&table, cx), [LENA]);
    keys(cx, "escape");
    assert!(!table.read_with(cx, |t, _| t.has_popover()));
    // ⌘/, and ⌥↓ while rows are selected (else it's the column's menu).
    keys(cx, "secondary-/");
    assert!(drawn(cx, "menu-item-Duplicate"));
    keys(cx, "escape");
    keys(cx, "alt-down");
    assert!(drawn(cx, "menu-item-Duplicate"));
    keys(cx, "escape escape");
    assert!(selected(&table, cx).is_empty());
    keys(cx, "alt-down");
    assert!(drawn(cx, "menu-item-Sort ascending"));
    assert!(!drawn(cx, "menu-item-Duplicate"));
    keys(cx, "escape");
    // Several rows: no Open, a heading with their count.
    press_with(cx, ROWAN, Modifiers::command());
    press_with(cx, IMANI, Modifiers::command());
    keys(cx, "secondary-/");
    assert!(!drawn(cx, "menu-item-Open"));
    assert!(drawn(cx, "menu-item-Delete"));
}

#[gpui::test]
fn move_up_and_down_from_the_menu_and_keys(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open(cx, &host, plain(), wide());
    // ⌘⇧↓ on Rowan (a0): past Imani (a1), before Theo (a2).
    table.update(cx, |t, cx| t.focus_cell(ROWAN, ROLE, cx));
    keys(cx, "secondary-shift-down");
    let Edit::RowsMoved { row_ids, orders } = host.last_edit() else {
        panic!("{:?}", host.edits())
    };
    assert_eq!(row_ids, [ROWAN]);
    assert!("a1" < orders[0].as_str() && orders[0].as_str() < "a2");
    assert_eq!(titles(&table, cx)[..2], ["Imani Castell", "Rowan Pike"]);
    // The focus stays on it, in its column.
    assert_eq!(
        table.update(cx, |t, _| t.focused_row_cell()),
        Some((ROWAN.into(), ROLE.into()))
    );
    // The menu's Move up puts it back on top; at the top, Move up does nothing.
    press(cx, ROWAN);
    click(cx, "menu-item-Move up");
    assert_eq!(titles(&table, cx)[0], "Rowan Pike");
    host.clear();
    press(cx, ROWAN);
    click(cx, "menu-item-Move up");
    keys(cx, "escape");
    assert!(host.edits().is_empty(), "{:?}", host.edits());
}

#[gpui::test]
fn in_a_grouped_table_rows_move_within_their_group(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open(cx, &host, by_stage(|_| {}), wide());
    // Lena is Active's second (Rowan a0, Lena a3, Ana a7): up a place is its top.
    table.update(cx, |t, cx| t.focus_cell(LENA, ROLE, cx));
    keys(cx, "secondary-shift-up");
    assert_eq!(
        group(&table, cx, &opt(ACTIVE)),
        ["Lena Varga", "Rowan Pike", "Ana Sofía Durán"]
    );
    let Edit::RowsMoved { orders, .. } = host.last_edit() else {
        panic!("{:?}", host.edits())
    };
    assert!(orders[0].as_str() < "a0");
    // At its group's top it goes no further: no edit, not into Warm above.
    host.clear();
    keys(cx, "secondary-shift-up");
    assert!(host.edits().is_empty(), "{:?}", host.edits());
    assert_eq!(
        group(&table, cx, &opt(WARM)),
        ["Imani Castell", "Sami Halloran"]
    );
    // Two selected rows of a group move together; rows of two groups don't move.
    press_with(cx, ROWAN, Modifiers::command());
    press_with(cx, LENA, Modifiers::command());
    keys(cx, "secondary-shift-down");
    assert_eq!(
        group(&table, cx, &opt(ACTIVE)),
        ["Ana Sofía Durán", "Lena Varga", "Rowan Pike"]
    );
    host.clear();
    press_with(cx, IMANI, Modifiers::command());
    keys(cx, "secondary-shift-down");
    assert!(host.edits().is_empty(), "{:?}", host.edits());
}

// --- Duplicating ---------------------------------------------------------------------------------

#[gpui::test]
fn duplicating_copies_the_row_and_writes_its_relations_back(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open(cx, &host, plain(), wide());
    let (fields, rowan) = table.read_with(cx, |t, _| {
        let r = t.rows().iter().find(|r| r.id == ROWAN).unwrap().clone();
        (t.database().database.clone().unwrap(), r)
    });
    press(cx, ROWAN);
    click(cx, "menu-item-Duplicate");
    let b = batches(&host);
    assert_eq!(b.len(), 1, "one batch: {b:?}");
    let Edit::RowCreated {
        row_id: copy,
        database_id,
        order,
        title,
        values,
    } = &b[0][0]
    else {
        panic!("{b:?}")
    };
    assert_ne!(copy, ROWAN);
    assert_eq!(database_id, PEOPLE);
    assert_eq!(title, "Rowan Pike");
    assert_eq!(Some(values), rowan.values.as_ref());
    assert!(
        "a0" < order.as_str() && order.as_str() < "a1",
        "right after it"
    );
    // Each two-way relation it lists rows in: a relationChanged, so the other side lists the
    // copy too.
    let two_way: Vec<&Property> = fields
        .properties
        .iter()
        .filter(|p| p.relation().is_some_and(|t| t.two_way))
        .filter(|p| {
            rowan
                .value(&p.id)
                .and_then(Value::as_ids)
                .is_some_and(|v| !v.is_empty())
        })
        .collect();
    assert!(!two_way.is_empty());
    assert_eq!(b[0].len(), 1 + two_way.len(), "{b:?}");
    let company = b[0]
        .iter()
        .find(|e| matches!(e, Edit::RelationChanged { property_id, .. } if property_id == COMPANY))
        .expect("Company's other side")
        .clone();
    assert_eq!(
        company,
        Edit::RelationChanged {
            row_id: copy.clone(),
            property_id: COMPANY.into(),
            database_id: COMPANIES.into(),
            back_property_id: COMPANY_PEOPLE.into(),
            added: vec![KESTREL.into()],
            removed: vec![],
        }
    );
    // The host's helper turns it into Kestrel's People gaining the copy.
    let back = relation::back_edits(
        &company,
        |id| {
            host.companies.iter().find(|r| r.id == id).map(|r| {
                r.value(COMPANY_PEOPLE)
                    .and_then(Value::as_ids)
                    .unwrap_or(&[])
                    .to_vec()
            })
        },
        &Limits::default(),
    );
    let [Edit::RowValueChanged { row_id, value, .. }] = back.edits.as_slice() else {
        panic!("{back:?}")
    };
    assert_eq!(row_id, KESTREL);
    assert!(
        value
            .as_ref()
            .and_then(Value::as_ids)
            .unwrap()
            .contains(copy)
    );
    // Handed back, the copy is shown after it, and focused.
    assert_eq!(titles(&table, cx)[..2], ["Rowan Pike", "Rowan Pike"]);
    assert_eq!(
        table.update(cx, |t, _| t.focused_row_cell()).map(|c| c.0),
        Some(copy.clone())
    );
    // ⌘D duplicates the selected rows, each after its own, in one batch.
    host.clear();
    press_with(cx, THEO, Modifiers::command());
    press_with(cx, SAMI, Modifiers::command());
    keys(cx, "secondary-d");
    let b = batches(&host);
    assert_eq!(b.len(), 1);
    let created: Vec<(&str, &str)> = b[0]
        .iter()
        .filter_map(|e| match e {
            Edit::RowCreated { title, order, .. } => Some((title.as_str(), order.as_str())),
            _ => None,
        })
        .collect();
    assert_eq!(created.len(), 2);
    assert_eq!(created[0].0, "Theo Brandt");
    assert!("a2" < created[0].1 && created[0].1 < "a3");
    assert_eq!(created[1].0, "Sami Halloran");
    assert!("a4" < created[1].1 && created[1].1 < "a5");
}

#[gpui::test]
fn duplicating_between_keys_that_cant_be_put_between_renumbers(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (db, mut rows) = plain();
    // Theo has Rowan's key (and sorts after it, by id): nothing goes between them.
    rows.iter_mut().find(|r| r.id == THEO).unwrap().order = "a0".into();
    let (table, cx) = open(cx, &host, (db, rows), wide());
    table.update(cx, |t, cx| t.focus_cell(ROWAN, NAME, cx));
    keys(cx, "secondary-d");
    let b = batches(&host);
    let [
        Edit::RowsMoved { orders, .. },
        Edit::RowCreated { order, .. },
        ..,
    ] = b[0].as_slice()
    else {
        panic!("{b:?}")
    };
    assert!(orders.iter().chain([order]).all(|o| order::is_valid(o)));
    let shown = titles(&table, cx);
    assert_eq!(
        shown[..3],
        ["Rowan Pike", "Rowan Pike", "Theo Brandt"],
        "{shown:?}"
    );
}

// --- Deleting ----------------------------------------------------------------------------------

#[gpui::test]
fn one_row_deletes_at_once_and_several_after_asking(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open(cx, &host, plain(), wide());
    // ⌘⌫ on a focused row: gone at once, the focus on the row after it.
    table.update(cx, |t, cx| t.focus_cell(IMANI, ROLE, cx));
    keys(cx, "secondary-backspace");
    assert_eq!(deleted(&host), vec![vec![IMANI.to_owned()]]);
    assert!(!titles(&table, cx).contains(&"Imani Castell".to_owned()));
    assert_eq!(
        table.update(cx, |t, _| t.focused_row_cell()),
        Some((THEO.into(), ROLE.into()))
    );
    // The menu's Delete, for one row: at once too.
    host.clear();
    press(cx, THEO);
    click(cx, "menu-item-Delete");
    assert_eq!(deleted(&host), vec![vec![THEO.to_owned()]]);
    // Two selected: Delete asks first. Cancel keeps them.
    host.clear();
    press_with(cx, ROWAN, Modifiers::command());
    press_with(cx, SAMI, Modifiers::command());
    keys(cx, "delete");
    assert!(drawn(cx, "menu-item-Delete 2 rows"));
    assert!(deleted(&host).is_empty());
    click(cx, "menu-item-Cancel");
    assert!(deleted(&host).is_empty());
    assert_eq!(selected(&table, cx), [ROWAN, SAMI]);
    // Escape doesn't delete them either.
    keys(cx, "secondary-backspace");
    assert!(drawn(cx, "menu-item-Delete 2 rows"));
    keys(cx, "escape");
    assert!(deleted(&host).is_empty());
    // Confirmed from the menu, they go in one rowsDeleted.
    keys(cx, "secondary-/");
    click(cx, "menu-item-Delete");
    assert!(deleted(&host).is_empty(), "asked first");
    click(cx, "menu-item-Delete 2 rows");
    assert_eq!(
        deleted(&host),
        vec![vec![ROWAN.to_owned(), SAMI.to_owned()]]
    );
    assert!(selected(&table, cx).is_empty());
}

#[gpui::test]
fn selected_rows_delete_together(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open(cx, &host, plain(), wide());
    press_with(cx, ROWAN, Modifiers::command());
    press_with(cx, SAMI, Modifiers::command());
    keys(cx, "backspace");
    click(cx, "menu-item-Delete 2 rows");
    assert_eq!(
        batches(&host),
        vec![vec![Edit::RowsDeleted {
            row_ids: vec![ROWAN.into(), SAMI.into()]
        }]]
    );
    assert!(selected(&table, cx).is_empty());
    let shown = titles(&table, cx);
    assert!(
        !shown.contains(&"Rowan Pike".to_owned()) && !shown.contains(&"Sami Halloran".to_owned())
    );
    assert_eq!(shown.len(), 6);
}

#[gpui::test]
fn delete_empties_a_cell_but_deletes_selected_rows(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open(cx, &host, plain(), wide());
    // A focused cell: Delete empties it, as before.
    table.update(cx, |t, cx| t.focus_cell(LENA, ROLE, cx));
    keys(cx, "delete");
    assert_eq!(
        host.edits(),
        vec![Edit::RowValueChanged {
            row_id: LENA.into(),
            property_id: ROLE.into(),
            value: None,
        }]
    );
    // Typing in a cell, Delete, Backspace and ⌘⌫ are the field's.
    host.clear();
    table.update(cx, |t, cx| t.focus_cell(ROWAN, ROLE, cx));
    keys(cx, "enter");
    assert!(editing(&table, cx).is_some());
    keys(cx, "backspace delete secondary-backspace");
    assert!(editing(&table, cx).is_some());
    assert!(deleted(&host).is_empty());
    keys(cx, "escape");
    assert!(host.edits().is_empty(), "{:?}", host.edits());
    // The whole row selected (Space): Delete deletes it rather than its cell.
    keys(cx, "space delete");
    assert_eq!(deleted(&host), vec![vec![ROWAN.to_owned()]]);
    assert!(
        !host
            .edits()
            .iter()
            .any(|e| matches!(e, Edit::RowValueChanged { .. })),
        "{:?}",
        host.edits()
    );
}

// --- Touch -------------------------------------------------------------------------------------

fn crowded() -> (Doc, Vec<Doc>) {
    let (db, mut rows) = plain();
    let template = rows.iter().find(|r| r.id == ROWAN).unwrap().clone();
    for i in 0..40 {
        let mut r = template.clone();
        r.id = format!("extra-{i:02}");
        r.title = format!("Extra {i:02}");
        r.order = format!("b{i:02}");
        rows.push(r);
    }
    (db, rows)
}

#[gpui::test]
fn a_long_press_on_the_handle_drags_and_a_pan_scrolls(cx: &mut TestAppContext) {
    let host = TestHost::touch();
    let (table, cx) = open(cx, &host, crowded(), size(px(700.), px(700.)));
    // A pan from a handle scrolls the table, and picks nothing up.
    let start = grip(cx, LENA);
    pan(cx, start.center(), point(px(0.), px(-150.)));
    assert!(host.edits().is_empty(), "{:?}", host.edits());
    assert!(!table.read_with(cx, |t, _| t.is_dragging_card()));
    let moved = grip(cx, LENA);
    assert!(
        moved.top() < start.top() - px(100.),
        "{moved:?} from {start:?}"
    );
    touch(cx, TouchPhase::Started, moved.center());
    touch(cx, TouchPhase::Cancelled, moved.center());
    draw(cx);
    // Scrolled back to the top for the rest.
    let at = grip(cx, LENA).center();
    pan(cx, at, point(px(0.), px(400.)));
    let at = grip(cx, LENA).center();
    touch(cx, TouchPhase::Started, at);
    touch(cx, TouchPhase::Cancelled, at);
    draw(cx);
    // A long press on a cell (not the handle) picks nothing up.
    let at = rect(cx, cell(SAMI, ROLE)).center();
    long_press(cx, at, at);
    assert!(!table.read_with(cx, |t, _| t.is_dragging_card()));
    assert!(deleted(&host).is_empty());
    assert!(
        !host
            .edits()
            .iter()
            .any(|e| matches!(e, Edit::RowsMoved { .. })),
        "{:?}",
        host.edits()
    );
    // A long press on Sami's handle carries Sami to the top, above Rowan.
    host.clear();
    let from = grip(cx, SAMI).center();
    let to = over(cx, ROWAN, false);
    long_press(cx, from, to);
    let b = batches(&host);
    assert_eq!(b.len(), 1, "{b:?}");
    let [Edit::RowsMoved { row_ids, orders }] = b[0].as_slice() else {
        panic!("{b:?}")
    };
    assert_eq!(row_ids, &[SAMI]);
    assert!(orders[0].as_str() < "a0");
    assert_eq!(titles(&table, cx)[0], "Sami Halloran");
    assert!(!table.read_with(cx, |t, _| t.is_dragging_card()));
    // A tap on the handle selects the row and opens its menu, with a finger's rows.
    let at = grip(cx, IMANI).center();
    tap(cx, at);
    assert_eq!(selected(&table, cx), [IMANI]);
    let entry = rect(cx, "menu-item-Move down");
    assert!(entry.size.height >= px(44.), "{entry:?}");
}

#[gpui::test]
fn the_handle_takes_44_points_on_touch(cx: &mut TestAppContext) {
    let host = TestHost::touch();
    let (_, cx) = open(cx, &host, plain(), wide());
    // Shown without hovering, as nothing is hover-only on touch.
    for id in [ROWAN, IMANI, ANA] {
        let b = rect(cx, handle(id));
        assert!(
            b.size.width >= px(44.) && b.size.height >= px(44.),
            "{id}'s handle is {:?}",
            b.size
        );
        // In its row's title cell, at its left edge.
        let c = rect(cx, cell(id, NAME));
        assert_eq!(b.left(), c.left());
    }
}

#[gpui::test]
fn with_a_pointer_the_handle_is_small_and_shown_on_hover(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open(cx, &host, plain(), wide());
    // Only the row under the pointer has one: the others' aren't made at all.
    let r = rect(cx, row(LENA));
    cx.simulate_mouse_move(r.center(), None, Modifiers::none());
    draw(cx);
    assert!(drawn(cx, handle(LENA)));
    assert!(!drawn(cx, handle(ROWAN)) && !drawn(cx, handle(ANA)));
    let b = grip(cx, ROWAN);
    assert!(b.size.width < px(30.), "{b:?}");
    // The title's text starts after it, and the header's name over that.
    let c = rect(cx, cell(ROWAN, NAME));
    assert_eq!(b.left(), c.left());
    // Shown on a focused row without a pointer over it.
    table.update(cx, |t, cx| t.focus_cell(ROWAN, ROLE, cx));
    draw(cx);
    assert!(drawn(cx, handle(ROWAN)));
}

// --- Idle --------------------------------------------------------------------------------------

#[gpui::test]
fn nothing_draws_while_rows_are_selected_and_nothing_changes(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open(cx, &host, plain(), wide());
    press_with(cx, ROWAN, Modifiers::command());
    press_with(cx, LENA, Modifiers::command());
    for hover in [false, true] {
        if hover {
            let at = grip(cx, IMANI).center();
            cx.simulate_mouse_move(at, None, Modifiers::none());
        }
        draw(cx);
        draw(cx);
        let draws = host.draws.get();
        cx.executor().advance_clock(Duration::from_secs(10));
        cx.run_until_parked();
        assert_eq!(
            cx.update(|window, cx| window.simulate_next_frame(cx)),
            0,
            "no frames asked for"
        );
        assert_eq!(host.draws.get(), draws, "nothing drew");
    }
    let _ = table;
}
