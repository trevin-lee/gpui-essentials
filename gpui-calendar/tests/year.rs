//! The year view: twelve small months, driven with the mouse and the keyboard.

use chrono::{DateTime, Duration, NaiveDate, Utc};
use gpui::{
    Entity, Focusable as _, Modifiers, Pixels, Point, TestAppContext, VisualTestContext, point, px,
};
use gpui_calendar::*;

const TZ: chrono_tz::Tz = chrono_tz::America::Los_Angeles;

fn date(m: u32, d: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, m, d).unwrap()
}

fn at(d: NaiveDate, h: u32) -> DateTime<Utc> {
    time::at_minutes(d, (h * 60) as f32, TZ)
}

/// The calendar on Wednesday 18 November 2026, weeks from Monday, in the year view.
fn open(cx: &mut TestAppContext) -> (Entity<Calendar>, &mut VisualTestContext) {
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
            .with_now(at(date(11, 18), 10))
            .with_date(date(11, 18))
            .with_view(CalendarView::Year)
            .with_events(vec![
                Event::timed("a", "Planning", at(date(11, 18), 11), at(date(11, 18), 12)),
                Event::timed("b", "Review", at(date(11, 18), 14), at(date(11, 18), 15)),
                Event::all_day("c", "Trip", date(3, 9), date(3, 11)),
            ])
    });
    cx.update(|window, cx| calendar.read(cx).focus_handle(cx).focus(window, cx));
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

fn spot(c: &Entity<Calendar>, cx: &VisualTestContext, d: NaiveDate) -> Point<Pixels> {
    c.read_with(cx, |c, _| c.point_at(d, 0.))
        .expect("on screen")
}

fn click(cx: &mut VisualTestContext, p: Point<Pixels>) {
    cx.simulate_click(p, Modifiers::none());
    draw(cx);
}

fn state(c: &Entity<Calendar>, cx: &VisualTestContext) -> (CalendarView, NaiveDate) {
    c.read_with(cx, |c, _| (c.view(), c.date()))
}

#[gpui::test]
fn the_year_shows_its_whole_year_and_steps_a_year_at_a_time(cx: &mut TestAppContext) {
    let (c, cx) = open(cx);
    c.read_with(cx, |c, _| {
        assert_eq!(c.title(), "2026");
        assert_eq!(c.visible_days(), (date(1, 1), 365));
    });
    // Every day of the year is on screen, the last below and right of the first.
    let (first, last) = (spot(&c, cx, date(1, 1)), spot(&c, cx, date(12, 31)));
    assert!(last.x > first.x && last.y > first.y);
    cx.simulate_keystrokes("j");
    assert_eq!(
        state(&c, cx),
        (CalendarView::Year, date(11, 18) + chrono::Months::new(12))
    );
    c.read_with(cx, |c, _| assert_eq!(c.visible_days().1, 365));
    cx.simulate_keystrokes("k m");
    assert_eq!(state(&c, cx), (CalendarView::Month, date(11, 18)));
    cx.simulate_keystrokes("y");
    assert_eq!(state(&c, cx).0, CalendarView::Year);
}

#[gpui::test]
fn a_day_opens_that_day_and_a_month_s_name_that_month(cx: &mut TestAppContext) {
    let (c, cx) = open(cx);
    click(cx, spot(&c, cx, date(3, 10)));
    assert_eq!(state(&c, cx), (CalendarView::Day, date(3, 10)));

    // November 2026 starts on a Sunday: its 1st is alone on the first row, the 2nd starts the
    // next. Its name is above the row of weekday letters, above the 1st.
    c.update(cx, |c, cx| c.set_view(CalendarView::Year, cx));
    draw(cx);
    let (sun, mon) = (spot(&c, cx, date(11, 1)), spot(&c, cx, date(11, 2)));
    let cell = mon.y - sun.y;
    click(cx, point(mon.x, sun.y - cell / 2. - px(30.)));
    assert_eq!(state(&c, cx), (CalendarView::Month, date(11, 1)));
}

#[gpui::test]
fn the_arrows_move_a_day_or_a_week_and_enter_opens_the_day(cx: &mut TestAppContext) {
    let (c, cx) = open(cx);
    let selected = |c: &Entity<Calendar>, cx: &VisualTestContext| {
        c.read_with(cx, |c, _| match c.selection() {
            Some(Selection::Slot { date, .. }) => Some(*date),
            _ => None,
        })
    };
    // The first arrow selects today.
    cx.simulate_keystrokes("down");
    assert_eq!(selected(&c, cx), Some(date(11, 18)));
    cx.simulate_keystrokes("right down");
    assert_eq!(selected(&c, cx), Some(date(11, 26)));
    cx.simulate_keystrokes("up up left");
    assert_eq!(selected(&c, cx), Some(date(11, 11)));
    // Past the year's end, the next year.
    c.update(cx, |c, cx| {
        c.select(
            Some(Selection::Slot {
                date: date(12, 30),
                minute: 0,
            }),
            cx,
        )
    });
    cx.simulate_keystrokes("down");
    assert_eq!(selected(&c, cx), Some(date(12, 30) + Duration::days(7)));
    assert_eq!(c.read_with(cx, |c, _| c.title()), "2027");
    cx.simulate_keystrokes("enter");
    assert_eq!(
        state(&c, cx),
        (CalendarView::Day, date(12, 30) + Duration::days(7))
    );
}

#[gpui::test]
fn each_day_says_how_many_events_it_has(cx: &mut TestAppContext) {
    let (c, cx) = open(cx);
    let items = c.read_with(cx, |c, _| c.accessibility_items());
    let label = |d: NaiveDate| {
        items
            .iter()
            .find(|i| i.target == A11yTarget::Day(d))
            .map(|i| i.label.clone())
    };
    assert_eq!(items.len(), 365);
    assert_eq!(
        label(date(11, 18)).as_deref(),
        Some("Wednesday, November 18, today, 2 events")
    );
    assert_eq!(
        label(date(3, 10)).as_deref(),
        Some("Tuesday, March 10, 1 event")
    );
    assert_eq!(label(date(3, 12)).as_deref(), Some("Thursday, March 12"));
}

#[gpui::test]
fn the_year_has_no_events_to_pick_up_or_menus_to_open(cx: &mut TestAppContext) {
    let (c, cx) = open(cx);
    // An event selected in another view stays selected, but Space doesn't pick it up here.
    c.update(cx, |c, cx| c.select(Some(Selection::Event("a".into())), cx));
    cx.simulate_keystrokes("space");
    draw(cx);
    assert!(!c.read_with(cx, |c, _| c.is_dragging()));
    // A right-click on a day opens no menu.
    let p = spot(&c, cx, date(11, 18));
    cx.simulate_mouse_down(p, gpui::MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(p, gpui::MouseButton::Right, Modifiers::none());
    draw(cx);
    assert_eq!(c.read_with(cx, |c, _| c.menu_items()), None);
}
