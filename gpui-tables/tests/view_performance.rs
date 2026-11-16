//! The table, board and list views with 10,000 rows of every type: their first draws,
//! scrolling, dragging, and working the rows out again after a sort, timed with Core Text
//! laying the text out as on screen. Each step fails past a bound, in a release build, or
//! forty times that in a debug one. On CI (`CI` set, as GitHub's runners do) the bound is four
//! times looser: the shared runners' timings swing with their load, and the strict bound is for
//! a quiet machine.
//! `cargo test --release --test view_performance -- --nocapture` prints the numbers the README
//! quotes.

#![cfg(all(target_os = "macos", feature = "gpui"))]

mod common;

use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use common::{NOW, TestHost, draw, setup};
use gpui::*;
use gpui_tables::engine::{self, time};
use gpui_tables::view::{TableHost, TableView};
use gpui_tables::*;
use gpui_tables::{Value, View};
use serde_json::json;
// The built-in #[test], not GPUI's, which gpui::* brings in.
use std::prelude::v1::test;

const ROWS: usize = 10_000;

/// A deterministic generator (xorshift), so every run draws the same rows.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

const WORDS: &[&str] = &[
    "kiln", "freight", "studio", "robot", "café", "harbour", "ledger", "pine", "orbit", "maple",
    "Über", "north", "quartz", "delta", "ember", "lumen",
];

fn database() -> Doc {
    let options = |n: usize, prefix: &str| PropertyConfig {
        options: Some(
            (0..n)
                .map(|i| SelectOption {
                    id: format!("{prefix}{i}"),
                    name: format!("{prefix} {i}"),
                    color: Color::ALL[i % Color::ALL.len()].clone(),
                })
                .collect(),
        ),
        ..PropertyConfig::default()
    };
    let p = |id: &str, kind, order: &str| Property::new(id, id, kind, order);
    let mut doc = Doc::new("db", DocKind::Database, "Ten thousand");
    doc.database = Some(DatabaseFields {
        properties: vec![
            p("name", PropertyType::Title, "a0"),
            p("note", PropertyType::Text, "a1"),
            p("n", PropertyType::Number, "a2"),
            p("stage", PropertyType::Select, "a3").with_config(options(6, "s")),
            p("tags", PropertyType::MultiSelect, "a4").with_config(options(12, "t")),
            p("when", PropertyType::Date, "a5"),
            p("done", PropertyType::Checkbox, "a6"),
            p("email", PropertyType::Email, "a7"),
            p("who", PropertyType::Person, "a8"),
            p("rel", PropertyType::Relation, "a9").with_config(PropertyConfig {
                database_id: Some("companies".into()),
                ..PropertyConfig::default()
            }),
            p("added", PropertyType::CreatedTime, "aA"),
        ],
        views: vec![View::new("v", "All", ViewType::Table, "a0")],
        ..DatabaseFields::default()
    });
    doc
}

fn rows() -> Vec<Doc> {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let orders = order::spread(ROWS);
    let today = time::days_from_civil(2026, 11, 15);
    (0..ROWS)
        .map(|i| {
            let mut w = |n: usize| {
                (0..n)
                    .map(|_| WORDS[rng.below(WORDS.len() as u64) as usize])
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            let title = w(3);
            let note = w(8);
            let mut values = Values::new();
            values.insert("note".into(), Value::text(note));
            values.insert(
                "n".into(),
                Value::Number((rng.below(100_000) as f64) / 100.0),
            );
            values.insert("stage".into(), Value::text(format!("s{}", rng.below(6))));
            let mut tags: Vec<String> = (0..rng.below(4))
                .map(|_| format!("t{}", rng.below(12)))
                .collect();
            tags.sort();
            tags.dedup();
            values.insert("tags".into(), Value::Ids(tags));
            values.insert(
                "when".into(),
                Value::date_on_day(today - 400 + rng.below(800) as i64),
            );
            values.insert("done".into(), Value::Bool(rng.below(3) == 0));
            values.insert(
                "email".into(),
                Value::text(format!("person{i}@example.com")),
            );
            values.insert(
                "who".into(),
                Value::People(vec![Person::Owner, Person::agent("agent-1")]),
            );
            values.insert("rel".into(), Value::ids([format!("c{}", rng.below(500))]));
            let mut r = Doc::row(
                format!("row-{i:05}"),
                "db",
                title,
                orders[i].clone(),
                values,
            );
            r.created = NOW - rng.below(400 * engine::DAY_MS as u64) as i64;
            r.updated = r.created;
            r
        })
        .collect()
}

fn bound(ms: u64) -> Duration {
    let ms = if cfg!(debug_assertions) { ms * 40 } else { ms };
    // Shared CI runners are loaded by other jobs; a regression large enough to matter still
    // fails at four times the bound.
    let ms = if std::env::var_os("CI").is_some() {
        ms * 4
    } else {
        ms
    };
    Duration::from_millis(ms)
}

fn report(label: &str, mut times: Vec<Duration>, bound_ms: u64) {
    times.sort();
    let median = times[times.len() / 2];
    eprintln!(
        "{label}: {:.2} ms (median of {})",
        median.as_secs_f64() * 1e3,
        times.len()
    );
    assert!(
        median < bound(bound_ms),
        "{label} took {median:?}, over {:?}",
        bound(bound_ms)
    );
}

/// One frame drawn now, timed.
fn frame(cx: &mut VisualTestContext) -> Duration {
    let start = Instant::now();
    cx.update(|window, cx| {
        window.refresh();
        let _ = window.draw(cx);
    });
    start.elapsed()
}

#[test]
fn ten_thousand_rows_draw_and_scroll_without_dropping_frames() {
    let (db, rows) = (database(), rows());
    let mut firsts = Vec::new();
    let mut scrolls = Vec::new();
    let mut sorts = Vec::new();
    for run in 0..3 {
        let mut app = TestAppContext::build_with_text_system(
            TestDispatcher::new(run),
            None,
            Arc::new(gpui_apple::AppleTextSystem::new()),
        );
        setup(&mut app);
        let host = TestHost::new();
        host.apply.set(false);
        let h: Rc<dyn TableHost> = host.clone();
        let (db, rows) = (db.clone(), rows.clone());
        let made: Rc<std::cell::RefCell<Option<Entity<TableView>>>> = Rc::default();
        let (_, cx) = app.add_window_view({
            let made = made.clone();
            move |window, cx| {
                let t = cx.new(|cx| TableView::new(h, db, rows, cx));
                *made.borrow_mut() = Some(t.clone());
                gpui_core::Root::new(t, window, cx)
            }
        });
        let table = made.borrow().clone().unwrap();
        cx.simulate_resize(size(px(1400.), px(900.)));
        // The first draw: the rows worked out, the visible ones laid out.
        table.update(cx, |t, cx| t.set_rows(t.rows().to_vec(), cx));
        firsts.push(frame(cx));
        draw(cx);
        assert_eq!(table.update(cx, |t, _| t.shown_rows().len()), ROWS);
        // Scrolling a page at a time.
        for _ in 0..20 {
            cx.simulate_event(ScrollEvent {
                position: point(px(600.), px(400.)),
                delta: ScrollDelta::Pixels(point(px(0.), px(-800.))),
                ..Default::default()
            });
            scrolls.push(frame(cx));
        }
        // And sideways.
        for _ in 0..5 {
            cx.simulate_event(ScrollEvent {
                position: point(px(600.), px(400.)),
                delta: ScrollDelta::Pixels(point(px(-120.), px(0.))),
                ..Default::default()
            });
            scrolls.push(frame(cx));
        }
        assert!(table.update(cx, |t, _| t.scroll_x()) > px(0.));
        // A sort and a group: the rows worked out again and drawn.
        let mut view = table.read_with(cx, |t, _| t.view().unwrap().clone());
        view.sort = vec![Sort::ascending("name")];
        view.group_by = Some("stage".into());
        view.filter = Some(Filter::condition(
            "note",
            FilterOp::DoesNotContain,
            json!("zzz"),
        ));
        let mut db = table.read_with(cx, |t, _| t.database().clone());
        db.database.as_mut().unwrap().views[0] = view;
        let start = Instant::now();
        table.update(cx, |t, cx| t.set_database(db, cx));
        cx.update(|window, cx| {
            window.refresh();
            let _ = window.draw(cx);
        });
        sorts.push(start.elapsed());
    }
    report("first draw of 10,000 rows", firsts, 50);
    report("a frame while scrolling", scrolls, 12);
    report("sort, filter and group, then draw", sorts, 50);
}

#[test]
fn ten_thousand_cards_draw_and_scroll_without_dropping_frames() {
    let (mut db, rows) = (database(), rows());
    // A board of the same rows: six columns of about 1,700 cards, each showing four values.
    let mut board = View::new("b", "Board", ViewType::Board, "a1");
    board.group_by = Some("stage".into());
    board.card_properties = vec!["note".into(), "n".into(), "tags".into(), "when".into()];
    db.database.as_mut().unwrap().views = vec![board];
    let mut firsts = Vec::new();
    let mut scrolls = Vec::new();
    let mut drags = Vec::new();
    for run in 0..3 {
        let mut app = TestAppContext::build_with_text_system(
            TestDispatcher::new(run),
            None,
            Arc::new(gpui_apple::AppleTextSystem::new()),
        );
        setup(&mut app);
        let host = TestHost::new();
        host.apply.set(false);
        let h: Rc<dyn TableHost> = host.clone();
        let (db, rows) = (db.clone(), rows.clone());
        let made: Rc<std::cell::RefCell<Option<Entity<TableView>>>> = Rc::default();
        let (_, cx) = app.add_window_view({
            let made = made.clone();
            move |window, cx| {
                let t = cx.new(|cx| TableView::new(h, db, rows, cx));
                *made.borrow_mut() = Some(t.clone());
                gpui_core::Root::new(t, window, cx)
            }
        });
        let table = made.borrow().clone().unwrap();
        cx.simulate_resize(size(px(1400.), px(900.)));
        table.update(cx, |t, cx| t.set_rows(t.rows().to_vec(), cx));
        firsts.push(frame(cx));
        draw(cx);
        let cols = table.update(cx, |t, _| t.board_columns().to_vec());
        assert_eq!(cols.len(), 7, "no stage, then six options");
        assert_eq!(cols.iter().map(|c| c.rows.len()).sum::<usize>(), ROWS);
        // Down a column a page at a time.
        for _ in 0..20 {
            cx.simulate_event(ScrollEvent {
                position: point(px(450.), px(400.)),
                delta: ScrollDelta::Pixels(point(px(0.), px(-800.))),
                ..Default::default()
            });
            scrolls.push(frame(cx));
        }
        // And the board sideways.
        for _ in 0..5 {
            cx.simulate_event(ScrollEvent {
                position: point(px(600.), px(400.)),
                delta: ScrollDelta::Pixels(point(px(-120.), px(0.))),
                ..Default::default()
            });
            scrolls.push(frame(cx));
        }
        assert!(table.read_with(cx, |t, _| t.board_scroll_x()) > px(0.));
        // A card dragged across the columns, a frame for each move.
        let from = table
            .update(cx, |t, _| {
                let r = t.board_columns()[2].rows[0];
                let id = t.rows()[r].id.clone();
                t.card_bounds(&id)
            })
            .expect("a card on screen")
            .center();
        cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
        frame(cx);
        for i in 1..=10 {
            let at = from + point(px(60. * i as f32), px(15. * i as f32));
            cx.simulate_mouse_move(at, MouseButton::Left, Modifiers::none());
            drags.push(frame(cx));
        }
        assert!(table.read_with(cx, |t, _| t.is_dragging_card()));
        cx.simulate_mouse_up(from, MouseButton::Left, Modifiers::none());
    }
    report("first draw of a board of 10,000 cards", firsts, 50);
    report("a frame while scrolling the board", scrolls, 12);
    report("a frame while dragging a card", drags, 12);
}

#[test]
fn ten_thousand_lines_draw_and_scroll_without_dropping_frames() {
    let (mut db, rows) = (database(), rows());
    // A list of the same rows, grouped by stage, each line showing four values.
    let mut list = View::new("l", "List", ViewType::List, "a2");
    list.group_by = Some("stage".into());
    list.card_properties = vec!["stage".into(), "tags".into(), "when".into(), "done".into()];
    db.database.as_mut().unwrap().views = vec![list];
    let mut firsts = Vec::new();
    let mut scrolls = Vec::new();
    let mut drags = Vec::new();
    for run in 0..3 {
        let mut app = TestAppContext::build_with_text_system(
            TestDispatcher::new(run),
            None,
            Arc::new(gpui_apple::AppleTextSystem::new()),
        );
        setup(&mut app);
        let host = TestHost::new();
        host.apply.set(false);
        let h: Rc<dyn TableHost> = host.clone();
        let (db, rows) = (db.clone(), rows.clone());
        let made: Rc<std::cell::RefCell<Option<Entity<TableView>>>> = Rc::default();
        let (_, cx) = app.add_window_view({
            let made = made.clone();
            move |window, cx| {
                let t = cx.new(|cx| TableView::new(h, db, rows, cx));
                *made.borrow_mut() = Some(t.clone());
                gpui_core::Root::new(t, window, cx)
            }
        });
        let table = made.borrow().clone().unwrap();
        cx.simulate_resize(size(px(1400.), px(900.)));
        table.update(cx, |t, cx| t.set_rows(t.rows().to_vec(), cx));
        firsts.push(frame(cx));
        draw(cx);
        let groups = table.update(cx, |t, _| t.list_groups().to_vec());
        assert_eq!(groups.len(), 7, "no stage, then six options");
        assert_eq!(groups.iter().map(|g| g.rows.len()).sum::<usize>(), ROWS);
        // Down the list a page at a time.
        for _ in 0..20 {
            cx.simulate_event(ScrollEvent {
                position: point(px(600.), px(400.)),
                delta: ScrollDelta::Pixels(point(px(0.), px(-800.))),
                ..Default::default()
            });
            scrolls.push(frame(cx));
        }
        // A row dragged down the list, a frame for each move.
        let from = table
            .update(cx, |t, _| {
                let shown: Vec<usize> = t
                    .list_groups()
                    .iter()
                    .flat_map(|g| g.rows.iter().copied())
                    .collect();
                let ids: Vec<String> = shown.iter().map(|&r| t.rows()[r].id.clone()).collect();
                ids.iter().find_map(|id| {
                    t.line_bounds(id)
                        .filter(|b| b.top() > px(100.) && b.bottom() < px(300.))
                })
            })
            .expect("a line on screen")
            .center();
        cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
        frame(cx);
        for i in 1..=10 {
            let at = from + point(px(4. * i as f32), px(40. * i as f32));
            cx.simulate_mouse_move(at, MouseButton::Left, Modifiers::none());
            drags.push(frame(cx));
        }
        assert!(table.read_with(cx, |t, _| t.is_dragging_card()));
        cx.simulate_mouse_up(from, MouseButton::Left, Modifiers::none());
    }
    report("first draw of a list of 10,000 rows", firsts, 50);
    report("a frame while scrolling the list", scrolls, 12);
    report("a frame while dragging a row", drags, 12);
}
