//! The calendar driven as a person drives it: a window, the mouse and the keyboard. Every
//! change comes back as a proposal, and the events the host gave stay as they were.

use std::cell::RefCell;
use std::rc::Rc;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use gpui::{
    Entity, Focusable as _, Modifiers, MouseButton, MouseDownEvent, Pixels, Point, TestAppContext,
    VisualTestContext, point, px, size,
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
        Event::timed("a", "Planning", at(day(2), 11, 0), at(day(2), 12, 0))
            .calendar("work")
            .link(EventLink::doc("doc-plan", "Plan")),
        Event::timed("notes", "Review", at(day(4), 13, 0), at(day(4), 15, 0))
            .calendar("work")
            .link(EventLink::doc("doc-review", "Review notes"))
            .link(EventLink::agent("planner", "Planner")),
        Event::timed("many", "Sync", at(day(5), 13, 0), at(day(5), 13, 45))
            .calendar("work")
            .link(EventLink::doc("x", "X"))
            .link(EventLink::doc("y", "Y"))
            .link(EventLink::doc("z", "Z")),
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

#[gpui::test]
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

#[gpui::test]
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
            time: want,
            scope: EditScope::This,
        }
    ));
    assert!(has(&seen, &CalendarEvent::Select(Some("a".into()))));
    unchanged(&c, cx);
}

#[gpui::test]
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
            time: want,
            scope: EditScope::This,
        }
    ));
    unchanged(&c, cx);
}

#[gpui::test]
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

#[gpui::test]
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
    assert!(has(
        &seen,
        &CalendarEvent::Delete {
            event: reference("a"),
            scope: EditScope::This
        }
    ));
    unchanged(&c, cx);
}

#[gpui::test]
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
            .any(|e| matches!(e, CalendarEvent::Delete { .. }))
    );
}

#[gpui::test]
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

#[gpui::test]
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

#[gpui::test]
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
            time: EventTime::all_day(day(7), day(9)),
            scope: EditScope::This,
        }
    ));
    unchanged(&c, cx);
}

#[gpui::test]
fn the_host_applies_a_proposal_by_setting_the_event(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    drag(
        cx,
        spot(&c, cx, day(2), 11, 15),
        spot(&c, cx, day(2), 14, 15),
    );
    let moved = seen.borrow().iter().find_map(|e| match e {
        CalendarEvent::Move { event, time, .. } => Some((event.id.clone(), *time)),
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
            CalendarEvent::Move { .. }
                | CalendarEvent::Resize { .. }
                | CalendarEvent::Delete { .. }
        )
    })
}

#[gpui::test]
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

#[gpui::test]
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

#[gpui::test]
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
    assert!(has(
        &seen,
        &CalendarEvent::Delete {
            event: reference("hol"),
            scope: EditScope::This
        }
    ));
    unchanged(&c, cx);
}

fn gym4() -> EventRef {
    EventRef {
        id: "gym@4".into(),
        series: Some("gym".into()),
        calendar: "home".into(),
    }
}

fn asking(c: &Entity<Calendar>, cx: &VisualTestContext) -> bool {
    c.read_with(cx, |c, _| c.is_asking())
}

fn edits(seen: &Seen) -> usize {
    seen.borrow()
        .iter()
        .filter(|e| {
            matches!(
                e,
                CalendarEvent::Move { .. }
                    | CalendarEvent::Resize { .. }
                    | CalendarEvent::Delete { .. }
            )
        })
        .count()
}

/// Gym on Friday dragged an hour later: an occurrence of a weekly series.
fn drag_gym(c: &Entity<Calendar>, cx: &mut VisualTestContext) {
    c.update(cx, |c, _| c.scroll_to_minute(14. * 60.));
    draw(cx);
    drag(cx, spot(c, cx, day(4), 18, 15), spot(c, cx, day(4), 19, 15));
}

#[gpui::test]
fn moving_an_occurrence_asks_which_ones_and_says_so(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    drag_gym(&c, cx);
    // Nothing goes to the host until the person answers; the ghost stays where it was let go.
    assert!(asking(&c, cx));
    assert_eq!(edits(&seen), 0);
    assert!(cx.debug_bounds("gpui-core-popover").is_some());
    // A click on "This and following events".
    let row = cx.debug_bounds("scope-1").expect("the answers");
    cx.simulate_click(row.center(), Modifiers::none());
    draw(cx);
    assert!(!asking(&c, cx));
    assert!(has(
        &seen,
        &CalendarEvent::Move {
            event: gym4(),
            time: EventTime::timed(at(day(4), 19, 0), at(day(4), 20, 0)),
            scope: EditScope::ThisAndFollowing,
        }
    ));
    // The other occurrence wasn't touched.
    assert!(
        !seen
            .borrow()
            .iter()
            .any(|e| matches!(e, CalendarEvent::Move { event, .. } if event.id == "gym@1"))
    );
    unchanged(&c, cx);
}

#[gpui::test]
fn the_keys_answer_and_escape_puts_the_event_back(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    drag_gym(&c, cx);
    assert!(asking(&c, cx));
    // Escape: no edit, no ghost.
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(!asking(&c, cx));
    assert_eq!(edits(&seen), 0);
    assert!(cx.debug_bounds("gpui-core-popover").is_none());
    let b = c
        .read_with(cx, |c, _| c.event_bounds("gym@4"))
        .expect("drawn");
    assert!((b.top() - spot(&c, cx, day(4), 18, 0).y).abs() < px(2.));
    // Delete asks too; down twice and Enter is "All events".
    c.update(cx, |c, cx| {
        c.select(Some(Selection::Event("gym@4".into())), cx)
    });
    cx.simulate_keystrokes("delete");
    assert!(asking(&c, cx));
    cx.simulate_keystrokes("down down enter");
    assert!(has(
        &seen,
        &CalendarEvent::Delete {
            event: gym4(),
            scope: EditScope::All
        }
    ));
    unchanged(&c, cx);
}

#[gpui::test]
fn the_question_offers_only_the_scopes_the_host_allows(cx: &mut TestAppContext) {
    // A host that can't split a series: "This and following" isn't offered.
    let (c, seen, cx) = open_with(cx, |c| {
        c.with_options(|o| o.scopes = &[EditScope::This, EditScope::All])
    });
    drag_gym(&c, cx);
    assert!(asking(&c, cx));
    assert!(cx.debug_bounds("scope-0").is_some());
    assert!(cx.debug_bounds("scope-1").is_some());
    assert!(cx.debug_bounds("scope-2").is_none());
    // The second answer is now "All events", by a click and by the keys.
    let row = cx.debug_bounds("scope-1").expect("the answers");
    cx.simulate_click(row.center(), Modifiers::none());
    draw(cx);
    assert!(!asking(&c, cx));
    assert!(has(
        &seen,
        &CalendarEvent::Move {
            event: gym4(),
            time: EventTime::timed(at(day(4), 19, 0), at(day(4), 20, 0)),
            scope: EditScope::All,
        }
    ));
    c.update(cx, |c, cx| {
        c.select(Some(Selection::Event("gym@4".into())), cx)
    });
    cx.simulate_keystrokes("delete");
    assert!(asking(&c, cx));
    // Down twice wraps round the two answers to "This event".
    cx.simulate_keystrokes("down down enter");
    assert!(has(
        &seen,
        &CalendarEvent::Delete {
            event: gym4(),
            scope: EditScope::This
        }
    ));
    unchanged(&c, cx);
}

#[gpui::test]
fn one_allowed_scope_skips_the_question(cx: &mut TestAppContext) {
    let (c, seen, cx) = open_with(cx, |c| c.with_options(|o| o.scopes = &[EditScope::All]));
    drag_gym(&c, cx);
    assert!(!asking(&c, cx));
    assert!(cx.debug_bounds("gpui-core-popover").is_none());
    assert!(has(
        &seen,
        &CalendarEvent::Move {
            event: gym4(),
            time: EventTime::timed(at(day(4), 19, 0), at(day(4), 20, 0)),
            scope: EditScope::All,
        }
    ));
    // None allowed counts as "This event", also without asking.
    c.update(cx, |c, cx| {
        c.set_options(|o| o.scopes = &[], cx);
        c.select(Some(Selection::Event("gym@4".into())), cx)
    });
    cx.simulate_keystrokes("delete");
    assert!(!asking(&c, cx));
    assert!(has(
        &seen,
        &CalendarEvent::Delete {
            event: gym4(),
            scope: EditScope::This
        }
    ));
}

#[gpui::test]
fn a_click_outside_the_question_drops_the_edit(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    drag_gym(&c, cx);
    assert!(asking(&c, cx));
    cx.simulate_click(spot(&c, cx, day(0), 15, 0), Modifiers::none());
    draw(cx);
    assert!(!asking(&c, cx));
    assert_eq!(edits(&seen), 0);
    // An event that doesn't repeat isn't asked about.
    drag(
        cx,
        spot(&c, cx, day(2), 11, 15),
        spot(&c, cx, day(2), 12, 15),
    );
    assert!(!asking(&c, cx));
    assert!(seen.borrow().iter().any(|e| matches!(
        e,
        CalendarEvent::Move { event, scope: EditScope::This, .. } if event.id == "a"
    )));
}

#[gpui::test]
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

#[gpui::test]
fn a_hidden_default_calendar_is_passed_over(cx: &mut TestAppContext) {
    // "old" can be edited but isn't shown: an event made there would vanish.
    let (_, seen, cx) = open_with(cx, |c| {
        c.with_options(|o| o.default_calendar = Some("old".into()))
    });
    cx.simulate_keystrokes("down enter");
    let made: Vec<_> = seen
        .borrow()
        .iter()
        .filter_map(|e| match e {
            CalendarEvent::Create { calendar, .. } => Some(calendar.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(made, vec![Some("work".into())]);
}

#[gpui::test]
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

fn opened_links(seen: &Seen) -> Vec<(EventId, EventLink)> {
    seen.borrow()
        .iter()
        .filter_map(|e| match e {
            CalendarEvent::OpenLink { event, link } => Some((event.id.clone(), link.clone())),
            _ => None,
        })
        .collect()
}

#[gpui::test]
fn a_click_on_a_listed_link_asks_the_host_to_open_it(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    c.update(cx, |c, _| c.scroll_to_minute(12. * 60.));
    draw(cx);
    // Two hours is room to list both links under the title.
    let row = c
        .read_with(cx, |c, _| c.link_bounds("notes", 1))
        .expect("listed");
    let chip = c.read_with(cx, |c, _| c.event_bounds("notes")).unwrap();
    assert!(chip.contains(&row.center()));
    cx.simulate_click(row.center(), Modifiers::none());
    draw(cx);
    assert_eq!(
        opened_links(&seen),
        vec![("notes".into(), EventLink::agent("planner", "Planner"))]
    );
    assert!(has(&seen, &CalendarEvent::Select(Some("notes".into()))));
    unchanged(&c, cx);
}

#[gpui::test]
fn a_small_chip_shows_its_one_link_as_a_mark_and_several_as_a_count(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    // An hour is too short to list a link: the mark by the title stands for it.
    let mark = c
        .read_with(cx, |c, _| c.link_bounds("a", 0))
        .expect("a mark");
    cx.simulate_click(mark.center(), Modifiers::none());
    draw(cx);
    assert_eq!(
        opened_links(&seen),
        vec![("a".into(), EventLink::doc("doc-plan", "Plan"))]
    );
    // Three links on a short chip are a count, which isn't any one of them.
    c.update(cx, |c, _| c.scroll_to_minute(12. * 60.));
    draw(cx);
    assert!(c.read_with(cx, |c, _| c.event_bounds("many")).is_some());
    assert!(c.read_with(cx, |c, _| c.link_bounds("many", 0)).is_none());
}

#[gpui::test]
fn a_drag_that_starts_on_a_link_moves_its_event(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    c.update(cx, |c, _| c.scroll_to_minute(12. * 60.));
    draw(cx);
    let row = c
        .read_with(cx, |c, _| c.link_bounds("notes", 0))
        .expect("listed");
    drag(
        cx,
        row.center(),
        point(row.center().x, row.center().y + px(48.)),
    );
    assert!(opened_links(&seen).is_empty());
    assert!(
        seen.borrow()
            .iter()
            .any(|e| matches!(e, CalendarEvent::Move { event, .. } if event.id == "notes"))
    );
    unchanged(&c, cx);
}

#[gpui::test]
fn the_agenda_lists_events_by_day_and_the_arrows_walk_it(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    cx.simulate_keystrokes("a");
    draw(cx);
    draw(cx);
    assert_eq!(c.read_with(cx, |c, _| c.view()), CalendarView::AGENDA);
    let (start, end) = c.read_with(cx, |c, _| c.visible_range());
    assert!(has(
        &seen,
        &CalendarEvent::RangeChange {
            view: CalendarView::AGENDA,
            start,
            end
        }
    ));
    // Two weeks from Wednesday: Planning is listed; Monday's trip isn't in range.
    assert!(c.read_with(cx, |c, _| c.event_bounds("a")).is_some());
    assert!(c.read_with(cx, |c, _| c.event_bounds("trip")).is_none());
    // Planning (Wednesday) is listed above the all-hands (Thursday).
    let (a, ro) = c.read_with(cx, |c, _| {
        (c.event_bounds("a").unwrap(), c.event_bounds("ro").unwrap())
    });
    assert!(a.bottom() <= ro.top());
    // The arrows go from event to event in the list's order.
    let selected = |c: &Entity<Calendar>, cx: &VisualTestContext| {
        c.read_with(cx, |c, _| c.selected_event().cloned())
    };
    cx.simulate_keystrokes("down");
    assert_eq!(selected(&c, cx), Some("a".into()));
    cx.simulate_keystrokes("down");
    assert_eq!(selected(&c, cx), Some("ro".into()));
    cx.simulate_keystrokes("enter");
    assert!(has(&seen, &CalendarEvent::Open("ro".into())));
    cx.simulate_keystrokes("up backspace");
    assert!(has(
        &seen,
        &CalendarEvent::Delete {
            event: reference("a"),
            scope: EditScope::This
        }
    ));
    unchanged(&c, cx);
}

#[gpui::test]
fn the_agenda_opens_links_and_moves_nothing(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    cx.simulate_keystrokes("a");
    draw(cx);
    draw(cx);
    // A drag in the list is a press, not a move.
    let a = c.read_with(cx, |c, _| c.event_bounds("a")).unwrap();
    drag(cx, a.center(), point(a.center().x, a.center().y + px(120.)));
    assert!(!proposed_edit(&seen));
    assert!(has(&seen, &CalendarEvent::Select(Some("a".into()))));
    // Every link is a pill in the list.
    for _ in 0..3 {
        cx.simulate_event(gpui::ScrollEvent {
            position: a.center(),
            delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-200.))),
            ..Default::default()
        });
        draw(cx);
    }
    let pill = c
        .read_with(cx, |c, _| c.link_bounds("notes", 1))
        .expect("listed");
    cx.simulate_click(pill.center(), Modifiers::none());
    draw(cx);
    assert_eq!(
        opened_links(&seen),
        vec![("notes".into(), EventLink::agent("planner", "Planner"))]
    );
    unchanged(&c, cx);
}

#[gpui::test]
fn a_query_dims_the_grid_and_filters_the_month_and_agenda(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    // "PLAN" finds Planning, and nothing else this week; "plan" (the doc link) too.
    c.update(cx, |c, cx| c.set_query("PLAN", cx));
    draw(cx);
    let found: Vec<String> = c.read_with(cx, |c, _| {
        c.found().iter().map(|e| e.id.to_string()).collect()
    });
    assert_eq!(found, vec!["a".to_string()]);
    assert_eq!(c.read_with(cx, |c, _| c.query().to_string()), "PLAN");
    // The grid still draws the rest, dimmed.
    assert!(c.read_with(cx, |c, _| c.event_bounds("ro")).is_some());
    // The month and the agenda show only what it found.
    for keys in ["m", "a"] {
        cx.simulate_keystrokes(keys);
        draw(cx);
        draw(cx);
        assert!(
            c.read_with(cx, |c, _| c.event_bounds("a")).is_some(),
            "{keys}"
        );
        assert!(
            c.read_with(cx, |c, _| c.event_bounds("ro")).is_none(),
            "{keys}"
        );
    }
    // Clearing it brings everything back.
    c.update(cx, |c, cx| c.set_query("  ", cx));
    draw(cx);
    assert!(c.read_with(cx, |c, _| c.event_bounds("ro")).is_some());
    // The calendar never sent a query of its own.
    assert!(
        !seen
            .borrow()
            .iter()
            .any(|e| matches!(e, CalendarEvent::QueryChange(_)))
    );
}

#[gpui::test]
fn command_f_asks_the_host_and_typing_stays_keys_until_asked(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    // Command-F on the Mac and the iPad, Control-F elsewhere, as the calendar binds it.
    cx.simulate_keystrokes(if cfg!(any(target_os = "macos", target_os = "ios")) {
        "cmd-f"
    } else {
        "ctrl-f"
    });
    assert!(has(&seen, &CalendarEvent::Find));
    // Unasked, letters are still the calendar's keys: m is the month.
    cx.simulate_keystrokes("m");
    assert_eq!(c.read_with(cx, |c, _| c.view()), CalendarView::Month);
    assert_eq!(c.read_with(cx, |c, _| c.query().to_string()), "");
    cx.simulate_keystrokes("w");
    // Asked, they go into the query: "m", "t" and "d" don't switch views.
    cx.update(|window, cx| c.update(cx, |c, cx| c.type_query(window, cx)));
    cx.simulate_keystrokes("t m d");
    assert_eq!(c.read_with(cx, |c, _| c.query().to_string()), "tmd");
    assert_eq!(c.read_with(cx, |c, _| c.view()), CalendarView::Week);
    cx.simulate_keystrokes("backspace backspace backspace p l a n");
    assert!(has(&seen, &CalendarEvent::QueryChange("plan".into())));
    // Enter keeps it and gives the keys back.
    cx.simulate_keystrokes("enter");
    assert!(!c.read_with(cx, |c, _| c.is_typing_query()));
    assert_eq!(c.read_with(cx, |c, _| c.query().to_string()), "plan");
    cx.simulate_keystrokes("m");
    assert_eq!(c.read_with(cx, |c, _| c.view()), CalendarView::Month);
    // Escape while typing clears it.
    cx.update(|window, cx| c.update(cx, |c, cx| c.type_query(window, cx)));
    cx.simulate_keystrokes("escape");
    assert_eq!(c.read_with(cx, |c, _| c.query().to_string()), "");
    assert!(has(&seen, &CalendarEvent::QueryChange("".into())));
}

fn panel_on(c: Calendar) -> Calendar {
    let c = c.with_options(|o| o.event_panel = true);
    let mut events = events();
    let a = events.iter_mut().find(|e| e.id == "a").unwrap();
    a.notes = Some("Bring the numbers.".into());
    a.attendees = vec![
        Attendee::new("Priya", Response::Accepted).organizer(true),
        Attendee::new("Me", Response::NeedsAction).me(true),
    ];
    c.with_events(events)
}

fn double_click(cx: &mut VisualTestContext, p: Point<Pixels>) {
    cx.simulate_event(MouseDownEvent {
        position: p,
        modifiers: Modifiers::none(),
        button: MouseButton::Left,
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_mouse_up(p, MouseButton::Left, Modifiers::none());
    draw(cx);
}

#[gpui::test]
fn opening_an_event_shows_its_panel_when_the_host_asks(cx: &mut TestAppContext) {
    // Off (the default): Open, and no panel.
    let (c, seen, cx) = open(cx);
    double_click(cx, spot(&c, cx, day(2), 11, 30));
    assert!(has(&seen, &CalendarEvent::Open("a".into())));
    assert!(c.read_with(cx, |c, _| c.panel_event().is_none()));
    assert!(cx.debug_bounds("panel").is_none());
}

#[gpui::test]
fn the_panel_sends_its_actions_to_the_host(cx: &mut TestAppContext) {
    let (c, seen, cx) = open_with(cx, panel_on);
    let at = spot(&c, cx, day(2), 11, 30);
    double_click(cx, at);
    assert!(has(&seen, &CalendarEvent::Open("a".into())));
    assert_eq!(
        c.read_with(cx, |c, _| c.panel_event().cloned()),
        Some("a".into())
    );
    let panel = cx.debug_bounds("panel").expect("the panel");
    let chip = c.read_with(cx, |c, _| c.event_bounds("a")).unwrap();
    let both = panel.intersect(&chip);
    assert!(
        both.size.width <= px(0.) || both.size.height <= px(0.),
        "over the event"
    );
    // The answer to the invitation: Yes is the group's first third.
    let rsvp = cx.debug_bounds("panel-rsvp").expect("Going?");
    cx.simulate_click(
        point(rsvp.left() + rsvp.size.width / 6., rsvp.center().y),
        Modifiers::none(),
    );
    draw(cx);
    assert!(has(
        &seen,
        &CalendarEvent::Rsvp {
            event: reference("a"),
            response: Response::Accepted,
            scope: EditScope::This,
        }
    ));
    // A one-off answer leaves the panel open; a link in it opens the link.
    assert_eq!(
        c.read_with(cx, |c, _| c.panel_event().cloned()),
        Some("a".into())
    );
    let link = cx.debug_bounds("panel-link-0").expect("the link");
    cx.simulate_click(link.center(), Modifiers::none());
    draw(cx);
    assert!(seen.borrow().iter().any(|e| matches!(
        e,
        CalendarEvent::OpenLink { event, .. } if event.id == "a"
    )));
    // Escape closes it.
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(c.read_with(cx, |c, _| c.panel_event().is_none()));
    // Delete from the panel: not repeating, so straight to the host.
    double_click(cx, at);
    let delete = cx.debug_bounds("panel-delete").expect("Delete");
    cx.simulate_click(delete.center(), Modifiers::none());
    draw(cx);
    assert!(has(
        &seen,
        &CalendarEvent::Delete {
            event: reference("a"),
            scope: EditScope::This
        }
    ));
    assert!(c.read_with(cx, |c, _| c.panel_event().is_none()));
}

#[gpui::test]
fn a_web_address_in_the_panel_asks_the_host_to_open_it(cx: &mut TestAppContext) {
    let (c, seen, cx) = open_with(cx, |c| {
        let c = panel_on(c);
        let mut events = events();
        let a = events.iter_mut().find(|e| e.id == "a").unwrap();
        a.notes = Some("https://meet.example/abc-defg".into());
        a.location = Some("Room 4".into());
        c.with_events(events)
    });
    c.update(cx, |c, cx| c.open_panel("a".into(), cx));
    draw(cx);
    // A click on the place, which has no address, opens nothing.
    let place = cx.debug_bounds("panel-place").expect("the place");
    cx.simulate_click(
        point(place.left() + px(8.), place.center().y),
        Modifiers::none(),
    );
    draw(cx);
    let opened = |seen: &Seen| {
        seen.borrow()
            .iter()
            .filter_map(|e| match e {
                CalendarEvent::OpenUrl { event, url } => Some((event.id.clone(), url.clone())),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert!(opened(&seen).is_empty());
    // One on the address in the notes asks the host to open it; the panel stays.
    let notes = cx.debug_bounds("panel-notes").expect("the notes");
    cx.simulate_click(
        point(notes.left() + px(12.), notes.center().y),
        Modifiers::none(),
    );
    draw(cx);
    assert_eq!(
        opened(&seen),
        vec![("a".into(), "https://meet.example/abc-defg".into())]
    );
    assert_eq!(
        c.read_with(cx, |c, _| c.panel_event().cloned()),
        Some("a".into())
    );
}

#[gpui::test]
fn delete_with_the_panel_open_deletes_its_event(cx: &mut TestAppContext) {
    let (c, seen, cx) = open_with(cx, panel_on);
    // The panel on Planning, opened by the host with another event selected.
    c.update(cx, |c, cx| {
        c.select(Some(Selection::Event("notes".into())), cx);
        c.open_panel("a".into(), cx);
    });
    draw(cx);
    cx.simulate_keystrokes("delete");
    draw(cx);
    assert!(has(
        &seen,
        &CalendarEvent::Delete {
            event: reference("a"),
            scope: EditScope::This
        }
    ));
    assert!(
        !seen
            .borrow()
            .iter()
            .any(|e| matches!(e, CalendarEvent::Delete { event, .. } if event.id == "notes"))
    );
    assert!(c.read_with(cx, |c, _| c.panel_event().is_none()));
    // A repeating one: the panel gives way to the question, never under it.
    c.update(cx, |c, cx| c.open_panel("gym@4".into(), cx));
    draw(cx);
    cx.simulate_keystrokes("backspace");
    draw(cx);
    assert!(c.read_with(cx, |c, _| c.is_asking()));
    assert!(c.read_with(cx, |c, _| c.panel_event().is_none()));
    assert!(cx.debug_bounds("panel").is_none());
    cx.simulate_keystrokes("enter");
    assert!(has(
        &seen,
        &CalendarEvent::Delete {
            event: gym4(),
            scope: EditScope::This
        }
    ));
    // A read-only event's panel deletes nothing.
    c.update(cx, |c, cx| c.open_panel("ro".into(), cx));
    draw(cx);
    cx.simulate_keystrokes("delete");
    draw(cx);
    assert!(
        !seen
            .borrow()
            .iter()
            .any(|e| matches!(e, CalendarEvent::Delete { event, .. } if event.id == "ro"))
    );
    // The host was only asked: Planning is still there.
    assert!(c.read_with(cx, |c, _| c.event("a").is_some()));
}

#[gpui::test]
fn the_panel_adds_the_events_own_zone_where_it_differs(cx: &mut TestAppContext) {
    let (c, _, cx) = open_with(cx, panel_on);
    c.update(cx, |c, cx| c.open_panel("a".into(), cx));
    draw(cx);
    assert!(cx.debug_bounds("panel").is_some());
    assert!(cx.debug_bounds("panel-zone").is_none());
    // Made in New York: a second line with its time there, under the first.
    c.update(cx, |c, cx| {
        let e = c
            .event("a")
            .unwrap()
            .clone()
            .time_zone(chrono_tz::America::New_York);
        c.upsert_event(e, cx);
    });
    draw(cx);
    let zone = cx.debug_bounds("panel-zone").expect("the event's own zone");
    let title = cx.debug_bounds("panel-title").unwrap();
    assert!(zone.top() > title.bottom());
    // In the calendar's own zone: nothing more.
    c.update(cx, |c, cx| {
        let e = c.event("a").unwrap().clone().time_zone(TZ);
        c.upsert_event(e, cx);
    });
    draw(cx);
    assert!(cx.debug_bounds("panel-zone").is_none());
}

#[gpui::test]
fn the_panel_joins_the_call_and_opens_the_events_page(cx: &mut TestAppContext) {
    let (c, seen, cx) = open_with(cx, panel_on);
    c.update(cx, |c, cx| c.open_panel("a".into(), cx));
    draw(cx);
    // Without either, neither is shown.
    assert!(cx.debug_bounds("panel-join").is_none());
    assert!(cx.debug_bounds("panel-url").is_none());
    c.update(cx, |c, cx| {
        let e = c
            .event("a")
            .unwrap()
            .clone()
            .conference(
                "https://meet.example/abc-defg",
                Some("Join with Meet".into()),
            )
            .url("https://calendar.example/event?eid=a", None);
        c.upsert_event(e, cx);
    });
    draw(cx);
    let opened = |seen: &Seen| {
        seen.borrow()
            .iter()
            .filter_map(|e| match e {
                CalendarEvent::OpenUrl { event, url } => Some((event.id.clone(), url.clone())),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let join = cx.debug_bounds("panel-join").expect("Join");
    cx.simulate_click(join.center(), Modifiers::none());
    draw(cx);
    assert_eq!(
        opened(&seen),
        vec![("a".into(), "https://meet.example/abc-defg".into())]
    );
    let page = cx.debug_bounds("panel-url").expect("the event's page");
    cx.simulate_click(page.center(), Modifiers::none());
    draw(cx);
    assert_eq!(
        opened(&seen).last().cloned(),
        Some(("a".into(), "https://calendar.example/event?eid=a".into()))
    );
    // Neither closes the panel, and they're there for a read-only event too.
    assert_eq!(
        c.read_with(cx, |c, _| c.panel_event().cloned()),
        Some("a".into())
    );
    c.update(cx, |c, cx| {
        let e = c
            .event("ro")
            .unwrap()
            .clone()
            .conference("https://meet.example/ro", None);
        c.upsert_event(e, cx);
        c.open_panel("ro".into(), cx);
    });
    draw(cx);
    assert!(cx.debug_bounds("panel-join").is_some());
}

#[gpui::test]
fn e_in_the_open_panel_asks_to_edit_its_event(cx: &mut TestAppContext) {
    let (c, seen, cx) = open_with(cx, panel_on);
    let edits = |seen: &Seen| {
        seen.borrow()
            .iter()
            .filter_map(|e| match e {
                CalendarEvent::Edit(r) => Some(r.id.clone()),
                _ => None,
            })
            .collect::<Vec<EventId>>()
    };
    // Without the panel, e does nothing.
    c.update(cx, |c, cx| c.select(Some(Selection::Event("a".into())), cx));
    cx.simulate_keystrokes("e");
    draw(cx);
    assert!(edits(&seen).is_empty());
    // With it open (by the host, on another event than the selection), e edits its event.
    c.update(cx, |c, cx| c.open_panel("notes".into(), cx));
    draw(cx);
    cx.simulate_keystrokes("e");
    draw(cx);
    assert_eq!(edits(&seen), vec![EventId::from("notes")]);
    assert!(c.read_with(cx, |c, _| c.panel_event().is_none()));
    // A read-only event's panel stays, and nothing is asked.
    c.update(cx, |c, cx| c.open_panel("ro".into(), cx));
    draw(cx);
    cx.simulate_keystrokes("e");
    draw(cx);
    assert_eq!(edits(&seen).len(), 1);
    assert_eq!(
        c.read_with(cx, |c, _| c.panel_event().cloned()),
        Some("ro".into())
    );
    // The panel's Edit does the same.
    c.update(cx, |c, cx| c.open_panel("a".into(), cx));
    draw(cx);
    let edit = cx.debug_bounds("panel-actions").expect("Edit and Delete");
    cx.simulate_click(
        point(edit.left() + px(12.), edit.center().y),
        Modifiers::none(),
    );
    draw(cx);
    assert_eq!(edits(&seen).last(), Some(&EventId::from("a")));
}

#[gpui::test]
fn a_read_only_events_panel_has_no_edits(cx: &mut TestAppContext) {
    let (c, _, cx) = open_with(cx, panel_on);
    double_click(cx, spot(&c, cx, day(3), 9, 30));
    assert_eq!(
        c.read_with(cx, |c, _| c.panel_event().cloned()),
        Some("ro".into())
    );
    assert!(cx.debug_bounds("panel").is_some());
    assert!(cx.debug_bounds("panel-actions").is_none());
}

#[gpui::test]
fn moving_a_repeating_event_to_another_calendar_asks_first(cx: &mut TestAppContext) {
    let (c, seen, cx) = open_with(cx, panel_on);
    c.update(cx, |c, cx| c.open_panel("gym@4".into(), cx));
    draw(cx);
    // "Change" opens the list of the other calendars that can take it: Work only.
    assert!(cx.debug_bounds("panel-calendar-0").is_none());
    let change = cx.debug_bounds("panel-change").expect("Change");
    cx.simulate_click(change.center(), Modifiers::none());
    draw(cx);
    assert!(cx.debug_bounds("panel-calendar-1").is_none());
    let work = cx.debug_bounds("panel-calendar-0").expect("Work");
    cx.simulate_click(work.center(), Modifiers::none());
    draw(cx);
    // A repeating event: the panel gives way to the question.
    assert!(c.read_with(cx, |c, _| c.panel_event().is_none()));
    assert!(c.read_with(cx, |c, _| c.is_asking()));
    cx.simulate_keystrokes("down down enter");
    assert!(has(
        &seen,
        &CalendarEvent::ChangeCalendar {
            event: gym4(),
            calendar: "work".into(),
            scope: EditScope::All,
        }
    ));
}

#[gpui::test]
fn find_looks_in_the_notes_too(cx: &mut TestAppContext) {
    let (c, _, cx) = open_with(cx, panel_on);
    c.update(cx, |c, cx| c.set_query("NUMBERS", cx));
    draw(cx);
    let found: Vec<String> = c.read_with(cx, |c, _| {
        c.found().iter().map(|e| e.id.to_string()).collect()
    });
    assert_eq!(found, vec!["a".to_string()]);
    // And the panel shows the notes it found them in.
    c.update(cx, |c, cx| c.open_panel("a".into(), cx));
    draw(cx);
    assert!(cx.debug_bounds("panel").is_some());
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
fn n_proposes_a_new_event_at_the_slot_or_the_next_half_hour(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    // Nothing selected: the next round half hour today (it's 10:40 on Wednesday).
    cx.simulate_keystrokes("n");
    assert_eq!(
        created(&seen),
        vec![EventTime::timed(at(day(2), 11, 0), at(day(2), 12, 0))]
    );
    // From next week, it comes back to today.
    cx.simulate_keystrokes("j");
    assert_eq!(c.read_with(cx, |c, _| c.date()), day(9));
    cx.simulate_keystrokes("n");
    assert_eq!(c.read_with(cx, |c, _| c.visible_days().0), day(0));
    // A selected slot: there.
    c.update(cx, |c, cx| {
        c.select(
            Some(Selection::Slot {
                date: day(4),
                minute: 15 * 60 + 30,
            }),
            cx,
        )
    });
    cx.simulate_keystrokes("n");
    assert_eq!(
        created(&seen).last(),
        Some(&EventTime::timed(at(day(4), 15, 30), at(day(4), 16, 30)))
    );
    // In the month, a day.
    cx.simulate_keystrokes("m");
    c.update(cx, |c, cx| {
        c.select(
            Some(Selection::Slot {
                date: day(5),
                minute: 0,
            }),
            cx,
        )
    });
    cx.simulate_keystrokes("n");
    assert_eq!(
        created(&seen).last(),
        Some(&EventTime::all_day(day(5), day(6)))
    );
}

#[gpui::test]
fn the_arrows_cross_days_in_the_day_week_and_month(cx: &mut TestAppContext) {
    let (c, _, cx) = open(cx);
    let slot = |c: &Entity<Calendar>, cx: &VisualTestContext| {
        c.read_with(cx, |c, _| match c.selection() {
            Some(Selection::Slot { date, .. }) => Some(*date),
            _ => None,
        })
    };
    // The day: → is the next day, and the view follows it.
    cx.simulate_keystrokes("d down right");
    assert_eq!(slot(&c, cx), Some(day(3)));
    assert_eq!(
        c.read_with(cx, |c, _| (c.view(), c.date())),
        (CalendarView::Day, day(3))
    );
    cx.simulate_keystrokes("left left");
    assert_eq!(c.read_with(cx, |c, _| c.date()), day(1));
    // The week: from Sunday, → is next Monday, in next week.
    cx.simulate_keystrokes("w");
    c.update(cx, |c, cx| {
        c.select(
            Some(Selection::Slot {
                date: day(6),
                minute: 600,
            }),
            cx,
        )
    });
    cx.simulate_keystrokes("right");
    assert_eq!(slot(&c, cx), Some(day(7)));
    assert_eq!(c.read_with(cx, |c, _| c.visible_days().0), day(7));
    // The month: → a day, ↓ a week.
    cx.simulate_keystrokes("m right down");
    assert_eq!(slot(&c, cx), Some(day(15)));
    // t, d, w, m and a all still do what they say.
    cx.simulate_keystrokes("t");
    assert_eq!(c.read_with(cx, |c, _| c.date()), day(2));
    for (key, view) in [
        ("d", CalendarView::Day),
        ("w", CalendarView::Week),
        ("m", CalendarView::Month),
        ("a", CalendarView::AGENDA),
    ] {
        cx.simulate_keystrokes(key);
        assert_eq!(c.read_with(cx, |c, _| c.view()), view, "{key}");
    }
}

#[gpui::test]
fn the_now_line_redraws_once_a_minute_and_no_more(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_core::init(cx);
        gpui_calendar::init(cx);
    });
    // The real clock, not a pretended one: the line at the current time keeps moving.
    let (calendar, vcx) = cx.add_window_view(|_, cx| Calendar::new(cx));
    let redraws = Rc::new(std::cell::Cell::new(0));
    vcx.update(|_, cx| {
        let r = redraws.clone();
        cx.observe(&calendar, move |_, _| r.set(r.get() + 1))
            .detach();
    });
    vcx.run_until_parked();
    // Ten minutes in steps of a second: at most one redraw a minute.
    for _ in 0..600 {
        vcx.executor()
            .advance_clock(std::time::Duration::from_secs(1));
        vcx.run_until_parked();
    }
    let n = redraws.get();
    assert!((9..=11).contains(&n), "{n} redraws in ten minutes");
}

fn mini_on(c: Calendar) -> Calendar {
    c.with_options(|o| o.mini_month = true)
}

fn day_cell(d: NaiveDate) -> &'static str {
    Box::leak(format!("day-{d}").into_boxed_str())
}

#[gpui::test]
fn the_mini_month_beside_a_wide_calendar_goes_to_a_date(cx: &mut TestAppContext) {
    let (c, seen, cx) = open_with(cx, mini_on);
    cx.simulate_resize(size(px(1280.), px(800.)));
    draw(cx);
    assert!(cx.debug_bounds("calendar-mini").is_some());
    // A day in the mini month: the calendar goes there, and tells the host what to supply.
    let friday = NaiveDate::from_ymd_opt(2026, 11, 27).unwrap();
    let cell = cx.debug_bounds(day_cell(friday)).expect("the 27th");
    cx.simulate_click(cell.center(), Modifiers::none());
    draw(cx);
    assert_eq!(c.read_with(cx, |c, _| c.date()), friday);
    assert!(
        seen.borrow()
            .iter()
            .any(|e| matches!(e, CalendarEvent::RangeChange { .. }))
    );
    // It keeps in step: the calendar a month on, the mini month shows December.
    cx.simulate_keystrokes("m j");
    draw(cx);
    draw(cx);
    let december = NaiveDate::from_ymd_opt(2026, 12, 15).unwrap();
    assert!(cx.debug_bounds(day_cell(december)).is_some());
}

#[gpui::test]
fn the_mini_month_is_behind_the_title_when_narrow(cx: &mut TestAppContext) {
    let (c, _, cx) = open_with(cx, |c| mini_on(c).with_header(true));
    cx.simulate_resize(size(px(390.), px(800.)));
    draw(cx);
    draw(cx);
    assert!(cx.debug_bounds("calendar-mini").is_none());
    // The title opens it; Escape closes it; g opens it too.
    let title = cx.debug_bounds("calendar-title").expect("a title");
    cx.simulate_click(title.center(), Modifiers::none());
    draw(cx);
    assert!(c.read_with(cx, |c, _| c.is_mini_month_open()));
    assert!(cx.debug_bounds("calendar-mini").is_some());
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(!c.read_with(cx, |c, _| c.is_mini_month_open()));
    cx.simulate_keystrokes("g");
    draw(cx);
    assert!(c.read_with(cx, |c, _| c.is_mini_month_open()));
    // A pick goes there and closes it.
    let d = NaiveDate::from_ymd_opt(2026, 11, 26).unwrap();
    let cell = cx.debug_bounds(day_cell(d)).expect("the 26th");
    cx.simulate_click(cell.center(), Modifiers::none());
    draw(cx);
    assert_eq!(c.read_with(cx, |c, _| c.date()), d);
    assert!(!c.read_with(cx, |c, _| c.is_mini_month_open()));
}

#[gpui::test]
fn on_a_desktop_without_room_beside_the_days_the_title_opens_the_mini_month(
    cx: &mut TestAppContext,
) {
    // 736 points: the desktop header, but less than the 848 the side column needs.
    let (c, _, cx) = open_with(cx, |c| mini_on(c).with_header(true));
    cx.simulate_resize(size(px(736.), px(800.)));
    draw(cx);
    draw(cx);
    assert!(cx.debug_bounds("calendar-mini").is_none());
    let title = cx.debug_bounds("calendar-title").expect("a title");
    cx.simulate_click(title.center(), Modifiers::none());
    draw(cx);
    assert!(c.read_with(cx, |c, _| c.is_mini_month_open()));
    assert!(cx.debug_bounds("calendar-mini").is_some());
    // A press on the title again closes it.
    cx.simulate_click(title.center(), Modifiers::none());
    draw(cx);
    assert!(!c.read_with(cx, |c, _| c.is_mini_month_open()));

    // Wide enough, the mini month is beside the days and the title is only the title.
    cx.simulate_resize(size(px(1280.), px(800.)));
    draw(cx);
    draw(cx);
    assert!(cx.debug_bounds("calendar-mini").is_some());
    let title = cx.debug_bounds("calendar-title").expect("a title");
    cx.simulate_click(title.center(), Modifiers::none());
    draw(cx);
    assert!(!c.read_with(cx, |c, _| c.is_mini_month_open()));
}

#[gpui::test]
fn every_event_is_described_for_a_screen_reader(cx: &mut TestAppContext) {
    let (c, _, cx) = open(cx);
    c.read_with(cx, |c, _| {
        let said = |id: &str| c.describe_event(id).unwrap();
        assert_eq!(
            said("a"),
            "Planning, Wednesday 11 AM to 12 PM, Work calendar, 1 link"
        );
        assert_eq!(
            said("many"),
            "Sync, Saturday 1 to 1:45 PM, Work calendar, 3 links"
        );
        assert_eq!(
            said("ro"),
            "All-hands, Thursday 9 to 10 AM, Work calendar, read-only"
        );
        assert_eq!(
            said("hol"),
            "Holiday, Tuesday 1 to 2 PM, Holidays calendar, read-only"
        );
        assert_eq!(
            said("gym@1"),
            "Gym, Tuesday 6 to 7 PM, Home calendar, repeating"
        );
        assert_eq!(
            said("trip"),
            "Trip, Monday to Tuesday, all day, Home calendar"
        );
        assert_eq!(c.describe_event("nope"), None);
    });
    // One day, a span longer than a week, and overnight.
    c.update(cx, |c, cx| {
        c.set_events(
            vec![
                Event::all_day("one", "Rent", day(2), day(3)),
                Event::all_day("long", "Leave", day(0), day(10)),
                Event::timed("late", "Deploy", at(day(3), 22, 0), at(day(4), 1, 30)),
            ],
            cx,
        )
    });
    c.read_with(cx, |c, _| {
        assert_eq!(c.describe_event("one").unwrap(), "Rent, Wednesday, all day");
        assert_eq!(
            c.describe_event("long").unwrap(),
            "Leave, November 16 to November 25, all day"
        );
        assert_eq!(
            c.describe_event("late").unwrap(),
            "Deploy, Thursday 10 PM to Friday 1:30 AM"
        );
    });
}

#[gpui::test]
fn an_invitation_says_whether_it_was_answered(cx: &mut TestAppContext) {
    let (c, _, cx) = open(cx);
    let invited = |id: &str, me: Attendee| {
        let mut e = Event::timed(id, "Review", at(day(2), 13, 0), at(day(2), 14, 0));
        e.attendees = vec![Attendee::new("Sam", Response::Accepted).organizer(true), me];
        e
    };
    c.update(cx, |c, cx| {
        c.set_events(
            vec![
                invited("new", Attendee::new("Alex", Response::NeedsAction).me(true)),
                invited("no", Attendee::new("Alex", Response::Declined).me(true)),
                invited("yes", Attendee::new("Alex", Response::Accepted).me(true)),
                // Sent by the user: there's nothing for them to answer.
                invited(
                    "mine",
                    Attendee::new("Alex", Response::NeedsAction)
                        .me(true)
                        .organizer(true),
                ),
            ],
            cx,
        )
    });
    draw(cx);
    c.read_with(cx, |c, _| {
        let said = |id: &str| c.describe_event(id).unwrap();
        assert_eq!(said("new"), "Review, Wednesday 1 to 2 PM, not answered");
        assert_eq!(said("no"), "Review, Wednesday 1 to 2 PM, declined");
        assert_eq!(said("yes"), "Review, Wednesday 1 to 2 PM");
        assert_eq!(said("mine"), "Review, Wednesday 1 to 2 PM");
    });
}

#[gpui::test]
fn what_a_screen_reader_is_told_is_what_was_drawn(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    let items = c.read_with(cx, |c, _| c.accessibility_items());
    let find = |t: &A11yTarget| items.iter().find(|i| i.target == *t);
    // The week's days by name, today said as today.
    let days: Vec<_> = items
        .iter()
        .filter(|i| matches!(i.target, A11yTarget::Day(_)))
        .collect();
    assert_eq!(days.len(), 7);
    let wednesday = find(&A11yTarget::Day(day(2))).unwrap();
    assert_eq!(wednesday.label, "Wednesday, November 18, today");
    assert_eq!(wednesday.role, gpui::Role::Button);
    // Each shown event once, where it was drawn; a hidden calendar's not at all.
    let a = find(&A11yTarget::Event("a".into())).unwrap();
    assert_eq!(a.role, gpui::Role::Button);
    assert_eq!(Some(a.bounds), c.read_with(cx, |c, _| c.event_bounds("a")));
    assert!(a.label.starts_with("Planning, Wednesday"));
    assert!(find(&A11yTarget::Event("hidden".into())).is_none());
    let ro = find(&A11yTarget::Event("ro".into())).unwrap();
    assert!(ro.read_only);
    let trip: Vec<_> = items
        .iter()
        .filter(|i| i.target == A11yTarget::Event("trip".into()))
        .collect();
    assert_eq!(trip.len(), 1);
    // Read in order: the days, then the events by when they start (the trip first), then the
    // hours.
    let first_event = items
        .iter()
        .position(|i| matches!(i.target, A11yTarget::Event(_)))
        .unwrap();
    assert!(
        items[..first_event]
            .iter()
            .all(|i| matches!(i.target, A11yTarget::Day(_)))
    );
    assert_eq!(items[first_event].target, A11yTarget::Event("trip".into()));
    // The hours on screen, each a cell: scrolled to 8 AM, midnight isn't there.
    let nine = find(&A11yTarget::Slot {
        date: day(2),
        minute: 9 * 60,
    })
    .unwrap();
    assert_eq!(nine.label, "Wednesday, November 18, 9 AM");
    assert_eq!(nine.role, gpui::Role::Cell);
    assert!(
        find(&A11yTarget::Slot {
            date: day(2),
            minute: 0
        })
        .is_none()
    );

    // Activating them does what a click does.
    c.update(cx, |c, cx| c.activate(&A11yTarget::Event("a".into()), cx));
    assert_eq!(
        c.read_with(cx, |c, _| c.selected_event().cloned()),
        Some("a".into())
    );
    assert!(
        seen.borrow()
            .iter()
            .any(|e| matches!(e, CalendarEvent::Open(id) if id.as_ref() == "a"))
    );
    draw(cx);
    let selected = c.read_with(cx, |c, _| c.accessibility_items());
    assert!(
        selected
            .iter()
            .any(|i| i.target == A11yTarget::Event("a".into()) && i.selected)
    );
    c.update(cx, |c, cx| {
        c.activate(
            &A11yTarget::Slot {
                date: day(3),
                minute: 14 * 60,
            },
            cx,
        )
    });
    assert_eq!(
        c.read_with(cx, |c, _| c.selection().cloned()),
        Some(Selection::Slot {
            date: day(3),
            minute: 14 * 60
        })
    );
    c.update(cx, |c, cx| c.activate(&A11yTarget::Day(day(4)), cx));
    assert_eq!(
        c.read_with(cx, |c, _| (c.view(), c.date())),
        (CalendarView::Day, day(4))
    );

    // The month: its days, each a cell, and the canvas named for the period.
    c.update(cx, |c, cx| c.set_view(CalendarView::Month, cx));
    draw(cx);
    let month = c.read_with(cx, |c, _| c.accessibility_items());
    let cells = month.iter().filter(|i| i.role == gpui::Role::Cell).count();
    assert!(cells == 35 || cells == 42, "{cells}");
    assert_eq!(
        c.read_with(cx, |c, _| c.accessibility_label()),
        "Calendar, November 2026"
    );
}

/// Six events at once on Thursday afternoon, in a window where Thursday has room for two.
fn crowded(c: &Entity<Calendar>, cx: &mut VisualTestContext) -> Vec<EventId> {
    let ids: Vec<EventId> = (0..6).map(|i| format!("crowd-{i}").into()).collect();
    let mut events = events();
    for (i, id) in ids.iter().enumerate() {
        events.push(
            Event::timed(
                id.clone(),
                format!("Crowd {i}"),
                at(day(3), 14, 0),
                at(day(3), 15, 0),
            )
            .calendar("work"),
        );
    }
    c.update(cx, |c, cx| c.set_events(events, cx));
    cx.simulate_resize(size(px(700.), px(800.)));
    draw(cx);
    draw(cx);
    ids
}

#[gpui::test]
fn side_by_side_events_too_many_for_the_day_show_a_count_that_lists_the_rest(
    cx: &mut TestAppContext,
) {
    let (c, seen, cx) = open(cx);
    let ids = crowded(&c, cx);
    // Thursday has room for a few readable columns beside the strip: those show, none
    // narrower than 36 pixels, and the rest are behind one "+N".
    let shown: Vec<_> = ids
        .iter()
        .filter_map(|id| c.read_with(cx, |c, _| c.event_bounds(id)))
        .collect();
    assert!(
        !shown.is_empty() && shown.len() < ids.len(),
        "{}",
        shown.len()
    );
    assert!(
        shown.iter().all(|b| b.size.width >= px(36.) - px(0.5)),
        "{shown:?}"
    );
    let hidden: Vec<_> = ids
        .iter()
        .filter(|id| c.read_with(cx, |c, _| c.event_bounds(id)).is_none())
        .cloned()
        .collect();
    let more = c
        .read_with(cx, |c, _| c.hidden_bounds(&hidden[0]))
        .expect("a +N");
    assert!(
        hidden
            .iter()
            .all(|id| c.read_with(cx, |c, _| c.hidden_bounds(id)) == Some(more))
    );

    // A click on it lists them in a popover beside it, never over it.
    cx.simulate_click(more.center(), Modifiers::none());
    draw(cx);
    assert_eq!(
        c.read_with(cx, |c, _| c.crowd_events().map(<[EventId]>::to_vec)),
        Some(hidden.clone())
    );
    let list = cx.debug_bounds("gpui-core-popover").expect("the list");
    let both = list.intersect(&more);
    assert!(
        both.size.width <= px(0.) || both.size.height <= px(0.),
        "{list:?} over {more:?}"
    );
    // A click on "+N" again closes it; Escape does too.
    cx.simulate_click(more.center(), Modifiers::none());
    draw(cx);
    assert!(c.read_with(cx, |c, _| c.crowd_events().is_none()));
    cx.simulate_click(more.center(), Modifiers::none());
    draw(cx);
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(c.read_with(cx, |c, _| c.crowd_events().is_none()));

    // A click on a row opens that event.
    cx.simulate_click(more.center(), Modifiers::none());
    draw(cx);
    let row = cx.debug_bounds("crowd-row-0").expect("a row");
    cx.simulate_click(row.center(), Modifiers::none());
    draw(cx);
    assert!(c.read_with(cx, |c, _| c.crowd_events().is_none()));
    assert!(has(&seen, &CalendarEvent::Open(hidden[0].clone())));
}

#[gpui::test]
fn a_month_days_more_lists_its_events_beside_it(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    let ids = crowded(&c, cx);
    c.update(cx, |c, cx| c.set_view(CalendarView::Month, cx));
    draw(cx);
    draw(cx);
    let more = c
        .read_with(cx, |c, _| c.accessibility_items())
        .into_iter()
        .find(|i| i.target == A11yTarget::More(day(3)))
        .expect("Thursday's +N more")
        .bounds;
    // A click lists all of Thursday's events, by start, beside it; the view stays.
    cx.simulate_click(more.center(), Modifiers::none());
    draw(cx);
    assert_eq!(c.read_with(cx, |c, _| c.view()), CalendarView::Month);
    assert_eq!(c.read_with(cx, |c, _| c.more_day()), Some(day(3)));
    let mut want: Vec<EventId> = vec!["ro".into()];
    want.extend(ids.iter().cloned());
    assert_eq!(
        c.read_with(cx, |c, _| c.crowd_events().map(<[EventId]>::to_vec)),
        Some(want)
    );
    let list = cx.debug_bounds("gpui-core-popover").expect("the list");
    let both = list.intersect(&more);
    assert!(
        both.size.width <= px(0.) || both.size.height <= px(0.),
        "{list:?} over {more:?}"
    );
    // Again closes it; Escape does too.
    cx.simulate_click(more.center(), Modifiers::none());
    draw(cx);
    assert!(c.read_with(cx, |c, _| c.crowd_events().is_none()));
    c.update(cx, |c, cx| assert!(c.open_more(day(3), cx)));
    draw(cx);
    cx.simulate_keystrokes("escape");
    draw(cx);
    assert!(c.read_with(cx, |c, _| c.crowd_events().is_none()));
    // A click on a row opens that event, and the list closes.
    cx.simulate_click(more.center(), Modifiers::none());
    draw(cx);
    let row = cx.debug_bounds("crowd-row-0").expect("a row");
    cx.simulate_click(row.center(), Modifiers::none());
    draw(cx);
    assert!(c.read_with(cx, |c, _| c.crowd_events().is_none()));
    assert!(has(&seen, &CalendarEvent::Open("ro".into())));
    assert_eq!(c.read_with(cx, |c, _| c.view()), CalendarView::Month);
}

#[gpui::test]
fn a_row_of_the_list_drags_its_event_into_the_grid(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    let ids = crowded(&c, cx);
    let hidden = ids
        .iter()
        .find(|id| c.read_with(cx, |c, _| c.event_bounds(id)).is_none())
        .unwrap()
        .clone();
    let more = c.read_with(cx, |c, _| c.hidden_bounds(&hidden)).unwrap();
    cx.simulate_click(more.center(), Modifiers::none());
    draw(cx);
    let first = c.read_with(cx, |c, _| c.crowd_events().unwrap()[0].clone());
    let row = cx.debug_bounds("crowd-row-0").expect("a row");
    // Held by its start: let go at Friday 9 AM, it starts there, as long as it was.
    let to = spot(&c, cx, day(4), 9, 0);
    press(cx, row.center());
    move_to(cx, point(row.center().x + px(10.), row.center().y));
    // The list gives way to the drag.
    assert!(c.read_with(cx, |c, _| c.crowd_events().is_none()));
    move_to(cx, to);
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    draw(cx);
    let moved = seen.borrow().iter().find_map(|e| match e {
        CalendarEvent::Move { event, time, .. } if event.id == first => Some(*time),
        _ => None,
    });
    assert_eq!(
        moved,
        Some(EventTime::timed(at(day(4), 9, 0), at(day(4), 10, 0)))
    );
}

#[gpui::test]
fn a_click_on_a_row_that_wobbles_a_pixel_opens_its_event_and_moves_nothing(
    cx: &mut TestAppContext,
) {
    let (c, seen, cx) = open(cx);
    let ids = crowded(&c, cx);
    let hidden = ids
        .iter()
        .find(|id| c.read_with(cx, |c, _| c.event_bounds(id)).is_none())
        .unwrap()
        .clone();
    let more = c.read_with(cx, |c, _| c.hidden_bounds(&hidden)).unwrap();
    cx.simulate_click(more.center(), Modifiers::none());
    draw(cx);
    let first = c.read_with(cx, |c, _| c.crowd_events().unwrap()[0].clone());
    let row = cx.debug_bounds("crowd-row-0").expect("a row");
    // A trackpad's click: down, a pixel over, up.
    let at = row.center();
    press(cx, at);
    let wobble = point(at.x + px(1.), at.y);
    move_to(cx, wobble);
    assert!(
        c.read_with(cx, |c, _| !c.is_dragging()),
        "a pixel isn't a drag"
    );
    cx.simulate_mouse_up(wobble, MouseButton::Left, Modifiers::none());
    draw(cx);
    assert!(has(&seen, &CalendarEvent::Open(first.clone())));
    assert!(
        !seen
            .borrow()
            .iter()
            .any(|e| matches!(e, CalendarEvent::Move { .. })),
        "nothing moved"
    );
    assert!(c.read_with(cx, |c, _| c.crowd_events().is_none()));
}

#[gpui::test]
fn a_screen_reader_hears_how_many_didnt_fit_and_when(cx: &mut TestAppContext) {
    let (c, _, cx) = open(cx);
    let ids = crowded(&c, cx);
    let hidden: Vec<EventId> = ids
        .iter()
        .filter(|id| c.read_with(cx, |c, _| c.event_bounds(id)).is_none())
        .cloned()
        .collect();
    let items = c.read_with(cx, |c, _| c.accessibility_items());
    let more = items
        .iter()
        .find(|i| matches!(&i.target, A11yTarget::Crowd(_)))
        .expect("a node for the +N");
    assert_eq!(more.role, gpui::Role::Button);
    assert_eq!(more.target, A11yTarget::Crowd(hidden.clone()));
    assert_eq!(
        more.label,
        format!("{} more events, 2 to 3 PM", hidden.len())
    );
    assert_eq!(
        Some(more.bounds),
        c.read_with(cx, |c, _| c.hidden_bounds(&hidden[0]))
    );
    // Each hidden event is in it, not a node of its own.
    for id in &hidden {
        assert!(
            !items
                .iter()
                .any(|i| i.target == A11yTarget::Event(id.clone()))
        );
    }
    // Activating it opens the list, as a click does.
    let target = more.target.clone();
    c.update(cx, |c, cx| c.activate(&target, cx));
    draw(cx);
    assert_eq!(
        c.read_with(cx, |c, _| c.crowd_events().map(<[EventId]>::to_vec)),
        Some(hidden)
    );
}

#[gpui::test]
fn an_all_day_event_dragged_into_the_hours_takes_the_default_length_there(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    let b = c
        .read_with(cx, |c, _| c.event_bounds("trip"))
        .expect("drawn");
    let grab = point(b.left() + px(20.), b.center().y);
    let to = spot(&c, cx, day(2), 14, 5);
    press(cx, grab);
    move_to(cx, point(grab.x + px(10.), grab.y));
    move_to(cx, to);
    // The ghost is in the hours, an hour long from the snapped drop.
    let want = EventTime::timed(at(day(2), 14, 0), at(day(2), 15, 0));
    assert_eq!(
        c.read_with(cx, |c, _| c.drag_proposal()),
        Some((Some("trip".into()), want))
    );
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    draw(cx);
    assert!(has(
        &seen,
        &CalendarEvent::Move {
            event: reference("trip"),
            time: want,
            scope: EditScope::This,
        }
    ));
    unchanged(&c, cx);
}

#[gpui::test]
fn a_timed_event_dragged_into_the_all_day_row_becomes_that_day(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    let a = c.read_with(cx, |c, _| c.event_bounds("a")).expect("drawn");
    let row = c
        .read_with(cx, |c, _| c.event_bounds("trip"))
        .expect("drawn")
        .center()
        .y;
    let grab = point(a.center().x, a.top() + px(6.));
    let thursday = spot(&c, cx, day(3), 12, 0).x;
    press(cx, grab);
    move_to(cx, point(grab.x, grab.y - px(10.)));
    move_to(cx, point(thursday, row));
    let all_day = EventTime::all_day(day(3), day(4));
    assert_eq!(
        c.read_with(cx, |c, _| c.drag_proposal()),
        Some((Some("a".into()), all_day))
    );
    // And back down into the hours, before letting go: timed again, as long as it was.
    let back = spot(&c, cx, day(3), 15, 0);
    move_to(cx, back);
    assert_eq!(
        c.read_with(cx, |c, _| c.drag_proposal()),
        Some((
            Some("a".into()),
            EventTime::timed(at(day(3), 15, 0), at(day(3), 16, 0))
        ))
    );
    move_to(cx, point(thursday, row));
    cx.simulate_mouse_up(point(thursday, row), MouseButton::Left, Modifiers::none());
    draw(cx);
    assert!(has(
        &seen,
        &CalendarEvent::Move {
            event: reference("a"),
            time: all_day,
            scope: EditScope::This,
        }
    ));
    unchanged(&c, cx);
}

#[gpui::test]
fn a_multi_day_event_moves_across_the_week_by_days(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    let b = c
        .read_with(cx, |c, _| c.event_bounds("trip"))
        .expect("drawn");
    let grab = point(b.left() + px(20.), b.center().y);
    let two_days = spot(&c, cx, day(2), 12, 0).x - spot(&c, cx, day(0), 12, 0).x;
    drag(cx, grab, point(grab.x + two_days, grab.y));
    assert!(has(
        &seen,
        &CalendarEvent::Move {
            event: reference("trip"),
            time: EventTime::all_day(day(2), day(4)),
            scope: EditScope::This,
        }
    ));
    // Held by its second day, it keeps its length and where it was held.
    let b = c
        .read_with(cx, |c, _| c.event_bounds("trip"))
        .expect("drawn");
    let second = point(b.right() - px(20.), b.center().y);
    let one_day = spot(&c, cx, day(1), 12, 0).x - spot(&c, cx, day(0), 12, 0).x;
    drag(cx, second, point(second.x + one_day * 3., second.y));
    assert!(has(
        &seen,
        &CalendarEvent::Move {
            event: reference("trip"),
            time: EventTime::all_day(day(3), day(5)),
            scope: EditScope::This,
        }
    ));
    unchanged(&c, cx);
}

#[gpui::test]
fn a_repeating_event_dragged_between_the_rows_is_asked_about(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    c.update(cx, |c, _| c.scroll_to_minute(12. * 60.));
    draw(cx);
    let gym = c
        .read_with(cx, |c, _| c.event_bounds("gym@1"))
        .expect("drawn");
    let row = c
        .read_with(cx, |c, _| c.event_bounds("trip"))
        .expect("drawn")
        .center()
        .y;
    let grab = point(gym.center().x, gym.top() + px(6.));
    drag(cx, grab, point(grab.x, row));
    assert!(c.read_with(cx, |c, _| c.is_asking()));
    c.update(cx, |c, cx| c.answer(EditScope::All, cx));
    assert!(has(
        &seen,
        &CalendarEvent::Move {
            event: reference("gym@1"),
            time: EventTime::all_day(day(1), day(2)),
            scope: EditScope::All,
        }
    ));
}

fn selected_id(c: &Entity<Calendar>, cx: &VisualTestContext) -> Option<EventId> {
    c.read_with(cx, |c, _| c.selected_event().cloned())
}

fn right_click(cx: &mut VisualTestContext, p: Point<Pixels>) {
    cx.simulate_mouse_move(p, None, Modifiers::none());
    cx.simulate_mouse_down(p, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(p, MouseButton::Right, Modifiers::none());
    draw(cx);
}

#[gpui::test]
fn a_right_click_on_an_event_opens_its_menu(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    let p = spot(&c, cx, day(2), 11, 30);
    right_click(cx, p);
    // It selects the event, as Google Calendar does, and offers what its keys do.
    assert_eq!(selected_id(&c, cx), Some("a".into()));
    assert_eq!(
        c.read_with(cx, |c, _| c.menu_items()),
        Some(vec!["open", "edit", "delete"])
    );
    // Edit asks the host to edit it; the menu closes.
    c.update(cx, |c, cx| c.pick_menu_item("edit", cx));
    assert!(has(&seen, &CalendarEvent::Edit(reference("a"))));
    assert_eq!(c.read_with(cx, |c, _| c.menu_items()), None);
    // Delete asks the host to delete it.
    right_click(cx, p);
    c.update(cx, |c, cx| c.pick_menu_item("delete", cx));
    assert!(has(
        &seen,
        &CalendarEvent::Delete {
            event: reference("a"),
            scope: EditScope::This
        }
    ));
    // Escape closes it, and leaves the event selected.
    right_click(cx, p);
    cx.simulate_keystrokes("escape");
    assert_eq!(c.read_with(cx, |c, _| c.menu_items()), None);
    assert_eq!(selected_id(&c, cx), Some("a".into()));
}

#[gpui::test]
fn a_read_only_event_s_menu_only_opens_it(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    right_click(cx, spot(&c, cx, day(3), 9, 30));
    assert_eq!(c.read_with(cx, |c, _| c.menu_items()), Some(vec!["open"]));
    c.update(cx, |c, cx| c.pick_menu_item("open", cx));
    assert!(has(&seen, &CalendarEvent::Open("ro".into())));
}

#[gpui::test]
fn empty_time_s_menu_makes_an_event_there(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    right_click(cx, spot(&c, cx, day(2), 15, 10));
    assert_eq!(c.read_with(cx, |c, _| c.menu_items()), Some(vec!["new"]));
    c.update(cx, |c, cx| c.pick_menu_item("new", cx));
    // At the slot it was opened on (15:00, snapped), the usual hour long.
    assert!(seen.borrow().iter().any(|e| matches!(
        e,
        CalendarEvent::Create { time, .. }
            if *time == EventTime::timed(at(day(2), 15, 0), at(day(2), 16, 0))
    )));
}

#[gpui::test]
fn shift_f10_opens_the_selected_event_s_menu(cx: &mut TestAppContext) {
    let (c, _, cx) = open(cx);
    c.update(cx, |c, cx| {
        c.select(Some(Selection::Event("gym@1".into())), cx)
    });
    draw(cx);
    cx.simulate_keystrokes("shift-f10");
    draw(cx);
    assert_eq!(
        c.read_with(cx, |c, _| c.menu_items()),
        Some(vec!["open", "edit", "delete"])
    );
    // A repeating event's Delete still asks which occurrences first.
    c.update(cx, |c, cx| c.pick_menu_item("delete", cx));
    assert!(c.read_with(cx, |c, _| c.is_asking()));
}

fn wheel(cx: &mut VisualTestContext, at: Point<Pixels>, dy: f32) {
    cx.simulate_event(gpui::ScrollEvent {
        position: at,
        delta: gpui::ScrollDelta::Pixels(point(px(0.), px(dy))),
        ..Default::default()
    });
    draw(cx);
}

#[gpui::test]
fn the_grid_s_scroll_bar_shows_while_it_scrolls_and_then_hides(cx: &mut TestAppContext) {
    let (c, _, cx) = open(cx);
    // At rest there's no bar over the events.
    assert_eq!(c.read_with(cx, |c, _| c.scroll_bar()), None);
    let p = spot(&c, cx, day(2), 11, 30);
    // From the top of the day, so there's room to scroll down.
    wheel(cx, p, 5000.);
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(1100));
    draw(cx);
    assert_eq!(c.read_with(cx, |c, _| c.scroll_bar()), None);
    let before = c.read_with(cx, |c, _| c.scroll_minute());
    wheel(cx, p, -120.);
    let thumb = c.read_with(cx, |c, _| c.scroll_bar()).expect("shown");
    assert!(c.read_with(cx, |c, _| c.scroll_minute()) > before);
    // As tall as the share of the day on screen, at the right edge.
    let edge = c.read_with(cx, |c, _| c.event_bounds("a").unwrap());
    assert!(thumb.left() > edge.left());
    // A second after it stops, it's gone.
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(1100));
    draw(cx);
    assert_eq!(c.read_with(cx, |c, _| c.scroll_bar()), None);
}

#[gpui::test]
fn the_scroll_bar_s_thumb_drags_the_hours_and_its_track_pages_them(cx: &mut TestAppContext) {
    let (c, _, cx) = open(cx);
    let p = spot(&c, cx, day(2), 11, 30);
    // At the top of the day, so the thumb has room to go down.
    wheel(cx, p, 5000.);
    let thumb = c.read_with(cx, |c, _| c.scroll_bar()).expect("shown");
    let before = c.read_with(cx, |c, _| c.scroll_minute());
    // The thumb, carried down, scrolls later in the day.
    let grab = thumb.center();
    cx.simulate_mouse_move(grab, None, Modifiers::none());
    cx.simulate_mouse_down(grab, MouseButton::Left, Modifiers::none());
    let to = point(grab.x, grab.y + px(40.));
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    draw(cx);
    let dragged = c.read_with(cx, |c, _| c.scroll_minute());
    assert!(dragged > before + 30., "{before} → {dragged}");
    // Nothing was selected or made by the press.
    assert_eq!(selected_id(&c, cx), None);
    // A press on the track above the thumb pages back up.
    let thumb = c.read_with(cx, |c, _| c.scroll_bar()).expect("shown");
    let above = point(thumb.center().x, thumb.top() - px(10.));
    cx.simulate_mouse_move(above, None, Modifiers::none());
    cx.simulate_mouse_down(above, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(above, MouseButton::Left, Modifiers::none());
    draw(cx);
    assert!(c.read_with(cx, |c, _| c.scroll_minute()) < dragged);
}
