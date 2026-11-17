//! The calculation footer: the engine's calculations over 10,000 rows (timed), and the table's
//! footers driven as a person drives them, on the CRM fixtures. Each calculation's cases (no
//! rows, all empty, mixed shapes after a change of type) are the engine's unit tests
//! (`src/engine/calc.rs`).

use std::time::{Duration, Instant};

use gpui_tables::engine::{Calculated, calculate, time};
use gpui_tables::*;

const ROWS: usize = 10_000;

/// The median of five runs of `f`, after one to warm up, which must be under `bound_ms` in a
/// release build (forty times that in a debug one, four times on CI), as `tests/performance.rs`.
fn timed<T>(label: &str, bound_ms: f64, mut f: impl FnMut() -> T) -> T {
    let mut bound_ms = if cfg!(debug_assertions) {
        bound_ms * 40.
    } else {
        bound_ms
    };
    if std::env::var_os("CI").is_some() {
        bound_ms *= 4.;
    }
    let mut out = f();
    let mut times = Vec::new();
    for _ in 0..5 {
        let start = Instant::now();
        out = std::hint::black_box(f());
        times.push(start.elapsed());
    }
    times.sort();
    let median = times[2];
    eprintln!("{label}: {:.3} ms", median.as_secs_f64() * 1e3);
    assert!(
        median < Duration::from_secs_f64(bound_ms / 1e3),
        "{label} took {median:?}, over {bound_ms} ms"
    );
    out
}

#[test]
fn three_calculations_over_10000_rows() {
    let words = [
        "kiln", "freight", "studio", "robot", "café", "harbour", "ledger", "pine", "orbit",
    ];
    let today = time::days_from_civil(2026, 11, 15);
    let props = [
        Property::new("n", "n", PropertyType::Number, "a0"),
        Property::new("when", "when", PropertyType::Date, "a1"),
        Property::new("note", "note", PropertyType::Text, "a2"),
        Property::new("done", "done", PropertyType::Checkbox, "a3"),
    ];
    let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let rows: Vec<Doc> = (0..ROWS)
        .map(|i| {
            let mut values = Values::new();
            // A tenth of the rows empty, as a real table has.
            if i % 10 != 0 {
                values.insert("n".into(), Value::number((next() % 100_000) as f64 / 100.));
                values.insert(
                    "when".into(),
                    Value::date_on_day(today - (next() % 900) as i64),
                );
                let w = words[(next() % words.len() as u64) as usize];
                values.insert("note".into(), Value::text(format!("{w} {}", next() % 500)));
                values.insert("done".into(), Value::Bool(next() % 2 == 0));
            }
            Doc::row(format!("r{i}"), "db", format!("Row {i}"), "a0", values)
        })
        .collect();
    let all: Vec<usize> = (0..ROWS).collect();
    // The costliest three a footer can have: a median (a sort), the distinct texts (a set) and
    // a date's range; then a sum, a percent checked and a count, the common ones.
    let heavy = timed("median, count unique and a date range", 2., || {
        (
            calculate(&props[0], &Calculation::Median, &rows, &all),
            calculate(&props[2], &Calculation::CountUnique, &rows, &all),
            calculate(&props[1], &Calculation::Range, &rows, &all),
        )
    });
    assert!(matches!(heavy.0, Some(Calculated::Number(_))));
    assert!(matches!(heavy.1, Some(Calculated::Count(n)) if n > 100 && n <= 4_500));
    assert!(matches!(heavy.2, Some(Calculated::Duration(_))));
    let common = timed("sum, percent checked and count all", 2., || {
        (
            calculate(&props[0], &Calculation::Sum, &rows, &all),
            calculate(&props[3], &Calculation::PercentChecked, &rows, &all),
            calculate(&props[2], &Calculation::CountNotEmpty, &rows, &all),
        )
    });
    assert_eq!(common.2, Some(Calculated::Count(9_000)));
}

// The footers are found by their debug selectors, which only a debug build keeps, so the
// release run that times the engine (`--release -- --nocapture`) leaves them out.
#[cfg(all(target_os = "macos", feature = "gpui", debug_assertions))]
mod common;

#[cfg(all(target_os = "macos", feature = "gpui", debug_assertions))]
mod view {
    use std::collections::BTreeMap;

    use super::common::ids::*;
    use super::common::*;
    use gpui::{TestAppContext, px, size};
    use gpui_tables::*;

    const ANNUAL_VALUE: &str = "830e07bc-1e39-4f10-92bd-4acefaecbd38";

    /// The People database with these calculations on every view.
    fn people_with(calcs: &[(&str, Calculation)]) -> (Doc, Vec<Doc>) {
        let (mut db, rows) = load("people");
        for v in &mut db.database.as_mut().unwrap().views {
            v.calculations = calcs
                .iter()
                .map(|(p, c)| ((*p).to_owned(), c.clone()))
                .collect();
        }
        (db, rows)
    }

    #[gpui::test]
    fn a_footer_cell_sets_and_clears_its_columns_calculation(cx: &mut TestAppContext) {
        let host = TestHost::new();
        // Tall enough for a whole calculation menu over the footer (a shorter window scrolls it).
        let (db, rows) = load("people");
        let (table, cx) = open_with(cx, &host, db, rows, None, size(px(2600.), px(1100.)));
        // The footer is under the New row, a cell per column, blank to begin with.
        let footer = rect(cx, "calc:table");
        assert!(footer.top() >= rect(cx, "new:table").bottom() - px(1.));
        assert!(drawn(cx, format!("calc:table:{WARMTH}")));
        assert_eq!(
            table.update(cx, |t, _| t.calculation_text(WARMTH, None)),
            None
        );
        // Pressing a number's cell opens the counts, the percents and a number's own.
        click(cx, format!("calc:table:{WARMTH}"));
        assert!(table.read_with(cx, |t, _| t.has_popover()));
        for item in [
            "None",
            "Count all",
            "Count unique",
            "Percent not empty",
            "Sum",
        ] {
            assert!(drawn(cx, format!("menu-item-{item}")), "{item}");
        }
        for item in ["Median", "Range"] {
            assert!(drawn(cx, format!("menu-item-{item}")), "{item}");
        }
        assert!(!drawn(cx, "menu-item-Earliest"));
        assert!(!drawn(cx, "menu-item-Percent checked"));
        click(cx, "menu-item-Sum");
        assert!(!table.read_with(cx, |t, _| t.has_popover()));
        // A viewChanged with it in calculations, written in camelCase.
        let view = host.last_view();
        assert_eq!(
            view.calculations,
            BTreeMap::from([(WARMTH.to_owned(), Calculation::Sum)])
        );
        let json = serde_json::to_value(&view).unwrap();
        assert_eq!(json["calculations"], serde_json::json!({ WARMTH: "sum" }));
        // 5 + 4 + 2 + 5 + 3 + 1 + 4 (Jonah has none, the duplicate is deleted).
        assert_eq!(
            table.update(cx, |t, _| t.calculation_text(WARMTH, None)),
            Some("Sum 24".into())
        );
        // A second column's, then None takes the first off, and the key with it.
        click(cx, format!("calc:table:{FOLLOW_UP}"));
        assert!(drawn(cx, "menu-item-Percent checked"));
        assert!(!drawn(cx, "menu-item-Sum"));
        click(cx, "menu-item-Percent checked");
        assert_eq!(
            table.update(cx, |t, _| t.calculation_text(FOLLOW_UP, None)),
            Some("Checked 37.5%".into())
        );
        click(cx, format!("calc:table:{WARMTH}"));
        click(cx, "menu-item-None");
        let view = host.last_view();
        assert_eq!(
            view.calculations,
            BTreeMap::from([(FOLLOW_UP.to_owned(), Calculation::PercentChecked)])
        );
        click(cx, format!("calc:table:{FOLLOW_UP}"));
        click(cx, "menu-item-None");
        let json = serde_json::to_value(host.last_view()).unwrap();
        assert!(json.get("calculations").is_none(), "left out when empty");
    }

    #[gpui::test]
    fn results_are_formatted_as_their_columns_cells(cx: &mut TestAppContext) {
        let host = TestHost::new();
        let (db, rows) = people_with(&[
            (NAME, Calculation::CountAll),
            (LAST_CONTACTED, Calculation::Latest),
            (TAGS, Calculation::CountEmpty),
            (STAGE, Calculation::Sum), // Left from a number: works out as nothing.
        ]);
        let (table, cx) = open_with(cx, &host, db, rows, None, size(px(2600.), px(700.)));
        let text = |p: &str, cx: &mut gpui::VisualTestContext| {
            table.update(cx, |t, _| t.calculation_text(p, None))
        };
        let shown = table.update(cx, |t, _| t.shown_rows().len());
        assert_eq!(text(NAME, cx), Some(format!("Count {shown}")));
        assert_eq!(text(LAST_CONTACTED, cx), Some("Latest Nov 12, 2026".into()));
        assert!(text(TAGS, cx).is_some_and(|t| t.starts_with("Empty ")));
        assert_eq!(text(STAGE, cx), None);
    }

    #[gpui::test]
    fn a_currencys_sum_is_in_its_currency(cx: &mut TestAppContext) {
        let host = TestHost::new();
        let (mut db, rows) = load("companies");
        for v in &mut db.database.as_mut().unwrap().views {
            v.calculations.insert(ANNUAL_VALUE.into(), Calculation::Sum);
        }
        let (table, cx) = open_with(cx, &host, db, rows, None, size(px(2600.), px(700.)));
        // 18,000 + 96,000 + 240,000 (Cedar & Pine has none).
        assert_eq!(
            table.update(cx, |t, _| t.calculation_text(ANNUAL_VALUE, None)),
            Some("Sum $354,000".into())
        );
        assert!(drawn(cx, format!("calc:table:{ANNUAL_VALUE}")));
    }

    #[gpui::test]
    fn every_open_group_has_its_own_footer(cx: &mut TestAppContext) {
        let host = TestHost::new();
        let (db, rows) = people_with(&[(NAME, Calculation::CountAll), (WARMTH, Calculation::Sum)]);
        let (table, cx) = open_with(
            cx,
            &host,
            as_table(db, PIPELINE),
            rows,
            Some(PIPELINE),
            size(px(2600.), px(900.)),
        );
        let text = |p: &str, g: &str, cx: &mut gpui::VisualTestContext| {
            table.update(cx, |t, _| t.calculation_text(p, Some(g)))
        };
        let active = format!("option:{ACTIVE}");
        let lead = format!("option:{LEAD}");
        assert_eq!(text(WARMTH, &active, cx), Some("Sum 14".into()));
        assert_eq!(text(WARMTH, &lead, cx), Some("Sum 2".into()));
        assert_eq!(text(NAME, &active, cx), Some("Count 3".into()));
        assert_eq!(text(NAME, "empty", cx), Some("Count 0".into()));
        // Dormant is folded: no footer.
        let dormant = format!("option:{DORMANT}");
        assert_eq!(text(NAME, &dormant, cx), None);
        assert!(!drawn(cx, format!("calc:{dormant}")));
        // Each footer is under its group's New row.
        let footer = rect(cx, format!("calc:{active}"));
        assert!(footer.top() >= rect(cx, format!("new:{active}")).bottom() - px(1.));
        // A pick in one group's footer is the view's, so every footer shows it.
        click(cx, format!("calc:{lead}:{WARMTH}"));
        click(cx, "menu-item-Average");
        assert_eq!(
            host.last_view().calculations.get(WARMTH),
            Some(&Calculation::Average)
        );
        assert_eq!(text(WARMTH, &active, cx), Some("Average 5".into()));
        assert_eq!(text(WARMTH, &lead, cx), Some("Average 2".into()));
        // Folding a group closes its footer's menu.
        cx.update(|_, cx| {
            table.update(cx, |t, cx| {
                t.open_calculation_menu(WARMTH, Some(&active), cx)
            })
        });
        draw(cx);
        assert!(table.read_with(cx, |t, _| t.has_popover()));
        let mut db = table.read_with(cx, |t, _| t.database().clone());
        for v in &mut db.database.as_mut().unwrap().views {
            v.collapsed_groups.push(active.clone());
        }
        cx.update(|_, cx| table.update(cx, |t, cx| t.set_database(db, cx)));
        draw(cx);
        assert!(!table.read_with(cx, |t, _| t.has_popover()));
        assert!(!drawn(cx, format!("calc:{active}")));
    }

    #[gpui::test]
    fn the_keyboard_reaches_the_footer(cx: &mut TestAppContext) {
        let host = TestHost::new();
        let (table, cx) = open_people(cx, &host);
        let last = table.update(cx, |t, _| {
            let rows = t.shown_rows().to_vec();
            t.rows()[*rows.last().unwrap()].id.clone()
        });
        cx.update(|_, cx| table.update(cx, |t, cx| t.focus_cell(&last, WARMTH, cx)));
        // Down past the New row to the footer, in the same column, and Enter opens its menu
        // on None; Down goes to Count all (past the Count heading) and Enter picks it.
        keys(cx, "down down");
        let item = table.update(cx, |t, _| {
            let c = t.focused_cell().unwrap();
            t.items()[c.item]
        });
        assert_eq!(item, gpui_tables::view::Item::Calc(None));
        keys(cx, "enter");
        assert!(table.read_with(cx, |t, _| t.has_popover()));
        keys(cx, "down enter");
        assert_eq!(
            host.last_view().calculations.get(WARMTH),
            Some(&Calculation::CountAll)
        );
        // Escape from a menu goes back to the same cell.
        keys(cx, "enter");
        keys(cx, "escape");
        assert!(!table.read_with(cx, |t, _| t.has_popover()));
        let c = table.update(cx, |t, _| t.focused_cell().unwrap());
        assert_eq!(
            table.update(cx, |t, _| t.items()[c.item]),
            gpui_tables::view::Item::Calc(None)
        );
    }

    #[gpui::test]
    fn a_fingers_footer_is_44_points(cx: &mut TestAppContext) {
        let host = TestHost::touch();
        let (_, cx) = open_people(cx, &host);
        let footer = rect(cx, "calc:table");
        assert!(footer.size.height >= px(44.), "{footer:?}");
        let cell = rect(cx, format!("calc:table:{WARMTH}"));
        assert!(cell.size.height >= px(44.) && cell.size.width >= px(44.));
    }
}
