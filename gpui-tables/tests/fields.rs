//! The fields values are typed in, in place: typing stops at a value's limit and a paste is cut
//! to fit (in a cell and in a filter's field), what's committed is cut again (a host's field's
//! answer too), and a number is typed at the right, where its cell shows it. Laid out with Core Text, as on screen.

#![cfg(all(target_os = "macos", feature = "gpui"))]

mod common;

use std::sync::Arc;

use common::ids::*;
use common::*;
use gpui::*;
use gpui_tables::view::{TableView, TextTarget};
use gpui_tables::*;
use gpui_tables::{Edit, Filter, Value};
// The built-in #[test], not GPUI's, which gpui::* brings in.
use std::prelude::v1::test;

/// A test app that lays text out as the screen does (GPUI's test platform otherwise measures
/// every string as nothing).
fn app_with_text() -> TestAppContext {
    TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        Arc::new(gpui_apple::AppleTextSystem::new()),
    )
}

/// The default limits: what the test host's `limits` gives.
fn limits() -> Limits {
    Limits::default()
}

fn typed(table: &Entity<TableView>, cx: &mut VisualTestContext) -> String {
    editing(table, cx).expect("a field open in place").1
}

fn paste(cx: &mut VisualTestContext, text: String) {
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string(text)));
    keys(cx, "secondary-v");
}

fn last_text(host: &TestHost) -> String {
    match host.last_edit() {
        Edit::RowValueChanged {
            value: Some(Value::Text(s)),
            ..
        } => s,
        Edit::RowTitleChanged { title, .. } => title,
        other => panic!("not a text value: {other:?}"),
    }
}

#[test]
fn typing_stops_at_a_values_limit() {
    let mut app = app_with_text();
    let host = TestHost::new();
    let (table, cx) = open_people(&mut app, &host);
    let l = limits();

    // An email: typing past its limit adds nothing.
    table.update(cx, |t, cx| t.focus_cell(ANA, EMAIL, cx));
    keys(cx, "enter");
    select_all(&table, cx);
    type_text(cx, &"a".repeat(l.email_chars + 80));
    assert_eq!(typed(&table, cx).chars().count(), l.email_chars);
    type_text(cx, "b");
    assert!(!typed(&table, cx).contains('b'), "full: a key adds nothing");
    keys(cx, "enter");
    assert_eq!(last_text(&host).chars().count(), l.email_chars);

    // Characters, not bytes: two-byte letters stop at as many characters.
    table.update(cx, |t, cx| t.focus_cell(ANA, EMAIL, cx));
    keys(cx, "enter");
    select_all(&table, cx);
    type_text(cx, &"é".repeat(l.email_chars + 10));
    assert_eq!(typed(&table, cx).chars().count(), l.email_chars);
    keys(cx, "escape");

    // A title, at its own limit.
    table.update(cx, |t, cx| t.focus_cell(ANA, NAME, cx));
    keys(cx, "f2");
    select_all(&table, cx);
    type_text(cx, &"t".repeat(l.title_chars + 5));
    assert_eq!(typed(&table, cx).chars().count(), l.title_chars);
    keys(cx, "escape");

    // A filter's value, at the limit of a value of its column's type.
    open_menu(cx, "Email");
    click(cx, "menu-item-Contains…");
    assert!(matches!(
        editing(&table, cx),
        Some((TextTarget::Filter { .. }, _))
    ));
    type_text(cx, &"f".repeat(l.email_chars + 1));
    assert_eq!(typed(&table, cx).chars().count(), l.email_chars);
    keys(cx, "enter");
    assert_eq!(email_filter(&host).chars().count(), l.email_chars);
}

/// The value of the last view's filter on Email.
fn email_filter(host: &TestHost) -> String {
    host.last_view()
        .filter
        .iter()
        .flat_map(Filter::walk)
        .find_map(|(f, _)| match f {
            Filter::Condition(c) if c.property_id == EMAIL => c.value.as_str().map(str::to_owned),
            _ => None,
        })
        .expect("a filter on Email")
}

fn open_menu(cx: &mut VisualTestContext, name: &str) {
    let at = rect(cx, format!("header:{name}")).center();
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    draw(cx);
}

#[test]
fn a_hosts_field_gets_the_limit_and_its_answer_is_cut() {
    let mut app = app_with_text();
    let host = TestHost::with_hook();
    let (_, cx) = open_people(&mut app, &host);
    let l = limits();
    // A filter's value, as a cell's: the host's field is told the limit, and what it answers
    // is cut to it.
    open_menu(cx, "Email");
    click(cx, "menu-item-Contains…");
    let edit = host
        .texts
        .borrow_mut()
        .pop()
        .expect("a field for the filter");
    assert_eq!(edit.max_chars, Some(l.email_chars));
    let long = "f".repeat(l.email_chars + 40);
    cx.update(|window, cx| edit.reply.commit(&long, window, cx));
    draw(cx);
    assert_eq!(email_filter(&host), "f".repeat(l.email_chars));
}

#[test]
fn a_paste_over_the_limit_is_cut_to_fit() {
    let mut app = app_with_text();
    let host = TestHost::new();
    let (table, cx) = open_people(&mut app, &host);
    let l = limits();

    // Pasted after what's there, it fills the room left and no more.
    table.update(cx, |t, cx| t.focus_cell(ANA, EMAIL, cx));
    keys(cx, "enter");
    select_all(&table, cx);
    type_text(cx, "ana@");
    paste(cx, "x".repeat(l.email_chars));
    let text = typed(&table, cx);
    assert_eq!(text.chars().count(), l.email_chars);
    assert!(text.starts_with("ana@x"), "{text}");
    keys(cx, "enter");
    assert_eq!(last_text(&host).chars().count(), l.email_chars);

    // Over a selection, it gets the room the selection frees.
    table.update(cx, |t, cx| t.focus_cell(ANA, PROFILE, cx));
    keys(cx, "enter");
    select_all(&table, cx);
    paste(
        cx,
        format!("https://example.com/{}", "y".repeat(l.url_chars)),
    );
    assert_eq!(typed(&table, cx).chars().count(), l.url_chars);
    keys(cx, "escape");
}

#[test]
fn a_value_already_over_its_limit_opens_whole_and_commits_cut() {
    let mut app = app_with_text();
    let host = TestHost::new();
    let (db, mut rows) = load("people");
    let l = limits();
    // An agent wrote more than the limit allows.
    let long = "z".repeat(l.email_chars + 30);
    let ana = rows.iter_mut().find(|r| r.id == ANA).unwrap();
    ana.values
        .get_or_insert_with(Values::new)
        .insert(EMAIL.into(), Value::text(long.clone()));
    let (table, cx) = open_with(&mut app, &host, db, rows, None, size(px(2600.), px(700.)));
    table.update(cx, |t, cx| t.focus_cell(ANA, EMAIL, cx));
    keys(cx, "enter");
    // The field holds all of it, and typing adds nothing; what's committed is cut.
    assert_eq!(typed(&table, cx), long);
    type_text(cx, "q");
    assert_eq!(typed(&table, cx), long);
    keys(cx, "backspace enter");
    assert_eq!(last_text(&host), "z".repeat(l.email_chars));
}

/// The left and right edges of the glyphs drawn within `area`, in window pixels.
fn ink(cx: &mut VisualTestContext, area: Bounds<Pixels>) -> (f32, f32) {
    draw(cx);
    cx.update(|window, _| {
        let k = window.scale_factor();
        let scene = window.rendered_scene();
        let (top, bottom) = (f32::from(area.top()), f32::from(area.bottom()));
        let (left, right) = (f32::from(area.left()), f32::from(area.right()));
        let xs: Vec<(f32, f32)> = scene
            .monochrome_sprites
            .iter()
            .map(|g| g.bounds)
            .chain(scene.subpixel_sprites.iter().map(|g| g.bounds))
            .map(|b| {
                let x = b.origin.x.as_f32() / k;
                let y = b.origin.y.as_f32() / k;
                (x, x + b.size.width.as_f32() / k, y)
            })
            .filter(|&(l, r, y)| l >= left && r <= right && y >= top && y < bottom)
            .map(|(l, r, _)| (l, r))
            .collect();
        assert!(!xs.is_empty(), "text drawn in {area:?}");
        (
            xs.iter().map(|x| x.0).fold(f32::MAX, f32::min),
            xs.iter().map(|x| x.1).fold(f32::MIN, f32::max),
        )
    })
}

#[test]
fn a_number_is_typed_at_the_right_where_its_cell_shows_it() {
    let mut app = app_with_text();
    let host = TestHost::new();
    let (table, cx) = open_people(&mut app, &host);
    table.update(cx, |t, cx| t.focus_cell(ANA, WARMTH, cx));
    let place = rect(cx, cell(ANA, WARMTH));
    let (_, shown) = ink(cx, place);
    assert!(
        shown > f32::from(place.center().x),
        "the cell shows its number at the right"
    );

    keys(cx, "enter");
    assert_eq!(typed(&table, cx), "4");
    let field = rect(cx, "editor");
    let (left, right) = ink(cx, place);
    assert!(left > f32::from(field.center().x), "{left} {field:?}");
    // Where the cell showed it, give or take the caret's room.
    assert!((right - shown).abs() < 4., "{right} {shown}");
    // And it grows to the left as it's typed.
    type_text(cx, "200");
    let (wider, again) = ink(cx, place);
    assert!(
        wider < left && (again - right).abs() < 4.,
        "{wider} {again}"
    );
    keys(cx, "escape");

    // Text is typed from the left, as before.
    table.update(cx, |t, cx| t.focus_cell(ANA, EMAIL, cx));
    keys(cx, "enter");
    select_all(&table, cx);
    type_text(cx, "a");
    let place = rect(cx, cell(ANA, EMAIL));
    let (left, _) = ink(cx, place);
    assert!(left < f32::from(rect(cx, "editor").center().x), "{left}");
    keys(cx, "escape");
}
