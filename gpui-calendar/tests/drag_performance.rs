//! How long a frame of a drag takes on a busy week: an event moved across the days and up and
//! down the hours, each step a mouse move and a drawn frame. Ignored by default; run it in
//! release, one test at a time:
//! `cargo test --release -p gpui-calendar --test drag_performance -- --ignored --nocapture`.

use std::time::{Duration as StdDuration, Instant};

use chrono::{DateTime, Duration, NaiveDate, Utc};
use gpui::{
    Focusable as _, Modifiers, MouseButton, Pixels, Point, TestAppContext, VisualTestContext,
    point, px, size,
};
use gpui_calendar::*;

const TZ: chrono_tz::Tz = chrono_tz::America::Los_Angeles;

fn day(n: i64) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 16).unwrap() + Duration::days(n)
}

fn at(d: NaiveDate, h: u32, m: u32) -> DateTime<Utc> {
    time::at_minutes(d, (h * 60 + m) as f32, TZ)
}

/// Thirty events a day, some side by side, and the one dragged.
fn events() -> Vec<Event> {
    let mut all = vec![
        Event::timed("moved", "Planning", at(day(2), 11, 0), at(day(2), 12, 0)).calendar("work"),
    ];
    for d in 0..7 {
        for i in 0..30u32 {
            let start = 6 * 60 + i * 25;
            all.push(
                Event::timed(
                    format!("e{d}-{i}"),
                    format!("Event {i}"),
                    at(day(d), start / 60, start % 60),
                    at(day(d), (start + 50) / 60, (start + 50) % 60),
                )
                .calendar("work"),
            );
        }
    }
    all
}

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

#[gpui::test]
#[ignore]
fn frame_time_of_an_event_drag_on_a_busy_week(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_core::init(cx);
        gpui_calendar::init(cx);
    });
    let (c, cx) = cx.add_window_view(|_, cx| {
        let mut c = Calendar::new(cx)
            .with_options(|o| {
                o.time_zone = TZ;
                o.week_start = chrono::Weekday::Mon;
            })
            .with_header(false)
            .with_now(at(day(2), 10, 40))
            .with_date(day(2))
            .with_calendars(vec![CalendarSource::new("work", "Work", 0x5b8def)])
            .with_events(events());
        c.scroll_to_minute(8. * 60.);
        c
    });
    cx.simulate_resize(size(px(1200.), px(800.)));
    cx.update(|window, cx| c.read(cx).focus_handle(cx).focus(window, cx));
    draw(cx);
    draw(cx);
    let spot = |cx: &mut VisualTestContext, d: i64, h: u32| -> Point<Pixels> {
        c.read_with(cx, |c, _| c.point_at(day(d), (h * 60) as f32))
            .expect("on screen")
    };
    let from = spot(cx, 2, 11) + point(px(0.), px(10.));
    let (left, right) = (spot(cx, 0, 11).x, spot(cx, 6, 11).x);
    let (top, bottom) = (spot(cx, 2, 9).y, spot(cx, 2, 15).y);
    cx.simulate_mouse_move(from, None, Modifiers::none());
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    draw(cx);
    cx.simulate_mouse_move(
        from + point(px(0.), px(20.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    draw(cx);
    assert!(c.read_with(cx, |c, _| c.is_dragging()), "the drag began");
    let mut times: Vec<StdDuration> = Vec::new();
    for i in 0..300 {
        let t = (i % 150) as f32 / 150.;
        let at = point(left + (right - left) * t, top + (bottom - top) * t);
        let start = Instant::now();
        cx.simulate_mouse_move(at, MouseButton::Left, Modifiers::none());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        times.push(start.elapsed());
    }
    cx.simulate_keystrokes("escape");
    times.sort();
    let ms = |d: StdDuration| d.as_secs_f64() * 1e3;
    println!(
        "a frame of an event drag on a busy week: median {:.3} ms, p95 {:.3} ms, max {:.3} ms over {}",
        ms(times[times.len() / 2]),
        ms(times[times.len() * 95 / 100]),
        ms(times[times.len() - 1]),
        times.len()
    );
}
