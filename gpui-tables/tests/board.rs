//! The board view driven as a person drives it, on the CRM fixtures: dragging cards within and
//! across columns with the mouse and a finger, multi-select columns, folding, new cards, opening
//! and the keyboard, checking the edits it hands the host.

#![cfg(all(target_os = "macos", feature = "gpui"))]

mod common;

use std::time::Duration;

use common::ids::*;
use common::*;
use gpui::*;
use gpui_tables::engine::GroupKey;
use gpui_tables::view::{BoardSpot, TableView};
use gpui_tables::*;
use gpui_tables::{Edit, Sort, Value};
// The built-in #[test], not GPUI's, which gpui::* brings in.
use std::prelude::v1::test;

const LENA: &str = "4fd58dbe-7bdc-468b-bafb-2c68774b15d7";
const SAMI: &str = "bfeaa155-1a28-47b3-a4e4-e25a15fc899e";
const JONAH: &str = "842e7fc2-2954-4a6e-b12a-a1f6d42fddbb";
const NOOR: &str = "7a86f7a2-43c7-4b9a-bd87-a86557b6fb7e";

fn opt(key: &str) -> String {
    format!("option:{key}")
}

/// An option's id by its name.
fn option(db: &Doc, property: &str, name: &str) -> String {
    db.database
        .as_ref()
        .unwrap()
        .property(property)
        .unwrap()
        .options()
        .iter()
        .find(|o| o.name == name)
        .unwrap()
        .id
        .clone()
}

/// People's Pipeline board, grouped by Stage, with its sort (by warmth) or without.
fn pipeline(sorted: bool) -> (Doc, Vec<Doc>) {
    let (mut db, rows) = load("people");
    if !sorted {
        let f = db.database.as_mut().unwrap();
        f.views
            .iter_mut()
            .find(|v| v.id == PIPELINE)
            .unwrap()
            .sort
            .clear();
    }
    (db, rows)
}

fn open_board<'a>(
    cx: &'a mut TestAppContext,
    host: &std::rc::Rc<TestHost>,
    db: Doc,
    rows: Vec<Doc>,
    size: Size<Pixels>,
) -> (Entity<TableView>, &'a mut VisualTestContext) {
    let view = db
        .database
        .as_ref()
        .unwrap()
        .views
        .iter()
        .find(|v| v.kind == ViewType::Board)
        .unwrap()
        .id
        .clone();
    open_with(cx, host, db, rows, Some(&view), size)
}

fn wide() -> Size<Pixels> {
    size(px(1600.), px(800.))
}

/// The titles of a column's cards, by its key.
fn column(table: &Entity<TableView>, cx: &mut VisualTestContext, key: &str) -> Vec<String> {
    table.update(cx, |t, _| {
        let col = t
            .board_columns()
            .iter()
            .find(|c| c.key_string() == key)
            .unwrap_or_else(|| panic!("no column {key}"))
            .clone();
        col.rows
            .iter()
            .map(|&r| t.rows()[r].title.clone())
            .collect()
    })
}

fn card(row: &str, key: &str) -> String {
    format!("card:{row}:{key}")
}

/// A point over the gap above a column's `index`th card (or below its last, for its count):
/// where a card dropped lands in that slot.
fn slot(
    cx: &mut VisualTestContext,
    table: &Entity<TableView>,
    key: &str,
    index: usize,
) -> Point<Pixels> {
    let rows = table.update(cx, |t, _| {
        let c = t
            .board_columns()
            .iter()
            .find(|c| c.key_string() == key)
            .unwrap()
            .clone();
        c.rows
            .iter()
            .map(|&r| t.rows()[r].id.clone())
            .collect::<Vec<_>>()
    });
    if rows.is_empty() {
        return rect(cx, format!("column:{key}")).center();
    }
    if index < rows.len() {
        let b = rect(cx, card(&rows[index], key));
        point(b.center().x, b.top() - px(2.))
    } else {
        let b = rect(cx, card(rows.last().unwrap(), key));
        point(b.center().x, b.bottom() + px(2.))
    }
}

/// The batches handed to the host, each a list of edits.
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

#[gpui::test]
fn a_board_shows_a_column_per_group_with_its_cards(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (db, rows) = pipeline(true);
    let (table, cx) = open_board(cx, &host, db, rows, wide());
    assert!(drawn(cx, "board"));
    assert!(!drawn(cx, "table"), "a board, not a table");
    let keys: Vec<String> = table.update(cx, |t, _| {
        t.board_columns().iter().map(|c| c.key_string()).collect()
    });
    assert_eq!(
        keys,
        [
            "empty".to_owned(),
            opt(LEAD),
            opt(WARM),
            opt(ACTIVE),
            opt(DORMANT)
        ],
        "the empty column first, then the options in order"
    );
    // Sorted by warmth, highest first; ties in the manual order.
    assert_eq!(
        column(&table, cx, &opt(ACTIVE)),
        ["Rowan Pike", "Lena Varga", "Ana Sofía Durán"]
    );
    assert!(drawn(cx, card(ROWAN, &opt(ACTIVE))));
    assert!(drawn(cx, "board-new:empty"));
    // Dormant is folded in the fixture: a line in the Collapsed column, not a column of cards.
    assert!(drawn(cx, format!("folded:{}", opt(DORMANT))));
    assert!(!drawn(cx, format!("column:{}", opt(DORMANT))));
    assert!(!drawn(cx, card(NOOR, &opt(DORMANT))));
    // A card's values are the view's card properties, in words for assistive technology.
    let words = table.update(cx, |t, _| t.cell_words(ROWAN, COMPANY));
    assert_eq!(words.as_deref(), Some("Kestrel Ceramics"));
}

#[gpui::test]
fn the_same_entity_draws_a_table_or_a_board_by_the_views_type(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_people(cx, &host);
    assert!(drawn(cx, "table"));
    assert!(!table.update(cx, |t, _| t.is_board()));
    table.update(cx, |t, cx| t.set_view(PIPELINE, cx));
    draw(cx);
    assert!(drawn(cx, "board"));
    assert!(table.update(cx, |t, _| t.is_board()));
    assert!(table.update(cx, |t, _| t.items().len()) > 0);
    table.update(cx, |t, cx| t.set_view(ALL_PEOPLE, cx));
    draw(cx);
    assert!(drawn(cx, "table"));
    assert!(table.update(cx, |t, _| t.board_columns().is_empty()));
}

#[gpui::test]
fn a_card_dragged_within_its_column_moves_in_the_manual_order(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (db, rows) = pipeline(false);
    let (table, cx) = open_board(cx, &host, db, rows, wide());
    let active = opt(ACTIVE);
    assert_eq!(
        column(&table, cx, &active),
        ["Rowan Pike", "Lena Varga", "Ana Sofía Durán"],
        "with no sort, the manual order"
    );
    // Rowan (a0) between Lena (a3) and Ana (a7).
    let from = rect(cx, card(ROWAN, &active)).center();
    let to = slot(cx, &table, &active, 2);
    drag(cx, from, to);
    assert_eq!(
        batches(&host),
        vec![vec![Edit::RowsMoved {
            row_ids: vec![ROWAN.into()],
            orders: vec!["a4".into()],
        }]],
        "one key, between its new neighbours', and no value"
    );
    assert_eq!(
        column(&table, cx, &active),
        ["Lena Varga", "Rowan Pike", "Ana Sofía Durán"]
    );
    assert!(!table.read_with(cx, |t, _| t.is_dragging_card()));
    // To the top, before Lena (a3).
    host.clear();
    let from = rect(cx, card(ANA, &active)).center();
    let to = slot(cx, &table, &active, 0);
    drag(cx, from, to);
    let Edit::RowsMoved { row_ids, orders } = host.last_edit() else {
        panic!("{:?}", host.edits())
    };
    assert_eq!(row_ids, [ANA]);
    assert!(orders[0].as_str() < "a3" && order::is_valid(&orders[0]));
    assert_eq!(
        column(&table, cx, &active),
        ["Ana Sofía Durán", "Lena Varga", "Rowan Pike"]
    );
    // Let go where it was: nothing.
    host.clear();
    let from = rect(cx, card(LENA, &active)).center();
    drag(cx, from, from + point(px(0.), px(6.)));
    assert!(host.edits().is_empty(), "{:?}", host.edits());
    // The card it was moved to is focused.
    assert_eq!(
        table.update(cx, |t, _| t.focused_card()),
        Some(BoardSpot::Card {
            column: 3,
            index: 1
        })
    );
}

#[gpui::test]
fn a_card_dragged_to_another_column_changes_its_value_and_place_in_one_batch(
    cx: &mut TestAppContext,
) {
    let host = TestHost::new();
    let (db, rows) = pipeline(false);
    let (table, cx) = open_board(cx, &host, db, rows, wide());
    let (lead, warm) = (opt(LEAD), opt(WARM));
    assert_eq!(
        column(&table, cx, &warm),
        ["Imani Castell", "Sami Halloran"]
    );
    // Jonah (a6) from Lead to between Imani (a1) and Sami (a4) in Warm.
    let from = rect(cx, card(JONAH, &lead)).center();
    let to = slot(cx, &table, &warm, 1);
    // While it's carried, the drop is shown and the card it came from fades.
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    draw(cx);
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    draw(cx);
    cx.simulate_mouse_move(
        to + point(px(1.), px(0.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    draw(cx);
    assert!(table.read_with(cx, |t, _| t.is_dragging_card()));
    assert!(host.edits().is_empty(), "nothing until it's let go");
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    draw(cx);
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
        column(&table, cx, &warm),
        ["Imani Castell", "Jonah Whitfield-Arce", "Sami Halloran"]
    );
    assert_eq!(column(&table, cx, &lead), ["Theo Brandt"]);
    // To the empty column: the value goes.
    host.clear();
    let from = rect(cx, card(THEO, &lead)).center();
    let to = rect(cx, "column:empty").center();
    drag(cx, from, to);
    let b = batches(&host);
    assert_eq!(
        b,
        vec![vec![Edit::RowValueChanged {
            row_id: THEO.into(),
            property_id: STAGE.into(),
            value: None,
        }]],
        "an empty column has no neighbours to place it between"
    );
    assert_eq!(column(&table, cx, "empty"), ["Theo Brandt"]);
    // Let go over no column: nothing, and the drag is over.
    host.clear();
    let from = rect(cx, card(SAMI, &warm)).center();
    drag(cx, from, point(px(1590.), px(790.)));
    assert!(host.edits().is_empty(), "{:?}", host.edits());
    assert!(!table.read_with(cx, |t, _| t.is_dragging_card()));
}

#[gpui::test]
fn a_sorted_board_places_cards_by_its_sort(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (db, rows) = pipeline(true);
    let (table, cx) = open_board(cx, &host, db, rows, wide());
    let (active, warm) = (opt(ACTIVE), opt(WARM));
    // Within a column, a drag changes nothing: the sort decides the order.
    let from = rect(cx, card(ROWAN, &active)).center();
    let to = slot(cx, &table, &active, 3);
    drag(cx, from, to);
    assert!(host.edits().is_empty(), "{:?}", host.edits());
    // Across, only the value changes.
    let from = rect(cx, card(ANA, &active)).center();
    let to = slot(cx, &table, &warm, 0);
    drag(cx, from, to);
    assert_eq!(
        batches(&host),
        vec![vec![Edit::RowValueChanged {
            row_id: ANA.into(),
            property_id: STAGE.into(),
            value: Some(Value::text(WARM)),
        }]]
    );
    // Warmth 4 sorts Ana after Imani (4, earlier in the manual order) and before Sami (3).
    assert_eq!(
        column(&table, cx, &warm),
        ["Imani Castell", "Ana Sofía Durán", "Sami Halloran"]
    );
}

#[gpui::test]
fn a_multi_select_card_swaps_one_option_for_another(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (mut db, rows) = pipeline(false);
    let (friend, alumni, mentor, investor, recruiter) = (
        option(&db, TAGS, "Friend"),
        option(&db, TAGS, "Alumni"),
        MENTOR.to_owned(),
        INVESTOR.to_owned(),
        option(&db, TAGS, "Recruiter"),
    );
    {
        let v = db
            .database
            .as_mut()
            .unwrap()
            .views
            .iter_mut()
            .find(|v| v.id == PIPELINE)
            .unwrap();
        v.group_by = Some(TAGS.into());
        v.collapsed_groups.clear();
    }
    let (table, cx) = open_board(cx, &host, db, rows, size(px(2000.), px(800.)));
    // Lena is a friend and a mentor: in both columns.
    assert!(column(&table, cx, &opt(&friend)).contains(&"Lena Varga".to_owned()));
    assert!(column(&table, cx, &opt(&mentor)).contains(&"Lena Varga".to_owned()));
    // Dragged from Mentor to Investor, Mentor becomes Investor in its place.
    let from = rect(cx, card(LENA, &opt(&mentor))).center();
    let to = slot(cx, &table, &opt(&investor), 0);
    drag(cx, from, to);
    let value = host.edits().into_iter().find_map(|e| match e {
        Edit::RowValueChanged { row_id, value, .. } if row_id == LENA => Some(value),
        _ => None,
    });
    assert_eq!(
        value,
        Some(Some(Value::ids([friend.clone(), investor.clone()])))
    );
    assert!(!column(&table, cx, &opt(&mentor)).contains(&"Lena Varga".to_owned()));
    // Imani is an investor and alumna: from Alumni to Investor, Alumni just goes.
    host.clear();
    let from = rect(cx, card(IMANI, &opt(&alumni))).center();
    let to = slot(cx, &table, &opt(&investor), 0);
    drag(cx, from, to);
    let value = host.edits().into_iter().find_map(|e| match e {
        Edit::RowValueChanged { value, .. } => Some(value),
        _ => None,
    });
    assert_eq!(value, Some(Some(Value::ids([investor.clone()]))));
    // To "No Tags", every tag goes.
    host.clear();
    let from = rect(cx, card(THEO, &opt(&recruiter))).center();
    let to = rect(cx, "column:empty").center();
    drag(cx, from, to);
    let value = host.edits().into_iter().find_map(|e| match e {
        Edit::RowValueChanged { value, .. } => Some(value),
        _ => None,
    });
    assert_eq!(value, Some(None));
    // The same, through the API a host's own control (or assistive technology) can use.
    host.clear();
    let cols = table.update(cx, |t, _| t.board_columns().to_vec());
    let from_col = cols
        .iter()
        .position(|c| c.key == Some(GroupKey::Option(friend.clone())))
        .unwrap();
    let to_col = cols
        .iter()
        .position(|c| c.key == Some(GroupKey::Option(alumni.clone())))
        .unwrap();
    cx.update(|window, cx| {
        table.update(cx, |t, cx| {
            t.move_card(ANA, from_col, to_col, 0, window, cx)
        })
    });
    draw(cx);
    let value = host.edits().into_iter().find_map(|e| match e {
        Edit::RowValueChanged { value, .. } => Some(value),
        _ => None,
    });
    assert_eq!(value, Some(Some(Value::ids([alumni.clone()]))));
}

#[gpui::test]
fn keys_that_cant_be_put_between_renumber_the_order(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (db, mut rows) = pipeline(false);
    // Lena and Ana share a key, so nothing goes between them.
    rows.iter_mut().find(|r| r.id == ANA).unwrap().order = "a3".into();
    let (table, cx) = open_board(cx, &host, db, rows, wide());
    let active = opt(ACTIVE);
    let from = rect(cx, card(ROWAN, &active)).center();
    let to = slot(cx, &table, &active, 2);
    drag(cx, from, to);
    let Edit::RowsMoved { row_ids, orders } = host.last_edit() else {
        panic!("{:?}", host.edits())
    };
    assert!(row_ids.len() > 1, "the whole order renumbered");
    assert!(orders.iter().all(|k| order::is_valid(k)));
    assert_eq!(
        column(&table, cx, &active),
        ["Lena Varga", "Rowan Pike", "Ana Sofía Durán"]
    );
    // Every row has its own key now, rising in the manual order.
    let keys: Vec<String> = table.read_with(cx, |t, _| {
        let mut k: Vec<String> = t
            .rows()
            .iter()
            .filter(|r| !r.deleted)
            .map(|r| r.order.clone())
            .collect();
        k.sort();
        k
    });
    let mut unique = keys.clone();
    unique.dedup();
    assert_eq!(keys, unique);
}

#[gpui::test]
fn columns_fold_into_the_collapsed_column_and_back(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (db, rows) = pipeline(true);
    let (table, cx) = open_board(cx, &host, db, rows, wide());
    click(cx, format!("fold:{}", opt(ACTIVE)));
    assert_eq!(
        host.last_view().collapsed_groups,
        vec![opt(DORMANT), opt(ACTIVE)]
    );
    assert!(drawn(cx, format!("folded:{}", opt(ACTIVE))));
    assert!(!drawn(cx, card(ROWAN, &opt(ACTIVE))));
    assert!(table.update(cx, |t, _| t.board_columns()[3].folded));
    // Pressing its line unfolds it.
    click(cx, format!("folded:{}", opt(DORMANT)));
    assert_eq!(host.last_view().collapsed_groups, vec![opt(ACTIVE)]);
    assert!(drawn(cx, card(NOOR, &opt(DORMANT))));
}

#[gpui::test]
fn new_cards_get_their_columns_value_and_a_key_after_every_row(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (db, rows) = pipeline(true);
    let (table, cx) = open_board(cx, &host, db, rows, wide());
    click(cx, format!("board-new:{}", opt(WARM)));
    let Edit::RowCreated {
        row_id,
        database_id,
        order,
        title,
        values,
    } = host.last_edit()
    else {
        panic!("{:?}", host.edits())
    };
    assert_eq!(database_id, PEOPLE);
    assert_eq!(title, "");
    assert_eq!(values.get(STAGE), Some(&Value::text(WARM)));
    assert!(order.as_str() > "a8V" && order::is_valid(&order));
    // Handed back, the new card is focused, in Warm.
    let spot = table.update(cx, |t, _| t.focused_card());
    let Some(BoardSpot::Card { column, index }) = spot else {
        panic!("{spot:?}")
    };
    let id = table.update(cx, |t, _| {
        let r = t.board_columns()[column].rows[index];
        t.rows()[r].id.clone()
    });
    assert_eq!((column, id), (2, row_id));
    // The empty column's has no stage.
    click(cx, "board-new:empty");
    let Edit::RowCreated { values, .. } = host.last_edit() else {
        panic!()
    };
    assert_eq!(values.get(STAGE), None);
}

#[gpui::test]
fn pressing_a_card_opens_its_row(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (db, rows) = pipeline(true);
    let (_, cx) = open_board(cx, &host, db, rows, wide());
    click(cx, card(IMANI, &opt(WARM)));
    assert_eq!(
        host.events.borrow().last(),
        Some(&TableEvent::OpenRow {
            row_id: IMANI.into()
        })
    );
    assert!(host.edits().is_empty());
}

#[gpui::test]
fn the_keyboard_moves_between_cards_and_columns(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (db, rows) = pipeline(true);
    let (table, cx) = open_board(cx, &host, db, rows, wide());
    let spot = |table: &Entity<TableView>, cx: &mut VisualTestContext| {
        table.update(cx, |t, _| t.focused_card())
    };
    // The first arrow lands on the first column: No Stage has no cards, so its New.
    keys(cx, "down");
    assert_eq!(spot(&table, cx), Some(BoardSpot::New { column: 0 }));
    keys(cx, "right");
    assert_eq!(
        spot(&table, cx),
        Some(BoardSpot::Card {
            column: 1,
            index: 0
        })
    );
    keys(cx, "down");
    assert_eq!(
        spot(&table, cx),
        Some(BoardSpot::Card {
            column: 1,
            index: 1
        })
    );
    keys(cx, "right");
    assert_eq!(
        spot(&table, cx),
        Some(BoardSpot::Card {
            column: 2,
            index: 1
        }),
        "the same height in the next column"
    );
    keys(cx, "enter");
    assert_eq!(
        host.events.borrow().last(),
        Some(&TableEvent::OpenRow {
            row_id: SAMI.into()
        })
    );
    // Down past the last card is its column's New; Enter makes a card there.
    keys(cx, "down down down");
    assert_eq!(spot(&table, cx), Some(BoardSpot::New { column: 2 }));
    keys(cx, "enter");
    assert!(
        matches!(host.last_edit(), Edit::RowCreated { values, .. } if values.get(STAGE) == Some(&Value::text(WARM)))
    );
    // Right to the end reaches the folded columns; Enter unfolds one.
    keys(cx, "right right right");
    assert_eq!(spot(&table, cx), Some(BoardSpot::Folded { column: 4 }));
    keys(cx, "enter");
    assert!(host.last_view().collapsed_groups.is_empty());
    // Tab goes on down the columns; Escape lets go.
    keys(cx, "left tab");
    assert!(spot(&table, cx).is_some());
    keys(cx, "escape");
    assert_eq!(spot(&table, cx), None);
}

#[gpui::test]
fn a_long_press_picks_a_card_up_and_a_pan_scrolls(cx: &mut TestAppContext) {
    let host = TestHost::touch();
    let (db, mut rows) = pipeline(false);
    // Enough cards in Active to scroll.
    let template = rows.iter().find(|r| r.id == ROWAN).unwrap().clone();
    for i in 0..30 {
        let mut r = template.clone();
        r.id = format!("extra-{i:02}");
        r.title = format!("Extra {i:02}");
        r.order = format!("b{i:02}");
        rows.push(r);
    }
    let (table, cx) = open_board(cx, &host, db, rows, size(px(800.), px(600.)));
    let (active, warm) = (opt(ACTIVE), opt(WARM));
    // A plain pan sideways scrolls the board, and nothing's picked up.
    let at = rect(cx, card(IMANI, &warm)).center();
    pan(cx, at, point(px(-300.), px(0.)));
    let sx = table.read_with(cx, |t, _| t.board_scroll_x());
    assert!(sx > px(150.), "scrolled {sx:?}");
    assert!(host.edits().is_empty());
    draw(cx);
    // Up and down scrolls the column under the finger alone.
    let rowan = rect(cx, card(ROWAN, &active));
    let imani = rect(cx, card(IMANI, &warm));
    pan(cx, rowan.center(), point(px(0.), px(-200.)));
    assert!(
        !drawn(cx, card(ROWAN, &active))
            || rect(cx, card(ROWAN, &active)).top() < rowan.top() - px(100.)
    );
    assert_eq!(rect(cx, card(IMANI, &warm)).origin, imani.origin);
    assert_eq!(table.read_with(cx, |t, _| t.board_scroll_x()), sx);
    assert!(host.edits().is_empty(), "{:?}", host.edits());
    // A long press picks Imani up, and the finger carries it to the top of Active. (First a
    // touch to stop the pans' fling: GPUI times momentum by the real clock, which a test's
    // doesn't move, and a touch that stops a fling only stops it.)
    let from = rect(cx, card(IMANI, &warm)).center();
    touch(cx, TouchPhase::Started, from);
    touch(cx, TouchPhase::Cancelled, from);
    draw(cx);
    assert!(!table.read_with(cx, |t, _| t.is_dragging_card()));
    host.clear();
    let column_bounds = rect(cx, format!("column:{active}"));
    let to = point(column_bounds.center().x, column_bounds.top() + px(70.));
    long_press(cx, from, to);
    let b = batches(&host);
    assert_eq!(b.len(), 1, "{b:?}");
    // Its key (a1) already sits between its new neighbours' (Rowan's a0, Lena's a3), so the
    // batch is the value alone.
    assert_eq!(
        b[0],
        vec![Edit::RowValueChanged {
            row_id: IMANI.into(),
            property_id: STAGE.into(),
            value: Some(Value::text(ACTIVE)),
        }]
    );
    assert!(column(&table, cx, &active).contains(&"Imani Castell".to_owned()));
    assert!(!table.read_with(cx, |t, _| t.is_dragging_card()));
    // A tap opens a card.
    let at = rect(cx, card(SAMI, &warm)).center();
    tap(cx, at);
    assert_eq!(
        host.events.borrow().last(),
        Some(&TableEvent::OpenRow {
            row_id: SAMI.into()
        })
    );
}

#[gpui::test]
fn touch_targets_are_44_points(cx: &mut TestAppContext) {
    let host = TestHost::touch();
    let (db, rows) = pipeline(true);
    let (_, cx) = open_board(cx, &host, db, rows, wide());
    for what in [
        card(IMANI, &opt(WARM)),
        format!("board-new:{}", opt(WARM)),
        format!("folded:{}", opt(DORMANT)),
    ] {
        let b = rect(cx, what.clone());
        assert!(
            b.size.height >= px(44.) && b.size.width >= px(44.),
            "{what} is {:?}",
            b.size
        );
    }
    // The fold button is drawn small, and takes a finger's 44 points.
    let fold = rect(cx, format!("fold:{}", opt(WARM)));
    assert!(fold.size.width < px(44.));
    tap(cx, fold.center() + point(px(0.), px(19.)));
    assert!(host.last_view().collapsed_groups.contains(&opt(WARM)));
}

#[gpui::test]
fn a_board_draws_nothing_while_nothing_changes(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (db, rows) = pipeline(true);
    let (table, cx) = open_board(cx, &host, db, rows, wide());
    table.update(cx, |t, cx| t.focus_card(IMANI, cx));
    for hover in [false, true] {
        if hover {
            let at = rect(cx, card(IMANI, &opt(WARM))).center();
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
}

#[gpui::test]
fn a_board_without_a_group_is_one_column(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (mut db, rows) = pipeline(false);
    db.database
        .as_mut()
        .unwrap()
        .views
        .iter_mut()
        .find(|v| v.id == PIPELINE)
        .unwrap()
        .group_by = None;
    let (table, cx) = open_board(cx, &host, db, rows, wide());
    let cols = table.update(cx, |t, _| t.board_columns().to_vec());
    assert_eq!(cols.len(), 1);
    assert_eq!((cols[0].key.clone(), cols[0].rows.len()), (None, 8));
    // Its cards reorder; there's no value to change and nothing to fold.
    assert!(!drawn(cx, "fold:all"));
    let from = rect(cx, card(ROWAN, "all")).center();
    let to = slot(cx, &table, "all", 2);
    drag(cx, from, to);
    assert!(matches!(&batches(&host)[..], [b] if matches!(&b[..], [Edit::RowsMoved { .. }])));
    click(cx, "board-new:all");
    let Edit::RowCreated { values, .. } = host.last_edit() else {
        panic!()
    };
    assert!(values.is_empty());
    let _ = Sort::ascending(NAME);
}
