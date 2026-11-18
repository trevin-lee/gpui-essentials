//! Changing properties from the table, as a person does it: the + after the last column adds
//! one of each type, and a column's menu renames, retypes, edits options, a number's format, a
//! date's time, a relation's database and other side and a person's kinds, and deletes after a
//! confirm, each checked by the edits it hands the host; the title refused, and every limit.

#![cfg(all(target_os = "macos", feature = "gpui"))]

mod common;

use common::ids::*;
use common::*;
use gpui::*;
use gpui_tables::view::TableView;
use gpui_tables::*;
use gpui_tables::{Edit, Value};
// The built-in #[test], not GPUI's, which gpui::* brings in.
use std::prelude::v1::test;

const INTERACTIONS: &str = "8d116ece-1738-47d9-bd9c-172411e20b8f";

/// Opens a column's menu by pressing its header.
fn menu(cx: &mut VisualTestContext, name: &str) {
    let at = rect(cx, format!("header:{name}")).center();
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    draw(cx);
    assert!(drawn(cx, "menu"), "{name}'s menu opened");
}

fn item(label: &str) -> String {
    format!("menu-item-{label}")
}

/// The edits of the last batch.
fn last_batch(host: &TestHost) -> Vec<Edit> {
    match host.events.borrow().last() {
        Some(TableEvent::Edits { edits }) => edits.clone(),
        other => panic!("no batch: {other:?}"),
    }
}

/// The property in the last batch's first edit.
fn first_property(host: &TestHost) -> Property {
    property_of(&last_batch(host)[0]).1.clone()
}

fn batches(host: &TestHost) -> usize {
    host.events.borrow().len()
}

/// The property in a `propertyCreated` or `propertyChanged`, and its database.
fn property_of(e: &Edit) -> (&str, &Property) {
    match e {
        Edit::PropertyCreated {
            database_id,
            property,
        }
        | Edit::PropertyChanged {
            database_id,
            property,
        } => (database_id, property),
        other => panic!("not a property's edit: {other:?}"),
    }
}

fn property(table: &Entity<TableView>, cx: &mut VisualTestContext, id: &str) -> Property {
    table.read_with(cx, |t, _| {
        t.database()
            .database
            .as_ref()
            .unwrap()
            .property(id)
            .cloned()
            .unwrap_or_else(|| panic!("no property {id}"))
    })
}

fn properties(table: &Entity<TableView>, cx: &mut VisualTestContext) -> Vec<Property> {
    table.read_with(cx, |t, _| {
        t.database().database.as_ref().unwrap().properties.clone()
    })
}

/// Types into the open menu's field, replacing what's there.
fn type_name(cx: &mut VisualTestContext, text: &str) {
    keys(cx, "cmd-a");
    type_text(cx, text);
}

#[gpui::test]
fn the_plus_adds_a_property_of_each_type(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_people(cx, &host);
    for kind in PropertyType::ADDABLE {
        if *kind == PropertyType::Relation {
            continue;
        }
        let before = properties(&table, cx);
        click(cx, "add-property");
        assert!(drawn(cx, "menu-picker"), "the name's picker is drawn");
        let name = format!("New {}", kind.label());
        type_text(cx, &name);
        click(cx, item(kind.label()));
        let batch = last_batch(&host);
        assert_eq!(batch.len(), 2, "{kind}: {batch:?}");
        let (db, p) = property_of(&batch[0]);
        assert!(matches!(batch[0], Edit::PropertyCreated { .. }));
        assert_eq!(db, PEOPLE);
        assert_eq!(p.name, name);
        assert_eq!(p.kind, *kind);
        assert_eq!(p.config, PropertyConfig::default_for(kind));
        assert!(
            before.iter().all(|q| q.id != p.id && q.order < p.order),
            "{kind}: a new id, ordered after the others"
        );
        // The column added to the view shown, last, in the same batch.
        let Edit::ViewChanged { view, database_id } = &batch[1] else {
            panic!("not a viewChanged: {:?}", batch[1]);
        };
        assert_eq!(database_id, PEOPLE);
        assert_eq!(view.id, ALL_PEOPLE);
        let last = view.columns_in_order().last().cloned().unwrap();
        assert_eq!(last.property_id, p.id);
        assert!(!last.hidden);
        assert!(
            !table.read_with(cx, |t, _| t.has_popover()),
            "the menu closed"
        );
        assert!(
            drawn(cx, format!("header:{name}")),
            "{name}'s column is drawn"
        );
    }
    // A config for every type: a number plain, options empty, a date without its time, a
    // person allowing both kinds.
    let number = PropertyConfig::default_for(&PropertyType::Number);
    assert_eq!(number.format, Some(NumberFormat::Plain));
    assert_eq!(
        PropertyConfig::default_for(&PropertyType::Select).options,
        Some(Vec::new())
    );
    assert_eq!(
        PropertyConfig::default_for(&PropertyType::Date).include_time,
        Some(false)
    );
    assert_eq!(
        PropertyConfig::default_for(&PropertyType::Person).kinds,
        Some(vec![PersonKind::Owner, PersonKind::Agent])
    );
}

#[gpui::test]
fn the_add_menu_works_from_the_keyboard_and_names_what_it_isnt_given(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_people(cx, &host);
    // From a cell: ⌥↓ opens its column's menu, whose Add a property opens the + menu.
    table.update(cx, |t, cx| t.focus_cell(IMANI, ROLE, cx));
    keys(cx, "alt-down");
    click(cx, item("Add a property"));
    type_text(cx, "Deal size");
    // Up and Down move through the types while typing stays in the field.
    keys(cx, "down down up");
    assert!(drawn(cx, "menu-picker"));
    keys(cx, "enter");
    let p = first_property(&host);
    assert_eq!(p.name, "Deal size");
    assert_eq!(p.kind, PropertyType::Number);
    // Nothing typed: the type's name, numbered past the names taken.
    click(cx, "add-property");
    keys(cx, "enter");
    let p = first_property(&host);
    assert_eq!((p.name.as_str(), &p.kind), ("Text", &PropertyType::Text));
    click(cx, "add-property");
    keys(cx, "enter");
    let p = first_property(&host);
    assert_eq!(p.name, "Text 2");
    // A name taken (case aside) is refused with a note, and nothing is emitted.
    let n = batches(&host);
    click(cx, "add-property");
    type_text(cx, "stage");
    keys(cx, "enter");
    assert_eq!(batches(&host), n);
    assert!(drawn(cx, "menu-note"));
    // Escape closes it, and the table has the keyboard again.
    keys(cx, "escape");
    assert!(!table.read_with(cx, |t, _| t.has_popover()));
    keys(cx, "escape");
}

#[gpui::test]
fn a_property_is_renamed_in_its_menu(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_people(cx, &host);
    menu(cx, "Role");
    click(cx, item("Rename"));
    assert!(drawn(cx, "menu-field"));
    type_name(cx, "Title at work");
    keys(cx, "enter");
    let batch = last_batch(&host);
    assert_eq!(batch.len(), 1);
    assert!(matches!(batch[0], Edit::PropertyChanged { .. }));
    let (db, p) = property_of(&batch[0]);
    assert_eq!(db, PEOPLE);
    assert_eq!((p.id.as_str(), p.name.as_str()), (ROLE, "Title at work"));
    assert_eq!(p.kind, PropertyType::Text);
    assert!(drawn(cx, "header:Title at work"));
    // Empty, or another's name: refused with a note.
    let n = batches(&host);
    menu(cx, "Title at work");
    click(cx, item("Rename"));
    type_name(cx, "  ");
    keys(cx, "enter");
    assert!(drawn(cx, "menu-note"));
    type_name(cx, "EMAIL");
    keys(cx, "enter");
    assert!(drawn(cx, "menu-note"));
    assert_eq!(batches(&host), n);
    // Save, for a pointer or a finger, does what Enter does.
    type_name(cx, "Role");
    click(cx, item("Save"));
    assert_eq!(property(&table, cx, ROLE).name, "Role");
    // The title is renamed too.
    menu(cx, "Name");
    click(cx, item("Rename"));
    type_name(cx, "Person");
    keys(cx, "enter");
    assert_eq!(property(&table, cx, NAME).name, "Person");
}

#[gpui::test]
fn changing_type_keeps_rows_and_reads_what_fits(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_people(cx, &host);
    let tags_before = table.update(cx, |t, _| t.cell_words(IMANI, TAGS)).unwrap();
    assert!(!tags_before.is_empty());
    // A multi-select to a select: its options kept, a row's list read as its first option.
    menu(cx, "Tags");
    click(cx, item("Change type"));
    click(cx, item("Select"));
    let batch = last_batch(&host);
    assert_eq!(batch.len(), 1, "no row is rewritten: {batch:?}");
    let (_, p) = property_of(&batch[0]);
    assert_eq!(p.kind, PropertyType::Select);
    assert!(!p.options().is_empty());
    let first = tags_before.split(", ").next().unwrap().to_owned();
    assert_eq!(
        table.update(cx, |t, _| t.cell_words(IMANI, TAGS)).unwrap(),
        first
    );
    // And back: the list whole again.
    menu(cx, "Tags");
    click(cx, item("Change type"));
    click(cx, item("Multi-select"));
    assert_eq!(
        table.update(cx, |t, _| t.cell_words(IMANI, TAGS)).unwrap(),
        tags_before
    );
    // A select to text: its options kept, a row showing its option's name.
    let stage = table.update(cx, |t, _| t.cell_words(IMANI, STAGE)).unwrap();
    menu(cx, "Stage");
    click(cx, item("Change type"));
    click(cx, item("Text"));
    let p = first_property(&host);
    assert_eq!(p.kind, PropertyType::Text);
    assert_eq!(
        table.update(cx, |t, _| t.cell_words(IMANI, STAGE)).unwrap(),
        stage
    );
    // Text to a number: the config a number's, text that isn't one shown empty and kept.
    menu(cx, "Role");
    click(cx, item("Change type"));
    click(cx, item("Number"));
    let p = first_property(&host);
    assert_eq!(p.config.format, Some(NumberFormat::Plain));
    assert_eq!(
        table.update(cx, |t, _| t.cell_words(IMANI, ROLE)).unwrap(),
        ""
    );
    let kept = table.read_with(cx, |t, _| {
        t.rows()
            .iter()
            .find(|r| r.id == IMANI)
            .and_then(|r| r.value(ROLE).cloned())
    });
    assert!(
        matches!(kept, Some(Value::Text(_))),
        "the text stays: {kept:?}"
    );
    // A two-way relation retyped: its other side goes, in the same batch.
    menu(cx, "Company");
    click(cx, item("Change type"));
    click(cx, item("Text"));
    let batch = last_batch(&host);
    assert_eq!(batch.len(), 2, "{batch:?}");
    assert_eq!(
        batch[1],
        Edit::PropertyDeleted {
            database_id: COMPANIES.into(),
            property_id: "5790f82e-c1d3-4cff-aa3a-f4d46b0a18e8".into(),
        }
    );
    // To a relation: to this database, one-way, and its page opens to pick another.
    menu(cx, "Email");
    click(cx, item("Change type"));
    click(cx, item("Relation"));
    let p = first_property(&host);
    assert_eq!(p.config.database_id.as_deref(), Some(PEOPLE));
    assert_eq!(p.config.two_way, Some(false));
    assert!(drawn(cx, item("Companies")));
    click(cx, item("Companies"));
    let p = first_property(&host);
    assert_eq!(p.config.database_id.as_deref(), Some(COMPANIES));
}

#[gpui::test]
fn options_are_renamed_recoloured_reordered_added_and_deleted(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_people(cx, &host);
    let names = |table: &Entity<TableView>, cx: &mut VisualTestContext| -> Vec<String> {
        property(table, cx, STAGE)
            .options()
            .iter()
            .map(|o| o.name.clone())
            .collect()
    };
    let start = names(&table, cx);
    assert_eq!(start[0], "Lead");
    // The options open beside the column's menu, each opening its own beside them.
    menu(cx, "Stage");
    click(cx, item("Edit options"));
    assert!(drawn(cx, "pill:Lead"), "options drawn as their pills");
    assert!(drawn(cx, "menu-submenu-Edit options"));
    click(cx, item("Lead"));
    assert!(drawn(cx, "menu-submenu-Lead"));
    // Recoloured: one of the ten colours.
    for c in Color::ALL {
        assert!(drawn(cx, item(&color_name(c))), "{c} offered");
    }
    click(cx, item("Red"));
    let p = first_property(&host);
    assert_eq!(p.option(LEAD).unwrap().color, Color::Red);
    // Its submenu has no Move entries: the options are reordered in place. Alt-Down and
    // Alt-Up move the highlighted one, each a propertyChanged with the options' new order.
    assert!(!drawn(cx, item("Move down")) && !drawn(cx, item("Move up")));
    keys(cx, "left");
    keys(cx, "alt-down");
    let after = names(&table, cx);
    assert_eq!(
        (after[0].as_str(), after[1].as_str()),
        (start[1].as_str(), "Lead")
    );
    assert_eq!(first_property(&host).options()[1].name, "Lead");
    keys(cx, "alt-up");
    assert_eq!(names(&table, cx), start);
    // Dragged: dropped low on the next option's row, it goes after it.
    let from = rect(cx, item("Lead")).center();
    let next = rect(cx, item(&start[1]));
    let to = point(next.center().x, next.top() + next.size.height * 0.75);
    drag(cx, from, to);
    let after = names(&table, cx);
    assert_eq!(
        (after[0].as_str(), after[1].as_str()),
        (start[1].as_str(), "Lead")
    );
    assert_eq!(first_property(&host).options()[1].name, "Lead");
    // A click still opens its submenu.
    click(cx, item("Lead"));
    assert!(drawn(cx, "menu-submenu-Lead"));
    // Renamed, in a step of its own, then back on the column's menu where it came from: the
    // options open, the option's submenu open, the keyboard on Rename option.
    click(cx, item("Rename option"));
    type_name(cx, "Cold lead");
    keys(cx, "enter");
    assert_eq!(
        property(&table, cx, STAGE).option(LEAD).unwrap().name,
        "Cold lead"
    );
    assert!(drawn(cx, item("Edit options")), "back on the column's menu");
    assert!(drawn(cx, "menu-submenu-Edit options"));
    assert!(drawn(cx, "menu-submenu-Cold lead"));
    keys(cx, "enter");
    assert!(drawn(cx, "menu-field"), "Enter is Rename option's again");
    click(cx, item("Back"));
    // Another's name is refused.
    let n = batches(&host);
    click(cx, item("Edit options"));
    click(cx, item("Cold lead"));
    click(cx, item("Rename option"));
    type_name(cx, "warm");
    keys(cx, "enter");
    assert!(drawn(cx, "menu-note"));
    assert_eq!(batches(&host), n);
    click(cx, item("Back"));
    // Deleted: rows still name it, and show empty. Its submenu goes, the keyboard back on the
    // options.
    click(cx, item("Edit options"));
    click(cx, item("Cold lead"));
    click(cx, item("Delete option"));
    assert!(!drawn(cx, "menu-submenu-Cold lead"));
    assert!(drawn(cx, "menu-submenu-Edit options"));
    // On the option now in its place; the arrows move and open from there.
    keys(cx, "right");
    assert!(
        drawn(cx, format!("menu-submenu-{}", start[2])),
        "the keys open the next option's submenu"
    );
    keys(cx, "left up right");
    assert!(
        drawn(cx, format!("menu-submenu-{}", start[1])),
        "and the one before it"
    );
    keys(cx, "left");
    let batch = last_batch(&host);
    assert_eq!(batch.len(), 1, "no row is rewritten: {batch:?}");
    assert!(property(&table, cx, STAGE).option(LEAD).is_none());
    let lead_rows = table.read_with(cx, |t, _| {
        t.rows()
            .iter()
            .filter(|r| r.value(STAGE) == Some(&Value::text(LEAD)))
            .map(|r| r.id.clone())
            .collect::<Vec<_>>()
    });
    assert!(!lead_rows.is_empty());
    for r in &lead_rows {
        assert_eq!(table.update(cx, |t, _| t.cell_words(r, STAGE)).unwrap(), "");
    }
    // Added, after the others, then back on Add an option in the options.
    click(cx, item("Add an option"));
    type_text(cx, "Churned");
    keys(cx, "enter");
    let p = property(&table, cx, STAGE);
    assert_eq!(p.options().last().unwrap().name, "Churned");
    assert_eq!(p.options().last().unwrap().color, Color::Default);
    assert!(drawn(cx, "pill:Churned"), "in the options, open again");
    keys(cx, "enter");
    assert!(drawn(cx, "menu-field"), "Enter is Add an option's again");
}

fn color_name(c: &Color) -> String {
    let s = c.as_str();
    s[..1].to_uppercase() + &s[1..]
}

#[gpui::test]
fn a_numbers_format_and_a_dates_time(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_people(cx, &host);
    menu(cx, "Warmth");
    click(cx, item("Number format"));
    click(cx, item("Percent"));
    assert_eq!(
        property(&table, cx, WARMTH).config.format,
        Some(NumberFormat::Percent)
    );
    click(cx, item("Currency"));
    let p = property(&table, cx, WARMTH);
    assert_eq!(p.config.format, Some(NumberFormat::Currency));
    assert_eq!(p.config.currency.as_deref(), Some("USD"));
    // The currency in a submenu of its own.
    click(cx, item("Currency code"));
    click(cx, item("EUR"));
    click(cx, item("1.00"));
    let p = property(&table, cx, WARMTH);
    assert_eq!(p.config.currency.as_deref(), Some("EUR"));
    assert_eq!(p.config.precision, Some(2));
    let words = table
        .update(cx, |t, _| t.cell_words(IMANI, WARMTH))
        .unwrap();
    assert!(words.starts_with('€') && words.ends_with(".00"), "{words}");
    // Number picked while Currency code's submenu has the keyboard: the submenu goes with its
    // entry, and the keyboard is back in the format's.
    click(cx, item("Currency code"));
    assert!(drawn(cx, "menu-submenu-Currency code"));
    click(cx, item("Number"));
    let p = property(&table, cx, WARMTH);
    assert_eq!(p.config.format, Some(NumberFormat::Plain));
    assert_eq!(p.config.currency, None, "a currency only with its format");
    assert!(!drawn(cx, "menu-submenu-Currency code"));
    keys(cx, "down up");
    // Escape closes the submenu, then the menu.
    keys(cx, "escape");
    assert!(table.read_with(cx, |t, _| t.has_popover()));
    keys(cx, "escape");
    assert!(!table.read_with(cx, |t, _| t.has_popover()));
    // A date's time: what new values get, the menu staying open.
    menu(cx, "Last contacted");
    click(cx, item("Include time"));
    let p = first_property(&host);
    assert_eq!(p.config.include_time, Some(true));
    assert!(table.read_with(cx, |t, _| t.has_popover()));
    keys(cx, "escape");
    // An empty date picked now holds a time.
    let empty = table.read_with(cx, |t, _| {
        t.rows()
            .iter()
            .find(|r| r.value(LAST_CONTACTED).is_none())
            .map(|r| r.id.clone())
            .unwrap()
    });
    cx.update(|window, cx| table.update(cx, |t, cx| t.edit(&empty, LAST_CONTACTED, window, cx)));
    draw(cx);
    click(cx, "day-today");
    let Edit::RowValueChanged {
        value: Some(Value::Date(d)),
        ..
    } = host.last_edit()
    else {
        panic!("a date");
    };
    assert!(d.include_time);
}

#[gpui::test]
fn a_person_property_allows_the_owner_agents_or_both(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_people(cx, &host);
    menu(cx, "Owner");
    click(cx, item("People"));
    click(cx, item("Agents"));
    assert_eq!(
        property(&table, cx, OWNER).config.kinds,
        Some(vec![PersonKind::Owner])
    );
    // Not neither.
    let n = batches(&host);
    click(cx, item("Owner"));
    assert_eq!(batches(&host), n);
    assert!(drawn(cx, "menu-note"));
    click(cx, item("Agents"));
    assert_eq!(
        property(&table, cx, OWNER).config.kinds,
        Some(vec![PersonKind::Owner, PersonKind::Agent])
    );
}

#[gpui::test]
fn a_two_way_relation_is_made_in_both_databases_at_once(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_people(cx, &host);
    click(cx, "add-property");
    type_text(cx, "Investors");
    click(cx, item("Relation"));
    // Back (from the keyboard: the pointer would highlight what it's over): the + menu again,
    // on Relation, the name as typed; Enter goes on as before.
    keys(cx, "end enter");
    assert!(drawn(cx, "menu-picker"));
    keys(cx, "enter");
    // The other database first, checked, and whether it's two-way.
    assert!(drawn(cx, item("Companies")));
    assert!(drawn(cx, item("People")), "this database too");
    click(cx, item("Show on Companies"));
    click(cx, item("Add relation"));
    let batch = last_batch(&host);
    assert_eq!(batch.len(), 3, "{batch:?}");
    let (db, a) = property_of(&batch[0]);
    assert_eq!(db, PEOPLE);
    assert!(matches!(batch[0], Edit::PropertyCreated { .. }));
    let (other, b) = property_of(&batch[1]);
    assert!(matches!(batch[1], Edit::PropertyCreated { .. }));
    assert_eq!(other, COMPANIES);
    assert_eq!(a.name, "Investors");
    assert_eq!(a.kind, PropertyType::Relation);
    assert_eq!(b.kind, PropertyType::Relation);
    let (ta, tb) = (a.relation().unwrap(), b.relation().unwrap());
    assert_eq!((ta.database_id, ta.two_way), (COMPANIES, true));
    assert_eq!((tb.database_id, tb.two_way), (PEOPLE, true));
    assert_eq!(ta.back_property_id, Some(b.id.as_str()));
    assert_eq!(tb.back_property_id, Some(a.id.as_str()));
    // Named for this database, unlike Companies' own People, and after its properties.
    assert_eq!(b.name, "People 2");
    assert!(
        host.databases.borrow()[0]
            .properties
            .iter()
            .all(|p| p.order < b.order)
    );
    assert!(matches!(batch[2], Edit::ViewChanged { .. }));
    // The pair checks out.
    let mut companies = load("companies").0;
    companies
        .database
        .as_mut()
        .unwrap()
        .properties
        .push(b.clone());
    let people = table.read_with(cx, |t, _| t.database().clone());
    let (pf, cf) = (
        people.database.as_ref().unwrap(),
        companies.database.as_ref().unwrap(),
    );
    assert!(engine::relation::check_two_way(PEOPLE, pf, COMPANIES, cf).is_empty());
    assert!(engine::relation::check_two_way(COMPANIES, cf, PEOPLE, pf).is_empty());

    // One-way: a single propertyCreated, and to this database when it's picked.
    click(cx, "add-property");
    type_text(cx, "Mentors");
    click(cx, item("Relation"));
    click(cx, item("People"));
    click(cx, item("Add relation"));
    let batch = last_batch(&host);
    assert_eq!(batch.len(), 2, "{batch:?}");
    let (_, p) = property_of(&batch[0]);
    let t = p.relation().unwrap();
    assert_eq!((t.database_id, t.two_way), (PEOPLE, false));
}

#[gpui::test]
fn turning_a_relation_two_way_writes_its_rows_on_the_other_side(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (mut db, rows) = load("people");
    // Company made one-way.
    {
        let f = db.database.as_mut().unwrap();
        let p = f.properties.iter_mut().find(|p| p.id == COMPANY).unwrap();
        p.config.two_way = Some(false);
        p.config.back_property_id = None;
    }
    let related: Vec<(String, Vec<String>)> = rows
        .iter()
        .filter_map(|r| {
            r.value(COMPANY)
                .and_then(Value::as_ids)
                .filter(|ids| !ids.is_empty())
                .map(|ids| (r.id.clone(), ids.to_vec()))
        })
        .collect();
    assert!(!related.is_empty());
    let (table, cx) = open_with(cx, &host, db, rows, None, size(px(2600.), px(700.)));
    menu(cx, "Company");
    click(cx, item("Relation"));
    click(cx, item("Show on Companies"));
    let batch = last_batch(&host);
    let (_, p) = property_of(&batch[0]);
    assert!(matches!(batch[0], Edit::PropertyChanged { .. }));
    let (other, back) = property_of(&batch[1]);
    assert!(matches!(batch[1], Edit::PropertyCreated { .. }));
    assert_eq!(other, COMPANIES);
    assert_eq!(
        p.relation().unwrap().back_property_id,
        Some(back.id.as_str())
    );
    assert_eq!(back.relation().unwrap().back_property_id, Some(COMPANY));
    // A relationChanged for every row it lists, which the host's back_edits writes.
    let changed: Vec<(String, Vec<String>)> = batch[2..]
        .iter()
        .map(|e| match e {
            Edit::RelationChanged {
                row_id,
                property_id,
                database_id,
                back_property_id,
                added,
                removed,
            } => {
                assert_eq!(property_id, COMPANY);
                assert_eq!(database_id, COMPANIES);
                assert_eq!(*back_property_id, back.id);
                assert!(removed.is_empty());
                (row_id.clone(), added.clone())
            }
            other => panic!("not a relationChanged: {other:?}"),
        })
        .collect();
    assert_eq!(changed.len(), related.len());
    for r in &related {
        assert!(changed.contains(r), "{r:?}");
    }
    // And off again: its other side deleted.
    click(cx, item("Show on Companies"));
    let batch = last_batch(&host);
    assert_eq!(
        batch[1],
        Edit::PropertyDeleted {
            database_id: COMPANIES.into(),
            property_id: back.id.clone(),
        }
    );
    assert!(!property(&table, cx, COMPANY).relation().unwrap().two_way);
}

#[gpui::test]
fn deleting_asks_first_and_the_title_is_refused(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_people(cx, &host);
    menu(cx, "Role");
    click(cx, item("Delete property"));
    // Cancel first, so Enter does nothing harmful.
    let n = batches(&host);
    keys(cx, "enter");
    assert_eq!(batches(&host), n);
    assert!(
        drawn(cx, item("Delete property")),
        "back on the column's menu"
    );
    click(cx, item("Delete property"));
    click(cx, item("Delete property"));
    assert_eq!(
        last_batch(&host),
        vec![Edit::PropertyDeleted {
            database_id: PEOPLE.into(),
            property_id: ROLE.into(),
        }]
    );
    assert!(!drawn(cx, "header:Role"));
    // A two-way relation's other side goes with it.
    menu(cx, "Interactions");
    click(cx, item("Delete property"));
    click(cx, item("Delete property"));
    assert_eq!(
        last_batch(&host)[1],
        Edit::PropertyDeleted {
            database_id: INTERACTIONS.into(),
            property_id: "451abd81-f1d6-4ed6-97f5-e837d70820fe".into(),
        }
    );
    // From the keyboard: the menu's last entry, then up from Cancel.
    table.update(cx, |t, cx| t.focus_cell(IMANI, EMAIL, cx));
    keys(cx, "alt-down");
    keys(cx, "end enter");
    keys(cx, "up enter");
    assert_eq!(
        last_batch(&host),
        vec![Edit::PropertyDeleted {
            database_id: PEOPLE.into(),
            property_id: EMAIL.into(),
        }]
    );
    // The title: renamed, never retyped or deleted.
    menu(cx, "Name");
    assert!(drawn(cx, item("Rename")));
    assert!(!drawn(cx, item("Change type")));
    assert!(!drawn(cx, item("Delete property")));
    let title = property(&table, cx, NAME);
    assert_eq!(title.retyped(PropertyType::Text), title);
    let role = Property::new("x", "X", PropertyType::Text, "a9");
    assert_eq!(role.retyped(PropertyType::Title), role);
}

#[gpui::test]
fn limits_are_refused_with_a_note(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let count = load("people").0.database.unwrap().properties.len();
    let stage_options = 4;
    host.limits.set(Limits {
        properties: count,
        options: stage_options,
        name_chars: 8,
        option_name_chars: 6,
        ..Limits::default()
    });
    let (table, cx) = open_people(cx, &host);
    assert_eq!(property(&table, cx, STAGE).options().len(), stage_options);
    // Properties: the + menu says so and makes nothing.
    click(cx, "add-property");
    assert!(drawn(cx, "menu-note"));
    click(cx, item("Text"));
    keys(cx, "enter");
    assert!(host.events.borrow().is_empty());
    keys(cx, "escape");
    // Names stop at their limit, with a note.
    menu(cx, "Role");
    click(cx, item("Rename"));
    type_name(cx, "A much longer name");
    assert!(drawn(cx, "menu-note"));
    keys(cx, "enter");
    assert_eq!(property(&table, cx, ROLE).name, "A much l");
    // Options: Add an option is shown, refused, with a note.
    menu(cx, "Stage");
    click(cx, item("Edit options"));
    assert!(drawn(cx, "menu-note"));
    click(cx, item("Add an option"));
    assert!(!drawn(cx, "menu-field"));
    // An option's name stops at its own limit.
    click(cx, item("Lead"));
    click(cx, item("Rename option"));
    type_name(cx, "Prospective");
    keys(cx, "enter");
    assert_eq!(
        property(&table, cx, STAGE).option(LEAD).unwrap().name,
        "Prospe"
    );
    keys(cx, "escape");
    // A two-way relation's other side, past its database's limit: refused.
    host.limits.set(Limits::default());
    {
        let mut dbs = host.databases.borrow_mut();
        let full = &mut dbs[0].properties;
        while full.len() < Limits::default().properties {
            let n = full.len();
            full.push(Property::new(
                format!("p{n}"),
                format!("P{n}"),
                PropertyType::Text,
                format!("b{n:03}"),
            ));
        }
    }
    menu(cx, "Company");
    click(cx, item("Relation"));
    assert!(property(&table, cx, COMPANY).relation().unwrap().two_way);
    assert!(!drawn(cx, "menu-note"), "no note while it's two-way");
    // Made one-way, it can't be made two-way again while Companies is full.
    click(cx, item("Show on Companies"));
    assert!(!property(&table, cx, COMPANY).relation().unwrap().two_way);
    let n = batches(&host);
    click(cx, item("Show on Companies"));
    assert_eq!(batches(&host), n, "refused");
    assert!(!property(&table, cx, COMPANY).relation().unwrap().two_way);
    assert!(drawn(cx, "menu-note"));
}

#[gpui::test]
fn property_menus_are_44_points_on_touch(cx: &mut TestAppContext) {
    let host = TestHost::touch();
    let (_, cx) = open_people(cx, &host);
    let tall = |cx: &mut VisualTestContext, what: String| {
        let b = rect(cx, what.clone());
        assert!(b.size.height >= px(44.), "{what} is {:?}", b.size);
    };
    tall(cx, "add-property".into());
    click(cx, "add-property");
    tall(cx, "picker-field".into());
    tall(cx, item("Number"));
    keys(cx, "escape");
    // A tap opens a submenu, its rows as tall.
    menu(cx, "Stage");
    tall(cx, item("Edit options"));
    let at = rect(cx, item("Edit options")).center();
    tap(cx, at);
    tall(cx, item("Lead"));
    let at = rect(cx, item("Lead")).center();
    tap(cx, at);
    tall(cx, item("Rename option"));
    tall(cx, item("Red"));
    click(cx, item("Add an option"));
    tall(cx, "menu-field".into());
    tall(cx, item("Save"));
}

#[gpui::test]
fn a_column_menu_acts_on_its_property_after_the_columns_shift(cx: &mut TestAppContext) {
    // A two-way relation of People to itself: Mentees, then Mentors, among the columns.
    const MENTEES: &str = "mentees";
    const MENTORS: &str = "mentors";
    let host = TestHost::new();
    let (mut db, rows) = load("people");
    let side = |id: &str, name: &str, order: &str, back: &str| {
        Property::new(id, name, PropertyType::Relation, order).with_config(PropertyConfig {
            database_id: Some(PEOPLE.into()),
            two_way: Some(true),
            back_property_id: Some(back.into()),
            ..PropertyConfig::default()
        })
    };
    let f = db.database.as_mut().unwrap();
    f.properties.push(side(MENTEES, "Mentees", "a8m", MENTORS));
    f.properties.push(side(MENTORS, "Mentors", "a9m", MENTEES));
    let (table, cx) = open_with(
        cx,
        &host,
        db.clone(),
        rows.clone(),
        None,
        size(px(4000.), px(700.)),
    );
    let columns = |table: &Entity<TableView>, cx: &mut VisualTestContext| {
        table.update(cx, |t, _| {
            t.columns()
                .into_iter()
                .map(|(id, _)| id)
                .collect::<Vec<_>>()
        })
    };
    let before = columns(&table, cx);
    let at = before.iter().position(|id| id == MENTORS).unwrap();
    assert!(at + 1 < before.len(), "a column after Mentors");
    // One-way from Mentors' menu: Mentees goes, and the columns after it move left, while the
    // menu stays open.
    let one_way = |cx: &mut VisualTestContext| {
        menu(cx, "Mentors");
        click(cx, item("Relation"));
        click(cx, item("Show on People"));
        assert_eq!(
            last_batch(&host)[1],
            Edit::PropertyDeleted {
                database_id: PEOPLE.into(),
                property_id: MENTEES.into(),
            }
        );
    };
    one_way(cx);
    assert_eq!(columns(&table, cx)[at], before[at + 1]);
    // It still sorts by Mentors.
    click(cx, item("Sort ascending"));
    assert_eq!(host.last_view().sort, vec![Sort::ascending(MENTORS)]);
    // And still deletes Mentors.
    table.update(cx, |t, cx| t.set_docs(db, rows, cx));
    draw(cx);
    one_way(cx);
    click(cx, item("Delete property"));
    click(cx, item("Delete property"));
    assert_eq!(
        last_batch(&host),
        vec![Edit::PropertyDeleted {
            database_id: PEOPLE.into(),
            property_id: MENTORS.into(),
        }]
    );
    // Its property gone, the menu closes.
    assert!(!drawn(cx, "menu"));
}
