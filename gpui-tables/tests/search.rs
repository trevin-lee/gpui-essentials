//! Searching a view's rows from the bar, as a person does it: the search button and ⌘F opening
//! the field, typing narrowing a table, a board and a list to the rows found by what their
//! cells show (never by ids, never by hidden properties), on top of the view's own filter, with
//! empty groups left out and "No results" when nothing's found; Escape clearing then closing;
//! the view as it was once cleared; the host's `set_search`; nothing ever saved or emitted;
//! 44-point targets on touch; and the rows' words folded as the field opens, off the main thread,
//! so the first key only searches, never stale and never while the field's closed.

// The Mac's dev-dependencies switch GPUI's test support on; elsewhere, the test-support feature.
#![cfg(all(feature = "gpui", any(target_os = "macos", feature = "test-support")))]

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
    keys(cx, "secondary-f");
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
    keys(cx, "secondary-f");
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

    keys(cx, "secondary-f");
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

/// How many times the rows' words were folded.
fn folds(table: &Entity<TableView>, cx: &mut VisualTestContext) -> usize {
    table.read_with(cx, |t, _| t.search_folds())
}

/// Opens the field as its button does, without letting the background run (a test runs it only
/// when parked).
fn open_unparked(table: &Entity<TableView>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| table.update(cx, |t, cx| t.open_search(window, cx)));
}

/// The rows with `row`'s title changed to `title`.
fn retitled(
    table: &Entity<TableView>,
    cx: &mut VisualTestContext,
    row: &str,
    title: &str,
) -> Vec<gpui_tables::Doc> {
    let mut rows = table.read_with(cx, |t, _| t.rows().to_vec());
    rows.iter_mut().find(|r| r.id == row).unwrap().title = title.into();
    rows
}

#[gpui::test]
fn the_rows_are_folded_as_the_field_opens_so_the_first_key_only_searches(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_bar(cx, &host, None, size(px(1400.), px(700.)));
    assert_eq!(folds(&table, cx), 0, "nothing folded before a search");
    click(cx, "view-search");
    assert_eq!(folds(&table, cx), 1, "folded as the field opened");
    // By a related company's title and a person's name, which the host gives.
    type_text(cx, "kestrel");
    assert_eq!(
        titles(&table, cx),
        [DURAN, "Jonah Whitfield-Arce", "Rowan Pike"]
    );
    keys(cx, "escape");
    type_text(cx, "avery");
    assert_eq!(titles(&table, cx).len(), 5, "the rows Avery Lin owns");
    assert_eq!(folds(&table, cx), 1, "no key folded again");

    // ⌘F and the host's open_search do the same, for another view.
    keys(cx, "escape");
    keys(cx, "escape");
    table.update(cx, |t, cx| t.set_view(PIPELINE, cx));
    draw(cx);
    keys(cx, "secondary-f");
    assert_eq!(folds(&table, cx), 2, "the board's words, as it shows them");
    type_text(cx, "bluefin");
    assert_eq!(titles(&table, cx), ["Imani Castell", "Theo Brandt"]);
    assert_eq!(folds(&table, cx), 2);
}

#[gpui::test]
fn a_key_typed_before_the_fold_is_done_still_finds_the_right_rows(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_bar(cx, &host, None, size(px(1400.), px(700.)));
    open_unparked(&table, cx);
    assert_eq!(folds(&table, cx), 1, "folding in the background");
    // The first key comes before the background's done: folded then, on the main thread.
    table.update(cx, |t, cx| t.set_search(Some("kestrel".into()), cx));
    assert_eq!(folds(&table, cx), 2);
    assert_eq!(
        titles(&table, cx),
        [DURAN, "Jonah Whitfield-Arce", "Rowan Pike"]
    );
    // The background's fold, done now, changes nothing.
    draw(cx);
    assert_eq!(
        titles(&table, cx),
        [DURAN, "Jonah Whitfield-Arce", "Rowan Pike"]
    );
    table.update(cx, |t, cx| t.set_search(Some("kestrel ".into()), cx));
    assert_eq!(folds(&table, cx), 2);

    // The docs change while the field's open and empty: folded again in the background, for
    // the new docs, and a key typed before it's done finds by them too.
    table.update(cx, |t, cx| t.set_search(None, cx));
    open_unparked(&table, cx);
    assert_eq!(folds(&table, cx), 2, "the same docs: nothing to fold");
    let rows = retitled(&table, cx, THEO, "Theo Kestrel");
    // (A test's update ends in a draw, which begins the fold for the new docs.)
    table.update(cx, |t, cx| t.set_rows(rows, cx));
    assert_eq!(
        folds(&table, cx),
        3,
        "folding the new docs in the background"
    );
    table.update(cx, |t, cx| t.set_search(Some("kestrel".into()), cx));
    assert_eq!(
        folds(&table, cx),
        4,
        "the key came first: folded on the main thread"
    );
    assert_eq!(
        titles(&table, cx),
        [DURAN, "Jonah Whitfield-Arce", "Rowan Pike", "Theo Kestrel"]
    );
    draw(cx);
    assert_eq!(titles(&table, cx).len(), 4);
    assert_eq!(folds(&table, cx), 4);
}

#[gpui::test]
fn closing_the_search_keeps_its_words_and_folds_nothing_while_closed(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_bar(cx, &host, None, size(px(1400.), px(700.)));
    let all = titles(&table, cx);
    click(cx, "view-search");
    type_text(cx, "kestrel");
    keys(cx, "escape");
    keys(cx, "escape");
    assert!(!drawn(cx, "view-search-field"), "closed");
    assert_eq!(titles(&table, cx), all);
    // Opened again over the same docs: what was folded is kept.
    click(cx, "view-search");
    type_text(cx, "bluefin");
    assert_eq!(titles(&table, cx), ["Imani Castell", "Theo Brandt"]);
    assert_eq!(folds(&table, cx), 1);
    keys(cx, "escape");
    keys(cx, "escape");

    // The docs change while it's closed: what was folded is dropped, and nothing is folded
    // until it opens.
    let rows = retitled(&table, cx, THEO, "Theo Kestrel");
    table.update(cx, |t, cx| t.set_rows(rows, cx));
    draw(cx);
    assert_eq!(folds(&table, cx), 1, "nothing folded while closed");
    click(cx, "view-search");
    assert_eq!(folds(&table, cx), 2);
    type_text(cx, "kestrel");
    assert_eq!(
        titles(&table, cx),
        [DURAN, "Jonah Whitfield-Arce", "Rowan Pike", "Theo Kestrel"]
    );
    assert_eq!(folds(&table, cx), 2);
    keys(cx, "escape");
    keys(cx, "escape");

    // Closed before its fold is done: the fold stops, and the field folds again when it opens.
    let rows = retitled(&table, cx, THEO, "Theo Brandt");
    table.update(cx, |t, cx| t.set_rows(rows, cx));
    draw(cx);
    open_unparked(&table, cx);
    assert_eq!(folds(&table, cx), 3);
    table.update(cx, |t, cx| t.set_search(None, cx));
    draw(cx);
    assert!(!drawn(cx, "view-search-field"));
    assert_eq!(titles(&table, cx), all, "the view whole");
    click(cx, "view-search");
    assert_eq!(folds(&table, cx), 4, "the stopped fold left nothing behind");
    type_text(cx, "kestrel");
    assert_eq!(
        titles(&table, cx),
        [DURAN, "Jonah Whitfield-Arce", "Rowan Pike"]
    );
}

#[gpui::test]
fn nothing_redraws_while_the_field_waits_for_a_key(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_bar(cx, &host, None, size(px(1400.), px(700.)));
    let notified = std::rc::Rc::new(std::cell::Cell::new(0));
    let _watch = cx.update(|_, cx| {
        let notified = notified.clone();
        cx.observe(&table, move |_, _| notified.set(notified.get() + 1))
    });
    let idle = |cx: &mut VisualTestContext, what: &str| {
        notified.set(0);
        for _ in 0..30 {
            cx.executor()
                .advance_clock(std::time::Duration::from_millis(16));
            let asked = cx.update(|window, cx| window.simulate_next_frame(cx));
            cx.run_until_parked();
            assert_eq!(asked, 0, "asked for a frame {what}");
        }
        assert_eq!(notified.get(), 0, "redrew {what}");
    };
    idle(cx, "before a search");
    // The fold, done in the background, asks for no frame.
    open_unparked(&table, cx);
    idle(cx, "as the rows were folded");
    assert_eq!(folds(&table, cx), 1);
    // Nor once the docs change while the field's open.
    let rows = retitled(&table, cx, THEO, "Theo Kestrel");
    table.update(cx, |t, cx| t.set_rows(rows, cx));
    idle(cx, "as the new docs were folded");
    assert_eq!(folds(&table, cx), 2);
    type_text(cx, "kestrel");
    assert_eq!(titles(&table, cx).len(), 4);
    idle(cx, "with a search shown");
    assert_eq!(folds(&table, cx), 2);
}
