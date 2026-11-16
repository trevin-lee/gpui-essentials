//! The calendar driven as a person drives it: a window, the mouse and the keyboard. Every
//! change comes back as a proposal, and the events the host gave stay as they were.

use std::cell::RefCell;
use std::rc::Rc;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use gpui_calendar::*;
use gpui_kit::{
    Entity, Focusable as _, Modifiers, MouseButton, MouseDownEvent, Pixels, Point, TestAppContext,
    VisualTestContext, point, px,
};

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
        Event::timed("a", "Planning", at(day(2), 11, 0), at(day(2), 12, 0)).calendar("work"),
        Event::timed("ro", "All-hands", at(day(3), 9, 0), at(day(3), 10, 0))
            .calendar("work")
            .read_only(true),
        Event::all_day("trip", "Trip", day(0), day(2)).calendar("home"),
        // Two occurrences of a weekly series, expanded by the host.
        Event::timed("gym@1", "Gym", at(day(1), 18, 0), at(day(1), 19, 0))
            .calendar("home")
            .series("gym"),
        Event::timed("gym@4", "Gym", at(day(4), 18, 0), at(day(4), 19, 0))
            .calendar("home")
            .series("gym"),
        Event::timed("hol", "Holiday", at(day(1), 13, 0), at(day(1), 14, 0)).calendar("holidays"),
        Event::timed("hidden", "Hidden", at(day(0), 11, 0), at(day(0), 12, 0)).calendar("old"),
    ]
}

fn calendars() -> Vec<CalendarSource> {
    vec![
        CalendarSource::new("work", "Work", 0x5b8def),
        CalendarSource::new("home", "Home", 0x4cc38a),
        CalendarSource::new("holidays", "Holidays", 0x8b8b93).read_only(true),
        CalendarSource::new("old", "Old", 0xe06fa8).visible(false),
    ]
}

fn reference(id: &str) -> EventRef {
    events()
        .iter()
        .find(|e| e.id.as_ref() == id)
        .unwrap()
        .to_ref()
}

fn open(cx: &mut TestAppContext) -> (Entity<Calendar>, Seen, &mut VisualTestContext) {
    open_with(cx, |c| c)
}

fn open_with(
    cx: &mut TestAppContext,
    f: impl FnOnce(Calendar) -> Calendar,
) -> (Entity<Calendar>, Seen, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
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
            .with_calendars(calendars())
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

fn spot(
    c: &Entity<Calendar>,
    cx: &VisualTestContext,
    d: NaiveDate,
    h: u32,
    m: u32,
) -> Point<Pixels> {
    c.read_with(cx, |c, _| c.point_at(d, (h * 60 + m) as f32))
        .expect("on screen")
}

fn press(cx: &mut VisualTestContext, from: Point<Pixels>) {
    cx.simulate_mouse_move(from, None, Modifiers::none());
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    draw(cx);
}

fn move_to(cx: &mut VisualTestContext, to: Point<Pixels>) {
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    draw(cx);
}

fn drag(cx: &mut VisualTestContext, from: Point<Pixels>, to: Point<Pixels>) {
    press(cx, from);
    move_to(cx, point((from.x + to.x) / 2., (from.y + to.y) / 2.));
    move_to(cx, to);
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    draw(cx);
}

fn has(seen: &Seen, want: &CalendarEvent) -> bool {
    seen.borrow().iter().any(|e| e == want)
}

fn unchanged(c: &Entity<Calendar>, cx: &VisualTestContext) {
    c.read_with(cx, |c, _| assert_eq!(c.events(), events().as_slice()));
}

#[gpui_kit::test]
fn dragging_empty_time_proposes_an_event_snapped_outward(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    let (from, to) = (spot(&c, cx, day(0), 13, 5), spot(&c, cx, day(0), 14, 20));
    press(cx, from);
    move_to(cx, to);
    // The ghost is what letting go would propose.
    let ghost = c.read_with(cx, |c, _| c.drag_proposal());
    let want = EventTime::timed(at(day(0), 13, 0), at(day(0), 14, 30));
    assert_eq!(ghost, Some((None, want)));
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    draw(cx);
    assert!(has(
        &seen,
        &CalendarEvent::Create {
            calendar: Some("work".into()),
            time: want
        }
    ));
    unchanged(&c, cx);
}

#[gpui_kit::test]
fn dragging_an_event_proposes_a_move_across_days(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    // Grabbed at 11:15 on Wednesday, let go at 12:45 on Thursday: a day and an hour and a half
    // later, so it starts at 12:30 and keeps its hour.
    drag(
        cx,
        spot(&c, cx, day(2), 11, 15),
        spot(&c, cx, day(3), 12, 45),
    );
    let want = EventTime::timed(at(day(3), 12, 30), at(day(3), 13, 30));
    assert!(has(
        &seen,
        &CalendarEvent::Move {
            event: reference("a"),
            time: want
        }
    ));
    assert!(has(&seen, &CalendarEvent::Select(Some("a".into()))));
    unchanged(&c, cx);
}

#[gpui_kit::test]
fn dragging_the_bottom_edge_proposes_a_new_end(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    let b = c.read_with(cx, |c, _| c.event_bounds("a")).expect("drawn");
    let grab = point(b.left() + px(20.), b.bottom() - px(2.));
    drag(cx, grab, point(grab.x, spot(&c, cx, day(2), 13, 10).y));
    let want = EventTime::timed(at(day(2), 11, 0), at(day(2), 13, 15));
    assert!(has(
        &seen,
        &CalendarEvent::Resize {
            event: reference("a"),
            time: want
        }
    ));
    unchanged(&c, cx);
}

#[gpui_kit::test]
fn escape_drops_a_drag(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    let (from, to) = (spot(&c, cx, day(2), 11, 15), spot(&c, cx, day(4), 15, 0));
    press(cx, from);
    move_to(cx, to);
    assert!(c.read_with(cx, |c, _| c.is_dragging()));
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(!c.read_with(cx, |c, _| c.is_dragging()));
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    draw(cx);
    assert!(
        !seen
            .borrow()
            .iter()
            .any(|e| matches!(e, CalendarEvent::Move { .. }))
    );
    // The selection stays; a second Escape drops it.
    assert_eq!(
        c.read_with(cx, |c, _| c.selected_event().cloned()),
        Some("a".into())
    );
    cx.simulate_keystrokes("escape");
    assert_eq!(c.read_with(cx, |c, _| c.selected_event().cloned()), None);
}

#[gpui_kit::test]
fn a_click_selects_and_a_double_click_opens(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    let p = spot(&c, cx, day(2), 11, 30);
    cx.simulate_click(p, Modifiers::none());
    draw(cx);
    assert!(has(&seen, &CalendarEvent::Select(Some("a".into()))));
    cx.simulate_event(MouseDownEvent {
        position: p,
        modifiers: Modifiers::none(),
        button: MouseButton::Left,
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_mouse_up(p, MouseButton::Left, Modifiers::none());
    draw(cx);
    assert!(has(&seen, &CalendarEvent::Open("a".into())));
    // Delete asks the host.
    cx.simulate_keystrokes("backspace");
    assert!(has(&seen, &CalendarEvent::Delete(reference("a"))));
    unchanged(&c, cx);
}

#[gpui_kit::test]
fn a_read_only_event_is_selected_but_not_moved_or_deleted(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    drag(cx, spot(&c, cx, day(3), 9, 30), spot(&c, cx, day(4), 14, 0));
    assert!(has(&seen, &CalendarEvent::Select(Some("ro".into()))));
    assert!(
        !seen
            .borrow()
            .iter()
            .any(|e| matches!(e, CalendarEvent::Move { .. }))
    );
    cx.simulate_keystrokes("backspace");
    assert!(
        !seen
            .borrow()
            .iter()
            .any(|e| matches!(e, CalendarEvent::Delete(_)))
    );
}

#[gpui_kit::test]
fn the_keys_change_the_view_and_the_period(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    let state =
        |c: &Entity<Calendar>, cx: &VisualTestContext| c.read_with(cx, |c, _| (c.view(), c.date()));
    cx.simulate_keystrokes("m");
    assert_eq!(state(&c, cx), (CalendarView::Month, day(2)));
    let (start, end) = c.read_with(cx, |c, _| c.visible_range());
    assert!(has(
        &seen,
        &CalendarEvent::RangeChange {
            view: CalendarView::Month,
            start,
            end
        }
    ));
    cx.simulate_keystrokes("j");
    assert_eq!(
        state(&c, cx),
        (
            CalendarView::Month,
            NaiveDate::from_ymd_opt(2026, 12, 18).unwrap()
        )
    );
    cx.simulate_keystrokes("k k");
    assert_eq!(
        state(&c, cx).1,
        NaiveDate::from_ymd_opt(2026, 10, 18).unwrap()
    );
    cx.simulate_keystrokes("t d");
    assert_eq!(state(&c, cx), (CalendarView::Day, day(2)));
    cx.simulate_keystrokes("w");
    assert_eq!(state(&c, cx).0, CalendarView::Week);
    // With nothing selected, the arrows go a period at a time.
    cx.simulate_keystrokes("right");
    assert_eq!(state(&c, cx).1, day(9));
    cx.simulate_keystrokes("left");
    assert_eq!(state(&c, cx).1, day(2));
}

#[gpui_kit::test]
fn the_arrows_move_a_slot_and_enter_makes_an_event_there(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    // The first arrow selects the slot now is in.
    cx.simulate_keystrokes("down");
    let slot = |c: &Entity<Calendar>, cx: &VisualTestContext| {
        c.read_with(cx, |c, _| c.selection().cloned())
    };
    assert_eq!(
        slot(&c, cx),
        Some(Selection::Slot {
            date: day(2),
            minute: 630
        })
    );
    cx.simulate_keystrokes("right down down");
    assert_eq!(
        slot(&c, cx),
        Some(Selection::Slot {
            date: day(3),
            minute: 690
        })
    );
    cx.simulate_keystrokes("enter");
    assert!(has(
        &seen,
        &CalendarEvent::Create {
            calendar: Some("work".into()),
            time: EventTime::timed(at(day(3), 11, 30), at(day(3), 12, 30))
        }
    ));
    // Leaving the week brings the next one.
    cx.simulate_keystrokes("right right right right");
    assert_eq!(c.read_with(cx, |c, _| c.visible_days().0), day(7));
}

#[gpui_kit::test]
fn an_all_day_event_moves_by_weeks_in_the_month(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    cx.simulate_keystrokes("m");
    draw(cx);
    draw(cx);
    let b = c
        .read_with(cx, |c, _| c.event_bounds("trip"))
        .expect("drawn");
    let row = spot(&c, cx, day(7), 0, 0).y - spot(&c, cx, day(0), 0, 0).y;
    let grab = point(b.left() + px(20.), b.center().y);
    drag(cx, grab, point(grab.x, grab.y + row));
    assert!(has(
        &seen,
        &CalendarEvent::Move {
            event: reference("trip"),
            time: EventTime::all_day(day(7), day(9))
        }
    ));
    unchanged(&c, cx);
}

#[gpui_kit::test]
fn the_host_applies_a_proposal_by_setting_the_event(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    drag(
        cx,
        spot(&c, cx, day(2), 11, 15),
        spot(&c, cx, day(2), 14, 15),
    );
    let moved = seen.borrow().iter().find_map(|e| match e {
        CalendarEvent::Move { event, time } => Some((event.id.clone(), *time)),
        _ => None,
    });
    let (id, time) = moved.expect("a move");
    c.update(cx, |c, cx| {
        let mut e = c.event(&id).unwrap().clone();
        e.time = time;
        c.upsert_event(e, cx);
    });
    draw(cx);
    // Now it's drawn at 2 PM.
    let b = c.read_with(cx, |c, _| c.event_bounds("a")).expect("drawn");
    let two = spot(&c, cx, day(2), 14, 0).y;
    assert!((b.top() - two).abs() < px(2.));
}

fn proposed_edit(seen: &Seen) -> bool {
    seen.borrow().iter().any(|e| {
        matches!(
            e,
            CalendarEvent::Move { .. } | CalendarEvent::Resize { .. } | CalendarEvent::Delete(_)
        )
    })
}

#[gpui_kit::test]
fn events_take_their_calendars_colour(cx: &mut TestAppContext) {
    let (c, _, cx) = open(cx);
    c.read_with(cx, |c, _| {
        let color = |id: &str| c.color_of(c.event(id).unwrap());
        assert_eq!(color("a"), 0x5b8def);
        assert_eq!(color("trip"), 0x4cc38a);
        // Its own colour wins over its calendar's.
        let mut own = c.event("a").unwrap().clone();
        own.color = Some(0x123456);
        assert_eq!(c.color_of(&own), 0x123456);
        // A calendar the host didn't list gets a palette colour, the same every time.
        let stray = Event::timed("s", "Stray", at(day(0), 9, 0), at(day(0), 10, 0)).calendar("x");
        assert_eq!(c.color_of(&stray), palette_color("x"));
    });
}

#[gpui_kit::test]
fn a_hidden_calendars_events_are_not_drawn(cx: &mut TestAppContext) {
    let (c, _, cx) = open(cx);
    assert!(c.read_with(cx, |c, _| c.event_bounds("hidden")).is_none());
    let (start, end) = c.read_with(cx, |c, _| c.visible_range());
    let shown = |c: &Entity<Calendar>, cx: &VisualTestContext| {
        c.read_with(cx, |c, _| {
            c.events_in(start, end)
                .iter()
                .map(|e| e.id.to_string())
                .collect::<Vec<_>>()
        })
    };
    assert!(!shown(&c, cx).contains(&"hidden".to_string()));
    // Showing the calendar brings them back; hiding Home takes its events away and drops a
    // selection in it.
    c.update(cx, |c, cx| {
        c.select(Some(Selection::Event("trip".into())), cx);
        let mut cals = calendars();
        cals[3].visible = true;
        cals[1].visible = false;
        c.set_calendars(cals, cx);
    });
    draw(cx);
    assert!(c.read_with(cx, |c, _| c.event_bounds("hidden")).is_some());
    assert!(c.read_with(cx, |c, _| c.event_bounds("trip")).is_none());
    assert!(!shown(&c, cx).contains(&"gym@1".to_string()));
    assert_eq!(c.read_with(cx, |c, _| c.selected_event().cloned()), None);
    unchanged(&c, cx);
}

#[gpui_kit::test]
fn a_read_only_calendar_refuses_moves_resizes_and_deletes(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    // Move.
    drag(
        cx,
        spot(&c, cx, day(1), 13, 30),
        spot(&c, cx, day(2), 15, 0),
    );
    assert!(has(&seen, &CalendarEvent::Select(Some("hol".into()))));
    // Resize from its bottom edge.
    let b = c
        .read_with(cx, |c, _| c.event_bounds("hol"))
        .expect("drawn");
    let grab = point(b.left() + px(20.), b.bottom() - px(2.));
    drag(cx, grab, point(grab.x, grab.y + px(60.)));
    // Delete.
    cx.simulate_keystrokes("backspace");
    assert!(!proposed_edit(&seen));
    // The calendar turning editable lets the same edits through.
    c.update(cx, |c, cx| {
        let mut cals = calendars();
        cals[2].read_only = false;
        c.set_calendars(cals, cx);
    });
    draw(cx);
    cx.simulate_keystrokes("backspace");
    assert!(has(&seen, &CalendarEvent::Delete(reference("hol"))));
    unchanged(&c, cx);
}

#[gpui_kit::test]
fn an_edit_to_an_occurrence_names_it_and_its_series(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    c.update(cx, |c, _| c.scroll_to_minute(14. * 60.));
    draw(cx);
    drag(
        cx,
        spot(&c, cx, day(4), 18, 15),
        spot(&c, cx, day(4), 19, 15),
    );
    let want = EventRef {
        id: "gym@4".into(),
        series: Some("gym".into()),
        calendar: "home".into(),
    };
    assert!(has(
        &seen,
        &CalendarEvent::Move {
            event: want.clone(),
            time: EventTime::timed(at(day(4), 19, 0), at(day(4), 20, 0))
        }
    ));
    cx.simulate_keystrokes("delete");
    assert!(has(&seen, &CalendarEvent::Delete(want)));
    // The other occurrence wasn't touched.
    assert!(
        !seen
            .borrow()
            .iter()
            .any(|e| matches!(e, CalendarEvent::Move { event, .. } if event.id == "gym@1"))
    );
    unchanged(&c, cx);
}

#[gpui_kit::test]
fn new_events_go_in_the_default_calendar_or_nowhere(cx: &mut TestAppContext) {
    let (_, seen, cx) = open_with(cx, |c| {
        c.with_options(|o| o.default_calendar = Some("home".into()))
    });
    cx.simulate_keystrokes("down enter");
    assert!(seen.borrow().iter().any(|e| matches!(
        e,
        CalendarEvent::Create { calendar: Some(cal), .. } if cal == "home"
    )));
}

#[gpui_kit::test]
fn nothing_is_made_when_every_calendar_is_read_only(cx: &mut TestAppContext) {
    let (c, seen, cx) = open_with(cx, |c| {
        c.with_calendars(
            calendars()
                .into_iter()
                .map(|cal| cal.read_only(true))
                .collect(),
        )
    });
    // No drag, no Enter on a slot, no double-click.
    drag(cx, spot(&c, cx, day(0), 13, 0), spot(&c, cx, day(0), 15, 0));
    assert!(!c.read_with(cx, |c, _| c.is_dragging()));
    cx.simulate_keystrokes("down enter");
    let p = spot(&c, cx, day(5), 15, 0);
    cx.simulate_event(MouseDownEvent {
        position: p,
        modifiers: Modifiers::none(),
        button: MouseButton::Left,
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_mouse_up(p, MouseButton::Left, Modifiers::none());
    draw(cx);
    assert!(
        !seen
            .borrow()
            .iter()
            .any(|e| matches!(e, CalendarEvent::Create { .. }))
    );
}
