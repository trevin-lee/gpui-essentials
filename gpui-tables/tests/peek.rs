//! A row's peek: with peeking on, opening a row shows it in a panel at the view's right (its
//! title and every property's value) instead of asking the host to open its page; the panel
//! steps to the rows above and below, opens the page on request, and closes on Escape.

// The Mac's dev-dependencies switch GPUI's test support on; elsewhere, the test-support feature.
#![cfg(all(feature = "gpui", any(target_os = "macos", feature = "test-support")))]

mod common;

use common::ids::*;
use common::*;
use gpui::*;
use gpui_tables::*;
// The built-in #[test], not GPUI's, which gpui::* brings in.
use std::prelude::v1::test;

fn opened(host: &TestHost) -> Vec<String> {
    host.events
        .borrow()
        .iter()
        .filter_map(|e| match e {
            TableEvent::OpenRow { row_id } => Some(row_id.clone()),
            _ => None,
        })
        .collect()
}

#[gpui::test]
fn opening_a_row_peeks_it_when_peeking_is_on(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_people(cx, &host);
    table.update(cx, |t, cx| t.set_peek(true, cx));
    draw(cx);
    assert!(!drawn(cx, "peek"));
    click(cx, format!("open:{ROWAN}"));
    // The panel, not the host's page.
    assert!(opened(&host).is_empty(), "{:?}", opened(&host));
    assert_eq!(
        table.read_with(cx, |t, _| t.peeked_row().map(str::to_owned)),
        Some(ROWAN.into())
    );
    assert!(drawn(cx, "peek") && drawn(cx, "peek-title"));
    // Every property but the title, with its value as its cell shows it.
    let (db, rows) = load("people");
    let rowan = rows.iter().find(|r| r.id == ROWAN).unwrap();
    for p in &db.database.as_ref().unwrap().properties {
        if p.kind == PropertyType::Title {
            continue;
        }
        assert!(drawn(cx, format!("peek-value:{}", p.id)), "{}", p.name);
    }
    assert!(rowan.value(ROLE).is_some());
    // On the view's right, over the rows.
    let panel = rect(cx, "peek");
    let view = rect(cx, "table");
    assert!(
        (panel.right() - view.right()).abs() < px(1.),
        "{panel:?} {view:?}"
    );
    assert!(panel.size.width >= px(320.) && panel.size.width <= px(480.));

    // Down and up go through the rows in the view's order.
    let order: Vec<String> = table.update(cx, |t, _| {
        let shown = t.shown_rows().to_vec();
        shown.iter().map(|&i| t.rows()[i].id.clone()).collect()
    });
    let at = order.iter().position(|id| id == ROWAN).unwrap();
    click(cx, "peek-down");
    let next = table.read_with(cx, |t, _| t.peeked_row().map(str::to_owned));
    assert_eq!(next.as_deref(), order.get(at + 1).map(String::as_str));
    click(cx, "peek-up");
    assert_eq!(
        table.read_with(cx, |t, _| t.peeked_row().map(str::to_owned)),
        Some(ROWAN.into())
    );

    // Escape closes it.
    activate(cx);
    keys(cx, "escape");
    assert!(table.read_with(cx, |t, _| t.peeked_row().is_none()));
    assert!(!drawn(cx, "peek"));

    // Open as page: the host opens it, and the panel closes.
    click(cx, format!("open:{IMANI}"));
    click(cx, "peek-open");
    assert_eq!(opened(&host), [IMANI]);
    assert!(table.read_with(cx, |t, _| t.peeked_row().is_none()));
}

#[gpui::test]
fn with_peeking_off_opening_asks_the_host_and_a_deleted_row_closes_the_peek(
    cx: &mut TestAppContext,
) {
    let host = TestHost::new();
    let (table, cx) = open_people(cx, &host);
    // Off (the default): opening asks the host, as before.
    click(cx, format!("open:{ROWAN}"));
    assert_eq!(opened(&host), [ROWAN]);
    assert!(!drawn(cx, "peek"));
    // Peeked by the host itself, then the row goes from the host's docs: the panel goes too.
    table.update(cx, |t, cx| t.peek(THEO, cx));
    draw(cx);
    assert!(drawn(cx, "peek"));
    let (db, mut rows) = load("people");
    rows.retain(|r| r.id != THEO);
    table.update(cx, |t, cx| t.set_docs(db, rows, cx));
    draw(cx);
    assert!(!drawn(cx, "peek"));
    assert!(table.read_with(cx, |t, _| t.peeked_row().is_none()));
}
