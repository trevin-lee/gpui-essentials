//! The list view driven as a person drives it, on the CRM fixtures: lines and their values,
//! opening rows, the keyboard, groups that fold, dragging rows within and across groups with
//! the mouse and a finger, sorted lists, multi-select groups, new rows, touch targets and no
//! frames while idle, checking the edits it hands the host.

#![cfg(all(target_os = "macos", feature = "gpui"))]

mod common;

use std::rc::Rc;
use std::time::Duration;

use common::ids::*;
use common::*;
use gpui::*;
use gpui_tables::view::{Item, TableView};
use gpui_tables::*;
use gpui_tables::{Edit, Sort, Value};
// The built-in #[test], not GPUI's, which gpui::* brings in.
use std::prelude::v1::test;

const LENA: &str = "4fd58dbe-7bdc-468b-bafb-2c68774b15d7";
const SAMI: &str = "bfeaa155-1a28-47b3-a4e4-e25a15fc899e";
const JONAH: &str = "842e7fc2-2954-4a6e-b12a-a1f6d42fddbb";
const NOOR: &str = "7a86f7a2-43c7-4b9a-bd87-a86557b6fb7e";
/// People's "By stage" list: grouped by Stage, in the manual order.
const BY_STAGE: &str = "29bdd0f4-6c47-4d20-8928-78be9f6ae20b";
/// People's "Follow up" list: sorted by last contact, filtered to those to follow up.
const FOLLOW_UP_LIST: &str = "8f2c6ec8-cc41-49a3-ae3a-2b7fdfe01893";
/// Interactions' "Recent" list: most recent first.
const RECENT: &str = "877acaad-4e4c-4ad3-89ca-c6ab132e6cab";
const PORTFOLIO: &str = "c2216b02";

fn opt(key: &str) -> String {
    format!("option:{key}")
}

fn line(row: &str, key: &str) -> String {
    format!("line:{row}:{key}")
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

/// People with the "By stage" list changed by `f`.
fn people_with(f: impl FnOnce(&mut gpui_tables::View)) -> (Doc, Vec<Doc>) {
    let (mut db, rows) = load("people");
    f(db.database
        .as_mut()
        .unwrap()
        .views
        .iter_mut()
        .find(|v| v.id == BY_STAGE)
        .unwrap());
    (db, rows)
}

fn wide() -> Size<Pixels> {
    size(px(1200.), px(900.))
}

fn open_list<'a>(
    cx: &'a mut TestAppContext,
    host: &Rc<TestHost>,
    (db, rows): (Doc, Vec<Doc>),
    view: &str,
    size: Size<Pixels>,
) -> (Entity<TableView>, &'a mut VisualTestContext) {
    open_with(cx, host, db, rows, Some(view), size)
}

/// The titles of a group's rows, by its key ("all" for a list that doesn't group).
fn group(table: &Entity<TableView>, cx: &mut VisualTestContext, key: &str) -> Vec<String> {
    table.update(cx, |t, _| {
        let g = t
            .list_groups()
            .iter()
            .find(|g| g.key_string() == key)
            .unwrap_or_else(|| panic!("no group {key}"))
            .clone();
        g.rows.iter().map(|&r| t.rows()[r].title.clone()).collect()
    })
}

/// The focused line: a row's id, or what else it is.
fn focused(table: &Entity<TableView>, cx: &mut VisualTestContext) -> Option<String> {
    table.update(cx, |t, _| {
        let c = t.focused_cell()?;
        let item = *t.items().get(c.item)?;
        Some(match item {
            Item::Row { row, .. } => t.rows()[row].id.clone(),
            Item::Group(g) => format!("group {g}"),
            Item::New(g) => format!("new {g:?}"),
        })
    })
}

/// A point over the upper part of a group's `index`th line (or the lower part of its last,
/// for its count): where a row dropped lands in that slot.
fn slot(
    cx: &mut VisualTestContext,
    table: &Entity<TableView>,
    key: &str,
    index: usize,
) -> Point<Pixels> {
    let rows = table.update(cx, |t, _| {
        let g = t
            .list_groups()
            .iter()
            .find(|g| g.key_string() == key)
            .unwrap()
            .clone();
        g.rows
            .iter()
            .map(|&r| t.rows()[r].id.clone())
            .collect::<Vec<_>>()
    });
    if index < rows.len() {
        let b = rect(cx, line(&rows[index], key));
        point(b.center().x, b.top() + px(4.))
    } else {
        let b = rect(cx, line(rows.last().unwrap(), key));
        point(b.center().x, b.bottom() - px(4.))
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

fn last_open(host: &TestHost) -> Option<String> {
    host.events.borrow().iter().rev().find_map(|e| match e {
        TableEvent::OpenRow { row_id } => Some(row_id.clone()),
        _ => None,
    })
}

#[gpui::test]
fn a_list_shows_a_line_per_row_with_its_values(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_list(cx, &host, load("interactions"), RECENT, wide());
    assert!(drawn(cx, "list"));
    assert!(!drawn(cx, "table") && !drawn(cx, "board"));
    assert!(table.update(cx, |t, _| t.is_list()));
    // One group of every row, most recent first, with a New line at the end.
    assert_eq!(
        group(&table, cx, "all"),
        [
            "Portfolio review",
            "Catch-up on the new studio",
            "Quick thanks",
            "Pilot scoping",
            "Studio visit",
            "Referral for the platform role",
            "Coffee near the office",
            "Intro call about kiln sensors"
        ]
    );
    let ids: Vec<String> = table.update(cx, |t, _| {
        let g = t.list_groups()[0].clone();
        g.rows.iter().map(|&r| t.rows()[r].id.clone()).collect()
    });
    for id in &ids {
        assert!(drawn(cx, line(id, "all")), "{id} is drawn");
    }
    assert!(drawn(cx, "new:all"));
    // The line's icon is the doc's, and its words are its title and values.
    let portfolio = &ids[0];
    assert!(portfolio.starts_with(PORTFOLIO));
    let icon = table.read_with(cx, |t, _| {
        let r = t.rows().iter().find(|r| r.id == *portfolio).unwrap();
        r.icon().map(str::to_owned)
    });
    assert_eq!(icon.as_deref(), Some("☕"));
    // The values sit to the right of the title, in the line.
    let l = rect(cx, line(portfolio, "all"));
    assert!(l.size.width > px(1000.));
    // Pressing a line opens its row.
    click(cx, line(portfolio, "all"));
    assert_eq!(last_open(&host).as_deref(), Some(portfolio.as_str()));
    assert!(host.edits().is_empty());
    assert_eq!(focused(&table, cx).as_deref(), Some(portfolio.as_str()));
}

#[gpui::test]
fn an_asset_icon_or_an_invalid_one_draws_nothing(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (db, mut rows) = load("interactions");
    let asset = format!("asset:{}", "0123456789abcdef".repeat(4));
    let mut ids = Vec::new();
    for (r, icon) in rows
        .iter_mut()
        .zip([asset.as_str(), "asset:nothex", "☕☕", "🌲"])
    {
        r.extra.insert("icon".into(), icon.into());
        ids.push(r.id.clone());
    }
    let (table, cx) = open_list(cx, &host, (db, rows), RECENT, wide());
    for id in &ids {
        assert!(drawn(cx, line(id, "all")), "{id} is drawn");
    }
    // An image icon, a malformed one and two emoji have no icon, so no text; the emoji does.
    for id in &ids[..3] {
        assert!(!drawn(cx, format!("icon:{id}")), "{id} draws no icon");
    }
    assert!(drawn(cx, format!("icon:{}", ids[3])));
    let (icons, assets) = table.read_with(cx, |t, _| {
        let row = |id: &str| t.rows().iter().find(|r| r.id == id).unwrap();
        let icons: Vec<Option<String>> = ids
            .iter()
            .map(|id| row(id).icon().map(str::to_owned))
            .collect();
        let assets: Vec<Option<String>> = ids
            .iter()
            .map(|id| row(id).icon_asset().map(str::to_owned))
            .collect();
        (icons, assets)
    });
    assert_eq!(icons, [None, None, None, Some("🌲".to_owned())]);
    assert_eq!(assets[0].as_deref(), asset.strip_prefix("asset:"));
    assert_eq!(assets[1..], [None, None, None]);
}

#[gpui::test]
fn the_same_entity_draws_a_table_a_board_or_a_list(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_people(cx, &host);
    assert!(drawn(cx, "table"));
    table.update(cx, |t, cx| t.set_view(BY_STAGE, cx));
    draw(cx);
    assert!(drawn(cx, "list"));
    assert!(table.update(cx, |t, _| t.is_list() && !t.is_board()));
    table.update(cx, |t, cx| t.set_view(PIPELINE, cx));
    draw(cx);
    assert!(drawn(cx, "board"));
    assert!(table.update(cx, |t, _| t.list_groups().is_empty()));
    table.update(cx, |t, cx| t.set_view(ALL_PEOPLE, cx));
    draw(cx);
    assert!(drawn(cx, "table"));
}

#[gpui::test]
fn the_keyboard_moves_between_lines_and_opens_them(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_list(cx, &host, load("people"), BY_STAGE, wide());
    // The first arrow lands on the first line: No Stage's header.
    keys(cx, "down");
    assert_eq!(focused(&table, cx).as_deref(), Some("group 0"));
    keys(cx, "down");
    assert_eq!(focused(&table, cx).as_deref(), Some("new Some(0)"));
    keys(cx, "down down");
    assert_eq!(focused(&table, cx).as_deref(), Some(THEO));
    keys(cx, "down");
    assert_eq!(focused(&table, cx).as_deref(), Some(JONAH));
    keys(cx, "enter");
    assert_eq!(last_open(&host).as_deref(), Some(JONAH));
    keys(cx, "up");
    assert_eq!(focused(&table, cx).as_deref(), Some(THEO));
    // Enter on a New line makes a row in its group; Tab moves on too.
    keys(cx, "tab tab");
    assert_eq!(focused(&table, cx).as_deref(), Some("new Some(1)"));
    keys(cx, "enter");
    assert!(
        matches!(host.last_edit(), Edit::RowCreated { values, .. } if values.get(STAGE) == Some(&Value::text(LEAD)))
    );
    // Enter on a header folds its group.
    host.clear();
    keys(cx, "down down");
    let at = focused(&table, cx).unwrap();
    assert!(at.starts_with("group"), "{at}");
    keys(cx, "enter");
    assert_eq!(host.last_view().collapsed_groups, vec![opt(WARM)]);
    assert_eq!(
        focused(&table, cx),
        Some(at),
        "the focus stays on the header"
    );
    // Page Down goes a screenful; Escape lets go.
    keys(cx, "pagedown");
    assert!(focused(&table, cx).is_some());
    keys(cx, "escape");
    assert_eq!(focused(&table, cx), None);
}

#[gpui::test]
fn groups_have_headers_with_counts_and_fold(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_list(cx, &host, load("people"), BY_STAGE, wide());
    let keys: Vec<String> = table.update(cx, |t, _| {
        t.list_groups().iter().map(|g| g.key_string()).collect()
    });
    assert_eq!(
        keys,
        [
            "empty".to_owned(),
            opt(LEAD),
            opt(WARM),
            opt(ACTIVE),
            opt(DORMANT)
        ]
    );
    assert_eq!(
        group(&table, cx, &opt(ACTIVE)),
        ["Rowan Pike", "Lena Varga", "Ana Sofía Durán"],
        "the manual order"
    );
    for k in &keys {
        assert!(drawn(cx, format!("group:{k}")));
        assert!(drawn(cx, format!("new:{k}")));
    }
    // Pressing a header folds it: a viewChanged, and its rows and New line go.
    click(cx, format!("group:{}", opt(ACTIVE)));
    assert_eq!(host.last_view().collapsed_groups, vec![opt(ACTIVE)]);
    assert!(!drawn(cx, line(ROWAN, &opt(ACTIVE))));
    assert!(!drawn(cx, format!("new:{}", opt(ACTIVE))));
    assert!(drawn(cx, format!("group:{}", opt(ACTIVE))));
    assert!(drawn(cx, line(NOOR, &opt(DORMANT))));
    assert!(table.update(cx, |t, _| t.list_groups()[3].folded));
    // And again unfolds it.
    click(cx, format!("group:{}", opt(ACTIVE)));
    assert!(host.last_view().collapsed_groups.is_empty());
    assert!(drawn(cx, line(ROWAN, &opt(ACTIVE))));
}

#[gpui::test]
fn a_row_dragged_within_its_group_moves_in_the_manual_order(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_list(cx, &host, load("people"), BY_STAGE, wide());
    let active = opt(ACTIVE);
    // Rowan (a0) between Lena (a3) and Ana (a7).
    let from = rect(cx, line(ROWAN, &active)).center();
    let to = slot(cx, &table, &active, 2);
    drag(cx, from, to);
    let b = batches(&host);
    assert_eq!(b.len(), 1, "{b:?}");
    let [Edit::RowsMoved { row_ids, orders }] = b[0].as_slice() else {
        panic!("{b:?}")
    };
    assert_eq!(row_ids, &[ROWAN]);
    assert!("a3" < orders[0].as_str() && orders[0].as_str() < "a7");
    assert_eq!(
        group(&table, cx, &active),
        ["Lena Varga", "Rowan Pike", "Ana Sofía Durán"]
    );
    assert!(!table.read_with(cx, |t, _| t.is_dragging_card()));
    assert_eq!(focused(&table, cx).as_deref(), Some(ROWAN));
    // To the bottom, after Ana.
    host.clear();
    let from = rect(cx, line(LENA, &active)).center();
    let to = slot(cx, &table, &active, 3);
    drag(cx, from, to);
    let Edit::RowsMoved { row_ids, orders } = host.last_edit() else {
        panic!("{:?}", host.edits())
    };
    assert_eq!(row_ids, [LENA]);
    assert!(orders[0].as_str() > "a7" && order::is_valid(&orders[0]));
    assert_eq!(
        group(&table, cx, &active),
        ["Rowan Pike", "Ana Sofía Durán", "Lena Varga"]
    );
    // Let go where it was: nothing.
    host.clear();
    let from = rect(cx, line(ANA, &active)).center();
    drag(cx, from, from + point(px(30.), px(3.)));
    assert!(host.edits().is_empty(), "{:?}", host.edits());
}

#[gpui::test]
fn a_row_dragged_to_another_group_changes_its_value_and_place_in_one_batch(
    cx: &mut TestAppContext,
) {
    let host = TestHost::new();
    let (table, cx) = open_list(cx, &host, load("people"), BY_STAGE, wide());
    let (lead, warm) = (opt(LEAD), opt(WARM));
    assert_eq!(group(&table, cx, &warm), ["Imani Castell", "Sami Halloran"]);
    // Jonah (a6) from Lead to between Imani (a1) and Sami (a4) in Warm. While it's carried,
    // nothing's emitted.
    let from = rect(cx, line(JONAH, &lead)).center();
    let to = slot(cx, &table, &warm, 1);
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
    assert!(host.edits().is_empty());
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
        group(&table, cx, &warm),
        ["Imani Castell", "Jonah Whitfield-Arce", "Sami Halloran"]
    );
    assert_eq!(group(&table, cx, &lead), ["Theo Brandt"]);
    assert_eq!(focused(&table, cx).as_deref(), Some(JONAH));
    // Onto the empty group's New line: its value goes.
    host.clear();
    let from = rect(cx, line(THEO, &lead)).center();
    let to = rect(cx, "new:empty").center();
    drag(cx, from, to);
    assert_eq!(
        batches(&host),
        vec![vec![Edit::RowValueChanged {
            row_id: THEO.into(),
            property_id: STAGE.into(),
            value: None,
        }]],
        "an empty group has no neighbours to place it between"
    );
    assert_eq!(group(&table, cx, "empty"), ["Theo Brandt"]);
    // Onto a header: the top of its group.
    host.clear();
    let from = rect(cx, line(SAMI, &warm)).center();
    let to = rect(cx, format!("group:{}", opt(ACTIVE))).center();
    drag(cx, from, to);
    assert_eq!(
        group(&table, cx, &opt(ACTIVE))[0],
        "Sami Halloran",
        "{:?}",
        host.edits()
    );
    // The same, through the API a host's own control (or assistive technology) can use.
    host.clear();
    cx.update(|window, cx| table.update(cx, |t, cx| t.move_card(NOOR, 4, 1, 0, window, cx)));
    draw(cx);
    assert!(host.edits().iter().any(|e| matches!(e,
        Edit::RowValueChanged { row_id, value, .. } if row_id == NOOR && *value == Some(Value::text(LEAD)))));
    assert_eq!(group(&table, cx, &lead), ["Noor Ellison"]);
}

#[gpui::test]
fn a_sorted_list_places_rows_by_its_sort(cx: &mut TestAppContext) {
    let host = TestHost::new();
    // Recent is sorted by when: dragging within it does nothing.
    let (table, cx) = open_list(cx, &host, load("interactions"), RECENT, wide());
    let ids: Vec<String> = table.update(cx, |t, _| {
        let g = t.list_groups()[0].clone();
        g.rows.iter().map(|&r| t.rows()[r].id.clone()).collect()
    });
    let from = rect(cx, line(&ids[0], "all")).center();
    let to = slot(cx, &table, "all", 4);
    drag(cx, from, to);
    assert!(host.edits().is_empty(), "{:?}", host.edits());
    assert!(!table.read_with(cx, |t, _| t.is_dragging_card()));
}

#[gpui::test]
fn a_sorted_grouped_list_changes_only_the_value(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let data = people_with(|v| v.sort = vec![Sort::descending(WARMTH)]);
    let (table, cx) = open_list(cx, &host, data, BY_STAGE, wide());
    let (active, warm) = (opt(ACTIVE), opt(WARM));
    let from = rect(cx, line(ROWAN, &active)).center();
    let to = slot(cx, &table, &active, 3);
    drag(cx, from, to);
    assert!(host.edits().is_empty(), "{:?}", host.edits());
    let from = rect(cx, line(ANA, &active)).center();
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
        group(&table, cx, &warm),
        ["Imani Castell", "Ana Sofía Durán", "Sami Halloran"]
    );
}

#[gpui::test]
fn a_multi_select_row_swaps_one_option_for_another(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let data = people_with(|v| v.group_by = Some(TAGS.into()));
    let (friend, mentor) = (option(&data.0, TAGS, "Friend"), MENTOR.to_owned());
    let (table, cx) = open_list(cx, &host, data, BY_STAGE, size(px(1200.), px(1400.)));
    // Lena is a friend and a mentor: in both groups.
    assert!(group(&table, cx, &opt(&friend)).contains(&"Lena Varga".to_owned()));
    assert!(group(&table, cx, &opt(&mentor)).contains(&"Lena Varga".to_owned()));
    // Dragged from Mentor to Investor, Mentor becomes Investor in its place.
    let from = rect(cx, line(LENA, &opt(&mentor))).center();
    let to = slot(cx, &table, &opt(INVESTOR), 0);
    drag(cx, from, to);
    let value = host.edits().into_iter().find_map(|e| match e {
        Edit::RowValueChanged { row_id, value, .. } if row_id == LENA => Some(value),
        _ => None,
    });
    assert_eq!(
        value,
        Some(Some(Value::ids([friend.clone(), INVESTOR.to_owned()])))
    );
    assert!(!group(&table, cx, &opt(&mentor)).contains(&"Lena Varga".to_owned()));
    // The focus is on the line it was dropped in, not its first.
    let focus = table.update(cx, |t, _| {
        let c = t.focused_cell().unwrap();
        match t.items()[c.item] {
            Item::Row { group, .. } => t.list_groups()[group.unwrap()].key_string(),
            other => panic!("{other:?}"),
        }
    });
    assert_eq!(focus, opt(INVESTOR));
}

#[gpui::test]
fn new_rows_get_their_groups_and_the_filters_values(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_list(cx, &host, load("people"), BY_STAGE, wide());
    click(cx, format!("new:{}", opt(WARM)));
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
    assert_eq!((database_id.as_str(), title.as_str()), (PEOPLE, ""));
    assert_eq!(values.get(STAGE), Some(&Value::text(WARM)));
    assert!(order.as_str() > "a8V" && order::is_valid(&order));
    // Handed back, the new row is focused, in Warm.
    assert_eq!(focused(&table, cx), Some(row_id));
    assert_eq!(group(&table, cx, &opt(WARM)).len(), 3);
    click(cx, "new:empty");
    let Edit::RowCreated { values, .. } = host.last_edit() else {
        panic!()
    };
    assert_eq!(values.get(STAGE), None);
    // The Follow up list's filter wants the box checked: a new row has it.
    table.update(cx, |t, cx| t.set_view(FOLLOW_UP_LIST, cx));
    draw(cx);
    click(cx, "new:all");
    let Edit::RowCreated { values, .. } = host.last_edit() else {
        panic!()
    };
    assert_eq!(values.get(FOLLOW_UP), Some(&Value::Bool(true)));
}

/// People with forty more people at the end of Active, so the list scrolls.
fn crowded() -> (Doc, Vec<Doc>) {
    let (db, mut rows) = load("people");
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
fn a_long_press_picks_a_row_up_and_a_pan_scrolls(cx: &mut TestAppContext) {
    let host = TestHost::touch();
    let (table, cx) = open_list(cx, &host, crowded(), BY_STAGE, size(px(700.), px(700.)));
    let warm = opt(WARM);
    // A plain pan scrolls the list, and nothing's picked up.
    let imani = rect(cx, line(IMANI, &warm));
    pan(cx, imani.center(), point(px(0.), px(-150.)));
    assert!(host.edits().is_empty(), "{:?}", host.edits());
    assert!(!table.read_with(cx, |t, _| t.is_dragging_card()));
    let moved = rect(cx, line(IMANI, &warm));
    assert!(
        moved.top() < imani.top() - px(100.),
        "{moved:?} from {imani:?}"
    );
    // Stop the pan's fling (GPUI times momentum by the real clock, which a test's doesn't
    // move, and a touch that stops a fling only stops it).
    touch(cx, TouchPhase::Started, moved.center());
    touch(cx, TouchPhase::Cancelled, moved.center());
    draw(cx);
    // A long press picks Sami up, and the finger carries it to the top of Active.
    let from = rect(cx, line(SAMI, &warm)).center();
    let to = slot(cx, &table, &opt(ACTIVE), 0);
    host.clear();
    long_press(cx, from, to);
    let b = batches(&host);
    assert_eq!(b.len(), 1, "{b:?}");
    let [
        Edit::RowsMoved { row_ids, orders },
        Edit::RowValueChanged { row_id, value, .. },
    ] = b[0].as_slice()
    else {
        panic!("{b:?}")
    };
    assert_eq!((row_ids[0].as_str(), row_id.as_str()), (SAMI, SAMI));
    assert!(orders[0].as_str() < "a0", "before Rowan: {orders:?}");
    assert_eq!(value, &Some(Value::text(ACTIVE)));
    assert_eq!(group(&table, cx, &opt(ACTIVE))[0], "Sami Halloran");
    assert!(!table.read_with(cx, |t, _| t.is_dragging_card()));
    // A tap opens a row.
    let at = rect(cx, line(IMANI, &warm)).center();
    tap(cx, at);
    assert_eq!(last_open(&host).as_deref(), Some(IMANI));
}

#[gpui::test]
fn touch_targets_are_44_points(cx: &mut TestAppContext) {
    let host = TestHost::touch();
    let (_, cx) = open_list(cx, &host, load("people"), BY_STAGE, wide());
    for what in [
        line(IMANI, &opt(WARM)),
        format!("group:{}", opt(WARM)),
        format!("new:{}", opt(WARM)),
    ] {
        let b = rect(cx, what.clone());
        assert!(
            b.size.height >= px(44.) && b.size.width >= px(44.),
            "{what} is {:?}",
            b.size
        );
    }
}

#[gpui::test]
fn with_a_pointer_lines_are_the_themes_height(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (_, cx) = open_list(cx, &host, load("people"), BY_STAGE, wide());
    // 34 points at a rem of 15.
    let b = rect(cx, line(IMANI, &opt(WARM)));
    assert_eq!(b.size.height, px(34.));
}

#[gpui::test]
fn a_list_draws_nothing_while_nothing_changes(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_list(cx, &host, load("people"), BY_STAGE, wide());
    table.update(cx, |t, cx| t.focus_cell(IMANI, NAME, cx));
    for hover in [false, true] {
        if hover {
            let at = rect(cx, line(IMANI, &opt(WARM))).center();
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
fn line_bounds_are_where_the_line_is_drawn(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_list(cx, &host, crowded(), BY_STAGE, size(px(700.), px(500.)));
    let b = table.read_with(cx, |t, _| t.line_bounds(IMANI)).unwrap();
    assert_eq!(b, rect(cx, line(IMANI, &opt(WARM))));
    // Off screen, there's none.
    assert!(
        table
            .read_with(cx, |t, _| t.line_bounds("extra-39"))
            .is_none()
    );
}

#[gpui::test]
fn line_bounds_never_answer_from_an_older_draw(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (db, rows) = crowded();
    let ids: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();
    let (table, cx) = open_list(
        cx,
        &host,
        (db, rows.clone()),
        BY_STAGE,
        size(px(700.), px(500.)),
    );
    let all = |table: &Entity<TableView>, cx: &mut VisualTestContext| {
        table.read_with(cx, |t, _| {
            ids.iter().map(|id| t.line_bounds(id)).collect::<Vec<_>>()
        })
    };
    // Asked right after the rows are set, before the list can draw them: none known.
    let set = |table: &Entity<TableView>, cx: &mut VisualTestContext, rows: Vec<Doc>| {
        table.update(cx, |t, cx| {
            t.set_rows(rows, cx);
            ids.iter().map(|id| t.line_bounds(id)).collect::<Vec<_>>()
        })
    };
    // Reordered.
    let reversed: Vec<Doc> = rows.iter().rev().cloned().collect();
    assert!(set(&table, cx, reversed).iter().all(Option::is_none));
    draw(cx);
    assert_eq!(
        table.read_with(cx, |t, _| t.line_bounds(IMANI)),
        Some(rect(cx, line(IMANI, &opt(WARM))))
    );
    // Shrunk to two rows: no panic, then their lines and no others.
    let kept: Vec<Doc> = rows
        .iter()
        .filter(|r| r.id == IMANI || r.id == ROWAN)
        .cloned()
        .collect();
    assert!(set(&table, cx, kept).iter().all(Option::is_none));
    draw(cx);
    let after = all(&table, cx);
    for (id, b) in ids.iter().zip(after) {
        assert_eq!(b.is_some(), id == IMANI || id == ROWAN, "{id}");
    }
    assert_eq!(
        table.read_with(cx, |t, _| t.line_bounds(IMANI)),
        Some(rect(cx, line(IMANI, &opt(WARM))))
    );
}

#[gpui::test]
fn the_focus_stays_on_its_row_when_the_rows_are_reordered(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (db, rows) = crowded();
    let (table, cx) = open_list(
        cx,
        &host,
        (db, rows.clone()),
        BY_STAGE,
        size(px(700.), px(500.)),
    );
    click(cx, line(IMANI, &opt(WARM)));
    assert_eq!(focused(&table, cx).as_deref(), Some(IMANI));
    let reversed: Vec<Doc> = rows.iter().rev().cloned().collect();
    table.update(cx, |t, cx| t.set_rows(reversed, cx));
    draw(cx);
    assert_eq!(focused(&table, cx).as_deref(), Some(IMANI));
    // Twice before a draw: still the row focused before either.
    let shuffled: Vec<Doc> = rows
        .iter()
        .skip(3)
        .chain(rows.iter().take(3))
        .cloned()
        .collect();
    table.update(cx, |t, cx| {
        t.set_rows(rows.clone(), cx);
        t.set_rows(shuffled, cx);
    });
    draw(cx);
    assert_eq!(focused(&table, cx).as_deref(), Some(IMANI));
}
