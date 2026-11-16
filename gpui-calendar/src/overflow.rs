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
