//! Shift and the arrows stretch the selected slot, as dragging empty time does: on the grid a
//! slot at a time, in the month a day (or a week) at a time; Enter or `n` then proposes an
//! event over all of it. Any other selection drops the stretch.

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

fn open(
    cx: &mut TestAppContext,
    view: CalendarView,
) -> (Entity<Calendar>, Seen, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_core::init(cx);
        gpui_calendar::init(cx);
    });
    let (calendar, cx) = cx.add_window_view(|_, cx| {
        Calendar::new(cx)
            .with_options(|o| {
                o.time_zone = TZ;
                o.week_start = chrono::Weekday::Mon;
            })
            .with_header(false)
            .with_now(at(day(2), 10, 40))
            .with_date(day(2))
            .with_view(view)
            .with_calendars(vec![CalendarSource::new("work", "Work", 0x5b8def)])
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
    cx.run_until_parked();
    (calendar, seen, cx)
}

fn select_slot(c: &Entity<Calendar>, cx: &mut VisualTestContext, date: NaiveDate, minute: u32) {
    c.update(cx, |c, cx| {
        c.select(Some(Selection::Slot { date, minute }), cx)
    });
}

fn created(seen: &Seen) -> Vec<EventTime> {
    seen.borrow()
        .iter()
        .filter_map(|e| match e {
            CalendarEvent::Create { time, .. } => Some(*time),
            _ => None,
        })
        .collect()
}

#[gpui::test]
fn shift_down_stretches_a_slot_and_enter_makes_that_long_an_event(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx, CalendarView::Week);
    select_slot(&c, cx, day(1), 9 * 60);
    // Three slots down: 9 to 11 AM.
    cx.simulate_keystrokes("shift-down shift-down shift-down enter");
    assert_eq!(
        created(&seen),
        [EventTime::timed(at(day(1), 9, 0), at(day(1), 11, 0))]
    );
    // Up past the slot it started from: the other way, 8 to 9:30 AM.
    select_slot(&c, cx, day(1), 9 * 60);
    cx.simulate_keystrokes("shift-up shift-up n");
    assert_eq!(
        created(&seen)[1],
        EventTime::timed(at(day(1), 8, 0), at(day(1), 9, 30))
    );
}

#[gpui::test]
fn a_plain_arrow_drops_the_stretch(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx, CalendarView::Week);
    select_slot(&c, cx, day(1), 9 * 60);
    cx.simulate_keystrokes("shift-down shift-down down enter");
    // The slot moved on, a single one again: the usual hour from 9:30.
    assert_eq!(
        created(&seen),
        [EventTime::timed(at(day(1), 9, 30), at(day(1), 10, 30))]
    );
}

#[gpui::test]
fn in_the_month_shift_stretches_days_and_weeks(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx, CalendarView::Month);
    select_slot(&c, cx, day(1), 0);
    cx.simulate_keystrokes("shift-right shift-right enter");
    assert_eq!(created(&seen), [EventTime::all_day(day(1), day(4))]);
    select_slot(&c, cx, day(1), 0);
    cx.simulate_keystrokes("shift-down n");
    assert_eq!(created(&seen)[1], EventTime::all_day(day(1), day(9)));
}

#[gpui::test]
fn the_stretched_hours_are_selected_for_voiceover(cx: &mut TestAppContext) {
    let (c, _, cx) = open(cx, CalendarView::Week);
    select_slot(&c, cx, day(1), 9 * 60);
    cx.simulate_keystrokes("shift-down shift-down shift-down");
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
    let selected: Vec<String> = c.read_with(cx, |c, _| {
        c.accessibility_items()
            .into_iter()
            .filter(|i| i.selected)
            .map(|i| i.label)
            .collect()
    });
    assert_eq!(
        selected,
        ["Tuesday, November 17, 9 AM", "Tuesday, November 17, 10 AM"]
    );
}
