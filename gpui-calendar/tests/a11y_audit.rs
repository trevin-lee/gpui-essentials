//! Every control the calendar draws tells assistive technology what it is and what it's called:
//! a role (not a bare container) and a label. gpui-core's audit records what each control says
//! as it's drawn: the header's buttons, the view switcher, the mini month, the event panel, the
//! context menu, the list of what didn't fit, the repeating event's question and the find pill.
//! The canvas's own nodes are reported to the audit as they're built: its days, hours, events,
//! "+N more"s and the year's days. Anything without a role or a label fails here.
//!
//! GPUI's test windows never build the accessibility tree (no assistive technology connects to
//! them), so the controls report themselves instead.

use std::collections::BTreeSet;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use gpui::{
    Entity, Focusable as _, Modifiers, MouseButton, Role, TestAppContext, VisualTestContext, px,
    size,
};
use gpui_calendar::*;

const TZ: chrono_tz::Tz = chrono_tz::America::Los_Angeles;

/// Monday 16 November 2026, plus `n` days.
fn day(n: i64) -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 16).unwrap() + Duration::days(n)
}

fn at(d: NaiveDate, h: u32, m: u32) -> DateTime<Utc> {
    time::at_minutes(d, (h * 60 + m) as f32, TZ)
}

fn events() -> Vec<Event> {
    let mut out = vec![
        Event::timed("plan", "Planning", at(day(2), 11, 0), at(day(2), 12, 0))
            .calendar("work")
            .link(EventLink::doc("doc-plan", "Plan"))
            .url("https://example.com/plan", None)
            .conference("https://example.com/meet", None),
        Event::timed("gym@1", "Gym", at(day(1), 18, 0), at(day(1), 19, 0))
            .calendar("home")
            .series("gym"),
        Event::all_day("trip", "Trip", day(0), day(2)).calendar("home"),
        // An invitation: the panel's Yes, Maybe and No.
        Event::timed("invite", "Review", at(day(4), 9, 0), at(day(4), 10, 0))
            .calendar("work")
            .attendee(Attendee::new("You", Response::NeedsAction).me(true)),
    ];
    // A crowded hour and a crowded day: "+N" beside them, and "+N more" in the month.
    for i in 0..8 {
        out.push(
            Event::timed(
                format!("crowd{i}"),
                format!("Slot {i}"),
                at(day(3), 14, 0),
                at(day(3), 15, 0),
            )
            .calendar("work"),
        );
    }
    out
}

fn open(cx: &mut TestAppContext, width: f32) -> (Entity<Calendar>, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_core::init(cx);
        gpui_calendar::init(cx);
    });
    let (calendar, cx) = cx.add_window_view(|_, cx| {
        let mut c = Calendar::new(cx)
            .with_options(|o| {
                o.time_zone = TZ;
                o.week_start = chrono::Weekday::Mon;
                o.event_panel = true;
                o.mini_month = true;
            })
            .with_now(at(day(2), 10, 40))
            .with_date(day(2))
            .with_calendars(vec![
                CalendarSource::new("work", "Work", 0x5b8def),
                CalendarSource::new("home", "Home", 0x4cc38a),
            ])
            .with_events(events());
        c.scroll_to_minute(13. * 60.);
        c
    });
    cx.simulate_resize(size(px(width), px(820.)));
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

/// What the audit recorded since it started that has no role or no label, named for `state`;
/// the canvas's nodes are built (and so reported) as a screen reader would have them.
fn check(
    state: &str,
    c: &Entity<Calendar>,
    cx: &mut VisualTestContext,
    bad: &mut BTreeSet<String>,
) {
    draw(cx);
    let nodes = c.read_with(cx, |c, _| c.accessibility_items());
    assert!(!nodes.is_empty(), "{state}: the canvas made no nodes");
    let said = gpui_core::audit_take();
    assert!(
        said.iter().any(|a| a.what == "Calendar"),
        "{state}: the canvas didn't report itself"
    );
    for a in said {
        let no_role = matches!(a.role, Role::Unknown | Role::GenericContainer);
        let label = a.label.as_ref().map(|l| l.to_string()).unwrap_or_default();
        if no_role || label.trim().is_empty() {
            bad.insert(format!(
                "{state}: a {} with role {:?} and label {:?}",
                a.what, a.role, a.label
            ));
        }
    }
    gpui_core::audit_start();
}

fn right_click(cx: &mut VisualTestContext, p: gpui::Point<gpui::Pixels>) {
    cx.simulate_mouse_move(p, None, Modifiers::none());
    cx.simulate_mouse_down(p, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(p, MouseButton::Right, Modifiers::none());
}

#[gpui::test]
fn every_control_drawn_has_a_role_and_a_label(cx: &mut TestAppContext) {
    let mut bad = BTreeSet::new();
    gpui_core::audit_start();
    // Wide: the header's buttons, the mini month beside the days, the week's canvas.
    let (c, cx) = open(cx, 1280.);
    check("the week", &c, cx, &mut bad);

    // The event panel, with its links, address, call and calendars.
    c.update(cx, |c, cx| c.open_panel("plan".into(), cx));
    check("the event panel", &c, cx, &mut bad);
    // Its list of calendars to move the event to.
    let change = cx.debug_bounds("panel-change").expect("Change");
    cx.simulate_click(change.center(), Modifiers::none());
    check("the panel's calendars", &c, cx, &mut bad);
    c.update(cx, |c, cx| c.open_panel("invite".into(), cx));
    check("an invitation's panel", &c, cx, &mut bad);
    c.update(cx, |c, cx| c.close_panel(cx));

    // The context menu, on an event and on empty time.
    let p = c
        .read_with(cx, |c, _| c.point_at(day(2), 11. * 60. + 30.))
        .unwrap();
    right_click(cx, p);
    check("an event's menu", &c, cx, &mut bad);
    cx.simulate_keystrokes("escape");
    let p = c
        .read_with(cx, |c, _| c.point_at(day(4), 16. * 60.))
        .unwrap();
    right_click(cx, p);
    check("empty time's menu", &c, cx, &mut bad);
    cx.simulate_keystrokes("escape escape");

    // The repeating event's question.
    c.update(cx, |c, cx| {
        c.select(Some(Selection::Event("gym@1".into())), cx)
    });
    cx.simulate_keystrokes("backspace");
    check("the repeating question", &c, cx, &mut bad);
    c.update(cx, |c, cx| c.dismiss_question(cx));

    // The find pill.
    cx.update(|window, cx| c.update(cx, |c, cx| c.type_query(window, cx)));
    cx.simulate_input("plan");
    check("finding", &c, cx, &mut bad);
    cx.simulate_keystrokes("escape");
    c.update(cx, |c, cx| c.set_query("", cx));

    // The month, and its list of a crowded day.
    c.update(cx, |c, cx| c.set_view(CalendarView::Month, cx));
    check("the month", &c, cx, &mut bad);
    c.update(cx, |c, cx| {
        c.open_more(day(3), cx);
    });
    check("a day's list", &c, cx, &mut bad);
    cx.simulate_keystrokes("escape");

    // The year and the agenda.
    c.update(cx, |c, cx| c.set_view(CalendarView::Year, cx));
    check("the year", &c, cx, &mut bad);
    c.update(cx, |c, cx| c.set_view(CalendarView::AGENDA, cx));
    check("the agenda", &c, cx, &mut bad);

    // Narrow: the phone's header, its view menu, and the mini month behind the title.
    c.update(cx, |c, cx| c.set_view(CalendarView::Days(3), cx));
    cx.simulate_resize(size(px(390.), px(820.)));
    draw(cx);
    check("narrow", &c, cx, &mut bad);
    cx.update(|window, cx| c.update(cx, |c, cx| c.toggle_mini_month(window, cx)));
    check("the mini month behind the title", &c, cx, &mut bad);

    let _ = gpui_core::audit_take();
    let bad: Vec<String> = bad.into_iter().collect();
    assert!(
        bad.is_empty(),
        "{} problems:\n  {}",
        bad.len(),
        bad.join("\n  ")
    );
}

/// Every control the crate builds is labelled where it's built: a `Press` or a `Button` with
/// `.accessibility_label(..)` or `.label(..)` in its chain, or the line before it saying why not
/// (`// a11y: ..`). This catches a control in a state the test above doesn't draw.
#[test]
fn every_control_in_the_source_is_labelled() {
    let mut bad = Vec::new();
    let mut files = vec![std::path::PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src"
    ))];
    while let Some(path) = files.pop() {
        if path.is_dir() {
            files.extend(std::fs::read_dir(&path).unwrap().map(|e| e.unwrap().path()));
        } else if path.extension().is_some_and(|e| e == "rs") {
            let text = std::fs::read_to_string(&path).unwrap();
            for line in unlabelled(&text) {
                bad.push(format!("{}:{line}", path.display()));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "controls without a label (add .accessibility_label(..), or say why with // a11y:):\n  {}",
        bad.join("\n  ")
    );
}

/// The lines (from 1) where a control is made without a label in its chain: the lines from
/// its `::new(` up to the next control or the end of its statement.
fn unlabelled(text: &str) -> Vec<usize> {
    let lines: Vec<&str> = text.lines().collect();
    let starts = |l: &str| {
        (l.contains("Press::new(") && !l.contains("gpui_dnd::Press")) || l.contains("Button::new(")
    };
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if !starts(line) || line.trim_start().starts_with("//") {
            continue;
        }
        if i > 0 && lines[i - 1].contains("// a11y:") {
            continue;
        }
        let mut labelled = false;
        for (j, l) in lines[i..].iter().enumerate() {
            if j > 0 && starts(l) {
                break;
            }
            if l.contains(".accessibility_label(") || l.contains(".label(") {
                labelled = true;
                break;
            }
            if l.trim_end().ends_with(';') {
                break;
            }
        }
        if !labelled {
            out.push(i + 1);
        }
    }
    out
}

#[test]
fn the_source_check_finds_an_unlabelled_control() {
    let sample = "let a = Press::new(\"a\")\n    .on_click(f);\n\
                  let b = Press::new(\"b\")\n    .accessibility_label(\"B\")\n    .on_click(f);\n\
                  // a11y: drawn with words of its own\n\
                  let c = Press::new(\"c\");\n";
    assert_eq!(unlabelled(sample), [1]);
}

/// What a click on a day's name, a month's name or an agenda day does, the keyboard does by
/// switching views with that day selected.
#[gpui::test]
fn the_keyboard_opens_the_selected_day_or_month(cx: &mut TestAppContext) {
    let (c, cx) = open(cx, 1280.);
    let state =
        |c: &Entity<Calendar>, cx: &VisualTestContext| c.read_with(cx, |c, _| (c.view(), c.date()));
    // The week: a slot on Thursday, then `d`.
    cx.simulate_keystrokes("down right");
    cx.simulate_keystrokes("d");
    assert_eq!(state(&c, cx), (CalendarView::Day, day(3)));
    // The year: a day in March, then `m`.
    c.update(cx, |c, cx| {
        c.set_view(CalendarView::Year, cx);
        c.select(
            Some(Selection::Slot {
                date: NaiveDate::from_ymd_opt(2026, 3, 10).unwrap(),
                minute: 0,
            }),
            cx,
        );
    });
    cx.simulate_keystrokes("m");
    assert_eq!(
        state(&c, cx),
        (
            CalendarView::Month,
            NaiveDate::from_ymd_opt(2026, 3, 10).unwrap()
        )
    );
    // The agenda: an event, then `d`.
    c.update(cx, |c, cx| {
        c.select(None, cx);
        c.go_to(day(0), cx);
        c.set_view(CalendarView::AGENDA, cx);
    });
    draw(cx);
    c.update(cx, |c, cx| {
        c.select(Some(Selection::Event("invite".into())), cx)
    });
    cx.simulate_keystrokes("d");
    assert_eq!(state(&c, cx), (CalendarView::Day, day(4)));
}
