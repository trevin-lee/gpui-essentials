//! The table view driven as a person drives it: the mouse, the keyboard and a finger, on the
//! CRM fixtures, checking what it hands the host.

// The Mac's dev-dependencies switch GPUI's test support on; elsewhere, the test-support feature.
#![cfg(all(feature = "gpui", any(target_os = "macos", feature = "test-support")))]

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

/// Opens the Columns menu (show and hide properties) from the title's column menu.
fn columns_menu(cx: &mut VisualTestContext) {
    menu(cx, "Name");
    click(cx, "menu-item-Show and hide properties");
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
    click(cx, "menu-item-Sort descending");
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
    click_with(cx, "menu-item-Sort ascending", Modifiers::shift());
    assert_eq!(
        host.last_view().sort,
        vec![Sort::descending(WARMTH), Sort::ascending(NAME)]
    );
    assert_eq!(&titles(&table, cx)[..2], ["Lena Varga", "Rowan Pike"]);

    // And it can be taken away again.
    menu(cx, "Warmth");
    click(cx, "menu-item-Remove sort");
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
    click(cx, "menu-item-Warm");
    let v = host.last_view();
    assert_eq!(
        v.filter,
        Some(Filter::condition(STAGE, FilterOp::Is, WARM.into()))
    );
    assert_eq!(titles(&table, cx), ["Imani Castell", "Sami Halloran"]);
    // Another column's filter is and-ed with it.
    menu(cx, "Follow up");
    click(cx, "menu-item-Checked");
    assert!(matches!(&host.last_view().filter, Some(Filter::And { and }) if and.len() == 2));
    assert_eq!(titles(&table, cx), ["Imani Castell", "Sami Halloran"]);
    // Choosing another option replaces the first.
    menu(cx, "Stage");
    click(cx, "menu-item-Lead");
    assert_eq!(titles(&table, cx), ["Theo Brandt"]);
    menu(cx, "Stage");
    click(cx, "menu-item-Clear filter");
    assert_eq!(
        host.last_view().filter,
        Some(Filter::condition(
            FOLLOW_UP,
            FilterOp::IsChecked,
            serde_json::Value::Null
        ))
    );
    // A text filter's value is typed in the menu, under the header; Enter applies it.
    menu(cx, "Role");
    click(cx, "menu-item-Contains…");
    assert!(
        drawn(cx, "menu") && drawn(cx, "editor"),
        "a field in the menu"
    );
    assert_eq!(
        editing(&table, cx),
        Some((
            TextTarget::Filter {
                property_id: ROLE.into(),
                op: FilterOp::Contains
            },
            String::new()
        ))
    );
    assert!(rect(cx, "editor").top() >= rect(cx, "header:Role").bottom());
    type_text(cx, "partner");
    keys(cx, "enter");
    assert!(!drawn(cx, "menu") && editing(&table, cx).is_none());
    assert_eq!(titles(&table, cx), ["Sami Halloran"]);
    // A number's is read as a number, and Tab applies it too.
    menu(cx, "Warmth");
    click(cx, "menu-item-Greater than…");
    type_text(cx, "2");
    keys(cx, "tab");
    let v = host.last_view();
    let Some(Filter::And { and }) = &v.filter else {
        panic!("{:?}", v.filter)
    };
    assert!(and.contains(&Filter::condition(WARMTH, FilterOp::Greater, 2.into())));
    // Escape closes the field and filters by nothing.
    let n = host.edits().len();
    menu(cx, "Warmth");
    click(cx, "menu-item-Less than…");
    type_text(cx, "9");
    keys(cx, "escape");
    assert!(!drawn(cx, "menu") && editing(&table, cx).is_none());
    assert_eq!(host.edits().len(), n);
    // Pressing elsewhere applies it, as a cell's value; with grouping and a currency sign.
    menu(cx, "Warmth");
    click(cx, "menu-item-Equals…");
    type_text(cx, "$1,0");
    click(cx, "header:Role");
    keys(cx, "escape");
    let v = host.last_view();
    let Some(Filter::And { and }) = &v.filter else {
        panic!("{:?}", v.filter)
    };
    assert!(and.contains(&Filter::condition(WARMTH, FilterOp::Is, 10.into())));
    // What isn't a number filters by nothing.
    let n = host.edits().len();
    menu(cx, "Warmth");
    click(cx, "menu-item-Equals…");
    type_text(cx, "warm");
    keys(cx, "enter");
    assert_eq!(host.edits().len(), n);
}

#[gpui::test]
fn groups_show_counts_and_fold(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (db, rows) = load("people");
    let (table, cx) = open_with(
        cx,
        &host,
        as_table(db, PIPELINE),
        rows,
        Some(PIPELINE),
        size(px(1200.), px(900.)),
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
    assert_eq!(after, before - 5, "three rows, the New row and the footer");
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
    click(cx, "menu-item-Hide");
    assert!(
        host.last_view()
            .columns
            .iter()
            .any(|c| c.property_id == ROLE && c.hidden)
    );
    assert!(!drawn(cx, "header:Role"));
    columns_menu(cx);
    click(cx, "menu-item-Role");
    assert!(
        host.last_view()
            .columns
            .iter()
            .any(|c| c.property_id == ROLE && !c.hidden)
    );
    click(cx, "menu-item-Phone");
    assert!(table.update(cx, |t, _| t.columns().iter().any(|c| c.0 == PHONE)));
    keys(cx, "escape");
    menu(cx, "Stage");
    click(cx, "menu-item-Move left");
    let cols: Vec<String> = table.update(cx, |t, _| t.columns().into_iter().map(|c| c.0).collect());
    assert_eq!(&cols[..4], [NAME, COMPANY, STAGE, ROLE]);
    // The title can't be hidden or moved.
    menu(cx, "Name");
    assert!(!drawn(cx, "menu-item-Hide"));
    assert!(!drawn(cx, "menu-item-Move left"));
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
    click(cx, "menu-item-Active");
    assert_eq!(
        value_edit(&host.last_edit()),
        (IMANI, STAGE, Some(&Value::text(ACTIVE)))
    );

    // A multi-select toggles options and stays open.
    click(cx, cell(IMANI, TAGS));
    click(cx, cell(IMANI, TAGS));
    click(cx, "menu-item-Mentor");
    assert!(drawn(cx, "menu"));
    let last = host.last_edit();
    let (_, _, v) = value_edit(&last);
    assert_eq!(v.and_then(Value::as_ids).unwrap().last().unwrap(), MENTOR);
    click(cx, "menu-item-Investor");
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
    click(cx, "menu-item-Bluefin Robotics");
    let edits = host.edits();
    assert_eq!(edits.len(), 2, "{edits:?}");
    assert!(
        matches!(&edits[1], Edit::RelationChanged { added, .. } if added == &vec![BLUEFIN.to_owned()])
    );
    keys(cx, "escape");

    // A date picks a day from the grid, keeping no time.
    click(cx, cell(ANA, LAST_CONTACTED));
    click(cx, cell(ANA, LAST_CONTACTED));
    click(cx, "day-2026-11-20");
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

    // A typed value opens a field in the cell, holding what's there, sized to the cell.
    click(cx, cell(ANA, ROLE));
    keys(cx, "enter");
    let (target, text) = editing(&table, cx).expect("a field for Role");
    assert_eq!(
        target,
        TextTarget::Cell {
            row_id: ANA.into(),
            property_id: ROLE.into()
        }
    );
    assert_eq!(text, "Designer");
    let (field, place) = (rect(cx, "editor"), rect(cx, cell(ANA, ROLE)));
    assert!(field.left() >= place.left() && field.top() >= place.top() - px(1.));
    // Typing replaces the selection; Enter commits, trimmed, and the cell keeps the focus.
    select_all(&table, cx);
    type_text(cx, "  Lead designer ");
    keys(cx, "enter");
    assert!(editing(&table, cx).is_none() && !drawn(cx, "editor"));
    assert_eq!(
        value_edit(&host.last_edit()),
        (ANA, ROLE, Some(&Value::text("Lead designer")))
    );
    assert_eq!(
        table.update(cx, |t, _| t.focused_row_cell()),
        Some((ANA.into(), ROLE.into()))
    );

    // A number reads grouping, a currency sign and a percent sign; what isn't a number changes
    // nothing, and an empty field clears it.
    table.update(cx, |t, cx| t.focus_cell(ANA, WARMTH, cx));
    keys(cx, "enter");
    assert_eq!(editing(&table, cx).unwrap().1, "4");
    for (typed, value) in [
        ("1,234", Some(1234.)),
        ("$1,200", Some(1200.)),
        ("25%", Some(0.25)),
        (" -7.5 ", Some(-7.5)),
        ("", None),
    ] {
        if editing(&table, cx).is_none() {
            keys(cx, "enter");
        }
        select_all(&table, cx);
        if typed.is_empty() {
            keys(cx, "backspace");
        } else {
            type_text(cx, typed);
        }
        keys(cx, "enter");
        assert_eq!(
            value_edit(&host.last_edit()),
            (ANA, WARMTH, value.map(Value::number).as_ref()),
            "{typed:?}"
        );
    }
    keys(cx, "enter");
    let n = host.edits().len();
    type_text(cx, "lots");
    keys(cx, "enter");
    assert_eq!(host.edits().len(), n);
    assert!(
        editing(&table, cx).is_none(),
        "the field closes all the same"
    );

    // Email, URL and phone are text, cut to their limits.
    table.update(cx, |t, cx| t.focus_cell(ANA, EMAIL, cx));
    keys(cx, "enter");
    select_all(&table, cx);
    type_text(cx, &"a".repeat(400));
    keys(cx, "enter");
    let last = host.last_edit();
    let (_, _, v) = value_edit(&last);
    assert_eq!(v.and_then(Value::as_str).unwrap().chars().count(), 320);
    table.update(cx, |t, cx| t.focus_cell(ANA, PROFILE, cx));
    keys(cx, "enter");
    select_all(&table, cx);
    type_text(cx, "https://example.com/ana");
    keys(cx, "enter");
    assert_eq!(
        value_edit(&host.last_edit()),
        (ANA, PROFILE, Some(&Value::text("https://example.com/ana")))
    );
    // (Phone is hidden in this view: shown first.)
    columns_menu(cx);
    click(cx, "menu-item-Phone");
    keys(cx, "escape");
    table.update(cx, |t, cx| t.focus_cell(ANA, PHONE, cx));
    keys(cx, "enter");
    let phone = table.update(cx, |t, _| t.cell_words(ANA, PHONE)).unwrap();
    assert_eq!(editing(&table, cx).unwrap().1, phone);
    select_all(&table, cx);
    type_text(cx, "+1 415 555 0100");
    keys(cx, "enter");
    assert_eq!(
        value_edit(&host.last_edit()),
        (ANA, PHONE, Some(&Value::text("+1 415 555 0100")))
    );

    // A person toggles people the property allows.
    table.update(cx, |t, cx| t.focus_cell(ANA, OWNER, cx));
    keys(cx, "enter");
    click(cx, "menu-item-Career Planning");
    let last = host.last_edit();
    let (_, _, v) = value_edit(&last);
    assert_eq!(
        v.and_then(Value::as_people).unwrap(),
        [Person::Owner, Person::agent("career-planning")]
    );
    keys(cx, "escape");

    // The title edits in place with F2 or a second press (Enter opens the row), as a
    // rowTitleChanged.
    table.update(cx, |t, cx| t.focus_cell(ANA, NAME, cx));
    keys(cx, "f2");
    assert_eq!(editing(&table, cx).unwrap().1, "Ana Sofía Durán");
    select_all(&table, cx);
    type_text(cx, "Ana Durán");
    keys(cx, "enter");
    assert_eq!(
        host.last_edit(),
        Edit::RowTitleChanged {
            row_id: ANA.into(),
            title: "Ana Durán".into()
        }
    );
    click(cx, cell(ANA, NAME));
    assert!(drawn(cx, "editor"), "a second press edits the title");
    keys(cx, "escape");

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
fn typed_values_commit_with_tab_or_a_press_elsewhere_and_escape_cancels(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_people(cx, &host);
    activate(cx);
    let at = |table: &Entity<TableView>, cx: &mut VisualTestContext| {
        table.update(cx, |t, _| t.focused_row_cell())
    };
    let role = |table: &Entity<TableView>, cx: &mut VisualTestContext, row: &str| {
        table.update(cx, |t, _| t.cell_words(row, ROLE)).unwrap()
    };

    // The arrows move the caret while a field is open, never the focused cell.
    table.update(cx, |t, cx| t.focus_cell(ANA, ROLE, cx));
    keys(cx, "enter");
    keys(cx, "up down left left");
    assert_eq!(at(&table, cx), Some((ANA.into(), ROLE.into())));
    assert!(editing(&table, cx).is_some());
    type_text(cx, "X");
    assert_eq!(editing(&table, cx).unwrap().1, "DesignXer");
    // Tab commits and moves to the next cell, without opening it.
    let next = table.update(cx, |t, _| t.columns()[3].0.clone());
    keys(cx, "tab");
    assert_eq!(role(&table, cx, ANA), "DesignXer");
    assert_eq!(at(&table, cx), Some((ANA.into(), next)));
    assert!(editing(&table, cx).is_none());
    // Shift-Tab commits and moves back.
    keys(cx, "shift-tab");
    keys(cx, "enter");
    select_all(&table, cx);
    type_text(cx, "Designer");
    keys(cx, "shift-tab");
    assert_eq!(role(&table, cx, ANA), "Designer");
    assert_eq!(at(&table, cx), Some((ANA.into(), COMPANY.into())));
    // With no field open the arrows move the focused cell again.
    keys(cx, "right");
    assert_eq!(at(&table, cx), Some((ANA.into(), ROLE.into())));

    // Escape closes the field without an edit; the cell keeps the focus, and a second Escape
    // lets go of it.
    let n = host.edits().len();
    keys(cx, "enter");
    type_text(cx, " and more");
    keys(cx, "escape");
    assert!(editing(&table, cx).is_none() && !drawn(cx, "editor"));
    assert_eq!(host.edits().len(), n);
    assert_eq!(role(&table, cx, ANA), "Designer");
    assert_eq!(at(&table, cx), Some((ANA.into(), ROLE.into())));
    keys(cx, "escape");
    assert_eq!(at(&table, cx), None);

    // A field committed unchanged emits nothing.
    table.update(cx, |t, cx| t.focus_cell(ANA, ROLE, cx));
    keys(cx, "enter");
    keys(cx, "enter");
    assert_eq!(host.edits().len(), n);
    // The window going to the background leaves the field open, as it was.
    keys(cx, "enter");
    type_text(cx, "!");
    cx.deactivate_window();
    draw(cx);
    activate(cx);
    assert_eq!(editing(&table, cx).unwrap().1, "Designer!");
    keys(cx, "escape");
    assert_eq!(host.edits().len(), n);

    // Pressing another cell commits (the field loses the focus) and focuses that cell.
    click(cx, cell(IMANI, ROLE));
    click(cx, cell(IMANI, ROLE));
    assert!(drawn(cx, "editor"), "a second press edits");
    // A press inside the field is the field's: it stays open.
    click(cx, "editor");
    assert!(editing(&table, cx).is_some());
    select_all(&table, cx);
    type_text(cx, "Partner");
    click(cx, cell(THEO, STAGE));
    assert!(editing(&table, cx).is_none());
    assert_eq!(
        value_edit(&host.last_edit()),
        (IMANI, ROLE, Some(&Value::text("Partner")))
    );
    assert_eq!(at(&table, cx), Some((THEO.into(), STAGE.into())));
    // Delete and Backspace are the field's while it's open: they empty nothing else.
    table.update(cx, |t, cx| t.focus_cell(IMANI, ROLE, cx));
    keys(cx, "enter");
    let n = host.edits().len();
    keys(cx, "backspace delete");
    assert_eq!(host.edits().len(), n);
    assert_eq!(editing(&table, cx).unwrap().1, "Partne");
    keys(cx, "escape");
}

#[gpui::test]
fn text_values_grow_over_the_rows_below_and_take_new_lines(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (db, rows) = load("people");
    let (table, cx) = open_with(cx, &host, db, rows, None, size(px(1200.), px(700.)));
    table.update(cx, |t, cx| t.focus_cell(ANA, ROLE, cx));
    keys(cx, "enter");
    let place = rect(cx, cell(ANA, ROLE));
    let one = rect(cx, "editor");
    assert!((one.left() - place.left()).abs() < px(1.) && (one.top() - place.top()).abs() < px(1.));
    assert!(one.size.width >= place.size.width && one.size.width >= px(240.));
    assert!(
        one.size.height <= place.size.height + px(1.),
        "one line, the row's height"
    );
    // Shift-Enter starts a new line, and the field grows; Enter commits both.
    keys(cx, "shift-enter");
    type_text(cx, "Speaker");
    let two = rect(cx, "editor");
    assert!(
        two.size.height > one.size.height + px(8.),
        "{one:?} then {two:?}"
    );
    keys(cx, "enter");
    assert_eq!(
        value_edit(&host.last_edit()),
        (ANA, ROLE, Some(&Value::text("Designer\nSpeaker")))
    );
    // Opened again, it holds both lines.
    keys(cx, "enter");
    assert_eq!(editing(&table, cx).unwrap().1, "Designer\nSpeaker");
    keys(cx, "escape");
}

#[gpui::test]
fn the_hosts_field_wins_when_it_has_one(cx: &mut TestAppContext) {
    let host = TestHost::with_hook();
    let (table, cx) = open_people(cx, &host);
    // The host gets the request, and no field opens in place.
    click(cx, cell(ANA, ROLE));
    keys(cx, "enter");
    assert!(editing(&table, cx).is_none() && !drawn(cx, "editor"));
    let edit = host.texts.borrow_mut().pop().expect("a field for Role");
    assert_eq!(edit.text, "Designer");
    assert_eq!(edit.kind, PropertyType::Text);
    assert_eq!(edit.label, "Role");
    assert!(edit.bounds.size.width > px(100.));
    // Its answer is read, limited and emitted as the field in place's would be.
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
    // Numbers and limits too.
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
    table.update(cx, |t, cx| t.focus_cell(ANA, EMAIL, cx));
    keys(cx, "enter");
    let edit = host.texts.borrow_mut().pop().unwrap();
    assert_eq!(edit.max_chars, Some(320));
    cx.update(|window, cx| edit.reply.cancel(window, cx));
    draw(cx);
    assert_eq!(host.edits().len(), n + 1);
    // A filter's value too, under its header.
    menu(cx, "Role");
    click(cx, "menu-item-Contains…");
    assert!(!drawn(cx, "editor"));
    let edit = host
        .texts
        .borrow_mut()
        .pop()
        .expect("a field for the filter");
    assert_eq!(
        edit.target,
        TextTarget::Filter {
            property_id: ROLE.into(),
            op: FilterOp::Contains
        }
    );
    cx.update(|window, cx| edit.reply.commit("lead", window, cx));
    draw(cx);
    assert_eq!(titles(&table, cx), ["Ana Sofía Durán"]);
    // And the host can hand an edit back to be edited in place.
    table.update(cx, |t, cx| t.focus_cell(ANA, ROLE, cx));
    keys(cx, "enter");
    let edit = host.texts.borrow_mut().pop().unwrap();
    cx.update(|window, cx| edit.reply.edit_in_place(window, cx));
    draw(cx);
    assert_eq!(editing(&table, cx).unwrap().1, "Lead designer");
    type_text(cx, "!");
    keys(cx, "enter");
    assert_eq!(
        value_edit(&host.last_edit()),
        (ANA, ROLE, Some(&Value::text("Lead designer!")))
    );
}

#[gpui::test]
fn a_tap_on_a_focused_cell_edits_it_in_place(cx: &mut TestAppContext) {
    let host = TestHost::touch();
    let (db, rows) = load("people");
    let (table, cx) = open_with(cx, &host, db, rows, None, size(px(1200.), px(900.)));
    activate(cx);
    let at = rect(cx, cell(IMANI, ROLE)).center();
    tap(cx, at);
    assert!(!drawn(cx, "editor"), "the first tap focuses");
    tap(cx, at);
    assert!(drawn(cx, "editor"), "a tap on the focused cell edits it");
    // The field has the focus, so the platform's keyboard (and its input methods) come up.
    let state = table.read_with(cx, |t, _| t.editor_state()).unwrap();
    assert!(cx.update(|window, cx| state.read(cx).is_focused(window)));
    // A tap in the field keeps it open; a tap elsewhere commits.
    let inside = rect(cx, "editor").center();
    tap(cx, inside);
    assert!(editing(&table, cx).is_some());
    select_all(&table, cx);
    type_text(cx, "Founder");
    let elsewhere = rect(cx, cell(THEO, STAGE)).center();
    tap(cx, elsewhere);
    assert_eq!(
        value_edit(&host.last_edit()),
        (IMANI, ROLE, Some(&Value::text("Founder")))
    );
    // The title too, its field the cell's height.
    let at = rect(cx, cell(IMANI, NAME)).center();
    tap(cx, point(at.x - px(60.), at.y));
    tap(cx, point(at.x - px(60.), at.y));
    assert_eq!(editing(&table, cx).unwrap().1, "Imani Castell");
    assert!(rect(cx, "editor").size.height <= rect(cx, cell(IMANI, NAME)).size.height);
    keys(cx, "escape");
}

/// The solid fill drawn exactly over `what` (a day of the month, say), if any.
fn fill(cx: &mut VisualTestContext, what: &str) -> Option<Hsla> {
    let b = rect(cx, what);
    cx.update(|window, _| {
        let s = window.scale_factor();
        let near = |a: f32, b: Pixels| (a - f32::from(b) * s).abs() <= 1.;
        window
            .rendered_scene()
            .quads
            .iter()
            .filter(|q| {
                near(q.bounds.origin.x.as_f32(), b.origin.x)
                    && near(q.bounds.origin.y.as_f32(), b.origin.y)
                    && near(q.bounds.size.width.as_f32(), b.size.width)
                    && near(q.bounds.size.height.as_f32(), b.size.height)
            })
            .filter_map(|q| q.background.as_solid())
            .find(|c| c.a > 0.)
    })
}

/// The last batch of edits the host was handed.
fn last_batch(host: &TestHost) -> Vec<Edit> {
    match host.events.borrow().last() {
        Some(TableEvent::Edits { edits }) => edits.clone(),
        other => panic!("not edits: {other:?}"),
    }
}

#[gpui::test]
fn options_are_pills_and_pickers_search(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_people(cx, &host);
    // An option is drawn as its pill, as in a cell, in the entry its name labels.
    click(cx, cell(IMANI, STAGE));
    click(cx, cell(IMANI, STAGE));
    let row = rect(cx, "menu-item-Warm");
    assert!(row.contains(&rect(cx, "pill:Warm").center()));
    // Rows are 28 points at the table's text size (the theme's points are the window's rem
    // over 15).
    let rem = cx.update(|window, _| window.rem_size());
    let expected = TableTheme::dark().pixels(28., rem);
    assert!((row.size.height - expected).abs() < px(0.5), "{row:?}");
    // It opens on the current option, Warm: Down and Enter pick the next one.
    keys(cx, "down enter");
    assert_eq!(
        value_edit(&host.last_edit()),
        (IMANI, STAGE, Some(&Value::text(ACTIVE)))
    );
    assert!(!drawn(cx, "menu"), "a select's picker closes on a pick");
    // Typing searches the options by name (the label the pill stands for), and Enter picks
    // the first match.
    keys(cx, "enter");
    type_text(cx, "dor");
    assert!(drawn(cx, "pill:Dormant"));
    assert!(!drawn(cx, "pill:Lead") && !drawn(cx, "pill:Active"));
    keys(cx, "enter");
    assert_eq!(
        value_edit(&host.last_edit()),
        (IMANI, STAGE, Some(&Value::text(DORMANT)))
    );
    // People are searched the same way, and their picker stays open on a pick.
    table.update(cx, |t, cx| t.focus_cell(ANA, OWNER, cx));
    keys(cx, "enter");
    type_text(cx, "career");
    assert!(drawn(cx, "menu-item-Career Planning"));
    keys(cx, "enter");
    let last = host.last_edit();
    let (_, _, v) = value_edit(&last);
    assert_eq!(
        v.and_then(Value::as_people).unwrap(),
        [Person::Owner, Person::agent("career-planning")]
    );
    assert!(drawn(cx, "menu"));
}

#[gpui::test]
fn a_picker_creates_an_option(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_people(cx, &host);
    let options = |table: &Entity<TableView>, cx: &mut VisualTestContext, pid: &str| {
        table.update(cx, |t, _| {
            t.database()
                .database
                .as_ref()
                .unwrap()
                .property(pid)
                .unwrap()
                .options()
                .to_vec()
        })
    };
    let before = options(&table, cx, STAGE);
    // A select: what's typed and matches no option is offered as a new one, and Enter makes
    // it: the whole property with the option after the others, then the row's value, in one
    // batch.
    table.update(cx, |t, cx| t.focus_cell(IMANI, STAGE, cx));
    keys(cx, "enter");
    type_text(cx, "Prospect");
    keys(cx, "enter");
    let batch = last_batch(&host);
    assert_eq!(batch.len(), 2, "{batch:?}");
    let Edit::PropertyChanged {
        database_id,
        property,
    } = &batch[0]
    else {
        panic!("first a propertyChanged: {batch:?}");
    };
    assert_eq!(
        (database_id.as_str(), property.id.as_str()),
        (PEOPLE, STAGE)
    );
    let made = property.options().last().unwrap().clone();
    assert_eq!(property.options().len(), before.len() + 1);
    assert_eq!(&property.options()[..before.len()], &before[..]);
    assert_eq!(
        (made.name.as_str(), &made.color),
        ("Prospect", &Color::Default)
    );
    assert_eq!(made.id.len(), 36, "a new uuid");
    assert!(before.iter().all(|o| o.id != made.id));
    assert_eq!(
        value_edit(&batch[1]),
        (IMANI, STAGE, Some(&Value::text(made.id.clone())))
    );
    assert!(!drawn(cx, "menu"), "a select's picker closes");
    assert_eq!(options(&table, cx, STAGE).last(), Some(&made));

    // A multi-select: the new option is added to the others, and the picker stays open, its
    // search emptied, with the new option checked.
    table.update(cx, |t, cx| t.focus_cell(IMANI, TAGS, cx));
    keys(cx, "enter");
    let had = table.update(cx, |t, _| {
        let r = t.rows().iter().find(|r| r.id == IMANI).unwrap();
        r.value(TAGS).and_then(Value::as_ids).unwrap().to_vec()
    });
    type_text(cx, "Advisor");
    keys(cx, "enter");
    let batch = last_batch(&host);
    let Edit::PropertyChanged { property, .. } = &batch[0] else {
        panic!("first a propertyChanged: {batch:?}");
    };
    let made = property.options().last().unwrap().clone();
    assert_eq!(made.name, "Advisor");
    let mut ids = had.clone();
    ids.push(made.id.clone());
    assert_eq!(value_edit(&batch[1]), (IMANI, TAGS, Some(&Value::Ids(ids))));
    assert!(drawn(cx, "menu"), "a multi-select's picker stays open");
    assert!(drawn(cx, "pill:Advisor") && drawn(cx, "pill:Mentor"));
    // A name that's an option's already (case aside) is no new option: Enter toggles it.
    let n = host.edits().len();
    type_text(cx, "mentor");
    keys(cx, "enter");
    let edits = host.edits();
    assert_eq!(edits.len(), n + 1, "only the value");
    let (_, _, v) = value_edit(edits.last().unwrap());
    assert!(
        v.and_then(Value::as_ids)
            .unwrap()
            .contains(&MENTOR.to_string())
    );
}

#[gpui::test]
fn the_month_is_the_tables_with_its_footer_and_ranges(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_people(cx, &host);
    let theme = TableTheme::dark();
    // Imani was last contacted on October 2: picked in the table's accent, 28 points a day.
    table.update(cx, |t, cx| t.focus_cell(IMANI, LAST_CONTACTED, cx));
    keys(cx, "enter");
    assert_eq!(fill(cx, "day-2026-10-02"), Some(theme.colors.accent));
    let day = rect(cx, "day-2026-10-02");
    let rem = cx.update(|window, _| window.rem_size());
    let expected = theme.pixels(28., rem);
    assert!((day.size.width - expected).abs() < px(0.5), "{day:?}");
    // Today and Clear are in the month's footer, under its days and inside its popover.
    let footer = rect(cx, "month-footer");
    assert!(footer.top() > day.bottom());
    assert!(rect(cx, "menu").contains(&footer.center()));
    assert!(footer.contains(&rect(cx, "day-today").center()));
    assert!(footer.contains(&rect(cx, "day-clear").center()));
    click(cx, "day-today");
    let today = NOW.div_euclid(engine::time::DAY_MS);
    assert_eq!(
        value_edit(&host.last_edit()),
        (IMANI, LAST_CONTACTED, Some(&Value::date_on_day(today)))
    );
    assert!(!drawn(cx, "menu"));

    // A range is shown as one: its ends picked, the days between tinted.
    let d = |y, m, d| engine::time::days_from_civil(y, m, d) * engine::time::DAY_MS;
    let range = Value::Date(DateValue {
        start: d(2026, 11, 10),
        end: Some(d(2026, 11, 13)),
        include_time: false,
    });
    table.update(cx, |t, cx| {
        let db = t.database().clone();
        let mut rows = t.rows().to_vec();
        let r = rows.iter_mut().find(|r| r.id == ANA).unwrap();
        r.values
            .get_or_insert_with(Default::default)
            .insert(LAST_CONTACTED.into(), range.clone());
        t.set_docs(db, rows, cx);
    });
    table.update(cx, |t, cx| t.focus_cell(ANA, LAST_CONTACTED, cx));
    keys(cx, "enter");
    assert_eq!(fill(cx, "day-2026-11-10"), Some(theme.colors.accent));
    assert_eq!(fill(cx, "day-2026-11-13"), Some(theme.colors.accent));
    assert_eq!(fill(cx, "day-2026-11-11"), Some(theme.colors.selection));
    assert_eq!(fill(cx, "day-2026-11-12"), Some(theme.colors.selection));
    assert_eq!(fill(cx, "day-2026-11-16"), None);
    // A pick moves it, keeping its length.
    click(cx, "day-2026-11-20");
    assert_eq!(
        value_edit(&host.last_edit()),
        (
            ANA,
            LAST_CONTACTED,
            Some(&Value::Date(DateValue {
                start: d(2026, 11, 20),
                end: Some(d(2026, 11, 23)),
                include_time: false,
            }))
        )
    );
}

#[cfg(target_os = "macos")]
/// The glyphs drawn inside `within`, over the open popover (not the table's under it): where
/// each is, and its colour.
fn glyphs(cx: &mut VisualTestContext, within: Bounds<Pixels>) -> Vec<(Bounds<Pixels>, Hsla)> {
    let popover = TableTheme::dark().colors.popover;
    cx.update(|window, _| {
        let s = window.scale_factor();
        let scene = window.rendered_scene();
        let to_px = |b: Bounds<ScaledPixels>| {
            let at = point(px(b.origin.x.as_f32() / s), px(b.origin.y.as_f32() / s));
            Bounds::new(
                at,
                size(
                    px(b.size.width.as_f32() / s),
                    px(b.size.height.as_f32() / s),
                ),
            )
        };
        let over = scene
            .quads
            .iter()
            .filter(|q| q.background.as_solid() == Some(popover))
            .filter(|q| to_px(q.bounds).contains(&within.center()))
            .map(|q| q.order)
            .max()
            .expect("a popover is open");
        scene
            .monochrome_sprites
            .iter()
            .map(|g| (g.order, g.bounds, g.color))
            .chain(
                scene
                    .subpixel_sprites
                    .iter()
                    .map(|g| (g.order, g.bounds, g.color)),
            )
            .map(|(order, b, color)| (order, to_px(b), color))
            .filter(|(order, b, _)| *order > over && within.contains(&b.center()))
            .map(|(_, b, color)| (b, color))
            .collect()
    })
}

/// The colour of the border drawn exactly round `what` (a day of the month, say), if any.
fn ring(cx: &mut VisualTestContext, what: &str) -> Option<Hsla> {
    let b = rect(cx, what);
    cx.update(|window, _| {
        let s = window.scale_factor();
        let near = |a: f32, b: Pixels| (a - f32::from(b) * s).abs() <= 1.;
        window
            .rendered_scene()
            .quads
            .iter()
            .filter(|q| {
                near(q.bounds.origin.x.as_f32(), b.origin.x)
                    && near(q.bounds.origin.y.as_f32(), b.origin.y)
                    && near(q.bounds.size.width.as_f32(), b.size.width)
                    && near(q.bounds.size.height.as_f32(), b.size.height)
            })
            .filter(|q| q.border_widths.left.as_f32() > 0. && q.border_color.a > 0.)
            .map(|q| q.border_color)
            .next()
    })
}

#[cfg(target_os = "macos")]
/// In the open menu's entry `label`: the check's column comes before the label, keeping its
/// room whether or not the entry is checked, and holds the check only when it is. (GPUI's test
/// app has no assets, so the icon itself draws nothing: it's seen by the room it takes.)
/// Returns where the label starts.
fn check_leads(cx: &mut VisualTestContext, label: &str, checked: bool) -> Pixels {
    let row = rect(cx, format!("menu-item-{label}"));
    let column = rect(cx, format!("menu-check-{label}"));
    assert!(
        row.contains(&column.center()),
        "{label}'s check is in its entry"
    );
    let rem = cx.update(|window, _| window.rem_size());
    let room = TableTheme::dark().pixels(16., rem);
    assert!(
        (column.size.width - room).abs() < px(0.5),
        "{label}: {column:?}"
    );
    assert!(
        (column.left() - row.left()) < px(12.),
        "{label}'s check starts its entry"
    );
    assert_eq!(
        column.size.height > px(0.),
        checked,
        "{label}'s check: {column:?}"
    );
    // The label: its pill when it has one, else its glyphs.
    let start = if drawn(cx, format!("pill:{label}")) {
        rect(cx, format!("pill:{label}")).left()
    } else {
        glyphs(cx, row)
            .iter()
            .map(|(b, _)| b.left())
            .min()
            .unwrap_or_else(|| panic!("{label}'s label is drawn"))
    };
    assert!(
        start >= column.right(),
        "{label}'s check comes before its label"
    );
    start
}

// Laid out with Core Text, as on screen, so the labels' glyphs are drawn (the Mac only).
#[cfg(target_os = "macos")]
#[test]
fn checks_lead_the_labels_in_menus_and_pickers() {
    let mut cx = TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        std::sync::Arc::new(gpui_apple::AppleTextSystem::new()),
    );
    let host = TestHost::new();
    let (table, cx) = open_people(&mut cx, &host);
    // The column's menu: the sort that's on is checked, the other keeps the room, and their
    // labels line up.
    menu(cx, "Stage");
    click(cx, "menu-item-Sort ascending");
    menu(cx, "Stage");
    let on = check_leads(cx, "Sort ascending", true);
    let off = check_leads(cx, "Sort descending", false);
    assert!(
        (on - off).abs() <= px(1.),
        "the labels line up: {on:?}, {off:?}"
    );
    keys(cx, "escape");
    // The Columns menu: a shown column checked, a hidden one not.
    menu(cx, "Role");
    click(cx, "menu-item-Hide");
    columns_menu(cx);
    let on = check_leads(cx, "Stage", true);
    let off = check_leads(cx, "Role", false);
    assert!(
        (on - off).abs() <= px(1.),
        "the labels line up: {on:?}, {off:?}"
    );
    keys(cx, "escape");
    // A select's picker: its option checked, the others not, their pills lined up. Its search
    // field is drawn as the table's own fields: the placeholder faint.
    table.update(cx, |t, cx| t.focus_cell(IMANI, STAGE, cx));
    keys(cx, "enter");
    let faint = TableTheme::dark().colors.text_faint;
    let menu = rect(cx, "menu");
    let placeholder = glyphs(cx, menu).iter().filter(|(_, c)| *c == faint).count();
    assert!(
        placeholder > 10,
        "the placeholder is faint: {placeholder} glyphs"
    );
    let on = check_leads(cx, "Warm", true);
    let off = check_leads(cx, "Lead", false);
    assert!(
        (on - off).abs() < px(0.5),
        "the pills line up: {on:?}, {off:?}"
    );
    keys(cx, "escape");
    // A multi-select's (Imani is tagged Investor).
    table.update(cx, |t, cx| t.focus_cell(IMANI, TAGS, cx));
    keys(cx, "enter");
    let on = check_leads(cx, "Investor", true);
    let off = check_leads(cx, "Mentor", false);
    assert!(
        (on - off).abs() < px(0.5),
        "the pills line up: {on:?}, {off:?}"
    );
    keys(cx, "escape");
    // A person's.
    table.update(cx, |t, cx| t.focus_cell(ANA, OWNER, cx));
    keys(cx, "enter");
    click(cx, "menu-item-Career Planning");
    check_leads(cx, "Career Planning", true);
    keys(cx, "escape");
    // A relation's.
    table.update(cx, |t, cx| t.focus_cell(ANA, COMPANY, cx));
    keys(cx, "enter");
    click(cx, "menu-item-Bluefin Robotics");
    check_leads(cx, "Bluefin Robotics", true);
    keys(cx, "escape");
}

#[gpui::test]
fn the_month_rings_today(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_people(cx, &host);
    let theme = TableTheme::dark();
    // November 15 is today (NOW): in a month opened on the 20th it's ringed in the accent and
    // not filled; the picked day is filled and not ringed.
    let day = engine::time::days_from_civil(2026, 11, 20);
    table.update(cx, |t, cx| {
        let db = t.database().clone();
        let mut rows = t.rows().to_vec();
        let r = rows.iter_mut().find(|r| r.id == ANA).unwrap();
        r.values
            .get_or_insert_with(Default::default)
            .insert(LAST_CONTACTED.into(), Value::date_on_day(day));
        t.set_docs(db, rows, cx);
    });
    table.update(cx, |t, cx| t.focus_cell(ANA, LAST_CONTACTED, cx));
    keys(cx, "enter");
    assert_eq!(
        NOW.div_euclid(engine::time::DAY_MS),
        engine::time::days_from_civil(2026, 11, 15)
    );
    assert_eq!(ring(cx, "day-2026-11-15"), Some(theme.colors.accent));
    assert_eq!(fill(cx, "day-2026-11-15"), None);
    assert_eq!(fill(cx, "day-2026-11-20"), Some(theme.colors.accent));
    assert_ne!(ring(cx, "day-2026-11-20"), Some(theme.colors.accent));
    assert_eq!(ring(cx, "day-2026-11-16"), None);
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
    // ⌥↓ opens the column's menu on its first entry, and the arrows and Enter work in it.
    keys(cx, "alt-down");
    assert!(drawn(cx, "menu"));
    keys(cx, "down enter");
    assert_eq!(host.last_view().sort, vec![Sort::descending(NAME)]);
    // Shift-Enter adds a sort to the others, as a Shift-click does.
    keys(cx, "right alt-down");
    keys(cx, "shift-enter");
    assert_eq!(
        host.last_view().sort,
        vec![Sort::descending(NAME), Sort::ascending(COMPANY)]
    );
    keys(cx, "left");
    // Escape closes a menu and hands the keyboard back to the table: the arrows move the cell.
    keys(cx, "alt-down");
    assert!(drawn(cx, "menu"));
    keys(cx, "escape");
    assert!(!drawn(cx, "menu"));
    keys(cx, "right");
    assert_eq!(at(&table, cx), Some((ANA.into(), COMPANY.into())));
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
        as_table(db, PIPELINE),
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
    let (table, cx) = open_with(cx, &host, db, rows, None, size(px(1600.), px(900.)));

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
    click(cx, "menu-item-Avery Lin");
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
    let (table, cx) = open_with(
        cx,
        &host,
        as_table(db, PIPELINE),
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
    check(cx, "add-property".into(), true);
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
    check(cx, "menu-item-Sort ascending".into(), false);
    keys(cx, "escape");
    // Menus' entries and the month's days too.
    let at = rect(cx, cell(IMANI, STAGE)).center();
    tap(cx, at);
    let at = rect(cx, cell(IMANI, STAGE)).center();
    tap(cx, at);
    check(cx, "menu-item-Lead".into(), false);
    let at = rect(cx, "menu-item-Lead").center();
    tap(cx, at);
    // (Last contacted scrolled into view and focused first: a tap on it opens its month.)
    table.update(cx, |t, cx| t.focus_cell(IMANI, LAST_CONTACTED, cx));
    draw(cx);
    let at = rect(cx, cell(IMANI, LAST_CONTACTED)).center();
    tap(cx, at);
    check(cx, "day-2026-10-02".into(), true);
    keys(cx, "escape");
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

#[gpui::test]
fn the_focus_and_a_late_press_stay_on_their_row_when_the_rows_are_reordered(
    cx: &mut TestAppContext,
) {
    let host = TestHost::new();
    host.apply.set(false);
    let (db, rows) = load("people");
    let (table, cx) = open_with(cx, &host, db, rows.clone(), None, size(px(2600.), px(700.)));
    table.update(cx, |t, cx| t.focus_cell(ANA, FOLLOW_UP, cx));
    draw(cx);
    // Reordered, then shrunk, by the host: the focus follows Ana's row.
    let reversed: Vec<Doc> = rows.iter().rev().cloned().collect();
    table.update(cx, |t, cx| t.set_rows(reversed, cx));
    draw(cx);
    assert_eq!(
        table.update(cx, |t, _| t.focused_row_cell()),
        Some((ANA.into(), FOLLOW_UP.into()))
    );
    let fewer: Vec<Doc> = rows.iter().filter(|r| r.id != ROWAN).cloned().collect();
    table.update(cx, |t, cx| t.set_rows(fewer, cx));
    draw(cx);
    assert_eq!(
        table.update(cx, |t, _| t.focused_row_cell()),
        Some((ANA.into(), FOLLOW_UP.into()))
    );
    // A press on Imani's checkbox as drawn, as Ana's row goes (Imani's line moving up into
    // Ana's place, Jonah's into Imani's): it toggles no other row's (here, none: the lines it
    // was drawn from are gone).
    let at = rect(cx, cell(IMANI, FOLLOW_UP)).center();
    let without_ana: Vec<Doc> = rows.iter().filter(|r| r.id != ANA).cloned().collect();
    host.clear();
    click_as_rows_change(cx, &table, without_ana, at);
    assert!(
        host.edits().iter().all(|e| value_edit(e).0 == IMANI),
        "{:?}",
        host.edits()
    );
}
