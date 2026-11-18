//! The lists behind "+N", from the keyboard: Shift-F10's menu on what's behind one offers
//! "Show all", which opens the list with the event highlighted; the up and down arrows go from
//! row to row, Enter opens the row's event, and Escape closes the list.

use std::cell::RefCell;
use std::rc::Rc;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use gpui::{Entity, Focusable as _, TestAppContext, VisualTestContext, px, size};
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

/// Eight events at 2 PM on Thursday (too many side by side), and six all-day ones on Tuesday
/// (more than the all-day row's three).
fn events() -> Vec<Event> {
    let mut out = Vec::new();
    for i in 0..8 {
        out.push(
            Event::timed(
                format!("t{i}"),
                format!("Slot {i}"),
                at(day(3), 14, 0),
                at(day(3), 15, 0),
            )
            .calendar("work"),
        );
    }
    for i in 0..6 {
        out.push(
            Event::all_day(format!("a{i}"), format!("Note {i}"), day(1), day(2)).calendar("work"),
        );
    }
    out
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
        let mut c = Calendar::new(cx)
            .with_options(|o| {
                o.time_zone = TZ;
                o.week_start = chrono::Weekday::Mon;
            })
            .with_header(false)
            .with_now(at(day(2), 10, 40))
            .with_date(day(2))
            .with_view(view)
            .with_calendars(vec![CalendarSource::new("work", "Work", 0x5b8def)])
            .with_events(events());
        c.scroll_to_minute(12. * 60.);
        c
    });
    cx.simulate_resize(size(px(1024.), px(800.)));
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

fn select(c: &Entity<Calendar>, cx: &mut VisualTestContext, id: &str) {
    c.update(cx, |c, cx| c.select(Some(Selection::Event(id.into())), cx));
    draw(cx);
}

fn items(c: &Entity<Calendar>, cx: &VisualTestContext) -> Option<Vec<&'static str>> {
    c.read_with(cx, |c, _| c.menu_items())
}

/// Picks a menu entry as the menu does: the entry, then the keyboard back on the days.
fn pick(c: &Entity<Calendar>, cx: &mut VisualTestContext, item: &str) {
    c.update(cx, |c, cx| c.pick_menu_item(item, cx));
    cx.update(|window, cx| c.read(cx).focus_handle(cx).focus(window, cx));
    draw(cx);
}

fn highlight(c: &Entity<Calendar>, cx: &VisualTestContext) -> Option<EventId> {
    c.read_with(cx, |c, _| c.list_highlight().cloned())
}

#[gpui::test]
fn a_side_by_side_event_that_didnt_fit_opens_its_list(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx, CalendarView::Week);
    // One of Thursday's that only a "+N" stands for.
    let hidden = (0..8)
        .map(|i| format!("t{i}"))
        .find(|id| c.read_with(cx, |c, _| c.hidden_bounds(id).is_some()))
        .map(EventId::from)
        .expect("one didn't fit");
    select(&c, cx, &hidden);
    keys(cx, "shift-f10");
    assert_eq!(items(&c, cx), Some(vec!["open", "edit", "delete", "all"]));
    pick(&c, cx, "all");
    // The list, on that event's row.
    assert!(c.read_with(cx, |c, _| {
        c.crowd_events().is_some_and(|ids| ids.contains(&hidden))
    }));
    assert_eq!(highlight(&c, cx), Some(hidden.clone()));
    // Down goes to the next row and selects its event; Enter opens it and closes the list.
    let ids = c.read_with(cx, |c, _| c.crowd_events().unwrap().to_vec());
    let at = ids.iter().position(|i| *i == hidden).unwrap();
    let next = ids[(at + 1).min(ids.len() - 1)].clone();
    keys(cx, "down");
    assert_eq!(highlight(&c, cx), Some(next.clone()));
    assert_eq!(
        c.read_with(cx, |c, _| c.selected_event().cloned()),
        Some(next.clone())
    );
    keys(cx, "enter");
    assert!(
        seen.borrow()
            .iter()
            .any(|e| *e == CalendarEvent::Open(next.clone()))
    );
    assert!(c.read_with(cx, |c, _| c.crowd_events().is_none()));
}

#[gpui::test]
fn a_crowded_month_day_opens_its_list_and_escape_closes_it(cx: &mut TestAppContext) {
    let (c, _, cx) = open(cx, CalendarView::Month);
    c.update(cx, |c, cx| {
        c.select(
            Some(Selection::Slot {
                date: day(3),
                minute: 0,
            }),
            cx,
        )
    });
    draw(cx);
    keys(cx, "shift-f10");
    assert_eq!(items(&c, cx), Some(vec!["new", "all"]));
    pick(&c, cx, "all");
    assert_eq!(c.read_with(cx, |c, _| c.more_day()), Some(day(3)));
    // Nothing highlighted yet: the first down goes to the first row.
    assert_eq!(highlight(&c, cx), None);
    keys(cx, "down");
    assert_eq!(highlight(&c, cx), Some("t0".into()));
    keys(cx, "escape");
    assert_eq!(c.read_with(cx, |c, _| c.more_day()), None);
    // A day with room has no list to show.
    c.update(cx, |c, cx| {
        c.select(
            Some(Selection::Slot {
                date: day(4),
                minute: 0,
            }),
            cx,
        )
    });
    draw(cx);
    keys(cx, "shift-f10");
    assert_eq!(items(&c, cx), Some(vec!["new"]));
}

#[gpui::test]
fn an_all_day_event_the_row_had_no_room_for_opens_the_row(cx: &mut TestAppContext) {
    let (c, _, cx) = open(cx, CalendarView::Week);
    let hidden = (0..6)
        .map(|i| format!("a{i}"))
        .find(|id| c.read_with(cx, |c, _| c.event_bounds(id).is_none()))
        .expect("one didn't fit");
    select(&c, cx, &hidden);
    keys(cx, "shift-f10");
    assert_eq!(items(&c, cx), Some(vec!["open", "edit", "delete", "all"]));
    pick(&c, cx, "all");
    assert!(c.read_with(cx, |c, _| c.event_bounds(&hidden).is_some()));
}
