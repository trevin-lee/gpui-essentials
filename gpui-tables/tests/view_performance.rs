//! The table, board and list views with 10,000 rows of every type: their first draws,
//! scrolling, dragging, and working the rows out again after a sort, timed with Core Text
//! laying the text out as on screen. Each step fails past a bound, in a release build, or
//! forty times that in a debug one. On CI (`CI` set, as GitHub's runners do) the bound is four
//! times looser: the shared runners' timings swing with their load, and the strict bound is for
//! a quiet machine.
//!
//! Each step is timed with the one draw that does its work: a test window draws as each update
//! and each event ends, so that draw is in the update's or event's time. A redraw after it would
//! come from GPUI's caches (no new text to lay out) and measure nothing the step costs.
//!
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

/// A redraw now, timed: nothing new to lay out, so it mostly comes from GPUI's caches.
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
    let mut works = Vec::new();
    let mut keys = Vec::new();
    for run in 0..5 {
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
        let start = Instant::now();
        table.update(cx, |t, cx| t.set_rows(t.rows().to_vec(), cx));
        firsts.push(start.elapsed());
        draw(cx);
        assert_eq!(table.update(cx, |t, _| t.shown_rows().len()), ROWS);
        // Scrolling a page at a time.
        for _ in 0..20 {
            let start = Instant::now();
            cx.simulate_event(ScrollEvent {
                position: point(px(600.), px(400.)),
                delta: ScrollDelta::Pixels(point(px(0.), px(-800.))),
                ..Default::default()
            });
            scrolls.push(start.elapsed());
        }
        // And sideways: a new trackpad gesture, so it starts as one (a gesture keeps
        // to the way it started in).
        for i in 0..5 {
            let start = Instant::now();
            cx.simulate_event(ScrollEvent {
                position: point(px(600.), px(400.)),
                delta: ScrollDelta::Pixels(point(px(-120.), px(0.))),
                touch_phase: if i == 0 {
                    TouchPhase::Started
                } else {
                    TouchPhase::Moved
                },
                ..Default::default()
            });
            scrolls.push(start.elapsed());
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
        // Worked out, timed alone inside the update; then with the update's own draw (a test's
        // update ends in one), the step as a person sees it: one draw, its new text laid out.
        let start = Instant::now();
        works.push(table.update(cx, |t, cx| {
            t.set_database(db, cx);
            let start = Instant::now();
            let _ = t.shown_rows().len();
            start.elapsed()
        }));
        sorts.push(start.elapsed());
        // A filter's value typed a key at a time: the same rows, their notes kept lowercased.
        for typed in ["h", "ha", "har", "harb"] {
            let mut db = table.read_with(cx, |t, _| t.database().clone());
            let view = &mut db.database.as_mut().unwrap().views[0];
            view.filter = Some(Filter::condition("note", FilterOp::Contains, json!(typed)));
            keys.push(table.update(cx, |t, cx| {
                t.set_database(db, cx);
                let start = Instant::now();
                let _ = t.shown_rows().len();
                start.elapsed()
            }));
            draw(cx);
        }
    }
    report("first draw of 10,000 rows", firsts, 50);
    // A 60 Hz frame: a page of new rows lays out about 160 lines of text.
    report("a frame while scrolling", scrolls, 16);
    report("sort, filter and group, worked out", works, 25);
    report("sort, filter and group, then draw", sorts, 50);
    report("a filter's value typed, worked out", keys, 25);
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
        let start = Instant::now();
        table.update(cx, |t, cx| t.set_rows(t.rows().to_vec(), cx));
        firsts.push(start.elapsed());
        draw(cx);
        let cols = table.update(cx, |t, _| t.board_columns().to_vec());
        assert_eq!(cols.len(), 7, "no stage, then six options");
        assert_eq!(cols.iter().map(|c| c.rows.len()).sum::<usize>(), ROWS);
        // Down a column a page at a time.
        for _ in 0..20 {
            let start = Instant::now();
            cx.simulate_event(ScrollEvent {
                position: point(px(450.), px(400.)),
                delta: ScrollDelta::Pixels(point(px(0.), px(-800.))),
                ..Default::default()
            });
            scrolls.push(start.elapsed());
        }
        // And the board sideways: a new trackpad gesture, so it starts as one (a gesture keeps
        // to the way it started in).
        for i in 0..5 {
            let start = Instant::now();
            cx.simulate_event(ScrollEvent {
                position: point(px(600.), px(400.)),
                delta: ScrollDelta::Pixels(point(px(-120.), px(0.))),
                touch_phase: if i == 0 {
                    TouchPhase::Started
                } else {
                    TouchPhase::Moved
                },
                ..Default::default()
            });
            scrolls.push(start.elapsed());
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
            let start = Instant::now();
            cx.simulate_mouse_move(at, MouseButton::Left, Modifiers::none());
            drags.push(start.elapsed());
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
        let start = Instant::now();
        table.update(cx, |t, cx| t.set_rows(t.rows().to_vec(), cx));
        firsts.push(start.elapsed());
        draw(cx);
        let groups = table.update(cx, |t, _| t.list_groups().to_vec());
        assert_eq!(groups.len(), 7, "no stage, then six options");
        assert_eq!(groups.iter().map(|g| g.rows.len()).sum::<usize>(), ROWS);
        // Down the list a page at a time.
        for _ in 0..20 {
            let start = Instant::now();
            cx.simulate_event(ScrollEvent {
                position: point(px(600.), px(400.)),
                delta: ScrollDelta::Pixels(point(px(0.), px(-800.))),
                ..Default::default()
            });
            scrolls.push(start.elapsed());
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
            let start = Instant::now();
            cx.simulate_mouse_move(at, MouseButton::Left, Modifiers::none());
            drags.push(start.elapsed());
        }
        assert!(table.read_with(cx, |t, _| t.is_dragging_card()));
        cx.simulate_mouse_up(from, MouseButton::Left, Modifiers::none());
    }
    report("first draw of a list of 10,000 rows", firsts, 50);
    report("a frame while scrolling the list", scrolls, 12);
    report("a frame while dragging a row", drags, 12);
}

/// A grouped table regrouped as a value changes: one row's value changed by the host, then the
/// rows worked out again (the query, the groups and the lines), and then drawn, for each type
/// that groups.
#[test]
fn ten_thousand_rows_regroup_as_a_value_changes() {
    let (db, rows) = (database(), rows());
    let today = time::days_from_civil(2026, 11, 15);
    let mut app = TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        Arc::new(gpui_apple::AppleTextSystem::new()),
    );
    setup(&mut app);
    let host = TestHost::new();
    host.apply.set(false);
    let h: Rc<dyn TableHost> = host.clone();
    let made: Rc<std::cell::RefCell<Option<Entity<TableView>>>> = Rc::default();
    let (_, cx) = app.add_window_view({
        let (made, db, rows) = (made.clone(), db.clone(), rows.clone());
        move |window, cx| {
            let t = cx.new(|cx| TableView::new(h, db, rows, cx));
            *made.borrow_mut() = Some(t.clone());
            gpui_core::Root::new(t, window, cx)
        }
    });
    let table = made.borrow().clone().unwrap();
    cx.simulate_resize(size(px(1400.), px(900.)));
    let cases = [
        (
            "select",
            "stage",
            None,
            Value::text("s1"),
            Value::text("s4"),
        ),
        (
            "multi-select",
            "tags",
            None,
            Value::ids(["t2"]),
            Value::ids(["t5", "t9"]),
        ),
        (
            "date by month",
            "when",
            Some(DateBucket::Month),
            Value::date_on_day(today - 200),
            Value::date_on_day(today + 90),
        ),
        (
            "person",
            "who",
            None,
            Value::People(vec![Person::Owner]),
            Value::People(vec![Person::agent("agent-3")]),
        ),
    ];
    for (label, by, bucket, a, b) in cases {
        let mut db = db.clone();
        let view = &mut db.database.as_mut().unwrap().views[0];
        view.group_by = Some(by.into());
        view.group_date = bucket;
        table.update(cx, |t, cx| t.set_database(db, cx));
        draw(cx);
        let mut works = Vec::new();
        let mut frames = Vec::new();
        for n in 0..9 {
            // The host's new rows, made before the clock starts: one value changed.
            let mut next = rows.clone();
            let row = (n * 7_919 + 11) % ROWS;
            let value = if n.is_multiple_of(2) {
                a.clone()
            } else {
                b.clone()
            };
            next[row].values.as_mut().unwrap().insert(by.into(), value);
            // Worked out, timed inside the update; drawn, the whole update (a test window draws
            // as each update ends).
            let start = Instant::now();
            let (work, groups) = table.update(cx, |t, cx| {
                t.set_rows(next, cx);
                let start = Instant::now();
                let groups = t.groups().len();
                (start.elapsed(), groups)
            });
            assert!(groups > 1);
            works.push(work);
            frames.push(start.elapsed());
        }
        report(&format!("a {label} value changed, worked out"), works, 10);
        report(
            &format!("a {label} value changed, worked out and drawn"),
            frames,
            25,
        );
    }
}

#[test]
fn ten_thousand_rows_searched_a_key_at_a_time() {
    let (db, rows) = (database(), rows());
    let mut opens = Vec::new();
    let mut folds = Vec::new();
    let mut firsts = Vec::new();
    let mut first_frames = Vec::new();
    let mut unready = Vec::new();
    let mut keys = Vec::new();
    let mut frames = Vec::new();
    for run in 0..5 {
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
                let t = cx.new(|cx| TableView::new(h, db, rows, cx).with_view_bar(true));
                *made.borrow_mut() = Some(t.clone());
                gpui_core::Root::new(t, window, cx)
            }
        });
        let table = made.borrow().clone().unwrap();
        cx.simulate_resize(size(px(1400.), px(900.)));
        draw(cx);
        // Opening the field (its button, ⌘F), drawn with it: the rows' words are handed to the
        // background to fold (a test's update ends in a draw of its own).
        let start = Instant::now();
        cx.update(|window, cx| table.update(cx, |t, cx| t.open_search(window, cx)));
        opens.push(start.elapsed());
        // The fold, off the main thread in an app (a test runs it here, when parked).
        let start = Instant::now();
        cx.run_until_parked();
        folds.push(start.elapsed());
        // The first key only searches them: timed alone, then with its update's own draw.
        let start = Instant::now();
        firsts.push(table.update(cx, |t, cx| {
            let start = Instant::now();
            t.set_search(Some("h".into()), cx);
            start.elapsed()
        }));
        first_frames.push(start.elapsed());
        // Each key after that searches them again.
        for typed in [
            "ha", "har", "harb", "harbo", "harbou", "harbour", "harbou", "harbo",
        ] {
            // The search alone, then with the update's own draw (a test's update ends in one).
            let start = Instant::now();
            let key = table.update(cx, |t, cx| {
                let start = Instant::now();
                t.set_search(Some(typed.into()), cx);
                start.elapsed()
            });
            keys.push(key);
            frames.push(start.elapsed());
            let found = table.update(cx, |t, _| t.shown_rows().len());
            assert!(found > 0 && found < ROWS, "{typed}: {found}");
        }
        table.update(cx, |t, cx| t.set_search(None, cx));
        assert_eq!(table.update(cx, |t, _| t.shown_rows().len()), ROWS);
        // A first key with nothing folded yet (the docs just changed, and the host's search
        // opens the field with its query): folded then, on the main thread.
        let next = table.update(cx, |t, _| t.rows().to_vec());
        unready.push(table.update(cx, |t, cx| {
            t.set_rows(next, cx);
            let start = Instant::now();
            t.set_search(Some("h".into()), cx);
            start.elapsed()
        }));
        table.update(cx, |t, cx| t.set_search(None, cx));
    }
    report(
        "opening the search field over 10,000 rows, drawn",
        opens,
        25,
    );
    report("folding their words in the background", folds, 50);
    report("the first key of a search of 10,000 rows", firsts, 3);
    report("the first key, drawn", first_frames, 12);
    report("each key after it", keys, 3);
    report("each key after it, drawn", frames, 12);
    report("a first key before the fold is ready", unready, 50);
}
