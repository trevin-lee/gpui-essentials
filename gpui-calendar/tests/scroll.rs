//! The time grid's hours scroll as Google Calendar's day and week views do. An hour keeps its
//! height whatever the window's, every hour of the day scrolls (12 AM at the top, the end of the
//! day at the bottom), and the grid opens with the current time about a third of the way down,
//! or with an earlier event in view. Previous and next leave the hours where they were.

use chrono::{DateTime, Duration, NaiveDate, Utc};
use gpui::{Entity, Pixels, TestAppContext, VisualTestContext, point, px, size};
use gpui_calendar::*;

const TZ: chrono_tz::Tz = chrono_tz::America::Los_Angeles;

/// Monday 16 November 2026, plus `n` days. Today is Wednesday, `day(2)`.
fn day(n: i64) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 16).unwrap() + Duration::days(n)
}

fn at(d: NaiveDate, h: u32, m: u32) -> DateTime<Utc> {
    time::at_minutes(d, (h * 60 + m) as f32, TZ)
}

fn timed(id: &str, d: NaiveDate, h: u32, m: u32) -> Event {
    Event::timed(id, id, at(d, h, m), at(d, h + 1, m)).calendar("work")
}

/// A calendar in a `w` by `h` window, on today's week at `now` (hours and minutes on
/// Wednesday), with `events`, drawn and nobody having scrolled it.
fn open(
    cx: &mut TestAppContext,
    (w, h): (f32, f32),
    now: (u32, u32),
    events: Vec<Event>,
    f: impl FnOnce(Calendar) -> Calendar,
) -> (Entity<Calendar>, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_core::init(cx);
        gpui_calendar::init(cx);
    });
    let (calendar, cx) = cx.add_window_view(|_, cx| {
        f(Calendar::new(cx)
            .with_options(|o| {
                o.time_zone = TZ;
                o.week_start = chrono::Weekday::Mon;
            })
            .with_header(false)
            .with_now(at(day(2), now.0, now.1))
            .with_date(day(2))
            .with_view(CalendarView::Week)
            .with_calendars(vec![CalendarSource::new("work", "Work", 0x5b8def)])
            .with_events(events))
    });
    cx.simulate_resize(size(px(w), px(h)));
    draw(cx);
    draw(cx);
    (calendar, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

fn scroll(c: &Entity<Calendar>, cx: &VisualTestContext) -> f32 {
    c.read_with(cx, |c, _| c.scroll_minute())
}

fn y(c: &Entity<Calendar>, cx: &VisualTestContext, d: NaiveDate, minute: f32) -> Pixels {
    c.read_with(cx, |c, _| c.point_at(d, minute))
        .expect("on the grid")
        .y
}

/// An hour's height in the window: `hour_height` CSS pixels at the window's rem.
fn hour(cx: &mut VisualTestContext) -> f32 {
    let rem = cx.update(|window, _| f32::from(window.rem_size()));
    CalendarOptions::default().hour_height * rem / 15.
}

/// The grid's top and bottom: the top hour's line, and the window's foot.
fn grid(c: &Entity<Calendar>, cx: &mut VisualTestContext, d: NaiveDate) -> (f32, f32) {
    let top = f32::from(y(c, cx, d, scroll(c, cx)));
    let bottom = f32::from(cx.update(|window, _| window.viewport_size().height));
    (top, bottom)
}

/// How far down the grid a minute is, from 0 at its top to 1 at its bottom.
fn down(c: &Entity<Calendar>, cx: &mut VisualTestContext, d: NaiveDate, minute: f32) -> f32 {
    let (top, bottom) = grid(c, cx, d);
    (f32::from(y(c, cx, d, minute)) - top) / (bottom - top)
}

/// A turn of the wheel, `hours` up the day (down it when negative).
fn wheel(c: &Entity<Calendar>, cx: &mut VisualTestContext, hours: f32) {
    let dy = hours * hour(cx);
    let p = c
        .read_with(cx, |c, _| {
            c.point_at(c.visible_days().0, c.scroll_minute() + 60.)
        })
        .expect("on the grid");
    cx.simulate_event(gpui::ScrollEvent {
        position: point(p.x, p.y),
        delta: gpui::ScrollDelta::Pixels(point(px(0.), px(dy))),
        ..Default::default()
    });
    draw(cx);
}

fn near(a: f32, b: f32, by: f32) -> bool {
    (a - b).abs() <= by
}

#[gpui::test]
fn every_hour_scrolls_from_12_am_to_the_end_of_the_day(cx: &mut TestAppContext) {
    let (c, cx) = open(cx, (1280., 800.), (10, 40), Vec::new(), |c| c);
    for view in [CalendarView::Week, CalendarView::Day, CalendarView::Days(3)] {
        c.update(cx, |c, cx| c.set_view(view, cx));
        draw(cx);
        // Up past the top: 12 AM is the grid's first line.
        wheel(&c, cx, 1000.);
        assert_eq!(scroll(&c, cx), 0., "{view:?}");
        let (top, bottom) = grid(&c, cx, day(2));
        assert_eq!(f32::from(y(&c, cx, day(2), 0.)), top, "{view:?}");
        // Down past the bottom: the end of the day is the grid's foot, and no further.
        wheel(&c, cx, -1000.);
        let end = f32::from(y(&c, cx, day(2), 24. * 60.));
        assert!(
            near(end, bottom, 0.5),
            "{view:?}: the day ends at {end}, the grid at {bottom}"
        );
        let shown = (bottom - top) / hour(cx) * 60.;
        assert!(near(scroll(&c, cx), 24. * 60. - shown, 0.5), "{view:?}");
        // Partway, it moves by what the wheel moved.
        wheel(&c, cx, 2.);
        assert!(
            near(scroll(&c, cx), 24. * 60. - shown - 120., 0.5),
            "{view:?}"
        );
    }
}

#[gpui::test]
fn an_hour_is_as_tall_in_every_window(cx: &mut TestAppContext) {
    let (c, cx) = open(cx, (1280., 500.), (10, 40), Vec::new(), |c| c);
    let hour = hour(cx);
    let mut shown = Vec::new();
    for h in [500., 800., 1100., 1600.] {
        cx.simulate_resize(size(px(1280.), px(h)));
        c.update(cx, |c, _| c.scroll_to_minute(8. * 60.));
        draw(cx);
        let tall = f32::from(y(&c, cx, day(2), 10. * 60.) - y(&c, cx, day(2), 9. * 60.));
        assert!(
            near(tall, hour, 0.01),
            "{h} px: an hour is {tall}, not {hour}"
        );
        let (top, bottom) = grid(&c, cx, day(2));
        shown.push((bottom - top) / hour);
    }
    // A taller window shows more hours rather than taller ones.
    assert!(shown.windows(2).all(|w| w[1] > w[0]), "{shown:?}");
}

#[gpui::test]
fn it_opens_with_the_time_now_a_third_of_the_way_down(cx: &mut TestAppContext) {
    // Nothing starts above the line: the morning's events are after 10:40.
    let events = vec![
        timed("standup", day(2), 11, 0),
        timed("review", day(3), 14, 0),
    ];
    let (c, cx) = open(cx, (1280., 800.), (10, 40), events, |c| c);
    let third = down(&c, cx, day(2), 10. * 60. + 40.);
    assert!(near(third, 1. / 3., 0.01), "now is {third} of the way down");
    // In the evening the end of the day stops it lower, with the evening in view.
    c.update(cx, |c, cx| {
        c.set_now(Some(at(day(2), 21, 0)));
        c.go_to_today(cx);
    });
    draw(cx);
    let end = f32::from(y(&c, cx, day(2), 24. * 60.));
    let (_, bottom) = grid(&c, cx, day(2));
    assert!(near(end, bottom, 0.5));
    assert!(down(&c, cx, day(2), 21. * 60.) > 1. / 3.);
}

#[gpui::test]
fn an_earlier_event_on_the_days_shown_is_brought_into_view(cx: &mut TestAppContext) {
    // Monday's 7 AM is above where 3 PM a third of the way down would put the top.
    let events = vec![timed("early", day(0), 7, 0), timed("late", day(2), 16, 0)];
    let (c, cx) = open(cx, (1280., 800.), (15, 0), events, |c| c);
    assert_eq!(scroll(&c, cx), 6.5 * 60.);
    let early = f32::from(y(&c, cx, day(0), 7. * 60.));
    let (top, bottom) = grid(&c, cx, day(0));
    assert!(early > top && early < bottom);
    // An event that began the day before doesn't pull the grid to midnight.
    let overnight =
        Event::timed("night", "Night", at(day(0), 22, 0), at(day(1), 6, 0)).calendar("work");
    c.update(cx, |c, cx| c.set_events(vec![overnight], cx));
    draw(cx);
    assert!(scroll(&c, cx) > 6. * 60., "{}", scroll(&c, cx));
}

#[gpui::test]
fn days_without_today_open_at_their_first_event_or_at_8_am(cx: &mut TestAppContext) {
    let later = day(14);
    let (c, cx) = open(
        cx,
        (1280., 800.),
        (10, 40),
        vec![
            timed("first", later + Duration::days(1), 9, 15),
            timed("second", later, 13, 0),
        ],
        |c| c.with_date(later),
    );
    assert_eq!(scroll(&c, cx), 8. * 60. + 45.);
    // A week with no timed events opens at the host's `scroll_time`.
    c.update(cx, |c, cx| {
        c.set_events(Vec::new(), cx);
        c.set_view(CalendarView::Month, cx);
        c.set_view(CalendarView::Week, cx);
    });
    draw(cx);
    assert_eq!(c.read_with(cx, |c, _| c.visible_days().0), later);
    assert_eq!(scroll(&c, cx), 8. * 60.);
}

#[gpui::test]
fn previous_and_next_keep_the_hours_where_they_are(cx: &mut TestAppContext) {
    let events = vec![
        timed("this-week", day(2), 12, 0),
        // Next week starts early: it doesn't pull the hours up.
        timed("next-week", day(7), 6, 0),
    ];
    let (c, cx) = open(cx, (1280., 800.), (10, 40), events, |c| c);
    // As it opened, untouched.
    let opened = scroll(&c, cx);
    c.update(cx, |c, cx| c.next(cx));
    draw(cx);
    assert_eq!(scroll(&c, cx), opened);
    // Scrolled by hand, and on through the weeks and days.
    wheel(&c, cx, -3.);
    let left = scroll(&c, cx);
    assert!(near(left, opened + 180., 0.5));
    for step in 0..4 {
        c.update(cx, |c, cx| {
            if step % 2 == 0 {
                c.previous(cx)
            } else {
                c.next(cx)
            }
        });
        draw(cx);
        assert_eq!(scroll(&c, cx), left, "after step {step}");
    }
    // A picked date keeps them too.
    c.update(cx, |c, cx| c.go_to(day(20), cx));
    draw(cx);
    assert_eq!(scroll(&c, cx), left);
    // Today, though, opens it afresh, and so does a switch into the grid from the month.
    c.update(cx, |c, cx| c.go_to_today(cx));
    draw(cx);
    assert!(near(down(&c, cx, day(2), 10. * 60. + 40.), 1. / 3., 0.01));
    wheel(&c, cx, -3.);
    c.update(cx, |c, cx| c.set_view(CalendarView::Month, cx));
    draw(cx);
    c.update(cx, |c, cx| c.set_view(CalendarView::Week, cx));
    draw(cx);
    assert!(near(down(&c, cx, day(2), 10. * 60. + 40.), 1. / 3., 0.01));
}

#[gpui::test]
fn the_opening_waits_for_the_grid_s_height_and_the_host_s_events(cx: &mut TestAppContext) {
    // Drawn short first, as a window still opening can be, then at its size.
    let (c, cx) = open(cx, (1280., 260.), (10, 40), Vec::new(), |c| c);
    cx.simulate_resize(size(px(1280.), px(900.)));
    draw(cx);
    assert!(near(down(&c, cx, day(2), 10. * 60. + 40.), 1. / 3., 0.01));
    // The host's events come after the first frame: an early one is brought into view.
    c.update(cx, |c, cx| {
        c.set_events(vec![timed("early", day(1), 4, 0)], cx)
    });
    draw(cx);
    assert_eq!(scroll(&c, cx), 3.5 * 60.);
    // Once someone scrolls, the hours stay where they put them, but for the day's ends.
    wheel(&c, cx, -1.);
    let left = scroll(&c, cx);
    cx.simulate_resize(size(px(1280.), px(700.)));
    draw(cx);
    c.update(cx, |c, cx| {
        c.set_events(vec![timed("earlier", day(1), 2, 0)], cx)
    });
    draw(cx);
    assert_eq!(scroll(&c, cx), left);
}

#[gpui::test]
fn a_phone_s_compact_grid_follows_the_same_rules(cx: &mut TestAppContext) {
    let (c, cx) = open(cx, (390., 844.), (10, 40), Vec::new(), |c| {
        c.with_options(|o| o.touch = true)
            .with_header(true)
            .with_view(CalendarView::Day)
    });
    assert!(near(down(&c, cx, day(2), 10. * 60. + 40.), 1. / 3., 0.01));
    let tall = f32::from(y(&c, cx, day(2), 10. * 60.) - y(&c, cx, day(2), 9. * 60.));
    assert!(near(tall, hour(cx), 0.01));
    wheel(&c, cx, -1000.);
    let (_, bottom) = grid(&c, cx, day(2));
    assert!(near(f32::from(y(&c, cx, day(2), 24. * 60.)), bottom, 0.5));
    c.update(cx, |c, cx| c.next(cx));
    draw(cx);
    let end = scroll(&c, cx);
    c.update(cx, |c, cx| c.previous(cx));
    draw(cx);
    assert_eq!(scroll(&c, cx), end);
}
