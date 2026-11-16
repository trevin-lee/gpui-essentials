//! No text runs over: every view, drawn at many widths with long titles, locations and links,
//! laid out with Core Text and Inter as on screen. A line of text must stay inside the area it's
//! drawn in (so nothing is cut off mid-letter) and must not run into another line.

#![cfg(target_os = "macos")]

use std::sync::Arc;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use gpui_kit::component::Theme;
use gpui_kit::{Bounds, Pixels, TestAppContext, TestDispatcher, px, size};

use crate::paint::drawn;
use crate::*;

const TZ: chrono_tz::Tz = chrono_tz::America::Los_Angeles;

/// Monday 16 November 2026, plus `n` days.
fn day(n: i64) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 16).unwrap() + Duration::days(n)
}

fn at(d: NaiveDate, h: u32, m: u32) -> DateTime<Utc> {
    time::at_minutes(d, (h * 60 + m) as f32, TZ)
}

/// A week with the worst of everything: long titles, long locations, links with long labels,
/// events side by side, short ones, all-day spans across the week and crowded days.
fn worst_week() -> Vec<Event> {
    let long = "Quarterly planning with the platform, design and research teams";
    let mut out = vec![
        Event::timed("a", long, at(day(0), 9, 0), at(day(0), 11, 30))
            .location("Building 4, the big room at the end of the corridor")
            .link(EventLink::doc(
                "d1",
                "Q4 planning notes and the decisions we made",
            ))
            .link(EventLink::agent(
                "main",
                "Main, the agent that keeps the plan",
            )),
        Event::timed("b", long, at(day(0), 9, 30), at(day(0), 10, 0))
            .link(EventLink::doc("d2", "Agenda")),
        Event::timed("c", long, at(day(0), 10, 0), at(day(0), 10, 15))
            .link(EventLink::doc("d3", "Notes"))
            .link(EventLink::doc("d4", "More notes")),
        Event::timed("d", "Lunch", at(day(1), 12, 0), at(day(1), 13, 0))
            .link(EventLink::agent("health", "Health")),
        Event::timed("e", long, at(day(2), 13, 0), at(day(2), 16, 0)).link(EventLink::doc(
            "d5",
            "A doc with a very long name that goes on and on",
        )),
        Event::timed("f", "Overnight", at(day(3), 22, 0), at(day(4), 2, 0)),
        Event::all_day("g", long, day(0), day(7)).link(EventLink::doc("d6", "Trip")),
        Event::all_day("h", "Rent", day(2), day(3))
            .link(EventLink::doc("d7", "Rent"))
            .link(EventLink::agent("fin", "Finances")),
    ];
    // A crowded Thursday: eight overlapping events.
    for i in 0..8 {
        out.push(
            Event::timed(
                format!("crowd-{i}"),
                format!("{long} {i}"),
                at(day(3), 9 + i / 4, (i % 4) * 15),
                at(day(3), 11, 0),
            )
            .link(EventLink::doc(format!("c{i}"), "Notes")),
        );
    }
    // And a month's worth of busy days around it.
    for d in -16..30 {
        for k in 0..4 {
            out.push(
                Event::timed(
                    format!("m{d}-{k}"),
                    format!("{long} ({k})"),
                    at(day(d), 8 + k as u32 * 2, 30),
                    at(day(d), 9 + k as u32 * 2, 0),
                )
                .link(EventLink::doc(format!("m{d}"), "Notes")),
            );
        }
    }
    out
}

fn context() -> TestAppContext {
    let cx = TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        Arc::new(gpui_apple::AppleTextSystem::new()),
    );
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::init(cx);
        let _ = cx.text_system().add_fonts(vec![
            std::borrow::Cow::Borrowed(
                include_bytes!("../examples/fonts/Inter-Regular.ttf").as_slice(),
            ),
            std::borrow::Cow::Borrowed(
                include_bytes!("../examples/fonts/Inter-Medium.ttf").as_slice(),
            ),
            std::borrow::Cow::Borrowed(
                include_bytes!("../examples/fonts/Inter-SemiBold.ttf").as_slice(),
            ),
        ]);
        Theme::global_mut(cx).font_family = "Inter".into();
    });
    cx
}

/// What's wrong with a frame's text, if anything.
fn problems(texts: &[drawn::Text], canvas: Bounds<Pixels>) -> Vec<String> {
    let tolerance = px(0.75);
    let mut out = Vec::new();
    let mut shown = Vec::new();
    for t in texts {
        let seen = t.bounds.intersect(&t.mask).intersect(&canvas);
        if seen.size.width <= px(0.) || seen.size.height <= px(0.) {
            // Scrolled away, or culled: not on screen at all.
            continue;
        }
        for (name, area) in [("its area", t.mask), ("the calendar", canvas)] {
            if t.bounds.left() < area.left() - tolerance
                || t.bounds.right() > area.right() + tolerance
            {
                out.push(format!(
                    "{:?} runs out of {name}: {:?} in {:?}",
                    t.text, t.bounds, area
                ));
            }
        }
        shown.push((t, seen));
    }
    for (i, (a, sa)) in shown.iter().enumerate() {
        for (b, sb) in &shown[i + 1..] {
            let both = sa.intersect(sb);
            if both.size.width > px(1.) && both.size.height > px(1.) {
                out.push(format!(
                    "{:?} runs into {:?}: {:?} and {:?}",
                    a.text, b.text, a.bounds, b.bounds
                ));
            }
        }
    }
    out
}

/// Draws every view at each width and collects what overflows.
fn check_widths(widths: &[f32], height: f32) {
    let mut cx = context();
    let (calendar, vcx) = cx.add_window_view(|_, cx| {
        let mut c = Calendar::new(cx)
            .with_options(|o| o.time_zone = TZ)
            .with_header(false)
            .with_now(at(day(2), 10, 40))
            .with_date(day(2))
            .with_events(worst_week());
        c.scroll_to_minute(8. * 60.);
        c
    });
    let views = [
        CalendarView::Day,
        CalendarView::Days(3),
        CalendarView::Week,
        CalendarView::Month,
    ];
    let mut found = Vec::new();
    for &w in widths {
        vcx.simulate_resize(size(px(w), px(height)));
        for view in views {
            calendar.update(vcx, |c, cx| c.set_view(view, cx));
            vcx.run_until_parked();
            drawn::record();
            vcx.update(|window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            });
            let texts = drawn::take();
            assert!(!texts.is_empty(), "nothing drawn at {w} in {view:?}");
            let canvas = calendar
                .read_with(vcx, |c, _| c.shared.geo.get())
                .expect("drawn")
                .bounds;
            for p in problems(&texts, canvas) {
                found.push(format!("{w}px, {view:?}: {p}"));
            }
        }
    }
    assert!(found.is_empty(), "{}", found.join("\n"));
}

#[test]
fn no_text_runs_over_at_desktop_widths() {
    check_widths(&[720., 900., 1024., 1280., 1600., 2200.], 820.);
}
