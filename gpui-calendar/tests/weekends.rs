//! Weekends hidden (`show_weekends` off): the week and the month show the working week, the
//! other days share the width, and the arrows and keyboard drags step over Saturday and Sunday.

use std::cell::RefCell;
use std::rc::Rc;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use gpui::{
    Entity, Focusable as _, Modifiers, MouseButton, MouseDownEvent, Pixels, Point, TestAppContext,
    VisualTestContext, point, px,
};
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
        Event::timed("review", "Review", at(day(4), 9, 0), at(day(4), 10, 0)).calendar("work"),
        Event::timed("brunch", "Brunch", at(day(5), 11, 0), at(day(5), 12, 0)).calendar("work"),
        Event::all_day("trip", "Trip", day(3), day(8)).calendar("work"),
    ]
}

/// Wednesday 18 November, weeks from Monday, weekends hidden.
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
                o.show_weekends = false;
            })
            .with_header(false)
            .with_now(at(day(2), 10, 40))
            .with_date(day(2))
            .with_view(view)
            .with_calendars(vec![CalendarSource::new("work", "Work", 0x5b8def)])
            .with_events(events());
        c.scroll_to_minute(8. * 60.);
        c
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

fn spot(
    c: &Entity<Calendar>,
    cx: &VisualTestContext,
    d: NaiveDate,
    minute: f32,
) -> Option<Point<Pixels>> {
    c.read_with(cx, |c, _| c.point_at(d, minute))
}

fn keys(cx: &mut VisualTestContext, keys: &str) {
    cx.simulate_keystrokes(keys);
    draw(cx);
}

fn selected_day(c: &Entity<Calendar>, cx: &VisualTestContext) -> Option<NaiveDate> {
    c.read_with(cx, |c, _| match c.selection() {
        Some(Selection::Slot { date, .. }) => Some(*date),
        _ => None,
    })
}

#[gpui::test]
fn the_week_is_five_wider_days(cx: &mut TestAppContext) {
    let (c, _, cx) = open(cx, CalendarView::Week);
    let x = |cx: &VisualTestContext, d| spot(&c, cx, d, 600.).map(|p| p.x);
    // No column for Saturday or Sunday, nor for what's on them.
    assert_eq!(x(cx, day(5)), None);
    assert_eq!(x(cx, day(6)), None);
    assert_eq!(c.read_with(cx, |c, _| c.event_bounds("brunch")), None);
    assert!(c.read_with(cx, |c, _| c.event_bounds("review")).is_some());
    // Five even columns, wider than seven.
    let (mon, tue, fri) = (
        x(cx, day(0)).unwrap(),
        x(cx, day(1)).unwrap(),
        x(cx, day(4)).unwrap(),
    );
    let hidden = tue - mon;
    assert!(((fri - mon) - hidden * 4.).abs() < px(1.));
    c.update(cx, |c, cx| c.set_options(|o| o.show_weekends = true, cx));
    draw(cx);
    let shown = x(cx, day(1)).unwrap() - x(cx, day(0)).unwrap();
    assert!(((hidden / shown) - 7. / 5.).abs() < 0.01);
    assert!(x(cx, day(5)).is_some());
}

#[gpui::test]
fn a_press_lands_on_the_day_under_it(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx, CalendarView::Week);
    // Friday's right edge: the last column, not a weekend day.
    let fri = spot(&c, cx, day(4), 15. * 60.).unwrap();
    let thu = spot(&c, cx, day(3), 15. * 60.).unwrap();
    let edge = point(fri.x + (fri.x - thu.x) / 2. - px(2.), fri.y);
    cx.simulate_event(MouseDownEvent {
        position: edge,
        modifiers: Modifiers::none(),
        button: MouseButton::Left,
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_mouse_up(edge, MouseButton::Left, Modifiers::none());
    draw(cx);
    assert!(seen.borrow().iter().any(|e| matches!(
        e,
        CalendarEvent::Create { time: EventTime::Timed { start, .. }, .. }
            if time::local(*start, TZ).0 == day(4)
    )));
}

#[gpui::test]
fn the_arrows_step_over_the_weekend(cx: &mut TestAppContext) {
    let (c, _, cx) = open(cx, CalendarView::Week);
    c.update(cx, |c, cx| {
        c.select(
            Some(Selection::Slot {
                date: day(4),
                minute: 600,
            }),
            cx,
        )
    });
    keys(cx, "right");
    assert_eq!(selected_day(&c, cx), Some(day(7)));
    assert_eq!(c.read_with(cx, |c, _| c.visible_days().0), day(7));
    keys(cx, "left");
    assert_eq!(selected_day(&c, cx), Some(day(4)));

    // In the month, up and down keep the weekday, and left and right skip the weekend too.
    c.update(cx, |c, cx| c.set_view(CalendarView::Month, cx));
    draw(cx);
    assert_eq!(spot(&c, cx, day(5), 0.), None);
    keys(cx, "down");
    assert_eq!(selected_day(&c, cx), Some(day(11)));
    keys(cx, "right");
    assert_eq!(selected_day(&c, cx), Some(day(14)));
}

#[gpui::test]
fn the_first_arrow_picks_a_day_with_a_column(cx: &mut TestAppContext) {
    let (c, _, cx) = open(cx, CalendarView::Week);
    // Today is a Saturday: the first arrow picks the week's first working day instead.
    c.update(cx, |c, cx| {
        c.set_now(Some(at(day(5), 10, 0)));
        cx.notify()
    });
    draw(cx);
    keys(cx, "down");
    assert_eq!(selected_day(&c, cx), Some(day(0)));
}

#[gpui::test]
fn a_keyboard_drag_steps_over_the_weekend_and_says_monday(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx, CalendarView::Week);
    c.update(cx, |c, cx| {
        c.select(Some(Selection::Event("review".into())), cx)
    });
    draw(cx);
    keys(cx, "space right");
    let said = cx
        .update(|window, cx| gpui_dnd::announcement(window, cx))
        .map(|s| s.to_string())
        .unwrap_or_default();
    assert_eq!(said, "Review, Friday 9 to 10 AM, moving to Monday 9 AM");
    keys(cx, "space");
    assert!(seen.borrow().iter().any(|e| matches!(
        e,
        CalendarEvent::Move { time, .. }
            if *time == EventTime::timed(at(day(7), 9, 0), at(day(7), 10, 0))
    )));
}
