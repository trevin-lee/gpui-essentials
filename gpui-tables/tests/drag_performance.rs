//! How long a frame of a drag takes with 10,000 rows, in a table (a row by its handle), a board
//! (a card across the columns) and a list (a line down it): a move of the pointer a frame, the
//! landing following it, with the test platform's own text system. Gated to the Mac like the
//! other view tests (the crate takes GPUI's test support there only); it runs on Linux too with
//! that support added. Ignored by default:
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

fn database(view: View) -> Doc {
    let p = |id: &str, kind, order: &str| Property::new(id, id, kind, order);
    let stages = PropertyConfig {
        options: Some(
            (0..6)
                .map(|i| SelectOption {
                    id: format!("s{i}"),
                    name: format!("Stage {i}"),
                    color: Color::ALL[i % Color::ALL.len()].clone(),
                })
                .collect(),
        ),
        ..PropertyConfig::default()
    };
    let mut doc = Doc::new("db", DocKind::Database, "Ten thousand");
    doc.database = Some(DatabaseFields {
        properties: vec![
            p("name", PropertyType::Title, "a0"),
            p("note", PropertyType::Text, "a1"),
            p("n", PropertyType::Number, "a2"),
            p("stage", PropertyType::Select, "a3").with_config(stages),
        ],
        views: vec![view],
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
            values.insert("stage".into(), Value::text(format!("s{}", i % 6)));
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

type Grip = fn(&TableView, &str) -> Option<Bounds<Pixels>>;

/// Opens `view` over the rows, picks row `id` up by `grip` and times `moves` frames of the drag.
fn time_drag(
    cx: &mut TestAppContext,
    view: View,
    id: &str,
    grip: Grip,
    hover_first: bool,
    moves: impl Fn(usize) -> Point<Pixels>,
) -> Vec<Duration> {
    setup(cx);
    let host = TestHost::new();
    host.apply.set(false);
    let h: Rc<dyn TableHost> = host.clone();
    let made: Rc<std::cell::RefCell<Option<Entity<TableView>>>> = Rc::default();
    let (_, cx) = cx.add_window_view({
        let made = made.clone();
        move |window, cx| {
            let t = cx.new(|cx| TableView::new(h, database(view), rows(), cx));
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
    if hover_first {
        // A table row's handle is made for the row under the pointer.
        let row = table
            .read_with(cx, |t, _| t.line_bounds(id))
            .or_else(|| table.read_with(cx, |t, _| grip(t, id)));
        let y = row.map_or(px(160.), |b| b.center().y);
        cx.simulate_mouse_move(point(px(400.), y), None, Modifiers::none());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    }
    let from = table
        .read_with(cx, |t, _| grip(t, id))
        .expect("the row's grip is drawn")
        .center();
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
        let at = moves(i);
        let start = Instant::now();
        cx.simulate_mouse_move(at, MouseButton::Left, Modifiers::none());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        times.push(start.elapsed());
    }
    cx.simulate_mouse_up(moves(0), MouseButton::Left, Modifiers::none());
    cx.run_until_parked();
    times
}

fn report(what: &str, mut times: Vec<Duration>) {
    times.sort();
    let ms = |d: Duration| d.as_secs_f64() * 1e3;
    println!(
        "a frame of {what} at 10,000 rows: median {:.3} ms, p95 {:.3} ms, max {:.3} ms over {}",
        ms(times[times.len() / 2]),
        ms(times[times.len() * 95 / 100]),
        ms(times[times.len() - 1]),
        times.len()
    );
}

#[gpui::test]
#[ignore]
fn frame_time_of_a_row_drag_at_10000_rows(cx: &mut TestAppContext) {
    let times = time_drag(
        cx,
        View::new("v", "All", ViewType::Table, "a0"),
        "row-00003",
        |t, id| t.handle_bounds(id),
        true,
        |i| point(px(400.), px(120. + (i % 500) as f32)),
    );
    report("a row drag", times);
}

#[gpui::test]
#[ignore]
fn frame_time_of_a_card_drag_at_10000_rows(cx: &mut TestAppContext) {
    let mut view = View::new("v", "Board", ViewType::Board, "a0");
    view.group_by = Some("stage".into());
    // Across the columns and down them, back and forth.
    let times = time_drag(
        cx,
        view,
        // In the first stage's column (an empty column comes first).
        "row-00000",
        |t, id| t.card_bounds(id),
        false,
        |i| {
            let i = (i % 150) as f32;
            point(px(120. + i * 6.), px(160. + i * 3.))
        },
    );
    report("a card drag", times);
}

#[gpui::test]
#[ignore]
fn frame_time_of_a_list_drag_at_10000_rows(cx: &mut TestAppContext) {
    let times = time_drag(
        cx,
        View::new("v", "List", ViewType::List, "a0"),
        "row-00003",
        |t, id| t.line_bounds(id),
        false,
        |i| point(px(400.), px(120. + (i % 500) as f32)),
    );
    report("a list drag", times);
}
