//! How long a frame of a row drag takes in a table of 10,000 rows: a move of the pointer a
//! frame, the landing line following it, with the test platform's own text system. Gated to
//! the Mac like the other view tests (the crate takes GPUI's test support there only); it runs
//! on Linux too with that support added. Ignored by default:
//! `cargo test --release -p gpui-tables --test drag_performance -- --ignored --nocapture`.

#![cfg(all(target_os = "macos", feature = "gpui"))]

mod common;

use std::rc::Rc;
use std::time::{Duration, Instant};

use common::{TestHost, setup};
use gpui::*;
use gpui_tables::view::{TableHost, TableView};
use gpui_tables::*;
use gpui_tables::{Value, View};
// The built-in #[test], not GPUI's, which gpui::* brings in.
use std::prelude::v1::test;

const ROWS: usize = 10_000;

fn database() -> Doc {
    let p = |id: &str, kind, order: &str| Property::new(id, id, kind, order);
    let mut doc = Doc::new("db", DocKind::Database, "Ten thousand");
    doc.database = Some(DatabaseFields {
        properties: vec![
            p("name", PropertyType::Title, "a0"),
            p("note", PropertyType::Text, "a1"),
            p("n", PropertyType::Number, "a2"),
        ],
        views: vec![View::new("v", "All", ViewType::Table, "a0")],
        ..DatabaseFields::default()
    });
    doc
}

fn rows() -> Vec<Doc> {
    let orders = order::spread(ROWS);
    (0..ROWS)
        .map(|i| {
            let mut values = Values::new();
            values.insert("note".into(), Value::text(format!("note {i}")));
            values.insert("n".into(), Value::Number(i as f64));
            Doc::row(
                format!("row-{i:05}"),
                "db",
                format!("Row {i}"),
                orders[i].clone(),
                values,
            )
        })
        .collect()
}

#[gpui::test]
#[ignore]
fn frame_time_of_a_row_drag_at_10000_rows(cx: &mut TestAppContext) {
    setup(cx);
    let host = TestHost::new();
    host.apply.set(false);
    let h: Rc<dyn TableHost> = host.clone();
    let made: Rc<std::cell::RefCell<Option<Entity<TableView>>>> = Rc::default();
    let (_, cx) = cx.add_window_view({
        let made = made.clone();
        move |window, cx| {
            let t = cx.new(|cx| TableView::new(h, database(), rows(), cx));
            *made.borrow_mut() = Some(t.clone());
            gpui_core::Root::new(t, window, cx)
        }
    });
    let table = made.borrow().clone().unwrap();
    cx.simulate_resize(size(px(1200.), px(800.)));
    for _ in 0..2 {
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
    }
    // Over row 3, so its handle is made; then pressed and carried down.
    let row = table.read_with(cx, |t, _| t.handle_bounds("row-00003"));
    let hover = |cx: &mut VisualTestContext, at| {
        cx.simulate_mouse_move(at, None, Modifiers::none());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    };
    let guess = row.map_or(point(px(30.), px(160.)), |b| b.center());
    hover(cx, point(px(400.), guess.y));
    let handle = table
        .read_with(cx, |t, _| t.handle_bounds("row-00003"))
        .expect("the row's handle is drawn");
    let from = handle.center();
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    cx.simulate_mouse_move(
        from + point(px(0.), px(8.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    assert!(
        table.read_with(cx, |t, _| t.is_dragging_card()),
        "the drag began"
    );
    let mut times = Vec::new();
    for i in 0..300 {
        let at = point(px(400.), px(120. + (i % 500) as f32));
        let start = Instant::now();
        cx.simulate_mouse_move(at, MouseButton::Left, Modifiers::none());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        times.push(start.elapsed());
    }
    cx.simulate_mouse_up(
        point(px(400.), px(300.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    cx.run_until_parked();
    times.sort();
    let ms = |d: Duration| d.as_secs_f64() * 1e3;
    println!(
        "a frame of a row drag at 10,000 rows: median {:.3} ms, p95 {:.3} ms, max {:.3} ms over {}",
        ms(times[times.len() / 2]),
        ms(times[times.len() * 95 / 100]),
        ms(times[times.len() - 1]),
        times.len()
    );
}
