//! Changing views from the bar the table draws, as a person does it: switching tabs, "+" making
//! a view of each type (a board grouped by the first select, or asking what to group by),
//! renaming, duplicating, changing type, reordering and deleting (after a confirm, never the
//! last), the grouping, date buckets and properties on cards and lines, every limit refused
//! with a note, the keyboard, and 44-point targets on touch. Each is checked by the edits it
//! hands the host.

#![cfg(all(target_os = "macos", feature = "gpui"))]

mod common;

use common::ids::*;
use common::*;
use gpui::*;
use gpui_tables::view::TableView;
use gpui_tables::*;
use gpui_tables::{Edit, View};
// The built-in #[test], not GPUI's, which gpui::* brings in.
use std::prelude::v1::test;

const FOLLOW_UP_VIEW: &str = "Follow up";
const BY_STAGE: &str = "By stage";

fn item(label: &str) -> String {
    format!("menu-item-{label}")
}

fn tab(name: &str) -> String {
    format!("view-tab:{name}")
}

/// A database's table with the bar of views drawn.
fn open_bar<'a>(
    cx: &'a mut TestAppContext,
    host: &std::rc::Rc<TestHost>,
    db: Doc,
    rows: Vec<Doc>,
    size: Size<Pixels>,
) -> (Entity<TableView>, &'a mut VisualTestContext) {
    let (table, cx) = open_with(cx, host, db, rows, None, size);
    table.update(cx, |t, cx| t.set_view_bar(true, cx));
    draw(cx);
    draw(cx);
    (table, cx)
}

fn open_people_bar<'a>(
    cx: &'a mut TestAppContext,
    host: &std::rc::Rc<TestHost>,
) -> (Entity<TableView>, &'a mut VisualTestContext) {
    let (db, rows) = load("people");
    open_bar(cx, host, db, rows, size(px(1400.), px(700.)))
}

/// The edits of the last batch.
fn last_batch(host: &TestHost) -> Vec<Edit> {
    match host.events.borrow().last() {
        Some(TableEvent::Edits { edits }) => edits.clone(),
        other => panic!("no batch: {other:?}"),
    }
}

fn batches(host: &TestHost) -> usize {
    host.events.borrow().len()
}

fn views(table: &Entity<TableView>, cx: &mut VisualTestContext) -> Vec<View> {
    table.read_with(cx, |t, _| {
        t.database()
            .database
            .as_ref()
            .unwrap()
            .views_in_order()
            .into_iter()
            .cloned()
            .collect()
    })
}

fn shown(table: &Entity<TableView>, cx: &mut VisualTestContext) -> View {
    table.read_with(cx, |t, _| t.view().cloned().unwrap())
}

fn created(e: &Edit) -> &View {
    match e {
        Edit::ViewCreated { database_id, view } => {
            assert_eq!(database_id, PEOPLE);
            view
        }
        other => panic!("not a viewCreated: {other:?}"),
    }
}

fn changed(e: &Edit) -> &View {
    match e {
        Edit::ViewChanged { database_id, view } => {
            assert_eq!(database_id, PEOPLE);
            view
        }
        other => panic!("not a viewChanged: {other:?}"),
    }
}

/// Opens a view's menu: a press on the shown view's tab, a right-click on another's.
fn view_menu(table: &Entity<TableView>, cx: &mut VisualTestContext, name: &str) {
    if shown(table, cx).name == name {
        click(cx, tab(name));
    } else {
        let at = rect(cx, tab(name)).center();
        cx.simulate_mouse_down(at, MouseButton::Right, Modifiers::none());
        cx.simulate_mouse_up(at, MouseButton::Right, Modifiers::none());
        draw(cx);
    }
    assert!(drawn(cx, item("Rename")), "{name}'s menu opened");
}

/// Enter on a focused tab or button: GPUI presses it when the key comes back up.
fn enter_up(cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("enter");
    cx.simulate_event(KeyUpEvent {
        keystroke: Keystroke::parse("enter").unwrap(),
    });
    draw(cx);
}

fn note(cx: &mut VisualTestContext) -> bool {
    drawn(cx, "menu-note")
}

/// Types into the open menu's field, replacing what's there.
fn type_name(cx: &mut VisualTestContext, text: &str) {
    keys(cx, "cmd-a");
    type_text(cx, text);
}

/// The People database without its select (Stage), so a board has no select to group by.
fn people_without_select() -> (Doc, Vec<Doc>) {
    let (mut db, rows) = load("people");
    let f = db.database.as_mut().unwrap();
    f.properties.retain(|p| p.kind != PropertyType::Select);
    for v in &mut f.views {
        if v.group_by.as_deref() == Some(STAGE) {
            v.group_by = None;
        }
    }
    (db, rows)
}

#[gpui::test]
fn the_bar_shows_every_view_and_switches_between_them(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_people_bar(cx, &host);
    for name in ["All people", "Pipeline", FOLLOW_UP_VIEW, BY_STAGE] {
        assert!(drawn(cx, tab(name)), "{name}'s tab");
    }
    // In order, left to right, the bar over the table.
    let xs: Vec<Pixels> = ["All people", "Pipeline", FOLLOW_UP_VIEW, BY_STAGE]
        .iter()
        .map(|n| rect(cx, tab(n)).left())
        .collect();
    assert!(xs.windows(2).all(|w| w[0] < w[1]), "{xs:?}");
    assert!(rect(cx, "view-bar").bottom() <= rect(cx, "table").top());
    assert!(drawn(cx, "add-view") && drawn(cx, "view-settings"));

    click(cx, tab("Pipeline"));
    assert_eq!(shown(&table, cx).id, PIPELINE);
    assert!(drawn(cx, "board"), "the board is drawn");
    assert_eq!(*host.shown.borrow(), vec![PIPELINE.to_owned()]);
    assert!(host.edits().is_empty(), "switching is no edit");
    assert!(!table.read_with(cx, |t, _| t.has_popover()));

    // The shown view's tab opens its menu, hanging from the tab.
    click(cx, tab("Pipeline"));
    assert!(drawn(cx, item("Rename")));
    let (menu, at) = (rect(cx, "menu"), rect(cx, tab("Pipeline")));
    assert!(menu.top() >= at.bottom() && (menu.left() - at.left()).abs() < px(12.));
    keys(cx, "escape");
    // The + menu hangs from the +, the settings from their button (held in the window).
    click(cx, "add-view");
    let (menu, at) = (rect(cx, "menu"), rect(cx, "add-view"));
    assert!(menu.top() >= at.bottom() && (menu.left() - at.left()).abs() < px(12.));
    keys(cx, "escape");
    click(cx, "view-settings");
    let (menu, at) = (rect(cx, "menu"), rect(cx, "view-settings"));
    assert!(
        menu.top() >= at.bottom() && menu.right() >= at.left(),
        "{menu:?} {at:?}"
    );
    keys(cx, "escape");
    assert!(!table.read_with(cx, |t, _| t.has_popover()));
}

#[gpui::test]
fn the_plus_makes_a_view_of_each_type_and_shows_it(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_people_bar(cx, &host);
    for (kind, label) in [
        (ViewType::Table, "Table"),
        (ViewType::Board, "Board"),
        (ViewType::List, "List"),
    ] {
        let before = views(&table, cx);
        click(cx, "add-view");
        assert!(drawn(cx, "menu-field"), "the name's field");
        let name = format!("My {label}");
        type_text(cx, &name);
        click(cx, item(label));
        let batch = last_batch(&host);
        assert_eq!(batch.len(), 1, "{batch:?}");
        let v = created(&batch[0]);
        assert_eq!(v.name, name);
        assert_eq!(v.kind, kind);
        assert!(before.iter().all(|b| b.id != v.id && b.order < v.order));
        assert!(v.sort.is_empty() && v.filter.is_none() && v.columns.is_empty());
        match kind {
            // A board groups by the first select.
            ViewType::Board => assert_eq!(v.group_by.as_deref(), Some(STAGE)),
            _ => assert_eq!(v.group_by, None),
        }
        assert_eq!(shown(&table, cx).id, v.id, "the new view is shown");
        assert_eq!(host.shown.borrow().last(), Some(&v.id));
        assert!(drawn(cx, tab(&name)), "its tab");
        assert!(!table.read_with(cx, |t, _| t.has_popover()));
    }
    // Nothing typed: named for its type, unlike the others.
    click(cx, "add-view");
    click(cx, item("Table"));
    assert_eq!(created(&last_batch(&host)[0]).name, "Table");
    click(cx, "add-view");
    click(cx, item("Table"));
    assert_eq!(created(&last_batch(&host)[0]).name, "Table 2");
}

#[gpui::test]
fn a_board_without_a_select_asks_what_to_group_by(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (db, rows) = people_without_select();
    let (table, cx) = open_bar(cx, &host, db, rows, size(px(1400.), px(700.)));
    click(cx, "add-view");
    type_text(cx, "Tagged");
    click(cx, item("Board"));
    assert!(host.edits().is_empty(), "nothing made yet");
    assert!(note(cx), "it asks");
    // Only what can group is offered.
    assert!(drawn(cx, item("Tags")) && drawn(cx, item("Follow up")));
    assert!(!drawn(cx, item("Role")) && !drawn(cx, item("Company")));
    click(cx, item("Tags"));
    let v = created(&last_batch(&host)[0]).clone();
    assert_eq!((v.kind, v.name.as_str()), (ViewType::Board, "Tagged"));
    assert_eq!(v.group_by.as_deref(), Some(TAGS));
    assert_eq!(shown(&table, cx).id, v.id);

    // Changing a list to a board asks the same.
    view_menu(&table, cx, FOLLOW_UP_VIEW);
    click(cx, item("Change type"));
    click(cx, item("Board"));
    assert!(drawn(cx, item("Owner")), "the group asked for");
    click(cx, item("Owner"));
    let v = changed(&last_batch(&host)[0]).clone();
    assert_eq!(v.kind, ViewType::Board);
    assert_eq!(v.group_by.as_deref(), Some(OWNER));
}

#[gpui::test]
fn a_board_with_nothing_to_group_by_is_explained(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (mut db, rows) = load("people");
    let f = db.database.as_mut().unwrap();
    f.properties.retain(|p| !p.kind.can_group());
    for v in &mut f.views {
        v.group_by = None;
    }
    let (_table, cx) = open_bar(cx, &host, db, rows, size(px(1400.), px(700.)));
    click(cx, "add-view");
    click(cx, item("Board"));
    assert!(note(cx), "why not");
    assert!(host.edits().is_empty());
}

#[gpui::test]
fn views_are_renamed_duplicated_retyped_and_deleted(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_people_bar(cx, &host);

    // Rename.
    view_menu(&table, cx, "All people");
    click(cx, item("Rename"));
    assert!(drawn(cx, "menu-field"));
    type_name(cx, "Everyone");
    keys(cx, "enter");
    let v = changed(&last_batch(&host)[0]).clone();
    assert_eq!((v.id.as_str(), v.name.as_str()), (ALL_PEOPLE, "Everyone"));
    assert!(drawn(cx, tab("Everyone")));
    // An empty name is refused.
    view_menu(&table, cx, "Everyone");
    click(cx, item("Rename"));
    type_name(cx, " ");
    let n = batches(&host);
    keys(cx, "enter");
    assert_eq!(batches(&host), n);
    assert!(note(cx));
    keys(cx, "escape");

    // Rename another view, from its tab's right-click, without showing it.
    view_menu(&table, cx, BY_STAGE);
    click(cx, item("Rename"));
    type_name(cx, "Stages");
    click(cx, item("Save"));
    assert_eq!(changed(&last_batch(&host)[0]).name, "Stages");
    assert_eq!(shown(&table, cx).id, ALL_PEOPLE, "still on the view shown");

    // Duplicate: a copy after it, with a new id, shown.
    let all = views(&table, cx);
    view_menu(&table, cx, "Everyone");
    click(cx, item("Duplicate"));
    let copy = created(&last_batch(&host)[0]).clone();
    assert_eq!(copy.name, "Everyone copy");
    assert_ne!(copy.id, ALL_PEOPLE);
    assert!(all[0].order < copy.order && copy.order < all[1].order);
    assert_eq!(
        (copy.kind.clone(), copy.sort.clone(), copy.columns.clone()),
        (
            all[0].kind.clone(),
            all[0].sort.clone(),
            all[0].columns.clone()
        )
    );
    assert_eq!(views(&table, cx)[1].id, copy.id, "placed after it");
    assert_eq!(shown(&table, cx).id, copy.id);

    // Change type: a list, then a board grouped by the first select.
    view_menu(&table, cx, "Everyone copy");
    click(cx, item("Change type"));
    click(cx, item("List"));
    let v = changed(&last_batch(&host)[0]).clone();
    assert_eq!(
        (v.id.as_str(), v.kind.clone()),
        (copy.id.as_str(), ViewType::List)
    );
    assert!(drawn(cx, "list"));
    view_menu(&table, cx, "Everyone copy");
    click(cx, item("Change type"));
    click(cx, item("Board"));
    let v = changed(&last_batch(&host)[0]).clone();
    assert_eq!(v.kind, ViewType::Board);
    assert_eq!(v.group_by.as_deref(), Some(STAGE));
    assert!(drawn(cx, "board"));

    // Delete: asked first (Cancel where it opens), then gone, its neighbour shown.
    view_menu(&table, cx, "Everyone copy");
    click(cx, item("Delete view"));
    assert!(drawn(cx, item("Cancel")));
    let n = batches(&host);
    click(cx, item("Cancel"));
    assert_eq!(batches(&host), n, "Cancel deletes nothing");
    view_menu(&table, cx, "Everyone copy");
    click(cx, item("Delete view"));
    click(cx, item("Delete view"));
    assert_eq!(
        last_batch(&host),
        vec![Edit::ViewDeleted {
            database_id: PEOPLE.into(),
            view_id: copy.id.clone()
        }]
    );
    assert!(!drawn(cx, tab("Everyone copy")));
    assert_eq!(shown(&table, cx).id, PIPELINE, "the next view is shown");
    assert_eq!(
        host.shown.borrow().last().map(String::as_str),
        Some(PIPELINE)
    );
}

#[gpui::test]
fn the_last_view_is_never_deleted(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (mut db, rows) = load("people");
    db.database
        .as_mut()
        .unwrap()
        .views
        .retain(|v| v.id == ALL_PEOPLE);
    let (table, cx) = open_bar(cx, &host, db, rows, size(px(1400.), px(700.)));
    view_menu(&table, cx, "All people");
    assert!(!drawn(cx, item("Move left")) && !drawn(cx, item("Move right")));
    click(cx, item("Delete view"));
    assert!(note(cx), "refused, and said why");
    assert!(host.edits().is_empty());
    assert_eq!(views(&table, cx).len(), 1);
}

#[gpui::test]
fn views_move_by_their_menu_and_by_dragging_tabs(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_people_bar(cx, &host);
    let before = views(&table, cx);
    view_menu(&table, cx, "All people");
    assert!(!drawn(cx, item("Move left")));
    click(cx, item("Move right"));
    let batch = last_batch(&host);
    assert_eq!(batch.len(), 1, "one key changes");
    let v = changed(&batch[0]);
    assert_eq!(v.id, ALL_PEOPLE);
    assert!(before[1].order < v.order && v.order < before[2].order);
    let names: Vec<String> = views(&table, cx).into_iter().map(|v| v.name).collect();
    assert_eq!(names, ["Pipeline", "All people", FOLLOW_UP_VIEW, BY_STAGE]);
    assert!(rect(cx, tab("Pipeline")).left() < rect(cx, tab("All people")).left());
    // The menu stays open on its new place: back again.
    click(cx, item("Move left"));
    assert_eq!(views(&table, cx)[0].id, ALL_PEOPLE);
    keys(cx, "escape");

    // A tab dragged onto another takes its place.
    let from = rect(cx, tab(BY_STAGE)).center();
    let to = rect(cx, tab("Pipeline")).center();
    drag(cx, from, to);
    let names: Vec<String> = views(&table, cx).into_iter().map(|v| v.name).collect();
    assert_eq!(names, ["All people", BY_STAGE, "Pipeline", FOLLOW_UP_VIEW]);
    assert_eq!(changed(&last_batch(&host)[0]).name, BY_STAGE);

    // Keys that can't be put between renumber every view, in one batch.
    let (mut db, rows) = load("people");
    for v in &mut db.database.as_mut().unwrap().views {
        v.order = "a0".into();
    }
    table.update(cx, |t, cx| t.set_docs(db, rows, cx));
    draw(cx);
    host.clear();
    let first = views(&table, cx)[0].name.clone();
    view_menu(&table, cx, &first);
    click(cx, item("Move right"));
    let batch = last_batch(&host);
    assert!(batch.len() > 1, "{batch:?}");
    let orders: Vec<String> = views(&table, cx).into_iter().map(|v| v.order).collect();
    assert!(orders.windows(2).all(|w| w[0] < w[1]), "{orders:?}");
    assert_eq!(views(&table, cx)[1].name, first);
}

#[gpui::test]
fn the_settings_group_and_bucket_by_date(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_people_bar(cx, &host);
    click(cx, "view-settings");
    assert!(drawn(cx, item("Group by")));
    click(cx, item("Group by"));
    // A table can be ungrouped; only what can group is offered.
    assert!(drawn(cx, item("None")) && drawn(cx, item("Stage")));
    assert!(!drawn(cx, item("Role")));
    click(cx, item("Stage"));
    let v = changed(&last_batch(&host)[0]).clone();
    assert_eq!(v.group_by.as_deref(), Some(STAGE));
    assert!(table.update(cx, |t, _| !t.groups().is_empty()));
    // Back on the settings, no date grouping for a select.
    assert!(drawn(cx, item("Group by")) && !drawn(cx, item("Date grouping")));

    // By a date: its buckets come next.
    click(cx, item("Group by"));
    click(cx, item("Last contacted"));
    assert_eq!(
        changed(&last_batch(&host)[0]).group_by.as_deref(),
        Some(LAST_CONTACTED)
    );
    assert!(drawn(cx, item("Month")) && drawn(cx, item("Relative")));
    click(cx, item("Month"));
    let v = changed(&last_batch(&host)[0]).clone();
    assert_eq!(v.group_date, Some(DateBucket::Month));
    assert!(drawn(cx, item("Date grouping")));

    // Ungrouped again: the date bucket goes too.
    click(cx, item("Group by"));
    click(cx, item("None"));
    let v = changed(&last_batch(&host)[0]).clone();
    assert_eq!((v.group_by, v.group_date), (None, None));
    keys(cx, "escape");

    // A board can't be ungrouped.
    click(cx, tab("Pipeline"));
    click(cx, "view-settings");
    click(cx, item("Group by"));
    assert!(!drawn(cx, item("None")));
    click(cx, item("Tags"));
    assert_eq!(
        changed(&last_batch(&host)[0]).group_by.as_deref(),
        Some(TAGS)
    );
}

#[gpui::test]
fn card_and_line_properties_are_shown_hidden_and_ordered(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (_table, cx) = open_people_bar(cx, &host);
    click(cx, tab("Pipeline"));
    click(cx, "view-settings");
    click(cx, item("Card properties"));
    // Shown first, in order, then the rest (never the title).
    for name in ["Company", "Last contacted", "Owner", "Tags", "Role"] {
        assert!(drawn(cx, item(name)), "{name}");
    }
    assert!(!drawn(cx, item("Name")));
    click(cx, item("Tags"));
    let v = changed(&last_batch(&host)[0]).clone();
    assert_eq!(v.card_properties, [COMPANY, LAST_CONTACTED, OWNER, TAGS]);
    // A shown one's page moves and hides it.
    click(cx, item("Company"));
    click(cx, item("Move down"));
    let v = changed(&last_batch(&host)[0]).clone();
    assert_eq!(v.card_properties, [LAST_CONTACTED, COMPANY, OWNER, TAGS]);
    click(cx, item("Move up"));
    assert_eq!(
        changed(&last_batch(&host)[0]).card_properties,
        [COMPANY, LAST_CONTACTED, OWNER, TAGS]
    );
    click(cx, item("Hide"));
    assert_eq!(
        changed(&last_batch(&host)[0]).card_properties,
        [LAST_CONTACTED, OWNER, TAGS]
    );
    keys(cx, "escape");

    // A list's lines: the same, called Properties.
    click(cx, tab(FOLLOW_UP_VIEW));
    click(cx, "view-settings");
    assert!(!drawn(cx, item("Card properties")));
    click(cx, item("Properties"));
    click(cx, item("Role"));
    assert_eq!(
        changed(&last_batch(&host)[0]).card_properties,
        [COMPANY, LAST_CONTACTED, ROLE]
    );
    keys(cx, "escape");

    // A table's are its columns: the Properties menu its headers open.
    click(cx, tab("All people"));
    click(cx, "view-settings");
    click(cx, item("Properties"));
    assert!(drawn(cx, item("Role")) && drawn(cx, item("Stage")));
    click(cx, item("Role"));
    let v = changed(&last_batch(&host)[0]).clone();
    assert!(v.columns.iter().any(|c| c.property_id == ROLE && c.hidden));
}

#[gpui::test]
fn sorts_and_filters_are_summed_up_in_the_bar(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_people_bar(cx, &host);
    let mut v = shown(&table, cx);
    v.sort = vec![Sort::ascending(NAME), Sort::descending(ROLE)];
    v.filter = Some(Filter::condition(STAGE, FilterOp::Is, LEAD.into()));
    let (mut db, rows) = load("people");
    let f = db.database.as_mut().unwrap();
    let id = v.id.clone();
    *f.views.iter_mut().find(|x| x.id == id).unwrap() = v;
    table.update(cx, |t, cx| t.set_docs(db, rows, cx));
    draw(cx);
    assert!(drawn(cx, "view-sort") && drawn(cx, "view-filter"));
    click(cx, "view-sort");
    click(cx, item("Remove sorts"));
    assert!(changed(&last_batch(&host)[0]).sort.is_empty());
    click(cx, item("Clear filter"));
    assert!(changed(&last_batch(&host)[0]).filter.is_none());
    keys(cx, "escape");
    assert!(!drawn(cx, "view-sort") && !drawn(cx, "view-filter"));
}

#[gpui::test]
fn limits_are_refused_with_a_note(cx: &mut TestAppContext) {
    let host = TestHost::new();
    host.limits.set(Limits {
        views: 4,
        name_chars: 8,
        sorts: 1,
        ..Limits::default()
    });
    let (table, cx) = open_people_bar(cx, &host);
    // Views: People has its four.
    click(cx, "add-view");
    assert!(note(cx), "full");
    click(cx, item("Table"));
    keys(cx, "enter");
    assert!(host.edits().is_empty());
    keys(cx, "escape");
    view_menu(&table, cx, "All people");
    click(cx, item("Duplicate"));
    assert!(note(cx));
    assert!(host.edits().is_empty());
    keys(cx, "escape");

    // Names stop at the limit, saying so.
    view_menu(&table, cx, "All people");
    click(cx, item("Rename"));
    type_name(cx, "Everybody we know");
    assert!(note(cx), "at the limit");
    keys(cx, "enter");
    assert_eq!(changed(&last_batch(&host)[0]).name, "Everybod");

    // Sorts: a second one added is refused in the column's menu.
    let at = rect(cx, "header:Name").center();
    cx.simulate_click(at, Modifiers::none());
    draw(cx);
    click(cx, item("Sort ascending"));
    let at = rect(cx, "header:Role").center();
    cx.simulate_click(at, Modifiers::none());
    draw(cx);
    let n = batches(&host);
    click_with(cx, item("Sort ascending"), Modifiers::shift());
    assert_eq!(batches(&host), n, "no second sort");
    assert!(note(cx));
    assert_eq!(shown(&table, cx).sort.len(), 1);
}

#[gpui::test]
fn the_bar_works_from_the_keyboard(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_people_bar(cx, &host);
    let table_focused = |table: &Entity<TableView>, cx: &mut VisualTestContext| {
        cx.update(|window, cx| table.read(cx).focus_handle(cx).is_focused(window))
    };
    // Up from the top line goes to the bar; Right and Enter show the next view.
    keys(cx, "down");
    assert_eq!(
        table.read_with(cx, |t, _| t.focused_cell().unwrap().item),
        0
    );
    keys(cx, "up");
    assert!(!table_focused(&table, cx), "the bar has the keyboard");
    keys(cx, "right");
    enter_up(cx);
    assert_eq!(shown(&table, cx).id, PIPELINE);
    assert!(!table_focused(&table, cx), "still on the tab");
    // ⌥↓ opens its menu; Escape closes it, back on the tab; Down goes to the board.
    keys(cx, "alt-down");
    assert!(drawn(cx, item("Rename")));
    keys(cx, "down");
    keys(cx, "enter");
    assert!(drawn(cx, item("Board")), "Change type, picked by the keys");
    keys(cx, "escape");
    assert!(!table.read_with(cx, |t, _| t.has_popover()));
    keys(cx, "left");
    enter_up(cx);
    assert_eq!(shown(&table, cx).id, ALL_PEOPLE);
    keys(cx, "down");
    assert!(table_focused(&table, cx), "back on the view");

    // The + menu: typing names it, Up and Down pick its type, Enter makes it.
    click(cx, "add-view");
    type_text(cx, "Kanban");
    keys(cx, "down");
    keys(cx, "enter");
    let v = created(&last_batch(&host)[0]).clone();
    assert_eq!((v.name.as_str(), v.kind), ("Kanban", ViewType::Board));
    assert!(table_focused(&table, cx), "the new view has the keyboard");
}

#[gpui::test]
fn the_bar_and_its_menus_are_44_points_on_touch(cx: &mut TestAppContext) {
    let host = TestHost::touch();
    let (table, cx) = open_people_bar(cx, &host);
    let big = |cx: &mut VisualTestContext, what: String| {
        let b = rect(cx, what.clone());
        assert!(
            b.size.height >= px(44.) && b.size.width >= px(44.),
            "{what} is {:?}",
            b.size
        );
    };
    for what in [
        tab("All people"),
        tab("Pipeline"),
        "add-view".into(),
        "view-settings".into(),
    ] {
        big(cx, what);
    }
    // A tap on the shown tab opens its menu, its entries 44 points tall.
    let at = rect(cx, tab("All people")).center();
    tap(cx, at);
    assert!(table.read_with(cx, |t, _| t.has_popover()));
    for label in [
        "Rename",
        "Change type",
        "View settings",
        "Duplicate",
        "Delete view",
    ] {
        assert!(rect(cx, item(label)).size.height >= px(44.), "{label}");
    }
    keys(cx, "escape");
    click(cx, "add-view");
    assert!(rect(cx, item("Board")).size.height >= px(44.));
    assert!(rect(cx, "menu-field").size.height >= px(44.));
}
