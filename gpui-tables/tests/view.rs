//! The table view driven as a person drives it: the mouse, the keyboard and a finger, on the
//! CRM fixtures, checking what it hands the host.

#![cfg(all(target_os = "macos", feature = "gpui"))]

mod common;

use std::time::Duration;

use common::ids::*;
use common::*;
use gpui::*;
use gpui_tables::view::{Item, TableTheme, TableView, TextTarget};
use gpui_tables::*;
use gpui_tables::{Edit, Filter, Sort, Value};
// The built-in #[test] (which GPUI's test attribute expands to), not GPUI's, which gpui::*
// brings in.
use std::prelude::v1::test;

fn titles(table: &Entity<TableView>, cx: &mut VisualTestContext) -> Vec<String> {
    table.update(cx, |t, _| {
        let rows = t.shown_rows().to_vec();
        rows.iter().map(|&i| t.rows()[i].title.clone()).collect()
    })
}

fn value_edit(e: &Edit) -> (&str, &str, Option<&Value>) {
    match e {
        Edit::RowValueChanged {
            row_id,
            property_id,
            value,
        } => (row_id, property_id, value.as_ref()),
        other => panic!("not a rowValueChanged: {other:?}"),
    }
}

/// Opens a column's menu by pressing its header.
fn menu(cx: &mut VisualTestContext, name: &str) {
    let at = rect(cx, format!("header:{name}")).center();
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    draw(cx);
    assert!(drawn(cx, "menu"), "{name}'s menu opened");
}

#[gpui::test]
fn the_view_shows_its_columns_and_rows_in_its_order(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_people(cx, &host);
    assert_eq!(
        titles(&table, cx),
        [
            "Ana Sofía Durán",
            "Imani Castell",
            "Jonah Whitfield-Arce",
            "Lena Varga",
            "Noor Ellison",
            "Rowan Pike",
            "Sami Halloran",
            "Theo Brandt"
        ],
        "sorted by name, without the deleted row"
    );
    let cols = table.update(cx, |t, _| t.columns());
    assert_eq!(cols[0], (NAME.to_owned(), 240.));
    assert_eq!(cols[1].0, COMPANY);
    assert!(!cols.iter().any(|(p, _)| p == PHONE), "Phone is hidden");
    // The properties the view has no column for come after, at the default width.
    assert!(cols.iter().any(|(p, w)| p == FOLLOW_UP && *w == 180.));
    assert!(drawn(cx, cell(ANA, NAME)));
    assert!(drawn(cx, "new:table"));
}

#[gpui::test]
fn the_header_menu_sorts_and_shift_adds_a_sort(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_people(cx, &host);
    menu(cx, "Warmth");
    click(cx, "entry:Sort descending");
    assert!(!drawn(cx, "menu"), "the menu closes");
    let v = host.last_view();
    assert_eq!(v.sort, vec![Sort::descending(WARMTH)]);
    let t = titles(&table, cx);
    assert_eq!(
        &t[..2],
        ["Rowan Pike", "Lena Varga"],
        "5s first, in the manual order"
    );
    assert_eq!(
        t.last().unwrap(),
        "Jonah Whitfield-Arce",
        "no warmth goes last"
    );

    // Shift adds a sort to the ones there.
    menu(cx, "Name");
    click_with(cx, "entry:Sort ascending", Modifiers::shift());
    assert_eq!(
        host.last_view().sort,
        vec![Sort::descending(WARMTH), Sort::ascending(NAME)]
    );
    assert_eq!(&titles(&table, cx)[..2], ["Lena Varga", "Rowan Pike"]);

    // And it can be taken away again.
    menu(cx, "Warmth");
    click(cx, "entry:Remove sort");
    assert_eq!(host.last_view().sort, vec![Sort::ascending(NAME)]);
    // Every one of these is a viewChanged for this database.
    assert!(
        host.edits()
            .iter()
            .all(|e| matches!(e, Edit::ViewChanged { database_id, .. } if database_id == PEOPLE))
    );
}

#[gpui::test]
fn the_header_menu_filters_by_its_column(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_people(cx, &host);
    menu(cx, "Stage");
    click(cx, "entry:Warm");
    let v = host.last_view();
    assert_eq!(
        v.filter,
        Some(Filter::condition(STAGE, FilterOp::Is, WARM.into()))
    );
    assert_eq!(titles(&table, cx), ["Imani Castell", "Sami Halloran"]);
    // Another column's filter is and-ed with it.
    menu(cx, "Follow up");
    click(cx, "entry:Checked");
    assert!(matches!(&host.last_view().filter, Some(Filter::And { and }) if and.len() == 2));
    assert_eq!(titles(&table, cx), ["Imani Castell", "Sami Halloran"]);
    // Choosing another option replaces the first.
    menu(cx, "Stage");
    click(cx, "entry:Lead");
    assert_eq!(titles(&table, cx), ["Theo Brandt"]);
    menu(cx, "Stage");
    click(cx, "entry:Clear filter");
    assert_eq!(
        host.last_view().filter,
        Some(Filter::condition(
            FOLLOW_UP,
            FilterOp::IsChecked,
            serde_json::Value::Null
        ))
    );
    // A text filter asks the host for its value.
    menu(cx, "Role");
    click(cx, "entry:Contains…");
    let edit = host.texts.borrow_mut().pop().expect("a text field");
    assert_eq!(
        edit.target,
        TextTarget::Filter {
            property_id: ROLE.into(),
            op: FilterOp::Contains
        }
    );
    cx.update(|window, cx| edit.reply.commit("partner", window, cx));
    draw(cx);
    assert_eq!(titles(&table, cx), ["Sami Halloran"]);
    // A number's is read as a number.
    menu(cx, "Warmth");
    click(cx, "entry:Greater than…");
    let edit = host.texts.borrow_mut().pop().unwrap();
    cx.update(|window, cx| edit.reply.commit("2", window, cx));
    draw(cx);
    let v = host.last_view();
    let Some(Filter::And { and }) = &v.filter else {
        panic!("{:?}", v.filter)
    };
    assert!(and.contains(&Filter::condition(WARMTH, FilterOp::Greater, 2.into())));
}

#[gpui::test]
fn groups_show_counts_and_fold(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (db, rows) = load("people");
    let (table, cx) = open_with(
        cx,
        &host,
        db,
        rows,
        Some(PIPELINE),
        size(px(1200.), px(700.)),
    );
    let items = table.update(cx, |t, _| t.items().to_vec());
    // Five groups (no stage, then each option), Dormant folded in the fixture.
    let groups = items.iter().filter(|i| matches!(i, Item::Group(_))).count();
    assert_eq!(groups, 5);
    let news = items.iter().filter(|i| matches!(i, Item::New(_))).count();
    assert_eq!(news, 4, "a New row under every open group");
    assert!(drawn(cx, format!("group:option:{DORMANT}")));
    // Folding Active hides its three rows and is a viewChanged.
    let before = items.len();
    click(cx, format!("group:option:{ACTIVE}"));
    assert_eq!(
        host.last_view().collapsed_groups,
        vec![format!("option:{DORMANT}"), format!("option:{ACTIVE}")]
    );
    let after = table.update(cx, |t, _| t.items().len());
    assert_eq!(after, before - 4, "three rows and the New row");
    // Unfolding Dormant shows Noor.
    click(cx, format!("group:option:{DORMANT}"));
    assert_eq!(
        host.last_view().collapsed_groups,
        vec![format!("option:{ACTIVE}")]
    );
    assert!(drawn(
        cx,
        cell("7a86f7a2-43c7-4b9a-bd87-a86557b6fb7e", NAME)
    ));
    // Enter on a focused group header folds it too.
    table.update(cx, |t, cx| t.focus_cell(IMANI, NAME, cx));
    keys(cx, "up enter");
    assert!(
        host.last_view()
            .collapsed_groups
            .contains(&format!("option:{WARM}"))
    );
}

#[gpui::test]
fn columns_resize_and_move_by_dragging(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_people(cx, &host);
    let handle = rect(cx, "resize:Role");
    drag(
        cx,
        handle.center(),
        handle.center() + point(px(50.), px(0.)),
    );
    let v = host.last_view();
    let role = v.columns.iter().find(|c| c.property_id == ROLE).unwrap();
    assert_eq!(
        role.width,
        Some(250.),
        "the rem is 15 points, so a pixel is a point"
    );
    assert_eq!(table.update(cx, |t, _| t.columns()[2].1), 250.);
    // It can't go narrower than the least.
    let handle = rect(cx, "resize:Role");
    drag(
        cx,
        handle.center(),
        handle.center() - point(px(400.), px(0.)),
    );
    let role = host
        .last_view()
        .columns
        .into_iter()
        .find(|c| c.property_id == ROLE)
        .unwrap();
    assert_eq!(role.width, Some(56.));

    // Stage dragged onto Company's place moves it before Company.
    host.clear();
    let stage = rect(cx, "header:Stage").center();
    let company = rect(cx, "header:Company");
    drag(
        cx,
        stage,
        point(company.left() + px(10.), company.center().y),
    );
    let v = host.last_view();
    let order: Vec<&str> = v
        .columns_in_order()
        .iter()
        .map(|c| c.property_id.as_str())
        .collect();
    let (s, c) = (
        order.iter().position(|p| *p == STAGE).unwrap(),
        order.iter().position(|p| *p == COMPANY).unwrap(),
    );
    assert!(s < c, "{order:?}");
    let cols: Vec<String> = table.update(cx, |t, _| t.columns().into_iter().map(|c| c.0).collect());
    assert_eq!(&cols[..3], [NAME, STAGE, COMPANY]);
    // Every column has a valid order key, rising.
    assert!(
        v.columns_in_order()
            .windows(2)
            .all(|w| w[0].order < w[1].order)
    );
    assert!(v.columns.iter().all(|c| order::is_valid(&c.order)));
    // A press without a drag opens the menu instead and moves nothing.
    host.clear();
    menu(cx, "Company");
    assert!(host.edits().is_empty());
}

#[gpui::test]
fn columns_hide_and_show_and_move_from_the_menu(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_people(cx, &host);
    menu(cx, "Role");
    click(cx, "entry:Hide");
    assert!(
        host.last_view()
            .columns
            .iter()
            .any(|c| c.property_id == ROLE && c.hidden)
    );
    assert!(!drawn(cx, "header:Role"));
    click(cx, "columns-button");
    click(cx, "entry:Role");
    assert!(
        host.last_view()
            .columns
            .iter()
            .any(|c| c.property_id == ROLE && !c.hidden)
    );
    click(cx, "entry:Phone");
    assert!(table.update(cx, |t, _| t.columns().iter().any(|c| c.0 == PHONE)));
    keys(cx, "escape");
    menu(cx, "Stage");
    click(cx, "entry:Move left");
    let cols: Vec<String> = table.update(cx, |t, _| t.columns().into_iter().map(|c| c.0).collect());
    assert_eq!(&cols[..4], [NAME, COMPANY, STAGE, ROLE]);
    // The title can't be hidden or moved.
    menu(cx, "Name");
    assert!(!drawn(cx, "entry:Hide"));
    assert!(!drawn(cx, "entry:Move left"));
}

#[gpui::test]
fn every_editable_type_edits(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_people(cx, &host);

    // A checkbox toggles at once.
    table.update(cx, |t, cx| t.focus_cell(ANA, FOLLOW_UP, cx));
    draw(cx);
    click(cx, cell(ANA, FOLLOW_UP));
    assert_eq!(
        value_edit(&host.last_edit()),
        (ANA, FOLLOW_UP, Some(&Value::Bool(true)))
    );

    // A select: press to focus, press again for its options, pick one.
    click(cx, cell(IMANI, STAGE));
    assert!(!drawn(cx, "menu"), "the first press only focuses");
    click(cx, cell(IMANI, STAGE));
    click(cx, "entry:Active");
    assert_eq!(
        value_edit(&host.last_edit()),
        (IMANI, STAGE, Some(&Value::text(ACTIVE)))
    );

    // A multi-select toggles options and stays open.
    click(cx, cell(IMANI, TAGS));
    click(cx, cell(IMANI, TAGS));
    click(cx, "entry:Mentor");
    assert!(drawn(cx, "menu"));
    let last = host.last_edit();
    let (_, _, v) = value_edit(&last);
    assert_eq!(v.and_then(Value::as_ids).unwrap().last().unwrap(), MENTOR);
    click(cx, "entry:Investor");
    let last = host.last_edit();
    let (_, _, v) = value_edit(&last);
    assert!(
        !v.and_then(Value::as_ids)
            .unwrap()
            .contains(&INVESTOR.to_owned())
    );
    keys(cx, "escape");
    assert!(!drawn(cx, "menu"));

    // A relation toggles a related row, with both sides in the batch.
    click(cx, cell(ANA, COMPANY));
    click(cx, cell(ANA, COMPANY));
    host.clear();
    click(cx, "entry:Bluefin Robotics");
    let edits = host.edits();
    assert_eq!(edits.len(), 2, "{edits:?}");
    assert!(
        matches!(&edits[1], Edit::RelationChanged { added, .. } if added == &vec![BLUEFIN.to_owned()])
    );
    keys(cx, "escape");

    // A date picks a day from the grid, keeping no time.
    click(cx, cell(ANA, LAST_CONTACTED));
    click(cx, cell(ANA, LAST_CONTACTED));
    click(cx, "day:2026-11-20");
    let last = host.last_edit();
    let (_, _, v) = value_edit(&last);
    let day = engine::time::days_from_civil(2026, 11, 20);
    assert_eq!(v, Some(&Value::date_on_day(day)));
    // And the keyboard moves the grid: a week on, a day back, Enter.
    keys(cx, "enter");
    keys(cx, "down left enter");
    let last = host.last_edit();
    let (_, _, v) = value_edit(&last);
    assert_eq!(v, Some(&Value::date_on_day(day + 6)));

    // Typed values go through the host's field.
    click(cx, cell(ANA, ROLE));
    keys(cx, "enter");
    let edit = host.texts.borrow_mut().pop().expect("a field for Role");
    assert_eq!(edit.text, "Designer");
    assert_eq!(edit.kind, PropertyType::Text);
    assert!(edit.bounds.size.width > px(100.));
    cx.update(|window, cx| edit.reply.commit("  Lead designer ", window, cx));
    draw(cx);
    assert_eq!(
        value_edit(&host.last_edit()),
        (ANA, ROLE, Some(&Value::text("Lead designer")))
    );
    // Answering twice does nothing.
    let n = host.edits().len();
    cx.update(|window, cx| edit.reply.commit("again", window, cx));
    draw(cx);
    assert_eq!(host.edits().len(), n);

    // A number reads grouping; what isn't a number changes nothing.
    table.update(cx, |t, cx| t.focus_cell(ANA, WARMTH, cx));
    keys(cx, "enter");
    let edit = host.texts.borrow_mut().pop().unwrap();
    assert_eq!(edit.text, "4");
    cx.update(|window, cx| edit.reply.commit("1,234", window, cx));
    draw(cx);
    assert_eq!(
        value_edit(&host.last_edit()),
        (ANA, WARMTH, Some(&Value::number(1234.)))
    );
    keys(cx, "enter");
    let edit = host.texts.borrow_mut().pop().unwrap();
    let n = host.edits().len();
    cx.update(|window, cx| edit.reply.commit("lots", window, cx));
    draw(cx);
    assert_eq!(host.edits().len(), n);

    // Email and URL are text, cut to their limits.
    table.update(cx, |t, cx| t.focus_cell(ANA, EMAIL, cx));
    keys(cx, "enter");
    let edit = host.texts.borrow_mut().pop().unwrap();
    assert_eq!(edit.max_chars, Some(320));
    cx.update(|window, cx| edit.reply.commit(&"a".repeat(400), window, cx));
    draw(cx);
    let last = host.last_edit();
    let (_, _, v) = value_edit(&last);
    assert_eq!(v.and_then(Value::as_str).unwrap().chars().count(), 320);
    table.update(cx, |t, cx| t.focus_cell(ANA, PROFILE, cx));
    keys(cx, "enter");
    let edit = host.texts.borrow_mut().pop().unwrap();
    assert_eq!(edit.kind, PropertyType::Url);
    cx.update(|window, cx| edit.reply.commit("https://example.com/ana", window, cx));
    draw(cx);
    assert_eq!(
        value_edit(&host.last_edit()).2,
        Some(&Value::text("https://example.com/ana"))
    );

    // A person toggles people the property allows.
    table.update(cx, |t, cx| t.focus_cell(ANA, OWNER, cx));
    keys(cx, "enter");
    click(cx, "entry:Career Planning");
    let last = host.last_edit();
    let (_, _, v) = value_edit(&last);
    assert_eq!(
        v.and_then(Value::as_people).unwrap(),
        [Person::Owner, Person::agent("career-planning")]
    );
    keys(cx, "escape");

    // The title edits with F2 (Enter opens the row), as a rowTitleChanged.
    table.update(cx, |t, cx| t.focus_cell(ANA, NAME, cx));
    keys(cx, "f2");
    let edit = host.texts.borrow_mut().pop().unwrap();
    assert_eq!(edit.kind, PropertyType::Title);
    cx.update(|window, cx| edit.reply.commit("Ana Durán", window, cx));
    draw(cx);
    assert_eq!(
        host.last_edit(),
        Edit::RowTitleChanged {
            row_id: ANA.into(),
            title: "Ana Durán".into()
        }
    );

    // Delete clears a cell, as a null value; a created time can't be cleared.
    table.update(cx, |t, cx| t.focus_cell(ANA, ROLE, cx));
    keys(cx, "backspace");
    assert_eq!(value_edit(&host.last_edit()), (ANA, ROLE, None));
    let n = host.edits().len();
    table.update(cx, |t, cx| {
        t.focus_cell(ANA, "ec66a787-95e7-41d1-b731-af10506bf2ef", cx)
    });
    keys(cx, "delete");
    assert_eq!(host.edits().len(), n);
}

#[gpui::test]
fn the_keyboard_moves_the_focus_and_opens_rows(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (db, rows) = load("people");
    let (table, cx) = open_with(cx, &host, db, rows, None, size(px(1200.), px(700.)));
    let at = |table: &Entity<TableView>, cx: &mut VisualTestContext| {
        table.update(cx, |t, _| t.focused_row_cell())
    };
    keys(cx, "down");
    assert_eq!(
        at(&table, cx),
        Some((ANA.into(), NAME.into())),
        "the first key focuses the first cell"
    );
    keys(cx, "down right right");
    assert_eq!(at(&table, cx), Some((IMANI.into(), ROLE.into())));
    keys(cx, "left");
    assert_eq!(at(&table, cx), Some((IMANI.into(), COMPANY.into())));
    // Tab runs along the row and on to the next.
    let cols = table.update(cx, |t, _| t.columns().len());
    for _ in 0..cols - 1 {
        keys(cx, "tab");
    }
    assert_eq!(
        at(&table, cx).unwrap().0,
        "842e7fc2-2954-4a6e-b12a-a1f6d42fddbb",
        "Jonah's row"
    );
    assert_eq!(at(&table, cx).unwrap().1, NAME);
    keys(cx, "shift-tab");
    assert_eq!(at(&table, cx).unwrap().0, IMANI);
    // Moving past the edge scrolls the columns sideways, and the title stays.
    assert!(table.update(cx, |t, _| t.scroll_x()) > px(0.));
    // Enter on a title opens its row.
    keys(cx, "up");
    for _ in 0..cols {
        keys(cx, "left");
    }
    keys(cx, "enter");
    assert_eq!(
        host.events.borrow().last(),
        Some(&TableEvent::OpenRow { row_id: ANA.into() })
    );
    assert_eq!(table.update(cx, |t, _| t.scroll_x()), px(0.));
    // ⌥↓ opens the column's menu, and the arrows and Enter work in it.
    keys(cx, "alt-down");
    assert!(drawn(cx, "menu"));
    keys(cx, "down enter");
    assert_eq!(host.last_view().sort, vec![Sort::descending(NAME)]);
    // Escape lets go of the cell.
    keys(cx, "escape");
    assert_eq!(table.read_with(cx, |t, _| t.focused_cell()), None);
}

#[gpui::test]
fn the_open_button_opens_its_row(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (_, cx) = open_people(cx, &host);
    click(cx, format!("open:{THEO}"));
    assert_eq!(
        host.events.borrow().as_slice(),
        [TableEvent::OpenRow {
            row_id: THEO.into()
        }]
    );
}

#[gpui::test]
fn a_new_row_goes_after_every_row(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_people(cx, &host);
    let orders: Vec<String> = table.read_with(cx, |t, _| {
        t.rows().iter().map(|r| r.order.clone()).collect()
    });
    click(cx, "new:table");
    let Edit::RowCreated {
        row_id,
        database_id,
        order: key,
        title,
        values,
    } = host.last_edit()
    else {
        panic!()
    };
    assert_eq!(database_id, PEOPLE);
    assert!(title.is_empty() && values.is_empty());
    assert!(orders.iter().all(|o| *o < key), "{key} after every row");
    assert!(order::is_valid(&key));
    // When the host hands it back, the new row's title is focused.
    assert_eq!(
        table.update(cx, |t, _| t.focused_row_cell()),
        Some((row_id, NAME.into()))
    );
}

#[gpui::test]
fn new_rows_in_a_group_get_its_value_and_the_filters(cx: &mut TestAppContext) {
    // In a grouped view the row gets its group's option, and the filter's checked box.
    let host = TestHost::new();
    let (mut db, rows) = load("people");
    let f = db.database.as_mut().unwrap();
    let pipe = f.views.iter_mut().find(|v| v.id == PIPELINE).unwrap();
    pipe.filter = Some(Filter::condition(
        FOLLOW_UP,
        FilterOp::IsChecked,
        serde_json::Value::Null,
    ));
    let (_, cx) = open_with(
        cx,
        &host,
        db,
        rows,
        Some(PIPELINE),
        size(px(1200.), px(800.)),
    );
    click(cx, format!("new:option:{WARM}"));
    let Edit::RowCreated { values, .. } = host.last_edit() else {
        panic!()
    };
    assert_eq!(values.get(STAGE), Some(&Value::text(WARM)));
    assert_eq!(values.get(FOLLOW_UP), Some(&Value::Bool(true)));
    click(cx, "new:empty");
    let Edit::RowCreated { values, .. } = host.last_edit() else {
        panic!()
    };
    assert_eq!(values.get(STAGE), None);
}

#[gpui::test]
fn created_by_and_edited_by_show_who_and_never_edit(cx: &mut TestAppContext) {
    const BY: &str = "c1a7e2b4-5d38-4f0e-9b61-0e4c7d2a9f13";
    const EDITOR: &str = "e9d3b6f1-2a47-4c85-8e10-7f5b4a3c2d68";
    const JONAH: &str = "842e7fc2-2954-4a6e-b12a-a1f6d42fddbb";
    let host = TestHost::new();
    let (mut db, mut rows) = load("people");
    let f = db.database.as_mut().unwrap();
    f.properties
        .push(Property::new(BY, "Added by", PropertyType::CreatedBy, "aF"));
    f.properties.push(Property::new(
        EDITOR,
        "Edited by",
        PropertyType::EditedBy,
        "aG",
    ));
    let all = f.views.iter_mut().find(|v| v.id == ALL_PEOPLE).unwrap();
    for (id, order) in [(BY, "a05"), (EDITOR, "a08")] {
        all.columns.push(Column {
            property_id: id.into(),
            width: None,
            hidden: false,
            order: order.into(),
        });
    }
    // Theo's doc is older than updatedBy: his editor is his author.
    rows.iter_mut().find(|r| r.id == THEO).unwrap().updated_by = None;
    let (table, cx) = open_with(cx, &host, db, rows, None, size(px(1600.), px(700.)));

    let cols = table.update(cx, |t, _| t.columns());
    assert_eq!((cols[1].0.as_str(), cols[2].0.as_str()), (BY, EDITOR));
    assert!(drawn(cx, "header:Added by") && drawn(cx, "header:Edited by"));
    assert!(drawn(cx, cell(JONAH, BY)) && drawn(cx, cell(JONAH, EDITOR)));
    let words = |table: &Entity<TableView>, cx: &mut VisualTestContext, row: &str, p: &str| {
        table.update(cx, |t, _| t.cell_words(row, p)).unwrap()
    };
    // Drawn by the host's names, as a person cell is.
    assert_eq!(words(&table, cx, JONAH, BY), "inbox-triage");
    assert_eq!(words(&table, cx, JONAH, EDITOR), "Avery Lin");
    assert_eq!(words(&table, cx, THEO, BY), "Career Planning");
    assert_eq!(words(&table, cx, THEO, EDITOR), "Career Planning");

    // Pressing, Enter and Delete do nothing to them: no picker, no field, no edit.
    host.clear();
    for p in [BY, EDITOR] {
        click(cx, cell(JONAH, p));
        click(cx, cell(JONAH, p));
        assert!(!drawn(cx, "menu"), "no picker");
        keys(cx, "enter");
        keys(cx, "backspace");
        keys(cx, "delete");
        assert!(!drawn(cx, "menu"));
    }
    assert!(host.texts.borrow().is_empty(), "no field");
    assert_eq!(host.edits(), []);

    // The header's menu filters by who.
    menu(cx, "Added by");
    click(cx, "entry:Avery Lin");
    assert_eq!(
        host.last_view().filter,
        Some(Filter::condition(BY, FilterOp::Is, "owner".into()))
    );
    assert_eq!(
        titles(&table, cx),
        [
            "Ana Sofía Durán",
            "Imani Castell",
            "Lena Varga",
            "Noor Ellison",
            "Rowan Pike"
        ]
    );
    // A new row made under that filter gets no value for it: the host fills in the author.
    click(cx, "new:table");
    let Edit::RowCreated { values, .. } = host.last_edit() else {
        panic!()
    };
    assert!(!values.contains_key(BY) && !values.contains_key(EDITOR));

    // Grouped by who changed each row last, with a New row in each group that sets nothing.
    let (mut db, rows) = table.update(cx, |t, _| (t.database().clone(), t.rows().to_vec()));
    let all = db
        .database
        .as_mut()
        .unwrap()
        .views
        .iter_mut()
        .find(|v| v.id == ALL_PEOPLE);
    let all = all.unwrap();
    all.filter = None;
    all.group_by = Some(EDITOR.into());
    table.update(cx, |t, cx| t.set_docs(db, rows, cx));
    draw(cx);
    assert!(drawn(cx, "group:owner"));
    assert!(drawn(cx, "group:agent:career-planning"));
    assert!(drawn(cx, "group:agent:inbox-triage"));
    click(cx, "new:agent:inbox-triage");
    let Edit::RowCreated { values, .. } = host.last_edit() else {
        panic!()
    };
    assert!(values.is_empty(), "{values:?}");
}

#[gpui::test]
fn touch_targets_are_44_points(cx: &mut TestAppContext) {
    let host = TestHost::touch();
    let (db, rows) = load("people");
    let (_, cx) = open_with(
        cx,
        &host,
        db,
        rows,
        Some(PIPELINE),
        size(px(1200.), px(900.)),
    );
    let check = |cx: &mut VisualTestContext, what: String, w: bool| {
        let b = rect(cx, what.clone());
        assert!(b.size.height >= px(44.), "{what} is {:?}", b.size);
        if w {
            assert!(b.size.width >= px(44.), "{what} is {:?}", b.size);
        }
    };
    check(cx, format!("open:{IMANI}"), true);
    check(cx, "header:Stage".into(), true);
    check(cx, "resize:Stage".into(), true);
    check(cx, format!("group:option:{WARM}"), true);
    check(cx, format!("new:option:{WARM}"), true);
    check(cx, cell(IMANI, NAME), true);
    check(cx, "columns-button".into(), true);
    // The Open button shows without hover on touch, and a tap on it opens the row.
    let at = rect(cx, format!("open:{IMANI}")).center();
    tap(cx, at);
    assert_eq!(
        host.events.borrow().last(),
        Some(&TableEvent::OpenRow {
            row_id: IMANI.into()
        })
    );
    // A tap on a header opens its menu.
    let at = rect(cx, "header:Stage").center();
    tap(cx, at);
    assert!(drawn(cx, "menu"), "a tap opens the column's menu");
    check(cx, "entry:Sort ascending".into(), false);
    keys(cx, "escape");
    // Menus' entries and the day grid's days too.
    let at = rect(cx, cell(IMANI, STAGE)).center();
    tap(cx, at);
    let at = rect(cx, cell(IMANI, STAGE)).center();
    tap(cx, at);
    check(cx, "entry:Lead".into(), false);
    let at = rect(cx, "entry:Lead").center();
    tap(cx, at);
    // The visible line of a column's edge stays thin; only the target is wide.
    let edge = rect(cx, "resize:Stage");
    let header = rect(cx, "header:Stage");
    assert!((edge.center().x - header.right()).abs() < px(1.));
}

#[gpui::test]
fn a_finger_resizes_a_column_from_its_edge(cx: &mut TestAppContext) {
    let host = TestHost::touch();
    let (table, cx) = open_people(cx, &host);
    let edge = rect(cx, "resize:Stage");
    // From inside the 44-point target but off the line itself.
    let from = point(edge.center().x + px(15.), edge.center().y);
    pan(cx, from, point(px(60.), px(0.)));
    let w = table.update(cx, |t, _| t.columns()[3].1);
    assert_eq!(w, 180., "{:?}", host.edits());
}

#[gpui::test]
fn swipes_scroll_both_ways_and_the_title_stays(cx: &mut TestAppContext) {
    let host = TestHost::touch();
    let (mut db, mut rows) = load("people");
    // Enough rows to scroll down.
    let template = rows[0].clone();
    for i in 0..60 {
        let mut r = template.clone();
        r.id = format!("extra-{i:02}");
        r.title = format!("Extra {i:02}");
        r.order = format!("b{i:02}");
        rows.push(r);
    }
    db.database.as_mut().unwrap().views[0].sort.clear();
    let (table, cx) = open_with(cx, &host, db, rows, None, size(px(700.), px(500.)));
    let title = rect(cx, cell(ROWAN, NAME));
    let role = rect(cx, cell(ROWAN, ROLE));
    // Sideways: the columns move, the title doesn't.
    pan(cx, point(px(500.), px(300.)), point(px(-200.), px(0.)));
    let sx = table.update(cx, |t, _| t.scroll_x());
    assert!(sx > px(100.), "scrolled {sx:?}");
    assert_eq!(rect(cx, cell(ROWAN, NAME)).origin, title.origin);
    assert!(rect(cx, cell(ROWAN, ROLE)).left() < role.left() - px(100.));
    // Up and down: the rows move, sideways stays.
    pan(cx, point(px(500.), px(400.)), point(px(0.), px(-250.)));
    assert!(
        !drawn(cx, cell(ROWAN, NAME)) || rect(cx, cell(ROWAN, NAME)).top() < title.top() - px(100.)
    );
    assert_eq!(table.update(cx, |t, _| t.scroll_x()), sx);
    // A trackpad's sideways scroll moves the columns too, within their width.
    cx.simulate_event(ScrollEvent {
        position: point(px(500.), px(300.)),
        delta: ScrollDelta::Pixels(point(px(-100000.), px(0.))),
        ..Default::default()
    });
    draw(cx);
    let max = table.update(cx, |t, _| t.scroll_x());
    assert!(max > sx && max < px(5000.), "{max:?}");
}

#[gpui::test]
fn nothing_draws_while_nothing_changes(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_people(cx, &host);
    // With a cell focused and a menu open too.
    table.update(cx, |t, cx| t.focus_cell(IMANI, STAGE, cx));
    draw(cx);
    for open in [false, true] {
        if open {
            table.update(cx, |t, cx| t.open_column_menu(STAGE, cx));
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
fn the_view_follows_the_hosts_docs(cx: &mut TestAppContext) {
    let host = TestHost::new();
    host.apply.set(false);
    let (table, cx) = open_people(cx, &host);
    // Without the host applying it, an edit changes nothing on screen.
    click(cx, cell(ANA, FOLLOW_UP));
    click(cx, cell(ANA, FOLLOW_UP));
    assert_eq!(host.edits().len(), 2);
    assert_eq!(
        value_edit(&host.edits()[1]).2,
        Some(&Value::Bool(true)),
        "both toggles from what the docs say"
    );
    // The rows are worked out again only when the docs change.
    let (db, mut rows) = load("people");
    rows.retain(|r| r.id != ANA);
    table.update(cx, |t, cx| t.set_docs(db, rows, cx));
    draw(cx);
    assert_eq!(titles(&table, cx).len(), 7);
    let _ = TableTheme::dark();
}
