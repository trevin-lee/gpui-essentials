//! The calendar driven by a finger, on macOS with `touch` turned on, through real touch events.
//! GPUI recognizes taps, long presses and pans from touches itself, the same on every platform,
//! so these go through what an iPhone's touches go through.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use chrono::{DateTime, NaiveDate, Utc};
use gpui::{
    Entity, Focusable as _, Pixels, Point, TestAppContext, TouchEvent, TouchId, TouchPhase,
    VisualTestContext, point, px, size,
};
use gpui_calendar::*;

const TZ: chrono_tz::Tz = chrono_tz::America::Los_Angeles;
type Seen = Rc<RefCell<Vec<CalendarEvent>>>;

/// Monday 16 November 2026, plus `n` days.
fn day(n: i64) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 16).unwrap() + chrono::Duration::days(n)
}

fn at(d: NaiveDate, h: u32, m: u32) -> DateTime<Utc> {
    time::at_minutes(d, (h * 60 + m) as f32, TZ)
}

fn events() -> Vec<Event> {
    vec![
        Event::timed("a", "Planning", at(day(2), 11, 0), at(day(2), 12, 0)).calendar("work"),
        // A quarter of an hour: shorter than a finger.
        Event::timed("short", "Call", at(day(4), 10, 0), at(day(4), 10, 15)).calendar("work"),
        Event::timed("run@1", "Run", at(day(5), 8, 0), at(day(5), 9, 0))
            .calendar("work")
            .series("run"),
        Event::timed("ro", "All-hands", at(day(3), 9, 0), at(day(3), 10, 0))
            .calendar("work")
            .read_only(true),
    ]
}

fn open(cx: &mut TestAppContext) -> (Entity<Calendar>, Seen, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_core::init(cx);
        gpui_calendar::init(cx);
    });
    let (calendar, cx) = cx.add_window_view(|_, cx| {
        let mut c = Calendar::new(cx)
            .with_options(|o| {
                o.time_zone = TZ;
                o.touch = true;
            })
            .with_now(at(day(2), 10, 40))
            .with_date(day(2))
            .with_calendars(vec![CalendarSource::new("work", "Work", 0x5b8def)])
            .with_events(events());
        c.scroll_to_minute(8. * 60.);
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

fn touch(cx: &mut VisualTestContext, phase: TouchPhase, at: Point<Pixels>) {
    cx.simulate_event(TouchEvent {
        id: TouchId(1),
        phase,
        position: at,
        predicted_position: None,
        force: None,
    });
}

fn wait(cx: &mut VisualTestContext, ms: u64) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    cx.run_until_parked();
}

fn tap(cx: &mut VisualTestContext, at: Point<Pixels>) {
    touch(cx, TouchPhase::Started, at);
    touch(cx, TouchPhase::Ended, at);
    draw(cx);
}

/// The finger moved from `from` to `to` in steps a frame apart, then held still and lifted
/// (so it doesn't fling).
fn slide(cx: &mut VisualTestContext, from: Point<Pixels>, to: Point<Pixels>) {
    let steps = 8;
    for i in 1..=steps {
        wait(cx, 16);
        touch(
            cx,
            TouchPhase::Moved,
            from + (to - from) * (i as f32 / steps as f32),
        );
        draw(cx);
    }
    wait(cx, 200);
    touch(cx, TouchPhase::Ended, to);
    draw(cx);
}

/// A finger pressed and held at `from`, then dragged to `to` and lifted.
fn long_press(cx: &mut VisualTestContext, from: Point<Pixels>, to: Point<Pixels>) {
    touch(cx, TouchPhase::Started, from);
    wait(cx, 800);
    draw(cx);
    slide(cx, from, to);
}

fn pan(cx: &mut VisualTestContext, from: Point<Pixels>, to: Point<Pixels>) {
    touch(cx, TouchPhase::Started, from);
    slide(cx, from, to);
    settle(cx);
}

/// Frames a display draws for half a second, so any easing finishes.
fn settle(cx: &mut VisualTestContext) {
    for _ in 0..40 {
        wait(cx, 16);
        draw(cx);
    }
}

/// A finger dragged `by` in `ms` milliseconds with a touch event every `every` milliseconds
/// (16 at 60 Hz, 8 on a 120 Hz display), and lifted at once, still moving.
fn flick(cx: &mut VisualTestContext, from: Point<Pixels>, by: Pixels, ms: u64, every: u64) {
    touch(cx, TouchPhase::Started, from);
    let steps = ms / every;
    for i in 1..=steps {
        wait(cx, every);
        let at = point(from.x + by * (i as f32 / steps as f32), from.y);
        touch(cx, TouchPhase::Moved, at);
        draw(cx);
    }
    touch(cx, TouchPhase::Ended, point(from.x + by, from.y));
    draw(cx);
    settle(cx);
}

fn has(seen: &Seen, want: &CalendarEvent) -> bool {
    seen.borrow().iter().any(|e| e == want)
}

fn reference(id: &str) -> EventRef {
    events()
        .iter()
        .find(|e| e.id.as_ref() == id)
        .unwrap()
        .to_ref()
}

fn unchanged(c: &Entity<Calendar>, cx: &VisualTestContext) {
    c.read_with(cx, |c, _| assert_eq!(c.events(), events().as_slice()));
}

#[gpui::test]
fn a_long_press_on_empty_time_proposes_an_hour_there(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    let p = spot(&c, cx, day(1), 14, 5);
    long_press(cx, p, p);
    assert!(has(
        &seen,
        &CalendarEvent::Create {
            calendar: Some("work".into()),
            time: EventTime::timed(at(day(1), 14, 0), at(day(1), 15, 0)),
        }
    ));
    unchanged(&c, cx);
}

#[gpui::test]
fn a_long_press_picks_an_event_up_and_the_finger_moves_it(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    long_press(
        cx,
        spot(&c, cx, day(2), 11, 15),
        spot(&c, cx, day(3), 12, 45),
    );
    assert!(has(&seen, &CalendarEvent::Select(Some("a".into()))));
    assert!(has(
        &seen,
        &CalendarEvent::Move {
            event: reference("a"),
            time: EventTime::timed(at(day(3), 12, 30), at(day(3), 13, 30)),
            scope: EditScope::This,
        }
    ));
    // A read-only event isn't picked up.
    long_press(cx, spot(&c, cx, day(3), 9, 30), spot(&c, cx, day(4), 14, 0));
    assert!(
        !seen
            .borrow()
            .iter()
            .any(|e| matches!(e, CalendarEvent::Move { event, .. } if event.id == "ro"))
    );
    unchanged(&c, cx);
}

#[gpui::test]
fn the_selected_event_follows_a_finger_and_a_tap_on_it_opens_it(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    let on = spot(&c, cx, day(2), 11, 30);
    tap(cx, on);
    assert!(has(&seen, &CalendarEvent::Select(Some("a".into()))));
    // Selected, it moves with the finger at once: no long press.
    touch(cx, TouchPhase::Started, on);
    slide(cx, on, spot(&c, cx, day(2), 13, 30));
    assert!(has(
        &seen,
        &CalendarEvent::Move {
            event: reference("a"),
            time: EventTime::timed(at(day(2), 13, 0), at(day(2), 14, 0)),
            scope: EditScope::This,
        }
    ));
    // A tap on it, still selected, opens it.
    tap(cx, on);
    assert!(has(&seen, &CalendarEvent::Open("a".into())));
    unchanged(&c, cx);
}

#[gpui::test]
fn the_selected_events_handle_takes_a_finger_beside_its_edge(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    tap(cx, spot(&c, cx, day(2), 11, 30));
    let b = c.read_with(cx, |c, _| c.event_bounds("a")).expect("drawn");
    // Just under the event: outside it, inside the handle's 44 points.
    let grab = point(b.center().x, b.bottom() + px(16.));
    touch(cx, TouchPhase::Started, grab);
    slide(
        cx,
        grab,
        point(grab.x, grab.y + spot(&c, cx, day(2), 13, 0).y - b.bottom()),
    );
    assert!(seen.borrow().iter().any(|e| matches!(
        e,
        CalendarEvent::Resize { event, time: EventTime::Timed { end, .. }, .. }
            if event.id == "a" && *end == at(day(2), 13, 0)
    )));
    unchanged(&c, cx);
}

#[gpui::test]
fn a_tap_beside_a_short_event_selects_it(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    let b = c
        .read_with(cx, |c, _| c.event_bounds("short"))
        .expect("drawn");
    assert!(b.size.height < px(30.), "{:?}", b.size);
    tap(cx, point(b.center().x, b.bottom() + px(8.)));
    assert!(has(&seen, &CalendarEvent::Select(Some("short".into()))));
}

#[gpui::test]
fn a_swipe_sideways_turns_the_period_and_a_short_one_springs_back(cx: &mut TestAppContext) {
    let (c, _, cx) = open(cx);
    let mid = spot(&c, cx, day(2), 13, 0);
    let date = |c: &Entity<Calendar>, cx: &VisualTestContext| c.read_with(cx, |c, _| c.date());
    // A short pan: back where it was.
    pan(cx, mid, point(mid.x - px(60.), mid.y));
    assert_eq!(date(&c, cx), day(2));
    // Past a fifth of the width, to the left: next week.
    pan(cx, mid, point(mid.x - px(400.), mid.y));
    assert_eq!(date(&c, cx), day(9));
    // And to the right: back again.
    pan(cx, mid, point(mid.x + px(400.), mid.y));
    assert_eq!(date(&c, cx), day(2));
    // Settled: drawn in place again.
    let again = spot(&c, cx, day(2), 13, 0);
    assert!((again.x - mid.x).abs() < px(1.), "{again:?} vs {mid:?}");
}

#[gpui::test]
fn a_flick_turns_the_period_at_any_touch_rate(cx: &mut TestAppContext) {
    let (c, _, cx) = open(cx);
    let mid = spot(&c, cx, day(2), 13, 0);
    let date = |c: &Entity<Calendar>, cx: &VisualTestContext| c.read_with(cx, |c, _| c.date());
    // 120 points in a tenth of a second (1,200 points a second) is short of a fifth of the
    // width but a flick, whether touches come at 60 Hz or 120 Hz.
    flick(cx, mid, px(-120.), 96, 16);
    assert_eq!(date(&c, cx), day(9));
    flick(cx, mid, px(120.), 96, 8);
    assert_eq!(date(&c, cx), day(2));
    // The same distance over 600 ms (200 points a second) is neither, at either rate.
    flick(cx, mid, px(-120.), 608, 16);
    assert_eq!(date(&c, cx), day(2));
    flick(cx, mid, px(-120.), 608, 8);
    assert_eq!(date(&c, cx), day(2));
}

#[gpui::test]
fn a_pan_up_scrolls_the_day(cx: &mut TestAppContext) {
    let (c, _, cx) = open(cx);
    let before = c.read_with(cx, |c, _| c.scroll_minute());
    let p = spot(&c, cx, day(5), 13, 0);
    pan(cx, p, point(p.x, p.y - px(200.)));
    let after = c.read_with(cx, |c, _| c.scroll_minute());
    assert!(after > before + 60., "{before} -> {after}");
}

#[gpui::test]
fn the_header_on_touch_has_44_point_controls_and_no_arrows(cx: &mut TestAppContext) {
    let (_, _, cx) = open(cx);
    let controls = cx.debug_bounds("calendar-controls").expect("a header");
    assert!(controls.size.height >= px(44.), "{:?}", controls.size);
    // The swipe changes the period, so the arrows aren't drawn: Today and four views.
    let title = cx.debug_bounds("calendar-title").expect("a title");
    assert!(title.right() <= controls.left() + px(0.5));
}

#[gpui::test]
fn on_touch_the_recurring_question_is_a_sheet_with_44_point_answers(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    long_press(
        cx,
        spot(&c, cx, day(5), 8, 15),
        spot(&c, cx, day(5), 10, 15),
    );
    assert!(c.read_with(cx, |c, _| c.is_asking()));
    assert!(cx.debug_bounds("gpui-core-sheet").is_some());
    for i in 0..3 {
        let row = cx
            .debug_bounds(Box::leak(format!("scope-{i}").into_boxed_str()))
            .expect("an answer");
        assert!(row.size.height >= px(44.), "{:?}", row.size);
    }
    let all = cx.debug_bounds("scope-2").unwrap().center();
    tap(cx, all);
    assert!(seen.borrow().iter().any(|e| matches!(
        e,
        CalendarEvent::Move { event, scope: EditScope::All, .. } if event.id == "run@1"
    )));
    // Again, and a tap on the dimmed calendar above the sheet drops it.
    long_press(
        cx,
        spot(&c, cx, day(5), 8, 15),
        spot(&c, cx, day(5), 10, 15),
    );
    assert!(c.read_with(cx, |c, _| c.is_asking()));
    let sheet = cx.debug_bounds("gpui-core-sheet").unwrap();
    tap(cx, point(sheet.center().x, sheet.top() - px(40.)));
    assert!(!c.read_with(cx, |c, _| c.is_asking()));
    let moves = seen
        .borrow()
        .iter()
        .filter(|e| matches!(e, CalendarEvent::Move { .. }))
        .count();
    assert_eq!(moves, 1);
}

#[gpui::test]
fn on_touch_the_event_panel_is_a_sheet_with_44_point_rows(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    c.update(cx, |c, cx| {
        c.set_options(|o| o.event_panel = true, cx);
        let mut e = c.event("a").unwrap().clone();
        e.links = vec![EventLink::doc("d", "Plan")];
        c.upsert_event(e, cx);
    });
    draw(cx);
    // Selected, a tap opens it.
    let on = spot(&c, cx, day(2), 11, 30);
    tap(cx, on);
    tap(cx, on);
    assert!(has(&seen, &CalendarEvent::Open("a".into())));
    assert!(cx.debug_bounds("gpui-core-sheet").is_some());
    let link = cx.debug_bounds("panel-link-0").expect("the link");
    assert!(link.size.height >= px(44.), "{:?}", link.size);
    // A tap above the sheet closes it.
    let sheet = cx.debug_bounds("gpui-core-sheet").unwrap();
    tap(cx, point(sheet.center().x, sheet.top() - px(30.)));
    assert!(c.read_with(cx, |c, _| c.panel_event().is_none()));
}

/// Opens Planning's panel with a finger, with a link on it to measure a row by.
fn open_panel_by_touch(
    c: &Entity<Calendar>,
    cx: &mut VisualTestContext,
    style: PanelStyle,
) -> Point<Pixels> {
    c.update(cx, |c, cx| {
        c.set_options(
            |o| {
                o.event_panel = true;
                o.panel_style = style;
            },
            cx,
        );
        let mut e = c.event("a").unwrap().clone();
        e.links = vec![EventLink::doc("d", "Plan")];
        c.upsert_event(e, cx);
    });
    draw(cx);
    let on = spot(c, cx, day(2), 11, 30);
    tap(cx, on);
    tap(cx, on);
    on
}

#[gpui::test]
fn on_touch_the_panel_can_be_a_popover_beside_the_event(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    open_panel_by_touch(&c, cx, PanelStyle::Popover);
    assert!(has(&seen, &CalendarEvent::Open("a".into())));
    assert!(cx.debug_bounds("gpui-core-sheet").is_none());
    let popover = cx.debug_bounds("gpui-core-popover").expect("a popover");
    // Beside the event, not over it.
    let chip = c.read_with(cx, |c, _| c.event_bounds("a")).unwrap();
    let both = popover.intersect(&chip);
    assert!(
        both.size.width <= px(0.) || both.size.height <= px(0.),
        "{popover:?} over {chip:?}"
    );
    // A finger's targets.
    for target in ["panel-link-0", "panel-close"] {
        let b = cx.debug_bounds(target).expect(target);
        assert!(b.size.height >= px(44.), "{target}: {:?}", b.size);
    }
    // A tap on the link opens it.
    let link = cx.debug_bounds("panel-link-0").unwrap();
    tap(cx, link.center());
    assert!(
        seen.borrow()
            .iter()
            .any(|e| matches!(e, CalendarEvent::OpenLink { event, .. } if event.id == "a"))
    );
    c.update(cx, |c, cx| c.open_panel("a".into(), cx));
    draw(cx);
    assert!(cx.debug_bounds("gpui-core-popover").is_some());
    // A tap outside it closes it.
    tap(cx, spot(&c, cx, day(0), 15, 0));
    assert!(c.read_with(cx, |c, _| c.panel_event().is_none()));
    assert!(cx.debug_bounds("gpui-core-popover").is_none());
}

#[gpui::test]
fn the_panel_can_be_a_sheet_whatever_takes_it(cx: &mut TestAppContext) {
    let (c, _, cx) = open(cx);
    open_panel_by_touch(&c, cx, PanelStyle::Sheet);
    assert!(cx.debug_bounds("gpui-core-popover").is_none());
    assert!(cx.debug_bounds("gpui-core-sheet").is_some());
    let link = cx.debug_bounds("panel-link-0").expect("the link");
    assert!(link.size.height >= px(44.), "{:?}", link.size);
    // A tap above it closes it.
    let sheet = cx.debug_bounds("gpui-core-sheet").unwrap();
    tap(cx, point(sheet.center().x, sheet.top() - px(30.)));
    assert!(c.read_with(cx, |c, _| c.panel_event().is_none()));
    // With a pointer too, where it would otherwise be a popover.
    c.update(cx, |c, cx| {
        c.set_options(|o| o.touch = false, cx);
        c.open_panel("a".into(), cx);
    });
    draw(cx);
    assert!(cx.debug_bounds("gpui-core-popover").is_none());
    assert!(cx.debug_bounds("gpui-core-sheet").is_some());
    // And Auto is a popover there.
    c.update(cx, |c, cx| {
        c.set_options(|o| o.panel_style = PanelStyle::Auto, cx)
    });
    draw(cx);
    assert!(cx.debug_bounds("gpui-core-popover").is_some());
    assert!(cx.debug_bounds("gpui-core-sheet").is_none());
}

#[gpui::test]
fn on_touch_a_month_days_more_lists_its_events_in_a_sheet(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    c.update(cx, |c, cx| {
        for i in 0..6 {
            c.upsert_event(
                Event::timed(
                    format!("busy-{i}"),
                    format!("Busy {i}"),
                    at(day(3), 13, 0),
                    at(day(3), 14, 0),
                )
                .calendar("work"),
                cx,
            );
        }
        c.set_view(CalendarView::Month, cx);
    });
    draw(cx);
    draw(cx);
    let more = c
        .read_with(cx, |c, _| c.accessibility_items())
        .into_iter()
        .find(|i| i.target == A11yTarget::More(day(3)))
        .expect("Thursday's +N more")
        .bounds;
    tap(cx, more.center());
    assert_eq!(c.read_with(cx, |c, _| c.more_day()), Some(day(3)));
    assert_eq!(c.read_with(cx, |c, _| c.view()), CalendarView::Month);
    assert!(cx.debug_bounds("gpui-core-sheet").is_some());
    assert!(cx.debug_bounds("gpui-core-popover").is_none());
    let row = cx.debug_bounds("crowd-row-0").expect("a row");
    assert!(row.size.height >= px(44.), "{:?}", row.size);
    // A tap on a row opens its event.
    tap(cx, row.center());
    assert!(has(&seen, &CalendarEvent::Open("ro".into())));
    assert!(c.read_with(cx, |c, _| c.crowd_events().is_none()));
}

#[gpui::test]
fn on_touch_the_mini_month_is_a_sheet_of_44_point_days(cx: &mut TestAppContext) {
    let (c, _, cx) = open(cx);
    c.update(cx, |c, cx| c.set_options(|o| o.mini_month = true, cx));
    draw(cx);
    // Touch keeps it behind the title, whatever the width.
    assert!(cx.debug_bounds("calendar-mini").is_none());
    let title = cx.debug_bounds("calendar-title").expect("a title");
    tap(cx, title.center());
    assert!(cx.debug_bounds("gpui-core-sheet").is_some());
    let d = NaiveDate::from_ymd_opt(2026, 11, 24).unwrap();
    let cell = cx
        .debug_bounds(Box::leak(format!("day-{d}").into_boxed_str()))
        .expect("the 24th");
    assert!(cell.size.height >= px(44.), "{:?}", cell.size);
    tap(cx, cell.center());
    assert_eq!(c.read_with(cx, |c, _| c.date()), d);
    assert!(!c.read_with(cx, |c, _| c.is_mini_month_open()));
}

#[gpui::test]
fn with_reduce_motion_a_swipe_lands_at_once(cx: &mut TestAppContext) {
    let (c, _, cx) = open(cx);
    let mid = spot(&c, cx, day(2), 13, 0);
    let date = |c: &Entity<Calendar>, cx: &VisualTestContext| c.read_with(cx, |c, _| c.date());
    // Without it, the week eases out over a few frames: lifted, it's still this week.
    touch(cx, TouchPhase::Started, mid);
    slide(cx, mid, point(mid.x - px(400.), mid.y));
    assert_eq!(date(&c, cx), day(2));
    settle(cx);
    assert_eq!(date(&c, cx), day(9));

    // With it (the host sets it from the system's setting), the next week is there on the
    // frame the finger lifts, in place.
    cx.update(|_, cx| cx.set_reduce_motion(true));
    touch(cx, TouchPhase::Started, mid);
    slide(cx, mid, point(mid.x - px(400.), mid.y));
    assert_eq!(date(&c, cx), day(16));
    let landed = spot(&c, cx, day(16), 13, 0);
    assert!((landed.x - mid.x).abs() < px(1.), "{landed:?} vs {mid:?}");
    // And it stays put: there's nothing left to ease.
    settle(cx);
    assert_eq!(date(&c, cx), day(16));
    let still = spot(&c, cx, day(16), 13, 0);
    assert_eq!(still, landed);
}

#[gpui::test]
fn on_touch_columns_stay_a_finger_wide_and_the_rest_open_in_a_sheet(cx: &mut TestAppContext) {
    let (c, _, cx) = open(cx);
    // Ten at once on Wednesday afternoon.
    let ids: Vec<EventId> = (0..10).map(|i| format!("pile-{i}").into()).collect();
    let mut events = c.read_with(cx, |c, _| c.events().to_vec());
    for (i, id) in ids.iter().enumerate() {
        events.push(Event::timed(
            id.clone(),
            format!("Pile {i}"),
            at(day(2), 15, 0),
            at(day(2), 16, 0),
        ));
    }
    c.update(cx, |c, cx| c.set_events(events, cx));
    draw(cx);
    draw(cx);
    let shown: Vec<_> = ids
        .iter()
        .filter_map(|id| c.read_with(cx, |c, _| c.event_bounds(id)))
        .collect();
    assert!(!shown.is_empty() && shown.len() < ids.len());
    assert!(shown.iter().all(|b| b.size.width >= px(43.5)), "{shown:?}");
    let hidden = ids
        .iter()
        .find(|id| c.read_with(cx, |c, _| c.event_bounds(id)).is_none())
        .unwrap();
    let more = c.read_with(cx, |c, _| c.hidden_bounds(hidden)).unwrap();
    tap(cx, more.center());
    assert!(c.read_with(cx, |c, _| c.crowd_events().is_some()));
    assert!(cx.debug_bounds("gpui-core-sheet").is_some());
    let row = cx.debug_bounds("crowd-row-0").expect("a row");
    assert!(row.size.height >= px(44.), "{row:?}");
}

#[gpui::test]
fn a_tap_picks_empty_time_and_a_second_tap_makes_an_event_there(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    let p = spot(&c, cx, day(1), 14, 5);
    tap(cx, p);
    c.read_with(cx, |c, _| {
        assert_eq!(
            c.selection(),
            Some(&Selection::Slot {
                date: day(1),
                minute: 14 * 60
            })
        )
    });
    assert!(
        !seen
            .borrow()
            .iter()
            .any(|e| matches!(e, CalendarEvent::Create { .. }))
    );
    // Past the double-tap window, so it's a second tap and not a double one.
    wait(cx, 1000);
    tap(cx, p);
    assert!(has(
        &seen,
        &CalendarEvent::Create {
            calendar: Some("work".into()),
            time: EventTime::timed(at(day(1), 14, 0), at(day(1), 15, 0)),
        }
    ));
    unchanged(&c, cx);
}

#[gpui::test]
fn a_tap_on_other_empty_time_moves_the_pick_without_making_anything(cx: &mut TestAppContext) {
    let (c, seen, cx) = open(cx);
    tap(cx, spot(&c, cx, day(1), 14, 5));
    wait(cx, 1000);
    tap(cx, spot(&c, cx, day(3), 15, 5));
    c.read_with(cx, |c, _| {
        assert_eq!(
            c.selection(),
            Some(&Selection::Slot {
                date: day(3),
                minute: 15 * 60
            })
        )
    });
    assert!(
        !seen
            .borrow()
            .iter()
            .any(|e| matches!(e, CalendarEvent::Create { .. }))
    );
}

#[gpui::test]
fn a_phone_wide_calendar_still_offers_the_week(cx: &mut TestAppContext) {
    let (c, _, cx) = open(cx);
    cx.simulate_resize(size(px(390.), px(800.)));
    c.update(cx, |c, cx| c.set_view(CalendarView::Day, cx));
    draw(cx);
    draw(cx);
    for view in [
        "calendar-view-Day",
        "calendar-view-Week",
        "calendar-view-Month",
        "calendar-view-List",
    ] {
        assert!(cx.debug_bounds(view).is_some(), "no {view}");
    }
    let week = cx.debug_bounds("calendar-view-Week").unwrap();
    tap(cx, week.center());
    assert_eq!(c.read_with(cx, |c, _| c.view()), CalendarView::Week);
}
