//! Searching a view's rows from the bar, as a person does it: the search button and ⌘F opening
//! the field, typing narrowing a table, a board and a list to the rows found by what their
//! cells show (never by ids, never by hidden properties), on top of the view's own filter, with
//! empty groups left out and "No results" when nothing's found; Escape clearing then closing;
//! the view as it was once cleared; the host's `set_search`; nothing ever saved or emitted; and
//! 44-point targets on touch.

#![cfg(all(target_os = "macos", feature = "gpui"))]

mod common;

use common::ids::*;
use common::*;
use gpui::*;
use gpui_tables::view::TableView;
// The built-in #[test], not GPUI's, which gpui::* brings in.
use std::prelude::v1::test;

const DURAN: &str = "Ana Sofía Durán";

fn open_bar<'a>(
    cx: &'a mut TestAppContext,
    host: &std::rc::Rc<TestHost>,
    view: Option<&str>,
    size: Size<Pixels>,
) -> (Entity<TableView>, &'a mut VisualTestContext) {
    let (db, rows) = load("people");
    let (table, cx) = open_with(cx, host, db, rows, view, size);
    table.update(cx, |t, cx| t.set_view_bar(true, cx));
    draw(cx);
    (table, cx)
}

/// The titles of the rows shown, in order.
fn titles(table: &Entity<TableView>, cx: &mut VisualTestContext) -> Vec<String> {
    table.update(cx, |t, _| {
        let shown = t.shown_rows().to_vec();
        shown.iter().map(|&r| t.rows()[r].title.clone()).collect()
    })
}

/// Each group's key and its count, as the view shows them.
fn groups(table: &Entity<TableView>, cx: &mut VisualTestContext) -> Vec<(String, usize)> {
    table.update(cx, |t, _| {
        t.groups()
            .iter()
            .map(|g| (g.key.to_string(), g.rows.len()))
            .collect()
    })
}

fn search(table: &Entity<TableView>, cx: &mut VisualTestContext, query: Option<&str>) {
    table.update(cx, |t, cx| t.set_search(query.map(str::to_owned), cx));
    draw(cx);
}

#[gpui::test]
fn typing_in_the_bar_narrows_the_table_by_what_its_cells_show(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_bar(cx, &host, None, size(px(1400.), px(700.)));
    let all = titles(&table, cx);
    assert_eq!(all.len(), 8);
    assert!(drawn(cx, "view-search"), "the search button");
    assert!(!drawn(cx, "view-search-field"));
    // At the bar's right end, after the settings.
    assert!(rect(cx, "view-search").left() >= rect(cx, "view-settings").right());

    click(cx, "view-search");
    assert!(
        drawn(cx, "view-search-field"),
        "the field opens in its place"
    );
    assert!(!drawn(cx, "view-search"));
    // Typed, without case or accents: the title.
    type_text(cx, "DURAN");
    assert_eq!(titles(&table, cx), [DURAN]);
    assert_eq!(
        table.read_with(cx, |t, _| t.search().map(str::to_owned)),
        Some("DURAN".into())
    );
    assert!(drawn(cx, cell(ANA, NAME)));
    assert!(!drawn(cx, cell(ROWAN, NAME)));

    // A related row's title, a role: what the cells show, in the view's order (Duplicate of
    // Rowan, with Kestrel in its email, is deleted).
    search(&table, cx, Some("kestrel"));
    assert_eq!(
        titles(&table, cx),
        [DURAN, "Jonah Whitfield-Arce", "Rowan Pike"]
    );
    // An option's name, not its id.
    search(&table, cx, Some("dormant"));
    assert_eq!(titles(&table, cx), ["Noor Ellison"]);
    search(&table, cx, Some(&DORMANT[..8]));
    assert!(titles(&table, cx).is_empty(), "never by an option's id");
    // A person by their name; a hidden column (Phone) isn't searched.
    search(&table, cx, Some("career planning"));
    assert_eq!(titles(&table, cx), ["Imani Castell", "Theo Brandt"]);
    search(&table, cx, Some("555 0101"));
    assert!(titles(&table, cx).is_empty(), "the phone column is hidden");

    // Nothing found says so where the rows would be.
    assert!(drawn(cx, "no-results"));
    assert!(!drawn(cx, "table"));
    // Never saved, never an edit: the host heard nothing, the view is as it was.
    assert!(
        host.events.borrow().is_empty(),
        "{:?}",
        host.events.borrow()
    );
    assert!(table.read_with(cx, |t, _| t.view().unwrap().filter.is_none()));

    search(&table, cx, None);
    assert_eq!(titles(&table, cx), all);
    assert!(!drawn(cx, "view-search-field"), "None closes the field");
    assert!(drawn(cx, "table"));
}

#[gpui::test]
fn escape_clears_then_closes_and_cmd_f_opens(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_bar(cx, &host, None, size(px(1400.), px(700.)));
    let all = titles(&table, cx);
    // ⌘F from the table.
    click(cx, cell(THEO, ROLE));
    keys(cx, "cmd-f");
    assert!(drawn(cx, "view-search-field"));
    type_text(cx, "bluefin");
    assert_eq!(titles(&table, cx), ["Imani Castell", "Theo Brandt"]);
    assert!(drawn(cx, "view-search-clear"), "the clear button");

    keys(cx, "escape");
    assert_eq!(titles(&table, cx), all, "the first Escape clears");
    assert!(drawn(cx, "view-search-field"), "and leaves the field open");
    assert!(!drawn(cx, "view-search-clear"));
    keys(cx, "escape");
    assert!(!drawn(cx, "view-search-field"), "the second closes it");
    assert!(drawn(cx, "view-search"));
    let focused = table.update(cx, |t, _| t.focused_row_cell());
    assert_eq!(
        focused,
        Some((THEO.to_owned(), ROLE.to_owned())),
        "the keyboard's back on Theo"
    );
    // The table has the keyboard: the arrows move the cell.
    keys(cx, "down");
    assert_ne!(
        table.update(cx, |t, _| t.focused_row_cell()),
        Some((THEO.to_owned(), ROLE.to_owned()))
    );

    // The × clears too, and keeps the field open.
    keys(cx, "cmd-f");
    type_text(cx, "lumen");
    assert_eq!(titles(&table, cx).len(), 2);
    click(cx, "view-search-clear");
    assert_eq!(titles(&table, cx), all);
    assert!(drawn(cx, "view-search-field"));
    assert!(host.events.borrow().is_empty());
}

#[gpui::test]
fn a_search_narrows_a_board_and_a_list_and_leaves_out_empty_groups(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_bar(cx, &host, Some(PIPELINE), size(px(1400.), px(700.)));
    assert!(drawn(cx, "board"));
    let columns = table.update(cx, |t, _| t.board_columns().len());
    assert!(columns > 2);
    // The cards show the company: Rowan's and Ana's is Kestrel. Jonah has it only in his role,
    // which isn't on the cards.
    search(&table, cx, Some("Kestrel"));
    let found = table.update(cx, |t, _| {
        t.board_columns()
            .iter()
            .map(|c| (c.key.as_ref().map(|k| k.to_string()), c.rows.len()))
            .collect::<Vec<_>>()
    });
    assert_eq!(found, [(Some(format!("option:{ACTIVE}")), 2)]);
    assert_eq!(
        groups(&table, cx),
        [(format!("option:{ACTIVE}"), 2)],
        "counts are matches"
    );
    search(&table, cx, None);
    assert_eq!(table.update(cx, |t, _| t.board_columns().len()), columns);

    // A list grouped by stage: the company on its lines.
    let by_stage = view_id(
        &table.read_with(cx, |t, _| t.database().clone()),
        "By stage",
    );
    table.update(cx, |t, cx| t.set_view(by_stage, cx));
    draw(cx);
    search(&table, cx, Some("freight"));
    assert_eq!(
        groups(&table, cx),
        [(format!("option:{LEAD}"), 1), (format!("option:{WARM}"), 1)]
    );
    assert!(drawn(cx, "list"));

    // On top of the view's own filter: Follow up shows only rows to follow up.
    let follow = view_id(
        &table.read_with(cx, |t, _| t.database().clone()),
        "Follow up",
    );
    table.update(cx, |t, cx| t.set_view(follow, cx));
    draw(cx);
    assert_eq!(
        table.read_with(cx, |t, _| t.search().map(str::to_owned)),
        Some("freight".into()),
        "the search stays as views change"
    );
    assert_eq!(titles(&table, cx), ["Sami Halloran"]);
    search(&table, cx, Some("kestrel"));
    assert!(titles(&table, cx).is_empty(), "Rowan isn't to follow up");
    assert!(drawn(cx, "no-results"));
}

#[gpui::test]
fn clearing_shows_the_view_as_it_was(cx: &mut TestAppContext) {
    let host = TestHost::new();
    // Short enough to scroll: Theo, last by name, is scrolled to.
    let (table, cx) = open_bar(cx, &host, None, size(px(1400.), px(220.)));
    table.update(cx, |t, cx| t.focus_cell(ROWAN, ROLE, cx));
    draw(cx);
    keys(cx, "down");
    keys(cx, "down");
    let focused = table.update(cx, |t, _| t.focused_row_cell());
    assert_eq!(focused, Some((THEO.to_owned(), ROLE.to_owned())));
    let at = rect(cx, cell(THEO, NAME));
    assert!(!drawn(cx, cell(ANA, NAME)), "scrolled past the first rows");
    keys(cx, "space");
    assert_eq!(
        table.read_with(cx, |t, _| t.selected_rows().to_vec()),
        [THEO]
    );

    keys(cx, "cmd-f");
    type_text(cx, "duran");
    assert_eq!(titles(&table, cx), [DURAN]);
    assert!(drawn(cx, cell(ANA, NAME)), "the rows found from the top");
    assert_eq!(table.update(cx, |t, _| t.focused_row_cell()), None);
    keys(cx, "escape");
    assert_eq!(titles(&table, cx).len(), 8);
    assert_eq!(
        table.update(cx, |t, _| t.focused_row_cell()),
        focused,
        "the same cell"
    );
    assert_eq!(rect(cx, cell(THEO, NAME)), at, "scrolled where it was");
    assert_eq!(
        table.read_with(cx, |t, _| t.selected_rows().to_vec()),
        [THEO]
    );
    assert!(drawn(cx, "view-search-field"), "the field's still open");
}

#[gpui::test]
fn the_search_is_44_points_on_touch(cx: &mut TestAppContext) {
    let host = TestHost::touch();
    let (_, cx) = open_bar(cx, &host, None, size(px(1400.), px(700.)));
    let big = |cx: &mut VisualTestContext, what: &str| {
        let b = rect(cx, what);
        assert!(
            b.size.height >= px(44.) && b.size.width >= px(44.),
            "{what} is {:?}",
            b.size
        );
    };
    big(cx, "view-search");
    let (x, y) = {
        let b = rect(cx, "view-search");
        (b.center().x, b.center().y)
    };
    tap(cx, point(x, y));
    assert!(drawn(cx, "view-search-field"), "a tap opens it");
    type_text(cx, "a");
    big(cx, "view-search-field");
    big(cx, "view-search-clear");
}

#[gpui::test]
fn keys_typed_between_frames_all_count(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_bar(cx, &host, None, size(px(1400.), px(700.)));
    cx.update(|window, cx| table.update(cx, |t, cx| t.open_search(window, cx)));
    draw(cx);
    // Several keys before the window draws again, as fast typing does.
    cx.update(|window, cx| {
        for ch in "kestrel".chars() {
            window.dispatch_keystroke(
                Keystroke {
                    modifiers: Modifiers::none(),
                    key: ch.to_string(),
                    key_char: Some(ch.to_string()),
                },
                cx,
            );
        }
    });
    draw(cx);
    assert_eq!(
        table.read_with(cx, |t, _| t.search().map(str::to_owned)),
        Some("kestrel".into())
    );
    assert_eq!(
        titles(&table, cx),
        [DURAN, "Jonah Whitfield-Arce", "Rowan Pike"]
    );
    // And a key a frame.
    keys(cx, "escape");
    for ch in ["k", "e", "s"] {
        keys(cx, ch);
    }
    assert_eq!(
        table.read_with(cx, |t, _| t.search().map(str::to_owned)),
        Some("kes".into())
    );
}
