//! 10,000 rows: what a view costs to work out. Each test times the work, prints it, and fails
//! past a bound loose enough for a debug build on a slow CI runner, yet tight enough to catch a
//! step that went quadratic. `cargo test --release --test performance -- --nocapture` prints the
//! numbers the README quotes.

use std::time::{Duration, Instant};

use gpui_tables::engine::{self, Context, query, time};
use gpui_tables::*;
use serde_json::json;

const ROWS: usize = 10_000;

/// A deterministic generator (xorshift), so every run sorts the same rows.
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

fn now() -> i64 {
    time::days_from_civil(2026, 11, 15) * engine::DAY_MS + 12 * 3_600_000
}

fn database() -> DatabaseFields {
    let options = |n: usize, prefix: &str| PropertyConfig {
        options: Some(
            (0..n)
                .map(|i| SelectOption {
                    id: format!("{prefix}{i}"),
                    name: format!("{prefix} {i}"),
                    color: Color::Default,
                })
                .collect(),
        ),
        ..PropertyConfig::default()
    };
    let p = |id: &str, kind| Property::new(id, id, kind, "a0");
    DatabaseFields {
        properties: vec![
            p("name", PropertyType::Title),
            p("note", PropertyType::Text),
            p("n", PropertyType::Number),
            p("stage", PropertyType::Select).with_config(options(6, "s")),
            p("tags", PropertyType::MultiSelect).with_config(options(12, "t")),
            p("when", PropertyType::Date),
            p("done", PropertyType::Checkbox),
            p("email", PropertyType::Email),
            p("who", PropertyType::Person),
            p("rel", PropertyType::Relation).with_config(PropertyConfig {
                database_id: Some("companies".into()),
                ..PropertyConfig::default()
            }),
            p("added", PropertyType::CreatedTime),
        ],
        ..DatabaseFields::default()
    }
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
            if rng.below(10) > 0 {
                values.insert(
                    "n".into(),
                    Value::Number((rng.below(100_000) as f64) / 100.0),
                );
            }
            if rng.below(8) > 0 {
                values.insert("stage".into(), Value::text(format!("s{}", rng.below(6))));
            }
            let mut tags: Vec<String> = (0..rng.below(4))
                .map(|_| format!("t{}", rng.below(12)))
                .collect();
            tags.sort();
            tags.dedup();
            values.insert("tags".into(), Value::Ids(tags));
            if rng.below(5) > 0 {
                let day = today - 400 + rng.below(800) as i64;
                values.insert("when".into(), Value::date_on_day(day));
            }
            values.insert("done".into(), Value::Bool(rng.below(3) == 0));
            values.insert(
                "email".into(),
                Value::text(format!("person{i}@example.com")),
            );
            let who = match rng.below(4) {
                0 => vec![],
                1 => vec![Person::Owner],
                2 => vec![Person::agent(format!("agent-{}", rng.below(5)))],
                _ => vec![
                    Person::Owner,
                    Person::agent(format!("agent-{}", rng.below(5))),
                ],
            };
            values.insert("who".into(), Value::People(who));
            let mut rel: Vec<String> = (0..rng.below(3))
                .map(|_| format!("c{}", rng.below(500)))
                .collect();
            rel.sort();
            rel.dedup();
            values.insert("rel".into(), Value::Ids(rel));
            let mut r = Doc::row(
                format!("row-{i:05}"),
                "db",
                title,
                orders[i].clone(),
                values,
            );
            r.created = now() - rng.below(400 * engine::DAY_MS as u64) as i64;
            r.updated = r.created;
            r
        })
        .collect()
}

/// The median of five runs of `f`, after one to warm up, which must be under `bound_ms` in a
/// release build, or 40 times that in a debug one (unoptimized iterators and hashing are that
/// much slower, and CI runners are shared).
fn time<T>(label: &str, bound_ms: u64, mut f: impl FnMut() -> T) -> T {
    let bound_ms = if cfg!(debug_assertions) {
        bound_ms * 40
    } else {
        bound_ms
    };
    let mut out = f();
    let mut times: Vec<Duration> = (0..5)
        .map(|_| {
            let start = Instant::now();
            out = f();
            start.elapsed()
        })
        .collect();
    times.sort();
    let median = times[2];
    eprintln!("{label}: {:.2} ms", median.as_secs_f64() * 1e3);
    // Four times looser on CI (`CI` set): shared runners' timings swing with their load.
    let bound_ms = if std::env::var_os("CI").is_some() {
        bound_ms * 4
    } else {
        bound_ms
    };
    assert!(
        median < Duration::from_millis(bound_ms),
        "{label} took {median:?}, over {bound_ms} ms"
    );
    out
}

fn view() -> View {
    View::new("v", "All", ViewType::Table, "a0")
}

#[test]
fn filtering_sorting_and_grouping_10000_rows() {
    let (db, rows) = (database(), rows());
    let cx = Context::new(now());

    let plain = time("manual order only", 10, || query(&db, &view(), &rows, &cx));
    assert_eq!(plain.rows.len(), ROWS);
    // spread() made the keys in order, so the manual order is the generated order.
    assert!(plain.rows.windows(2).all(|w| w[0] < w[1]));

    let mut by_title = view();
    by_title.sort = vec![Sort::ascending("name")];
    let out = time("sort by title", 25, || query(&db, &by_title, &rows, &cx));
    let folded = |i: usize| rows[i].title.to_lowercase();
    assert!(out.rows.windows(2).all(|w| folded(w[0]) <= folded(w[1])));

    let mut two = view();
    two.sort = vec![Sort::ascending("stage"), Sort::descending("n")];
    let out = time("sort by select, then number", 25, || {
        query(&db, &two, &rows, &cx)
    });
    assert_eq!(out.rows.len(), ROWS);

    let mut busy = view();
    busy.filter = Some(Filter::And {
        and: vec![
            Filter::condition("note", FilterOp::Contains, json!("harbour")),
            Filter::Or {
                or: vec![
                    Filter::condition("n", FilterOp::Greater, json!(250)),
                    Filter::condition("tags", FilterOp::Contains, json!("t3")),
                ],
            },
            Filter::condition("when", FilterOp::Within, json!("pastYear")),
        ],
    });
    busy.sort = vec![Sort::descending("when"), Sort::ascending("name")];
    busy.group_by = Some("stage".into());
    let out = time("filter, two sorts and group by select", 25, || {
        query(&db, &busy, &rows, &cx)
    });
    assert!(out.issues.is_empty());
    let grouped: usize = out
        .groups
        .as_ref()
        .unwrap()
        .iter()
        .map(|g| g.rows.len())
        .sum();
    assert_eq!(grouped, out.rows.len());
    assert!(!out.rows.is_empty() && out.rows.len() < ROWS / 2);

    for (label, by, bucket) in [
        ("group by multi-select", "tags", None),
        ("group by person", "who", None),
        ("group by date, relative", "when", None),
        ("group by created month", "added", Some(DateBucket::Month)),
        ("group by checkbox", "done", None),
    ] {
        let mut v = view();
        v.group_by = Some(by.into());
        v.group_date = bucket;
        let out = time(label, 25, || query(&db, &v, &rows, &cx));
        assert!(out.issues.is_empty());
        assert!(out.groups.unwrap().len() > 1);
    }
}

#[test]
fn every_operator_over_10000_rows() {
    let (db, rows) = (database(), rows());
    let cx = Context::new(now());
    let ops = [
        ("name", FilterOp::Is, json!("kiln kiln kiln")),
        ("name", FilterOp::StartsWith, json!("über")),
        ("note", FilterOp::DoesNotContain, json!("café")),
        ("email", FilterOp::IsNotEmpty, json!(null)),
        ("n", FilterOp::LessOrEqual, json!(10)),
        ("stage", FilterOp::IsNot, json!("s2")),
        ("tags", FilterOp::IsEmpty, json!(null)),
        ("when", FilterOp::OnOrAfter, json!("2026-01-01")),
        ("added", FilterOp::Within, json!("pastMonth")),
        ("done", FilterOp::IsChecked, json!(null)),
        ("who", FilterOp::Contains, json!("agent:agent-3")),
        ("rel", FilterOp::Contains, json!("c42")),
    ];
    let mut v = view();
    v.filter = Some(Filter::Or {
        or: ops
            .iter()
            .map(|(p, op, value)| Filter::condition(*p, op.clone(), value.clone()))
            .collect(),
    });
    let out = time("twelve conditions, one per type", 25, || {
        query(&db, &v, &rows, &cx)
    });
    assert!(out.issues.is_empty());
}

#[test]
fn reading_and_checking_10000_rows() {
    let rows = rows();
    let db = database();
    let json = serde_json::to_string(&rows).unwrap();
    eprintln!("10,000 rows as JSON: {:.1} MB", json.len() as f64 / 1e6);
    let read: Vec<Doc> = time("read 10,000 rows from JSON", 250, || {
        serde_json::from_str::<Vec<Doc>>(&json).unwrap()
    });
    // (Compared as JSON: an empty person list reads back as an empty list of ids.)
    assert!(serde_json::to_string(&read).unwrap() == json);
    time("write 10,000 rows to JSON", 150, || {
        serde_json::to_string(&rows).unwrap()
    });
    let limits = Limits::default();
    let problems = time("check 10,000 rows against the limits", 50, || {
        rows.iter()
            .map(|r| limits.check_row(&db, r).len())
            .sum::<usize>()
    });
    assert_eq!(problems, 0);
}

#[test]
fn ordering_10000_rows() {
    let keys = time("10,000 fresh keys", 10, || order::spread(ROWS));
    assert!(keys.windows(2).all(|w| w[0] < w[1]));
    assert!(keys.iter().all(|k| k.len() <= 4));
    let between = time("10,000 keys between two neighbours", 15, || {
        order::n_between(Some("a0"), Some("a1"), ROWS).unwrap()
    });
    assert!(between.windows(2).all(|w| w[0] < w[1]));
    assert!(
        between.iter().all(|k| k.len() <= 6),
        "{}",
        between.iter().map(String::len).max().unwrap()
    );
    // Appending one at a time, as rows are made.
    let appended = time("10,000 appends", 10, || {
        let mut last = order::first();
        for _ in 1..ROWS {
            last = order::between(Some(&last), None).unwrap();
        }
        last
    });
    assert!(appended.len() <= 4);
}
