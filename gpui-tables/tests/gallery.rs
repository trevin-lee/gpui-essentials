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

/// A card from the keyboard: ⌘⇧→ and ⌘⇧← move it a place on and back in the manual order,
/// ⌘⇧↓ a line, the keyboard staying on it; ⌘⌫ deletes it.
#[gpui::test]
fn the_keyboard_moves_and_deletes_a_card(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_gallery(cx, &host);
    let before = order(&table, cx);
    let first = before[0].clone();
    // The first arrow lands on the first card.
    keys(cx, "right");
    keys(cx, "secondary-shift-right");
    let mut want = before.clone();
    want.swap(0, 1);
    assert_eq!(order(&table, cx), want, "a place on");
    keys(cx, "secondary-shift-left");
    assert_eq!(order(&table, cx), before, "and back");
    // A line down: past the cards of the first line.
    keys(cx, "secondary-shift-down");
    let at = order(&table, cx)
        .iter()
        .position(|id| *id == first)
        .unwrap();
    assert!(at >= 2, "a line down is more than a place: {at}");
    // The keyboard is still on it.
    keys(cx, "secondary-backspace");
    assert!(
        matches!(host.last_edit(), Edit::RowsDeleted { row_ids } if row_ids == [first.clone()]),
        "{:?}",
        host.last_edit()
    );
}

const ASSET: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

fn picture() -> std::sync::Arc<Image> {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="9"><rect width="16" height="9" fill="#3b82f6"/></svg>"##;
    std::sync::Arc::new(Image::from_bytes(ImageFormat::Svg, svg.as_bytes().to_vec()))
}

/// The gallery with its cards showing their rows' covers: Rowan's a colour, Imani's an image the
/// host has, Theo none.
fn open_covers<'a>(
    cx: &'a mut TestAppContext,
    host: &std::rc::Rc<TestHost>,
    preview: Option<CardPreview>,
) -> (Entity<TableView>, &'a mut VisualTestContext) {
    let (mut db, mut rows) = load("people");
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
    v.card_preview = preview;
    for r in &mut rows {
        r.cover = match r.id.as_str() {
            ROWAN => serde_json::json!({"style": "solid", "color": "#3B4A6BFF"}),
            IMANI => serde_json::json!({"style": "image", "assetId": ASSET}),
            _ => serde_json::json!({"style": "solid"}),
        };
    }
    host.images.borrow_mut().insert(ASSET.into(), picture());
    open_with(
        cx,
        host,
        db,
        rows,
        Some(ALL_PEOPLE),
        size(px(1400.), px(900.)),
    )
}

#[gpui::test]
fn with_the_card_preview_on_covers_each_card_starts_with_its_rows_cover(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (_, cx) = open_covers(cx, &host, Some(CardPreview::Cover));
    draw(cx);
    for row in [ROWAN, IMANI, THEO] {
        let band = rect(cx, format!("cover:{row}"));
        let card = rect(cx, card(row));
        // A 16:9 band across the card's top.
        assert!(
            (band.top() - card.top()).abs() < px(2.),
            "{band:?} {card:?}"
        );
        let ratio = band.size.width / band.size.height;
        assert!((ratio - 16. / 9.).abs() < 0.1, "{row}: {band:?}");
        assert!(card.size.height > band.size.height + px(40.), "{card:?}");
    }
    // Imani's image is the host's; the crate asked for it and fetched nothing.
    assert!(host.images_asked.borrow().iter().any(|a| a == ASSET));
}

#[gpui::test]
fn without_the_card_preview_cards_have_no_band(cx: &mut TestAppContext) {
    let host = TestHost::new();
    for preview in [
        None,
        Some(CardPreview::None),
        Some(CardPreview::Other("big".into())),
    ] {
        let (_, cx) = open_covers(cx, &host, preview.clone());
        assert!(drawn(cx, card(ROWAN)), "{preview:?}");
        assert!(!drawn(cx, format!("cover:{ROWAN}")), "{preview:?}");
    }
    assert!(host.images_asked.borrow().is_empty());
}

/// The gallery's settings set its card preview, writing only none or cover.
#[gpui::test]
fn the_settings_set_the_card_preview(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_covers(cx, &host, None);
    table.update(cx, |t, cx| t.set_view_bar(true, cx));
    draw(cx);
    let preview = |host: &TestHost| {
        edits(host).into_iter().rev().find_map(|e| match e {
            Edit::ViewChanged { view, .. } => Some(view.card_preview),
            _ => None,
        })
    };
    click(cx, "view-settings");
    click(cx, "menu-item-Card preview");
    click(cx, "menu-item-Page cover");
    assert_eq!(preview(&host), Some(Some(CardPreview::Cover)));
    click(cx, "menu-item-None");
    assert_eq!(preview(&host), Some(Some(CardPreview::None)));
    let written = edits(&host)
        .into_iter()
        .filter_map(|e| match e {
            Edit::ViewChanged { view, .. } => {
                Some(serde_json::to_value(&view).unwrap()["cardPreview"].clone())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        written,
        [serde_json::json!("cover"), serde_json::json!("none")]
    );
}
