//! What assistive technology gets of the views: every control drawn (a table's headers, rows,
//! cells and resize handles, a board's columns and cards, a list's lines, a gallery's cards,
//! the bar of views, the peek) has a role and a label. GPUI's test windows build no
//! accessibility tree, so the views report what they draw to gpui-core's audit instead.

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

/// People drawn as `kind` (its "All people" view retyped), with the bar of views over it.
fn open_as<'a>(
    cx: &'a mut TestAppContext,
    host: &std::rc::Rc<TestHost>,
    kind: ViewType,
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
    if kind != ViewType::Table {
        v.group_by = (kind != ViewType::Gallery).then(|| STAGE.into());
        v.kind = kind;
        v.card_properties = vec![COMPANY.into(), ROLE.into()];
    }
    let (table, cx) = open_with(
        cx,
        host,
        db,
        rows,
        Some(ALL_PEOPLE),
        size(px(1400.), px(900.)),
    );
    table.update(cx, |t, cx| t.set_view_bar(true, cx));
    draw(cx);
    (table, cx)
}

/// What the audit recorded while `act` ran, checked: every control has a label and a role.
fn audited(cx: &mut VisualTestContext, act: impl FnOnce(&mut VisualTestContext)) -> Vec<String> {
    gpui_core::audit_start();
    act(cx);
    draw(cx);
    let seen = gpui_core::audit_take();
    assert!(!seen.is_empty(), "nothing recorded");
    let unnamed: Vec<_> = seen
        .iter()
        .filter(|a| a.label.as_ref().is_none_or(|l| l.trim().is_empty()))
        .collect();
    assert!(unnamed.is_empty(), "controls with no label: {unnamed:#?}");
    let container: Vec<_> = seen
        .iter()
        .filter(|a| a.role == Role::GenericContainer)
        .collect();
    assert!(
        container.is_empty(),
        "controls with no role: {container:#?}"
    );
    seen.iter()
        .filter_map(|a| a.label.as_ref().map(|l| l.to_string()))
        .collect()
}

fn has(labels: &[String], expected: &[&str]) {
    for e in expected {
        assert!(
            labels.iter().any(|l| l == e),
            "no control says {e:?}: {labels:#?}"
        );
    }
}

/// The table, the bar over it, a row peeked and a column's menu open.
#[gpui::test]
fn every_control_of_a_table_has_a_role_and_a_label(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (table, cx) = open_as(cx, &host, ViewType::Table);
    let labels = audited(cx, |cx| {
        table.update(cx, |t, cx| {
            t.set_peek(true, cx);
            t.peek(IMANI, cx);
        });
        draw(cx);
        click(cx, "header:Company");
    });
    has(
        &labels,
        &[
            "Name",
            "Company",
            "Imani Castell",
            "Width of Company",
            "Add a view",
            "Title: Imani Castell",
            "Open as page",
        ],
    );
    assert!(
        labels.iter().any(|l| l.starts_with("Company: ")),
        "a cell or a peeked value: {labels:#?}"
    );
}

#[gpui::test]
fn every_control_of_a_board_has_a_role_and_a_label(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (_, cx) = open_as(cx, &host, ViewType::Board);
    let labels = audited(cx, |_| {});
    assert!(
        labels.iter().any(|l| l.starts_with("Imani Castell")),
        "her card: {labels:#?}"
    );
}

#[gpui::test]
fn every_control_of_a_list_has_a_role_and_a_label(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (_, cx) = open_as(cx, &host, ViewType::List);
    let labels = audited(cx, |_| {});
    assert!(
        labels.iter().any(|l| l.starts_with("Imani Castell")),
        "her line: {labels:#?}"
    );
}

#[gpui::test]
fn every_control_of_a_gallery_has_a_role_and_a_label(cx: &mut TestAppContext) {
    let host = TestHost::new();
    let (_, cx) = open_as(cx, &host, ViewType::Gallery);
    let labels = audited(cx, |_| {});
    has(&labels, &["New"]);
    assert!(
        labels.iter().any(|l| l.starts_with("Imani Castell")),
        "her card: {labels:#?}"
    );
}
