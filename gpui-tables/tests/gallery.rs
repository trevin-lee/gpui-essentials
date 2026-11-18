//! The gallery view: the rows as cards in a grid, several to a line; a card opens its row, the
//! arrows and Enter do the same from the keyboard, New makes a row, and a card dragged to
//! another place is a `rowsMoved`.

// The Mac's dev-dependencies switch GPUI's test support on; elsewhere, the test-support feature.
#![cfg(all(feature = "gpui", any(target_os = "macos", feature = "test-support")))]

mod common;

use common::ids::*;
use common::*;
use gpui::*;
use gpui_tables::view::TableView;
use gpui_tables::*;
// The built-in #[test], not GPUI's, which gpui::* brings in.
use std::prelude::v1::test;

/// People's All people view as a gallery in the manual order, its cards showing Company and Role.
fn open_gallery<'a>(
    cx: &'a mut TestAppContext,
    host: &std::rc::Rc<TestHost>,
) -> (Entity<TableView>, &'a mut VisualTestContext) {
    let (mut db, rows) = load("people");
    let v = db
        .database
        .as_mut()
        .unwrap()
        .views
        .iter_mut()
        .find(|v| v.id == ALL_PEOPLE)
        .unwrap();
    v.kind = ViewType::Gallery;
    v.sort.clear();
    v.card_properties = vec![COMPANY.into(), ROLE.into()];
    open_with(
        cx,
        host,
        db,
        rows,
        Some(ALL_PEOPLE),
        size(px(1400.), px(900.)),
    )
}

fn card(row: &str) -> String {
    format!("card:{row}:all")
}

fn order(table: &Entity<TableView>, cx: &mut VisualTestContext) -> Vec<String> {
    table.update(cx, |t, _| {
        let shown = t.shown_rows().to_vec();
        shown.iter().map(|&i| t.rows()[i].id.clone()).collect()
    })
}

fn edits(host: &TestHost) -> Vec<Edit> {
    host.events
        .borrow()
        .iter()
        .filter_map(|e| match e {
            TableEvent::Edits { edits } => Some(edits.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

#[gpui::test]
fn the_rows_are_cards_in_a_grid_that_open_their_rows(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_gallery(cx, &host);
    assert!(table.update(cx, |t, _| t.is_gallery()));
    assert!(drawn(cx, "gallery"));
    let ids = order(&table, cx);
    assert!(ids.len() >= 4);
    for id in &ids {
        assert!(drawn(cx, card(id)), "{id}");
    }
    // Several to a line: the second card beside the first, the same height.
    let (a, b) = (rect(cx, card(&ids[0])), rect(cx, card(&ids[1])));
    assert_eq!(a.top(), b.top());
    assert!(b.left() > a.right());
    // A card opens its row.
    click(cx, card(&ids[2]));
    assert!(matches!(
        host.events.borrow().last(),
        Some(TableEvent::OpenRow { row_id }) if *row_id == ids[2]
    ));
    // The arrows move between cards (the first arrow comes to the first card) and Enter opens
    // one: right from the first is the second.
    activate(cx);
    keys(cx, "right right enter");
    assert!(matches!(
        host.events.borrow().last(),
        Some(TableEvent::OpenRow { row_id }) if *row_id == ids[1]
    ));
    // New makes a row.
    host.clear();
    click(cx, "gallery-new");
    assert!(
        edits(&host)
            .iter()
            .any(|e| matches!(e, Edit::RowCreated { .. })),
        "{:?}",
        edits(&host)
    );
}

#[gpui::test]
fn a_card_dragged_to_another_place_moves_its_row(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_gallery(cx, &host);
    let ids = order(&table, cx);
    // The first card carried past the middle of the third: it lands after it.
    let from = rect(cx, card(&ids[0])).center();
    let third = rect(cx, card(&ids[2]));
    let to = point(third.left() + third.size.width * 0.75, third.center().y);
    drag(cx, from, to);
    let moved: Vec<Edit> = edits(&host)
        .into_iter()
        .filter(|e| matches!(e, Edit::RowsMoved { .. }))
        .collect();
    assert_eq!(moved.len(), 1, "{:?}", edits(&host));
    let Edit::RowsMoved { row_ids, .. } = &moved[0] else {
        unreachable!()
    };
    assert_eq!(row_ids, &[ids[0].clone()]);
    assert!(
        !drawn(cx, "gallery-landing"),
        "the landing line goes with the drag"
    );
}
