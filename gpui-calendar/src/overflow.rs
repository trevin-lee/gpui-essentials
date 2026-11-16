//! No text runs over: every view, drawn at many widths with long titles, locations and links,
//! laid out with Core Text and Inter as on screen. A line of text must stay inside the area it's
//! drawn in (so nothing is cut off mid-letter) and must not run into another line.

#![cfg(target_os = "macos")]

use std::sync::Arc;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use gpui::{Bounds, Pixels, TestAppContext, TestDispatcher, point, px, size};
use gpui_core::Theme;

use crate::paint::drawn;
use crate::*;

const TZ: chrono_tz::Tz = chrono_tz::America::Los_Angeles;

/// Monday 16 November 2026, plus `n` days.
fn day(n: i64) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 16).unwrap() + Duration::days(n)
}

fn at(d: NaiveDate, h: u32, m: u32) -> DateTime<Utc> {
    time::at_minutes(d, (h * 60 + m) as f32, TZ)
}

/// A week with the worst of everything: long titles, long locations, links with long labels,
/// events side by side, short ones, all-day spans across the week and crowded days.
fn worst_week() -> Vec<Event> {
    let long = "Quarterly planning with the platform, design and research teams";
    let mut out = vec![
        Event::timed("a", long, at(day(0), 9, 0), at(day(0), 11, 30))
            .location("Building 4, the big room at the end of the corridor")
            .link(EventLink::doc(
                "d1",
                "Q4 planning notes and the decisions we made",
            ))
            .link(EventLink::agent(
                "main",
                "Main, the agent that keeps the plan",
            )),
        Event::timed("b", long, at(day(0), 9, 30), at(day(0), 10, 0))
            .link(EventLink::doc("d2", "Agenda")),
        Event::timed("c", long, at(day(0), 10, 0), at(day(0), 10, 15))
            .link(EventLink::doc("d3", "Notes"))
            .link(EventLink::doc("d4", "More notes")),
        Event::timed("d", "Lunch", at(day(1), 12, 0), at(day(1), 13, 0))
            .link(EventLink::agent("health", "Health")),
        Event::timed("e", long, at(day(2), 13, 0), at(day(2), 16, 0)).link(EventLink::doc(
            "d5",
            "A doc with a very long name that goes on and on",
        )),
        Event::timed("f", "Overnight", at(day(3), 22, 0), at(day(4), 2, 0)),
        Event::all_day("g", long, day(0), day(7)).link(EventLink::doc("d6", "Trip")),
        Event::all_day("h", "Rent", day(2), day(3))
            .link(EventLink::doc("d7", "Rent"))
            .link(EventLink::agent("fin", "Finances")),
    ];
    // A crowded Thursday: eight overlapping events.
    for i in 0..8 {
        out.push(
            Event::timed(
                format!("crowd-{i}"),
                format!("{long} {i}"),
                at(day(3), 9 + i / 4, (i % 4) * 15),
                at(day(3), 11, 0),
            )
            .link(EventLink::doc(format!("c{i}"), "Notes")),
        );
    }
    // And a month's worth of busy days around it.
    for d in -16..30 {
        for k in 0..4 {
            out.push(
                Event::timed(
                    format!("m{d}-{k}"),
                    format!("{long} ({k})"),
                    at(day(d), 8 + k as u32 * 2, 30),
                    at(day(d), 9 + k as u32 * 2, 0),
                )
                .link(EventLink::doc(format!("m{d}"), "Notes")),
            );
        }
    }
    out
}

fn context() -> TestAppContext {
    let cx = TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        Arc::new(gpui_apple::AppleTextSystem::new()),
    );
    cx.update(|cx| {
        gpui_core::init(cx);
        crate::init(cx);
        let _ = cx.text_system().add_fonts(vec![
            std::borrow::Cow::Borrowed(
                include_bytes!("../examples/fonts/Inter-Regular.ttf").as_slice(),
            ),
            std::borrow::Cow::Borrowed(
                include_bytes!("../examples/fonts/Inter-Medium.ttf").as_slice(),
            ),
            std::borrow::Cow::Borrowed(
                include_bytes!("../examples/fonts/Inter-SemiBold.ttf").as_slice(),
            ),
        ]);
        Theme::global_mut(cx).font_family = "Inter".into();
    });
    cx
}

/// What's wrong with a frame's text, if anything.
fn problems(texts: &[drawn::Text], canvas: Bounds<Pixels>) -> Vec<String> {
    let tolerance = px(0.75);
    let mut out = Vec::new();
    let mut shown = Vec::new();
    for t in texts {
        let seen = t.bounds.intersect(&t.mask).intersect(&canvas);
        if seen.size.width <= px(0.) || seen.size.height <= px(0.) {
            // Scrolled away, or culled: not on screen at all.
            continue;
        }
        for (name, area) in [("its area", t.mask), ("the calendar", canvas)] {
            if t.bounds.left() < area.left() - tolerance
                || t.bounds.right() > area.right() + tolerance
            {
                out.push(format!(
                    "{:?} runs out of {name}: {:?} in {:?}",
                    t.text, t.bounds, area
                ));
            }
        }
        if t.text.ends_with('…') && crate::paint::kept_chars(&t.text) < crate::paint::MIN_KEPT {
            out.push(format!("{:?} is cut to almost nothing", t.text));
        }
        shown.push((t, seen));
    }
    for (i, (a, sa)) in shown.iter().enumerate() {
        for (b, sb) in &shown[i + 1..] {
            let both = sa.intersect(sb);
            if both.size.width > px(1.) && both.size.height > px(1.) {
                out.push(format!(
                    "{:?} runs into {:?}: {:?} and {:?}",
                    a.text, b.text, a.bounds, b.bounds
                ));
            }
        }
    }
    out
}

/// Draws these views at each width and collects what overflows.
fn check_widths(views: &[CalendarView], widths: &[f32], height: f32) {
    check_widths_with(views, widths, height, |_| {});
}

/// The same, with some options of the test's own.
fn check_widths_with(
    views: &[CalendarView],
    widths: &[f32],
    height: f32,
    options: impl Fn(&mut CalendarOptions),
) {
    let mut cx = context();
    let (calendar, vcx) = cx.add_window_view(|_, cx| {
        let mut c = Calendar::new(cx)
            .with_options(|o| {
                o.time_zone = TZ;
                options(o);
            })
            .with_header(false)
            .with_now(at(day(2), 10, 40))
            .with_date(day(2))
            .with_events(worst_week());
        c.scroll_to_minute(8. * 60.);
        c
    });
    let mut found = Vec::new();
    for &w in widths {
        vcx.simulate_resize(size(px(w), px(height)));
        for &view in views {
            calendar.update(vcx, |c, cx| c.set_view(view, cx));
            vcx.run_until_parked();
            drawn::record();
            vcx.update(|window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            });
            let texts = drawn::take();
            assert!(!texts.is_empty(), "nothing drawn at {w} in {view:?}");
            let canvas = calendar
                .read_with(vcx, |c, _| c.shared.geo.get())
                .expect("drawn")
                .bounds;
            for p in problems(&texts, canvas) {
                found.push(format!("{w}px, {view:?}: {p}"));
            }
        }
    }
    assert!(found.is_empty(), "{}", found.join("\n"));
}

#[test]
fn no_text_runs_over_at_desktop_widths() {
    let views = [
        CalendarView::Day,
        CalendarView::Days(3),
        CalendarView::Week,
        CalendarView::Month,
        CalendarView::AGENDA,
    ];
    check_widths(&views, &[720., 900., 1024., 1280., 1600., 2200.], 820.);
}

/// Every view fits a phone, from the narrowest iPhone up.
#[test]
fn no_text_runs_over_at_phone_widths() {
    let views = [
        CalendarView::Day,
        CalendarView::Days(3),
        CalendarView::Week,
        CalendarView::Month,
        CalendarView::AGENDA,
    ];
    check_widths(&views, &[320., 375., 390., 430., 520.], 844.);
}

/// The header fits too: its title gives way (with an ellipsis) before its buttons run out of
/// the calendar, from a phone to a wide window.
#[test]
fn the_header_fits_at_every_width() {
    check_header(false);
}

/// And the header for touch, with its 44-point controls and no arrows.
#[test]
fn the_touch_header_fits_at_every_width() {
    check_header(true);
}

fn check_header(touch: bool) {
    let mut cx = context();
    let (calendar, vcx) = cx.add_window_view(|_, cx| {
        Calendar::new(cx)
            .with_options(|o| {
                o.time_zone = TZ;
                o.touch = touch;
            })
            .with_now(at(day(2), 10, 40))
            .with_date(day(2))
            .with_events(worst_week())
    });
    let mut found = Vec::new();
    for w in [320., 375., 390., 430., 520., 599., 600., 720., 1024., 1600.] {
        vcx.simulate_resize(size(px(w), px(800.)));
        for view in [
            CalendarView::Day,
            CalendarView::Week,
            CalendarView::Month,
            CalendarView::AGENDA,
            CalendarView::Agenda(1),
            CalendarView::Agenda(40),
        ] {
            calendar.update(vcx, |c, cx| c.set_view(view, cx));
            // Twice: the first frame finds the width, the second draws the header for it.
            for _ in 0..2 {
                vcx.run_until_parked();
                vcx.update(|window, cx| {
                    window.refresh();
                    window.draw(cx).clear(cx);
                });
            }
            let title = vcx.debug_bounds("calendar-title").expect("a title");
            let controls = vcx.debug_bounds("calendar-controls").expect("controls");
            let tolerance = px(0.5);
            if controls.right() > px(w) + tolerance
                || title.left() < -tolerance
                || title.right() > controls.left() + tolerance
            {
                found.push(format!(
                    "{w}px, {view:?}: title {title:?}, controls {controls:?}"
                ));
            }
        }
    }
    assert!(found.is_empty(), "{}", found.join("\n"));
}

/// A swipe carries the days (their names, their events) and leaves the gutter's hours, and
/// the second zone's, where they are.
#[test]
fn a_swipe_leaves_the_hours_in_place() {
    let mut cx = context();
    let (calendar, vcx) = cx.add_window_view(|_, cx| {
        let mut c = Calendar::new(cx)
            .with_options(|o| {
                o.time_zone = TZ;
                o.secondary_zone = Some(chrono_tz::Asia::Kolkata);
            })
            .with_header(false)
            .with_now(at(day(2), 10, 40))
            .with_date(day(2))
            .with_view(CalendarView::Day)
            .with_events(worst_week());
        c.scroll_to_minute(8. * 60.);
        c
    });
    vcx.simulate_resize(size(px(390.), px(844.)));
    let find = |texts: &[drawn::Text], what: &str| {
        texts
            .iter()
            .find(|t| t.text == what)
            .map(|t| t.bounds.left())
            .unwrap_or_else(|| panic!("{what:?} isn't drawn"))
    };
    let mut frame = |swipe: f32| {
        calendar.update(vcx, |c, cx| {
            c.swipe = swipe;
            cx.notify();
        });
        vcx.run_until_parked();
        drawn::record();
        vcx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        drawn::take()
    };
    let still = frame(0.);
    let carried = frame(-120.);
    assert_eq!(find(&still, "1 PM"), find(&carried, "1 PM"));
    assert_eq!(find(&still, "PST"), find(&carried, "PST"));
    // The second zone's column stays with them.
    assert_eq!(find(&still, "IST"), find(&carried, "IST"));
    assert_eq!(find(&still, "2:30a"), find(&carried, "2:30a"));
    let moved = find(&still, "Wednesday") - find(&carried, "Wednesday");
    assert!((moved - px(120.)).abs() < px(0.5), "{moved:?}");
    // The next day comes in beside it, its name right of the gutter.
    assert!(carried.iter().any(|t| t.text == "Thursday"));
}

/// A second zone's column fits too, on a half-hour zone, in 12- and 24-hour time.
#[test]
fn a_second_zone_fits_at_every_width() {
    let views = [CalendarView::Day, CalendarView::Days(3), CalendarView::Week];
    let widths = [320., 390., 520., 720., 1280., 2200.];
    for hour12 in [true, false] {
        check_widths_with(&views, &widths, 820., |o| {
            o.secondary_zone = Some(chrono_tz::Asia::Kolkata);
            o.hour12 = hour12;
        });
    }
    check_widths_with(&views, &widths, 820., |o| {
        o.secondary_zone = Some(chrono_tz::Australia::Lord_Howe)
    });
}

/// The recurring-event question fits the window at every width and height: a popover on the
/// Mac, hung from the event (one at the week's right edge, and one too tall for room above or
/// below, where it goes beside) and never over it; a sheet on touch.
#[test]
fn the_recurring_question_fits_at_every_width() {
    for touch in [false, true] {
        let mut cx = context();
        let (calendar, vcx) = cx.add_window_view(|_, cx| {
            let mut events = worst_week();
            events.push(
                Event::timed("r@6", "Repeats", at(day(6), 9, 0), at(day(6), 10, 0)).series("r"),
            );
            events.push(
                Event::timed("tall@5", "Long", at(day(5), 8, 0), at(day(5), 20, 0)).series("t"),
            );
            let mut c = Calendar::new(cx)
                .with_options(|o| {
                    o.time_zone = TZ;
                    o.touch = touch;
                })
                .with_now(at(day(2), 10, 40))
                .with_date(day(2))
                .with_events(events);
            c.scroll_to_minute(8. * 60.);
            c
        });
        let mut found = Vec::new();
        for (w, h) in [
            (320., 800.),
            (390., 844.),
            (720., 600.),
            (1280., 800.),
            (2200., 900.),
        ] {
            vcx.simulate_resize(size(px(w), px(h)));
            for id in ["r@6", "tall@5"] {
                calendar.update(vcx, |c, cx| {
                    c.dismiss_question(cx);
                    c.set_view(CalendarView::Week, cx);
                    c.scroll_to_minute(8. * 60.);
                    c.select(Some(Selection::Event(id.into())), cx);
                });
                for _ in 0..2 {
                    vcx.run_until_parked();
                    vcx.update(|window, cx| {
                        window.refresh();
                        window.draw(cx).clear(cx);
                    });
                }
                let event = calendar.read_with(vcx, |c, _| c.event_bounds(id));
                calendar.update(vcx, |c, cx| c.delete_selected(cx));
                vcx.run_until_parked();
                vcx.update(|window, cx| {
                    window.refresh();
                    window.draw(cx).clear(cx);
                });
                assert!(calendar.read_with(vcx, |c, _| c.is_asking()));
                let window = Bounds::new(point(px(0.), px(0.)), size(px(w), px(h)));
                let panel = if touch {
                    "calendar-sheet"
                } else {
                    "calendar-popover"
                };
                let b = vcx.debug_bounds(panel).expect("the question");
                let inside = |r: Bounds<Pixels>, of: Bounds<Pixels>| {
                    r.left() >= of.left() - px(0.5)
                        && r.right() <= of.right() + px(0.5)
                        && r.top() >= of.top() - px(0.5)
                        && r.bottom() <= of.bottom() + px(0.5)
                };
                if !inside(b, window) {
                    found.push(format!("{w}x{h}, {id}, touch {touch}: {panel} {b:?}"));
                }
                if !touch && let Some(event) = event {
                    let both = b.intersect(&event);
                    if both.size.width > px(0.) && both.size.height > px(0.) {
                        found.push(format!("{w}x{h}, {id}: popover {b:?} covers {event:?}"));
                    }
                }
                for i in 0..3 {
                    let sel: &'static str = Box::leak(format!("scope-{i}").into_boxed_str());
                    let row = vcx.debug_bounds(sel).expect("an answer");
                    if !inside(row, b) {
                        found.push(format!("{w}x{h}, {id}: answer {i} {row:?} in {b:?}"));
                    }
                    if touch && row.size.height < px(44.) {
                        found.push(format!("{w}x{h}: answer {i} is {:?} tall", row.size.height));
                    }
                }
            }
        }
        assert!(found.is_empty(), "{}", found.join("\n"));
    }
}

/// With a query, found words are marked inside chips and the rest dim: nothing runs over, and
/// the pill the calendar shows while it takes typing fits at every width.
#[test]
fn a_query_fits_at_every_width() {
    let views = [
        CalendarView::Day,
        CalendarView::Week,
        CalendarView::Month,
        CalendarView::AGENDA,
    ];
    let mut cx = context();
    let (calendar, vcx) = cx.add_window_view(|_, cx| {
        let mut c = Calendar::new(cx)
            .with_options(|o| o.time_zone = TZ)
            .with_now(at(day(2), 10, 40))
            .with_date(day(2))
            .with_events(worst_week());
        c.scroll_to_minute(8. * 60.);
        c
    });
    vcx.update(|window, cx| {
        calendar.update(cx, |c, cx| {
            c.set_query("planning research notes 2", cx);
            c.type_query(window, cx);
        })
    });
    let mut found = Vec::new();
    for w in [320., 390., 720., 1280., 2200.] {
        vcx.simulate_resize(size(px(w), px(820.)));
        for &view in &views {
            calendar.update(vcx, |c, cx| c.set_view(view, cx));
            for _ in 0..2 {
                vcx.run_until_parked();
                drawn::record();
                vcx.update(|window, cx| {
                    window.refresh();
                    window.draw(cx).clear(cx);
                });
            }
            let texts = drawn::take();
            let canvas = calendar
                .read_with(vcx, |c, _| c.shared.geo.get())
                .expect("drawn")
                .bounds;
            for p in problems(&texts, canvas) {
                found.push(format!("{w}px, {view:?}: {p}"));
            }
            let pill = vcx.debug_bounds("calendar-query").expect("the pill");
            if pill.left() < px(0.) || pill.right() > px(w) {
                found.push(format!("{w}px, {view:?}: the pill {pill:?}"));
            }
        }
    }
    assert!(found.is_empty(), "{}", found.join("\n"));
}

/// The event panel fits at every width, on the Mac beside the event and never over it, and on
/// touch as a sheet: long titles, places, notes, links and guest names stay inside it.
#[test]
fn the_event_panel_fits_at_every_width() {
    for touch in [false, true] {
        let mut cx = context();
        let (calendar, vcx) = cx.add_window_view(|_, cx| {
            let mut events = worst_week();
            let a = events.iter_mut().find(|e| e.id == "a").unwrap();
            a.notes = Some(
                "A long description that goes on about the agenda, the people, the room, the \
                 prototype link and what has to be decided before the end of the week."
                    .repeat(3)
                    .into(),
            );
            a.attendees = (0..12)
                .map(|i| {
                    Attendee::new(
                        format!("Someone with a very long name indeed, number {i}"),
                        Response::Accepted,
                    )
                    .me(i == 1)
                })
                .collect();
            let mut c = Calendar::new(cx)
                .with_options(|o| {
                    o.time_zone = TZ;
                    o.touch = touch;
                    o.event_panel = true;
                })
                .with_calendars(vec![
                    CalendarSource::new("", "Work with an extremely long calendar name", 0x5b8def),
                    CalendarSource::new("home", "Home", 0x4cc38a),
                ])
                .with_now(at(day(2), 10, 40))
                .with_date(day(2))
                .with_events(events);
            c.scroll_to_minute(8. * 60.);
            c
        });
        let mut found = Vec::new();
        for (w, h) in [
            (320., 700.),
            (390., 844.),
            (720., 600.),
            (1280., 800.),
            (2200., 900.),
        ] {
            vcx.simulate_resize(size(px(w), px(h)));
            for view in [CalendarView::Day, CalendarView::Week] {
                calendar.update(vcx, |c, cx| {
                    c.close_panel(cx);
                    c.set_view(view, cx);
                    c.scroll_to_minute(8. * 60.);
                });
                for _ in 0..2 {
                    vcx.run_until_parked();
                    vcx.update(|window, cx| {
                        window.refresh();
                        window.draw(cx).clear(cx);
                    });
                }
                let event = calendar.read_with(vcx, |c, _| c.event_bounds("a"));
                calendar.update(vcx, |c, cx| c.open_panel("a".into(), cx));
                for _ in 0..2 {
                    vcx.run_until_parked();
                    vcx.update(|window, cx| {
                        window.refresh();
                        window.draw(cx).clear(cx);
                    });
                }
                let window = Bounds::new(point(px(0.), px(0.)), size(px(w), px(h)));
                // Narrow, the Mac shows the panel as a sheet too.
                let narrow = w < 600.;
                let outer = if touch || narrow {
                    "calendar-sheet"
                } else {
                    "calendar-popover"
                };
                let b = vcx
                    .debug_bounds(outer)
                    .unwrap_or_else(|| panic!("no {outer} at {w}x{h} {view:?} touch {touch}"));
                let inside = |r: Bounds<Pixels>, of: Bounds<Pixels>| {
                    r.left() >= of.left() - px(0.5)
                        && r.right() <= of.right() + px(0.5)
                        && r.top() >= of.top() - px(0.5)
                        && r.bottom() <= of.bottom() + px(0.5)
                };
                if !inside(b, window) {
                    found.push(format!("{w}x{h} {view:?} touch {touch}: {b:?}"));
                }
                // A sheet covers the calendar on purpose; a popover never covers the event.
                if !(touch || narrow)
                    && let Some(event) = event
                {
                    let both = b.intersect(&event);
                    if both.size.width > px(0.) && both.size.height > px(0.) {
                        found.push(format!("{w}x{h} {view:?}: panel {b:?} covers {event:?}"));
                    }
                }
                for sel in ["panel-title", "panel-link-0", "panel-guest-0", "panel-rsvp"] {
                    if let Some(r) = vcx.debug_bounds(sel)
                        && (r.left() < b.left() - px(0.5) || r.right() > b.right() + px(0.5))
                    {
                        found.push(format!(
                            "{w}x{h} {view:?} touch {touch}: {sel} {r:?} in {b:?}"
                        ));
                    }
                }
                if touch
                    && let Some(r) = vcx.debug_bounds("panel-link-0")
                    && r.size.height < px(44.)
                {
                    found.push(format!("{w}x{h}: a link row is {:?} tall", r.size.height));
                }
                // Each row's icon sits on the middle of its first line: a line of text, or a
                // pressable row (the calendar, a link) as tall as a target.
                let rem = vcx.update(|window, _| f32::from(window.rem_size()));
                let text_line = 20. * rem / 15.;
                let row = if touch { 44. } else { 28. };
                for (key, first_h) in [
                    ("when", text_line),
                    ("calendar", row),
                    ("place", text_line),
                    ("notes", text_line),
                    ("links", row),
                    ("guests", text_line),
                ] {
                    let sel = |what: &str| -> &'static str {
                        Box::leak(format!("panel-{what}-{key}").into_boxed_str())
                    };
                    if let (Some(icon), Some(first)) = (
                        vcx.debug_bounds(sel("icon")),
                        vcx.debug_bounds(sel("first")),
                    ) {
                        let middle = first.top() + px(first_h / 2.);
                        if (icon.center().y - middle).abs() > px(2.) {
                            found.push(format!(
                                "{w}x{h} {view:?} touch {touch}: the {key} icon is at {:?}, its line's middle at {middle:?}",
                                icon.center().y
                            ));
                        }
                    }
                }
                if let (Some(close), Some(title)) = (
                    vcx.debug_bounds("panel-close"),
                    vcx.debug_bounds("panel-title"),
                ) {
                    let middle = title.top() + px(22. * rem / 15. / 2.);
                    if (close.center().y - middle).abs() > px(2.) {
                        found.push(format!(
                            "{w}x{h} {view:?} touch {touch}: × at {:?}, the title's first line at {middle:?}",
                            close.center().y
                        ));
                    }
                }
            }
        }
        assert!(found.is_empty(), "{}", found.join("\n"));
    }
}

/// With the mini month beside it, the calendar's own text still fits, the month stays in the
/// window and left of the calendar, and its dots are the days the host's events fall on.
#[test]
fn the_mini_month_fits_beside_the_calendar() {
    let views = [CalendarView::Day, CalendarView::Week, CalendarView::Month];
    check_widths_with(&views, &[720., 1024., 1280., 2200.], 820., |o| {
        o.mini_month = true
    });

    let mut cx = context();
    let (calendar, vcx) = cx.add_window_view(|_, cx| {
        Calendar::new(cx)
            .with_options(|o| {
                o.time_zone = TZ;
                o.mini_month = true;
            })
            .with_now(at(day(2), 10, 40))
            .with_date(day(2))
            .with_events(worst_week())
    });
    for w in [1024., 1280., 2200.] {
        vcx.simulate_resize(size(px(w), px(820.)));
        for _ in 0..2 {
            vcx.run_until_parked();
            vcx.update(|window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            });
        }
        let mini = vcx
            .debug_bounds("calendar-mini")
            .unwrap_or_else(|| panic!("no mini month at {w}"));
        let canvas = calendar
            .read_with(vcx, |c, _| c.shared.geo.get())
            .expect("drawn")
            .bounds;
        assert!(
            mini.left() >= px(0.) && mini.bottom() <= px(820.),
            "{w}: {mini:?}"
        );
        assert!(
            mini.right() <= canvas.left() + px(0.5),
            "{w}: {mini:?} over {canvas:?}"
        );
        let title = vcx.debug_bounds("calendar-title").expect("a title");
        let controls = vcx.debug_bounds("calendar-controls").expect("controls");
        assert!(
            title.left() >= mini.right() - px(0.5),
            "{w}: title {title:?}"
        );
        assert!(
            controls.right() <= px(w) + px(0.5),
            "{w}: controls {controls:?}"
        );
    }
    // The dots: every day an event in worst_week covers, and none it doesn't.
    calendar.read_with(vcx, |c, _| {
        let busy = c.busy_days(day(2));
        assert!(busy.contains(&day(0)) && busy.contains(&day(3)) && busy.contains(&day(6)));
        // The all-day week-long event covers Sunday; the month around has filler every day,
        // so only past the filler is empty.
        assert!(!busy.contains(&(day(0) + chrono::Duration::days(60))));
    });
}
