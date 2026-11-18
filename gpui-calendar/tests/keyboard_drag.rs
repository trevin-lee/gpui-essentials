//! Moving and resizing an event with the keyboard: Space picks the selected event up (Shift+Space
//! its end), the arrows move it a snap step or a day (a day or a week in the month), Space puts
//! it down as a proposal and Escape puts it back. VoiceOver hears each step in the calendar's
//! own words: the event, where it was, and where it's going.

use std::cell::RefCell;
use std::rc::Rc;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use gpui::{Entity, Focusable as _, TestAppContext, VisualTestContext};
use gpui_calendar::*;

const TZ: chrono_tz::Tz = chrono_tz::America::Los_Angeles;
type Seen = Rc<RefCell<Vec<CalendarEvent>>>;

/// Monday 16 November 2026, plus `n` days.
fn day(n: i64) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 16).unwrap() + Duration::days(n)
}

fn at(d: NaiveDate, h: u32, m: u32) -> DateTime<Utc> {
    time::at_minutes(d, (h * 60 + m) as f32, TZ)
}

fn events() -> Vec<Event> {
    vec![
        Event::timed("standup", "Standup", at(day(1), 9, 0), at(day(1), 9, 30)).calendar("work"),
        Event::all_day("trip", "Trip", day(2), day(4)).calendar("work"),
        Event::timed("ro", "All-hands", at(day(3), 9, 0), at(day(3), 10, 0))
            .calendar("work")
            .read_only(true),
    ]
}

fn open(
    cx: &mut TestAppContext,
    f: impl FnOnce(Calendar) -> Calendar,
) -> (Entity<Calendar>, Seen, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_core::init(cx);
        gpui_calendar::init(cx);
    });
    let (calendar, cx) = cx.add_window_view(|_, cx| {
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
        f(c)
    });
    let seen: Seen = Rc::default();
    cx.update(|window, cx| {
        let s = seen.clone();
        cx.subscribe(&calendar, move |_, e: &CalendarEvent, _| {
            s.borrow_mut().push(e.clone())
        })
        .detach();
        calendar.read(cx).focus_handle(cx).focus(window, cx);
    });
    draw(cx);
    draw(cx);
    (calendar, seen, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
}

fn keys(cx: &mut VisualTestContext, keys: &str) {
    cx.simulate_keystrokes(keys);
    draw(cx);
}

fn said(cx: &mut VisualTestContext) -> String {
    cx.update(|window, cx| gpui_dnd::announcement(window, cx))
        .map(|s| s.to_string())
        .unwrap_or_default()
}

fn select(c: &Entity<Calendar>, cx: &mut VisualTestContext, id: &str) {
    c.update(cx, |c, cx| c.select(Some(Selection::Event(id.into())), cx));
    draw(cx);
}

fn proposals(seen: &Seen) -> Vec<CalendarEvent> {
    seen.borrow()
        .iter()
        .filter(|e| matches!(e, CalendarEvent::Move { .. } | CalendarEvent::Resize { .. }))
        .cloned()
        .collect()
}

fn reference(id: &str) -> EventRef {
    events()
        .iter()
        .find(|e| e.id.as_ref() == id)
        .unwrap()
        .to_ref()
}

#[gpui::test]
fn space_picks_the_event_up_and_the_arrows_move_it_said_at_each_step(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx, |c| c);
    select(&c, cx, "standup");
    keys(cx, "space");
    assert!(c.read_with(cx, |c, _| c.is_dragging()));
    assert_eq!(
        said(cx),
        "Picked up Standup. Arrow keys move it, Space drops it, Escape cancels."
    );
    keys(cx, "down");
    assert_eq!(said(cx), "Standup, Tuesday 9 to 9:30 AM, moving to 9:15 AM");
    keys(cx, "right");
    assert_eq!(
        said(cx),
        "Standup, Tuesday 9 to 9:30 AM, moving to Wednesday 9:15 AM"
    );
    keys(cx, "space");
    assert!(!c.read_with(cx, |c, _| c.is_dragging()));
    assert_eq!(said(cx), "Standup, moved to Wednesday 9:15 AM");
    assert_eq!(
        proposals(&seen),
        [CalendarEvent::Move {
            event: reference("standup"),
            time: EventTime::timed(at(day(2), 9, 15), at(day(2), 9, 45)),
            scope: EditScope::This,
        }]
    );
}

#[gpui::test]
fn shift_space_takes_the_end_and_escape_puts_it_back(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx, |c| c);
    select(&c, cx, "standup");
    keys(cx, "shift-space");
    keys(cx, "down");
    keys(cx, "down");
    assert_eq!(said(cx), "Standup, Tuesday 9 to 9:30 AM, ending at 10 AM");
    keys(cx, "escape");
    assert!(!c.read_with(cx, |c, _| c.is_dragging()));
    assert!(proposals(&seen).is_empty(), "nothing proposed");

    // Again, put down this time.
    keys(cx, "shift-space");
    keys(cx, "down");
    keys(cx, "enter");
    assert_eq!(said(cx), "Standup, now ending at 9:45 AM");
    assert_eq!(
        proposals(&seen),
        [CalendarEvent::Resize {
            event: reference("standup"),
            time: EventTime::timed(at(day(1), 9, 0), at(day(1), 9, 45)),
            scope: EditScope::This,
        }]
    );
}

#[gpui::test]
fn the_sentences_follow_the_calendars_24_hour_clock(cx: &mut TestAppContext) {
    let (c, _, cx) = open(cx, |c| c.with_options(|o| o.hour12 = false));
    select(&c, cx, "standup");
    keys(cx, "space");
    keys(cx, "up");
    let s = said(cx);
    assert!(
        s.starts_with("Standup, Tuesday ") && s.ends_with("moving to 08:45"),
        "{s}"
    );
    keys(cx, "escape");
}

#[gpui::test]
fn an_all_day_event_moves_by_days_and_its_end_by_days_too(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx, |c| c);
    select(&c, cx, "trip");
    keys(cx, "space");
    keys(cx, "right");
    assert_eq!(
        said(cx),
        "Trip, Wednesday to Thursday, all day, moving to Thursday"
    );
    keys(cx, "space");
    keys(cx, "shift-space");
    keys(cx, "right");
    assert_eq!(
        said(cx),
        "Trip, Wednesday to Thursday, all day, ending on Friday"
    );
    keys(cx, "space");
    assert_eq!(
        proposals(&seen),
        [
            CalendarEvent::Move {
                event: reference("trip"),
                time: EventTime::all_day(day(3), day(5)),
                scope: EditScope::This,
            },
            CalendarEvent::Resize {
                event: reference("trip"),
                time: EventTime::all_day(day(2), day(5)),
                scope: EditScope::This,
            },
        ]
    );
}

#[gpui::test]
fn a_read_only_event_isnt_picked_up(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx, |c| c);
    select(&c, cx, "ro");
    keys(cx, "space");
    assert!(!c.read_with(cx, |c, _| c.is_dragging()));
    keys(cx, "down");
    assert!(proposals(&seen).is_empty());
}

#[gpui::test]
fn in_the_month_up_and_down_move_by_weeks(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx, |c| c);
    c.update(cx, |c, cx| c.set_view(CalendarView::Month, cx));
    draw(cx);
    select(&c, cx, "standup");
    keys(cx, "space");
    keys(cx, "down");
    assert_eq!(
        said(cx),
        "Standup, Tuesday 9 to 9:30 AM, moving to Tuesday, November 24 9 AM"
    );
    keys(cx, "space");
    assert_eq!(
        proposals(&seen),
        [CalendarEvent::Move {
            event: reference("standup"),
            time: EventTime::timed(at(day(8), 9, 0), at(day(8), 9, 30)),
            scope: EditScope::This,
        }]
    );
}
