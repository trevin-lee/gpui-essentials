//! The calendar in a window, with a week of sample events, acting as a host would: it applies
//! every move, resize, new event and delete the calendar proposes, and says what happened at
//! the bottom.
//!
//! ```sh
//! cargo run --release --example calendar
//! cargo run --release --example calendar -- --bench [--events 2000] [--packed]
//! cargo run --release --features shots --example calendar -- --shot out.png [--scene week]
//! ```
//!
//! Scenes: `week`, `day`, `month`, `overlap`, `drag-create`, `drag-move`, `resize`, `dst`,
//! `dst-spring`, `dense`, `month-drag`. Pictures and the bench draw a window that's never shown
//! and never takes the keyboard.

use std::path::PathBuf;
use std::time::Instant;

use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, TimeZone, Utc, Weekday};
use chrono_tz::Tz;
use gpui_calendar::*;
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;

const TZ: Tz = chrono_tz::America::Los_Angeles;

struct Args {
    shot: Option<PathBuf>,
    scene: String,
    bench: bool,
    events: usize,
    packed: bool,
}

fn args() -> Args {
    let mut out = Args {
        shot: None,
        scene: "week".into(),
        bench: false,
        events: 2000,
        packed: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--shot" => out.shot = it.next().map(PathBuf::from),
            "--scene" => out.scene = it.next().unwrap_or_default(),
            "--bench" => out.bench = true,
            "--events" => out.events = it.next().and_then(|n| n.parse().ok()).unwrap_or(2000),
            "--packed" => out.packed = true,
            _ => {}
        }
    }
    out
}

/// Inter, in the three weights the calendar draws (examples/fonts, under the OFL). Without them
/// GPUI falls back to the system font; the calendar takes its font from the theme either way.
fn load_fonts(cx: &mut App) {
    let _ = cx.text_system().add_fonts(vec![
        std::borrow::Cow::Borrowed(include_bytes!("fonts/Inter-Regular.ttf").as_slice()),
        std::borrow::Cow::Borrowed(include_bytes!("fonts/Inter-Medium.ttf").as_slice()),
        std::borrow::Cow::Borrowed(include_bytes!("fonts/Inter-SemiBold.ttf").as_slice()),
    ]);
}

/// A charcoal dark theme for GPUI Kit, the one the pictures are drawn in.
fn apply_theme(cx: &mut App) {
    Theme::change(ThemeMode::Dark, None, cx);
    let t = Theme::global_mut(cx);
    t.font_family = "Inter".into();
    t.font_size = px(15.);
    t.radius = px(8.);
    t.radius_lg = px(10.);
    let c = &mut t.colors;
    let hover: Hsla = rgba(0xededef0d).into();
    c.background = rgb(0x0f0f11).into();
    c.foreground = rgb(0xededef).into();
    c.border = rgb(0x28282d).into();
    c.input = rgb(0x3a3a41).into();
    c.muted = rgb(0x1f1f23).into();
    c.muted_foreground = rgb(0x8b8b93).into();
    c.secondary = rgb(0x1f1f23).into();
    c.secondary_foreground = rgb(0xededef).into();
    c.primary = rgb(0xededef).into();
    c.primary_foreground = rgb(0x0f0f11).into();
    c.popover = rgb(0x18181b).into();
    c.popover_foreground = rgb(0xededef).into();
    c.accent = hover;
    c.accent_foreground = rgb(0xededef).into();
    c.list_hover = hover;
    c.list_active = hover;
    c.ring = rgb(0xededef).into();
    c.danger = rgb(0xe5665b).into();
}

/// The moment pictures pretend it is: Wednesday 18 November 2026, 10:40 in Los Angeles.
fn picture_now() -> DateTime<Utc> {
    TZ.with_ymd_and_hms(2026, 11, 18, 10, 40, 0)
        .unwrap()
        .with_timezone(&Utc)
}

fn local(date: NaiveDate, h: u32, m: u32) -> DateTime<Utc> {
    time::at_minutes(date, (h * 60 + m) as f32, TZ)
}

fn hm(h: u32, m: u32) -> (u32, u32) {
    (h, m)
}

/// A curated week in a life with work, school, health and agents that schedule things.
fn sample_week(monday: NaiveDate) -> Vec<Event> {
    let day = |n: i64| monday + Duration::days(n);
    let mut out = Vec::new();
    let mut add = |id: &str, title: &str, d: i64, from: (u32, u32), to: (u32, u32), cal: &str| {
        let date = day(d);
        let end_date = if to.0 * 60 + to.1 <= from.0 * 60 + from.1 {
            date + Duration::days(1)
        } else {
            date
        };
        out.push(
            Event::timed(
                id.to_string(),
                title.to_string(),
                local(date, from.0, from.1),
                local(end_date, to.0, to.1),
            )
            .calendar(cal.to_string()),
        );
    };
    for d in 0..5 {
        add(
            &format!("standup-{d}"),
            "Standup",
            d,
            hm(9, 0),
            hm(9, 15),
            "work",
        );
    }
    add("design", "Design review", 0, hm(10, 0), hm(11, 30), "work");
    add(
        "lunch-maya",
        "Lunch with Maya",
        0,
        hm(12, 0),
        hm(13, 0),
        "personal",
    );
    add(
        "lecture-1",
        "CS 162 lecture",
        0,
        hm(14, 0),
        hm(15, 0),
        "school",
    );
    add("deep-1", "Deep work", 0, hm(15, 0), hm(16, 30), "work");
    add("gym-1", "Gym", 0, hm(18, 0), hm(19, 0), "health");

    add(
        "one-on-one",
        "1:1 with Priya",
        1,
        hm(9, 30),
        hm(10, 30),
        "work",
    );
    add(
        "interview",
        "Interview loop",
        1,
        hm(10, 0),
        hm(11, 0),
        "work",
    );
    add(
        "dentist",
        "Dentist call",
        1,
        hm(10, 15),
        hm(10, 45),
        "health",
    );
    add(
        "section",
        "CS 162 section",
        1,
        hm(13, 0),
        hm(14, 30),
        "school",
    );
    add(
        "lease",
        "Main: review the lease draft",
        1,
        hm(16, 0),
        hm(17, 0),
        "agents",
    );
    add(
        "dinner",
        "Dinner at Kin Khao",
        1,
        hm(19, 30),
        hm(21, 0),
        "personal",
    );

    add("run", "Morning run", 2, hm(7, 30), hm(8, 30), "health");
    add("planning", "Planning", 2, hm(11, 0), hm(12, 0), "work");
    add(
        "statement",
        "Finances: card statement is ready",
        2,
        hm(11, 30),
        hm(12, 30),
        "agents",
    );
    add(
        "coffee",
        "Coffee chat",
        2,
        hm(13, 0),
        hm(13, 30),
        "personal",
    );
    add(
        "office-hours",
        "Office hours",
        2,
        hm(14, 0),
        hm(17, 0),
        "school",
    );
    add("alex", "Sync with Alex", 2, hm(15, 0), hm(16, 0), "work");

    add(
        "workshop",
        "Workshop: GPUI internals",
        3,
        hm(10, 0),
        hm(12, 0),
        "work",
    );
    add("lunch-2", "Lunch", 3, hm(12, 0), hm(13, 0), "personal");
    add(
        "lecture-2",
        "CS 162 lecture",
        3,
        hm(14, 0),
        hm(15, 0),
        "school",
    );
    add("physio", "Physio", 3, hm(16, 30), hm(17, 15), "health");
    add(
        "flight",
        "Red-eye to New York",
        3,
        hm(22, 0),
        hm(1, 30),
        "personal",
    );

    add("retro", "Retro", 4, hm(9, 30), hm(10, 30), "work");
    add(
        "pset",
        "School: problem set 6 due",
        4,
        hm(11, 0),
        hm(11, 30),
        "school",
    );
    add(
        "dinner-2",
        "Dinner with Sam",
        4,
        hm(19, 0),
        hm(21, 0),
        "personal",
    );

    add(
        "market",
        "Farmers market",
        5,
        hm(10, 0),
        hm(11, 30),
        "personal",
    );
    add("climb", "Climbing", 5, hm(14, 0), hm(16, 0), "health");
    add(
        "brunch",
        "Brunch with family",
        6,
        hm(11, 0),
        hm(12, 30),
        "personal",
    );
    add(
        "plan",
        "Main: plan the week",
        6,
        hm(19, 0),
        hm(19, 45),
        "agents",
    );

    // Somebody else's event: it can be opened but not moved.
    out.push(
        Event::timed(
            "all-hands",
            "Company all-hands",
            local(day(4), 16, 0),
            local(day(4), 17, 0),
        )
        .calendar("work")
        .location("Zoom")
        .read_only(true),
    );
    out.iter_mut().find(|e| e.id == "design").unwrap().location = Some("Room 4".into());

    out.push(Event::all_day("rustconf", "RustConf", day(0), day(3)).calendar("work"));
    out.push(
        Event::all_day("passport", "Passport renewal window", day(1), day(2)).calendar("agents"),
    );
    out.push(Event::all_day("trip", "New York", day(4), day(7)).calendar("personal"));
    out.push(Event::all_day("birthday", "Mom's birthday", day(4), day(5)).calendar("personal"));
    out.push(Event::all_day("rent", "Rent due", day(2), day(3)).calendar("finances"));
    out.push(Event::all_day("move-out", "Lab move-out", day(2), day(4)).calendar("school"));
    out
}

/// A small deterministic random number generator, so pictures and the bench repeat exactly.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) as u32
    }

    fn below(&mut self, n: u32) -> u32 {
        self.next() % n.max(1)
    }
}

const CALENDARS: [&str; 6] = ["work", "personal", "school", "health", "agents", "finances"];
const TITLES: [&str; 16] = [
    "Standup",
    "Review",
    "1:1",
    "Lecture",
    "Gym",
    "Call with Jordan",
    "Focus time",
    "Lunch",
    "Reading group",
    "Main: follow up",
    "Interview",
    "Office hours",
    "Errands",
    "Board meeting",
    "Lab",
    "Sync",
];

/// Ordinary days around the curated week: a few events each, and the odd all-day one.
fn filler(monday: NaiveDate, weeks: i64, per_day: u32, seed: u64) -> Vec<Event> {
    let mut rng = Lcg(seed);
    let mut out = Vec::new();
    for w in -weeks..=weeks {
        if w == 0 {
            continue;
        }
        for d in 0..7 {
            let date = monday + Duration::days(w * 7 + d);
            let n = if d >= 5 { per_day / 2 } else { per_day };
            for k in 0..n {
                let start = 7 * 60 + rng.below(13 * 4) * 15;
                let length = [30, 45, 60, 60, 90, 120][rng.below(6) as usize];
                let id = format!("f{w}-{d}-{k}");
                out.push(
                    Event::timed(
                        id,
                        TITLES[rng.below(TITLES.len() as u32) as usize],
                        time::at_minutes(date, start as f32, TZ),
                        time::at_minutes(date, (start + length) as f32, TZ),
                    )
                    .calendar(CALENDARS[rng.below(6) as usize]),
                );
            }
            if rng.below(9) == 0 {
                let days = 1 + rng.below(3) as i64;
                out.push(
                    Event::all_day(
                        format!("a{w}-{d}"),
                        ["Conference", "Trip", "Holiday", "Deadline"][rng.below(4) as usize],
                        date,
                        date + Duration::days(days),
                    )
                    .calendar(CALENDARS[rng.below(6) as usize]),
                );
            }
        }
    }
    out
}

/// A busy week: many short events piled into working hours.
fn dense_week(monday: NaiveDate, count: usize, seed: u64) -> Vec<Event> {
    let mut rng = Lcg(seed);
    (0..count)
        .map(|i| {
            let date = monday + Duration::days(rng.below(7) as i64);
            let start = 8 * 60 + rng.below(10 * 4) * 15;
            let length = [15, 30, 30, 45, 60, 60, 90, 120][rng.below(8) as usize];
            Event::timed(
                format!("d{i}"),
                TITLES[rng.below(TITLES.len() as u32) as usize],
                time::at_minutes(date, start as f32, TZ),
                time::at_minutes(date, (start + length) as f32, TZ),
            )
            .calendar(CALENDARS[rng.below(6) as usize])
        })
        .collect()
}

/// About `count` events over a year around the week: the measurement's data.
fn year_of_events(monday: NaiveDate, count: usize) -> Vec<Event> {
    let mut events = sample_week(monday);
    let per_day = ((count.saturating_sub(events.len())) as f32 / (52. * 6.)).ceil() as u32;
    events.extend(filler(monday, 26, per_day.max(1), 7));
    events.truncate(count.max(events.len().min(count)));
    events
}

/// Events around the 2026 autumn change in Los Angeles, when 1 AM happens twice.
fn dst_events() -> Vec<Event> {
    let sun = NaiveDate::from_ymd_opt(2026, 11, 1).unwrap();
    let pdt = chrono::FixedOffset::west_opt(7 * 3600).unwrap();
    let pst = chrono::FixedOffset::west_opt(8 * 3600).unwrap();
    let at = |o: chrono::FixedOffset, h, m| {
        o.from_local_datetime(&sun.and_hms_opt(h, m, 0).unwrap())
            .unwrap()
            .with_timezone(&Utc)
    };
    let sat = sun - Duration::days(1);
    vec![
        Event::timed("late-show", "Late show", local(sat, 23, 0), at(pdt, 1, 0))
            .calendar("personal"),
        Event::timed(
            "first",
            "Backup (first 1:30, PDT)",
            at(pdt, 1, 30),
            at(pdt, 1, 55),
        )
        .calendar("agents"),
        Event::timed(
            "second",
            "Backup again (second 1:30, PST)",
            at(pst, 1, 30),
            at(pst, 1, 55),
        )
        .calendar("work"),
        Event::timed(
            "long",
            "Overnight build, 4 real hours",
            at(pdt, 0, 30),
            at(pst, 3, 30),
        )
        .calendar("school"),
        Event::timed("brunch", "Brunch", local(sun, 11, 0), local(sun, 12, 30))
            .calendar("personal"),
        Event::timed(
            "mon",
            "Standup",
            local(sun + Duration::days(1), 9, 0),
            local(sun + Duration::days(1), 9, 15),
        )
        .calendar("work"),
    ]
}

/// Events around the 2027 spring change, when 2 AM is skipped.
fn dst_spring_events() -> Vec<Event> {
    let sun = NaiveDate::from_ymd_opt(2027, 3, 14).unwrap();
    vec![
        Event::timed(
            "night",
            "Overnight build, 2 real hours",
            local(sun, 1, 0),
            local(sun, 4, 0),
        )
        .calendar("school"),
        Event::timed("brunch", "Brunch", local(sun, 11, 0), local(sun, 12, 30))
            .calendar("personal"),
        Event::timed(
            "sat",
            "Party",
            local(sun - Duration::days(1), 21, 0),
            local(sun, 1, 30),
        )
        .calendar("personal"),
    ]
}

struct Root {
    calendar: Entity<Calendar>,
    status: SharedString,
    added: usize,
    _subscription: Subscription,
}

impl Root {
    fn new(args: &Args, cx: &mut Context<Self>) -> Self {
        let pictures = args.shot.is_some() || args.bench;
        let now = if pictures { picture_now() } else { Utc::now() };
        let today = now.with_timezone(&TZ).date_naive();
        let monday = time::week_start(today, Weekday::Mon);
        let mut events = if args.bench {
            if args.packed {
                dense_week(monday, args.events, 11)
            } else {
                year_of_events(monday, args.events)
            }
        } else {
            let mut e = sample_week(monday);
            e.extend(filler(monday, 6, 3, 3));
            e
        };
        let mut view = CalendarView::Week;
        let mut date = today;
        let mut week_start = Weekday::Mon;
        let mut now_at = now;
        match args.scene.as_str() {
            "day" => view = CalendarView::Day,
            "month" | "month-drag" => view = CalendarView::Month,
            "overlap" => {
                view = CalendarView::Days(3);
                date = monday + Duration::days(1);
                let thu = monday + Duration::days(3);
                let mut more = |id: &str, title: &str, from: u32, to: u32, cal: &str| {
                    events.push(
                        Event::timed(
                            id.to_string(),
                            title.to_string(),
                            local(thu, from / 60, from % 60),
                            local(thu, to / 60, to % 60),
                        )
                        .calendar(cal.to_string()),
                    )
                };
                more("judging", "Hackathon judging", 13 * 60, 16 * 60, "school");
                more("quick", "Quick sync", 13 * 60 + 30, 14 * 60, "work");
                more("vendor", "Vendor call", 14 * 60 + 30, 15 * 60 + 30, "work");
                more(
                    "checkin",
                    "Check-in with Main",
                    14 * 60 + 45,
                    15 * 60 + 15,
                    "agents",
                );
                more(
                    "pickup",
                    "Pick up parcel",
                    15 * 60 + 45,
                    16 * 60 + 30,
                    "personal",
                );
            }
            "dense" if !args.bench => events = dense_week(monday, 160, 5),
            "dst" => {
                events = dst_events();
                view = CalendarView::Days(3);
                date = NaiveDate::from_ymd_opt(2026, 10, 31).unwrap();
                week_start = Weekday::Sun;
                now_at = local(NaiveDate::from_ymd_opt(2026, 11, 3).unwrap(), 9, 20);
            }
            "dst-spring" => {
                events = dst_spring_events();
                view = CalendarView::Days(3);
                date = NaiveDate::from_ymd_opt(2027, 3, 13).unwrap();
                week_start = Weekday::Sun;
                now_at = local(NaiveDate::from_ymd_opt(2027, 3, 16).unwrap(), 9, 20);
            }
            _ => {}
        }
        let scene = args.scene.clone();
        let calendar = cx.new(|cx| {
            let mut c = Calendar::new(cx)
                .with_options(|o| {
                    o.time_zone = TZ;
                    o.week_start = week_start;
                })
                .with_calendar_color("work", 0x5b8def)
                .with_calendar_color("personal", 0x4cc38a)
                .with_calendar_color("school", 0x9b7cf0)
                .with_calendar_color("health", 0x3fb6c4)
                .with_calendar_color("agents", 0xe2a336)
                .with_calendar_color("finances", 0xe06fa8)
                .with_events(events)
                .with_view(view)
                .with_date(date);
            if pictures {
                c = c.with_now(now_at);
            }
            if scene.starts_with("dst") {
                c.scroll_to_minute(-1.);
            }
            if scene == "overlap" {
                c.scroll_to_minute(8. * 60. + 30.);
            }
            c
        });
        let subscription = cx.subscribe(&calendar, |this, calendar, event: &CalendarEvent, cx| {
            this.host(calendar, event, cx);
        });
        Self {
            calendar,
            status: "Drag empty time to add an event; drag one to move it; t d w m j k".into(),
            added: 0,
            _subscription: subscription,
        }
    }

    /// What a host does: agree to everything, and say so.
    fn host(&mut self, calendar: Entity<Calendar>, event: &CalendarEvent, cx: &mut Context<Self>) {
        let words = |t: &EventTime| match *t {
            EventTime::Timed { start, end } => {
                let s = start.with_timezone(&TZ);
                format!(
                    "{} {}",
                    time::weekday_name(s.weekday(), true),
                    time::format_range(s.time(), end.with_timezone(&TZ).time(), true)
                )
            }
            EventTime::AllDay { start, end } => {
                format!(
                    "{} for {} days",
                    start.format("%b %-d"),
                    (end - start).num_days()
                )
            }
        };
        let title = |id: &str, cx: &App| {
            calendar
                .read(cx)
                .event(id)
                .map(|e| e.title.to_string())
                .unwrap_or_default()
        };
        self.status = match event {
            CalendarEvent::RangeChange { .. } => return,
            CalendarEvent::Select(Some(id)) => format!("Selected {}", title(id, cx)),
            CalendarEvent::Select(None) => "Nothing selected".into(),
            CalendarEvent::Open(id) => format!("Open {}", title(id, cx)),
            CalendarEvent::Create(time) => {
                self.added += 1;
                let id = format!("new-{}", self.added);
                let said = format!("Added an event, {}", words(time));
                calendar.update(cx, |c, cx| {
                    c.upsert_event(Event::new(id, "New event", *time).calendar("work"), cx)
                });
                said
            }
            CalendarEvent::Move { id, time } | CalendarEvent::Resize { id, time } => {
                let said = format!("{} → {}", title(id, cx), words(time));
                calendar.update(cx, |c, cx| {
                    if let Some(mut e) = c.event(id).cloned() {
                        e.time = *time;
                        c.upsert_event(e, cx);
                    }
                });
                said
            }
            CalendarEvent::Delete(id) => {
                let said = format!("Deleted {}", title(id, cx));
                calendar.update(cx, |c, cx| c.remove_event(id, cx));
                said
            }
        }
        .into();
        cx.notify();
    }
}

impl Render for Root {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let c = Theme::global(cx).colors;
        div()
            .flex()
            .flex_col()
            .size_full()
            .font_family("Inter")
            .bg(c.background)
            .child(div().flex_1().min_h_0().child(self.calendar.clone()))
            .child(
                div()
                    .flex_none()
                    .px(px(16.))
                    .py(px(7.))
                    .border_t_1()
                    .border_color(c.border)
                    .text_size(px(12.))
                    .text_color(c.muted_foreground)
                    .child(self.status.clone()),
            )
    }
}

actions!(calendar_example, [Quit]);

fn main() {
    let args = args();
    // GPUI Kit's icons (the header's chevrons) come from the host's assets.
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets::new(""))
        .run(move |cx| {
            load_fonts(cx);
            gpui_kit::init(cx);
            gpui_calendar::init(cx);
            apply_theme(cx);
            cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
            cx.on_action(|_: &Quit, cx| cx.quit());
            let offscreen = args.shot.is_some() || args.bench;
            let bounds = Bounds::centered(None, size(px(1280.), px(820.)), cx);
            let options = WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some("Calendar".into()),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                // Pictures and the bench never show the window or take the keyboard.
                focus: !offscreen,
                show: !offscreen,
                ..Default::default()
            };
            let (window, root) = gpui_kit::open_window(options, cx, {
                let args = &args;
                move |_, cx| cx.new(|cx| Root::new(args, cx))
            })
            .expect("the window");
            if !offscreen {
                cx.activate(true);
                let calendar = root.read(cx).calendar.clone();
                let _ = cx.update_window(window, |_, window, cx| {
                    calendar.focus_handle(cx).focus(window, cx)
                });
            }
            if args.bench {
                bench::run(window, root.clone(), args.packed, cx);
            }
            #[cfg(feature = "shots")]
            if let Some(path) = args.shot.clone() {
                shots::shot(window, root.clone(), path, args.scene.clone(), cx);
            }
            let _ = (window, root, &args.scene);
        });
}

/// Draws the window now, as many times as asked. A window that isn't shown gets no frames from
/// the display, so pictures and the bench draw it themselves.
fn draw(window: AnyWindowHandle, cx: &mut AsyncApp) -> f32 {
    let mut ms = 0.;
    let _ = cx.update_window(window, |_, window, cx| {
        let t = Instant::now();
        window.refresh();
        window.draw(cx).clear(cx);
        ms = t.elapsed().as_secs_f32() * 1000.;
    });
    ms
}

async fn wait(cx: &mut AsyncApp, ms: u64) {
    cx.background_executor()
        .timer(std::time::Duration::from_millis(ms))
        .await;
}

fn send(window: AnyWindowHandle, cx: &mut AsyncApp, event: PlatformInput) {
    let _ = cx.update_window(window, |_, window, cx| {
        window.dispatch_event(event, cx);
    });
}

fn press(window: AnyWindowHandle, cx: &mut AsyncApp, at: Point<Pixels>) {
    send(
        window,
        cx,
        PlatformInput::MouseMove(MouseMoveEvent {
            position: at,
            pressed_button: None,
            modifiers: Modifiers::none(),
        }),
    );
    send(
        window,
        cx,
        PlatformInput::MouseDown(MouseDownEvent {
            button: MouseButton::Left,
            position: at,
            modifiers: Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        }),
    );
}

fn drag_to(window: AnyWindowHandle, cx: &mut AsyncApp, at: Point<Pixels>) {
    send(
        window,
        cx,
        PlatformInput::MouseMove(MouseMoveEvent {
            position: at,
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::none(),
        }),
    );
}

fn release(window: AnyWindowHandle, cx: &mut AsyncApp, at: Point<Pixels>) {
    send(
        window,
        cx,
        PlatformInput::MouseUp(MouseUpEvent {
            button: MouseButton::Left,
            position: at,
            modifiers: Modifiers::none(),
            click_count: 1,
        }),
    );
}

mod bench {
    use super::*;

    fn report(name: &str, values: &[f32]) {
        let mut v = values.to_vec();
        if v.is_empty() {
            return;
        }
        v.sort_by(f32::total_cmp);
        let at = |q: f32| v[((v.len() - 1) as f32 * q) as usize];
        println!(
            "{name:<9} median {:5.2} ms  p95 {:5.2} ms  max {:5.2} ms  ({} frames)",
            at(0.5),
            at(0.95),
            at(1.),
            v.len()
        );
    }

    /// Frame times while the grid scrolls, while an event is dragged across the week, while
    /// the period changes every frame (each a fresh layout), and in the month view.
    pub fn run(window: AnyWindowHandle, root: Entity<Root>, packed: bool, cx: &mut App) {
        cx.spawn(async move |cx| {
            wait(cx, 300).await;
            for _ in 0..20 {
                draw(window, cx);
            }
            let calendar = root.read_with(cx, |r, _| r.calendar.clone());
            let count = calendar.read_with(cx, |c, _| c.events().len());
            let (start, end) = calendar.read_with(cx, |c, _| c.visible_range());
            let on_screen = calendar.read_with(cx, |c, _| c.events_in(start, end).len());
            println!(
                "{count} events{}, {on_screen} in the week on screen; CPU time to lay out and paint a frame",
                if packed { " packed into one week" } else { " over a year" }
            );
            const FRAMES: usize = 300;

            let mut scroll = Vec::new();
            for i in 0..FRAMES {
                let minute = 480. + 300. * (i as f32 / 25.).sin();
                let _ = cx.update_window(window, |_, _, cx| {
                    calendar.update(cx, |c, cx| {
                        c.scroll_to_minute(minute);
                        cx.notify();
                    })
                });
                scroll.push(draw(window, cx));
            }
            report("scroll", &scroll);

            // Drag the design review around the week.
            let id = if packed { "d0" } else { "design" };
            calendar.update(cx, |c, cx| {
                c.scroll_to_minute(8. * 60.);
                cx.notify();
            });
            draw(window, cx);
            let from = calendar
                .read_with(cx, |c, _| c.event_bounds(id))
                .map(|b| b.center());
            let mut drag = Vec::new();
            if let Some(from) = from {
                press(window, cx, from);
                draw(window, cx);
                for i in 0..FRAMES {
                    let t = i as f32 / 30.;
                    let to = point(
                        from.x + px(420. * (0.5 + 0.5 * t.sin())),
                        from.y + px(160. * (t * 1.7).cos()),
                    );
                    let started = Instant::now();
                    drag_to(window, cx, to);
                    let dispatch = started.elapsed().as_secs_f32() * 1000.;
                    drag.push(dispatch + draw(window, cx));
                }
                // Measured only if it really was a drag, with a ghost to draw.
                if !calendar.read_with(cx, |c, _| c.is_dragging()) {
                    drag.clear();
                    println!("the drag didn't start");
                }
                let _ = cx.update_window(window, |_, window, cx| {
                    window.dispatch_action(Box::new(Cancel), cx)
                });
                release(window, cx, from);
            }
            report("drag", &drag);

            let mut navigate = Vec::new();
            for i in 0..FRAMES / 2 {
                let _ = cx.update_window(window, |_, _, cx| {
                    calendar.update(cx, |c, cx| {
                        if (i / 20) % 2 == 0 { c.next(cx) } else { c.previous(cx) }
                    })
                });
                navigate.push(draw(window, cx));
            }
            report("navigate", &navigate);

            calendar.update(cx, |c, cx| c.set_view(CalendarView::Month, cx));
            let mut month = Vec::new();
            for i in 0..FRAMES / 2 {
                let _ = cx.update_window(window, |_, _, cx| {
                    calendar.update(cx, |c, cx| {
                        if i % 10 == 0 {
                            if (i / 40) % 2 == 0 { c.next(cx) } else { c.previous(cx) }
                        } else {
                            cx.notify()
                        }
                    })
                });
                month.push(draw(window, cx));
            }
            report("month", &month);
            cx.update(|cx| cx.quit());
        })
        .detach();
    }
}

#[cfg(feature = "shots")]
mod shots {
    use super::*;

    async fn settle(window: AnyWindowHandle, cx: &mut AsyncApp) {
        for _ in 0..3 {
            draw(window, cx);
            wait(cx, 60).await;
        }
    }

    /// A picture of the window, rendered offscreen, then quits.
    pub fn shot(
        window: AnyWindowHandle,
        root: Entity<Root>,
        path: PathBuf,
        scene: String,
        cx: &mut App,
    ) {
        cx.spawn(async move |cx| {
            wait(cx, 400).await;
            settle(window, cx).await;
            let calendar = root.read_with(cx, |r, _| r.calendar.clone());
            let today = calendar.read_with(cx, |c, _| c.today());
            let monday = time::week_start(today, Weekday::Mon);
            let point_at = |cx: &mut AsyncApp, d: i64, h: u32, m: u32| {
                calendar
                    .read_with(cx, |c, _| {
                        c.point_at(monday + Duration::days(d), (h * 60 + m) as f32)
                    })
                    .expect("on screen")
            };
            match scene.as_str() {
                // Empty time on Monday afternoon dragged out.
                "drag-create" => {
                    let a = point_at(cx, 0, 16, 35);
                    let b = point_at(cx, 0, 17, 50);
                    press(window, cx, a);
                    settle(window, cx).await;
                    drag_to(window, cx, point(a.x, a.y + px(20.)));
                    drag_to(window, cx, b);
                }
                // The design review, picked up and carried to Tuesday afternoon.
                "drag-move" => {
                    let from = calendar
                        .read_with(cx, |c, _| c.event_bounds("design"))
                        .expect("drawn");
                    let grab = point(from.center().x, from.top() + px(14.));
                    press(window, cx, grab);
                    settle(window, cx).await;
                    let to = point_at(cx, 1, 15, 0);
                    drag_to(window, cx, point(grab.x + px(10.), grab.y + px(10.)));
                    drag_to(window, cx, point(to.x, to.y + px(10.)));
                }
                // Planning's end dragged from 12:00 to 1:15.
                "resize" => {
                    let b = calendar
                        .read_with(cx, |c, _| c.event_bounds("planning"))
                        .expect("drawn");
                    let grab = point(b.left() + px(20.), b.bottom() - px(2.));
                    press(window, cx, grab);
                    settle(window, cx).await;
                    let to = point_at(cx, 2, 13, 15);
                    drag_to(window, cx, point(grab.x, grab.y + px(8.)));
                    drag_to(window, cx, point(grab.x, to.y));
                }
                // An all-day trip dragged to the next week in the month.
                "month-drag" => {
                    let from = calendar
                        .read_with(cx, |c, _| c.event_bounds("trip"))
                        .expect("drawn");
                    let grab = point(from.left() + px(30.), from.center().y);
                    press(window, cx, grab);
                    settle(window, cx).await;
                    drag_to(window, cx, point(grab.x + px(8.), grab.y));
                    let to = calendar
                        .read_with(cx, |c, _| c.point_at(monday + Duration::days(11), 0.))
                        .expect("on screen");
                    drag_to(window, cx, point(grab.x + (to.x - grab.x) * 0.0, to.y));
                }
                "week" => {
                    let _ = cx.update_window(window, |_, _, cx| {
                        calendar.update(cx, |c, cx| {
                            c.select(Some(Selection::Event("statement".into())), cx)
                        })
                    });
                }
                _ => {}
            }
            settle(window, cx).await;
            let _ = cx.update_window(window, |_, window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
                match window.render_to_image() {
                    Ok(img) => {
                        if let Err(e) = img.save(&path) {
                            eprintln!("the picture wasn't saved: {e}");
                        }
                    }
                    Err(e) => eprintln!("the window didn't render to a picture: {e}"),
                }
                cx.quit();
            });
        })
        .detach();
    }
}

/// Every name a host is likely to write resolves with both crates imported whole.
#[allow(dead_code)]
fn names_dont_clash(
    _: (
        Calendar,
        CalendarEvent,
        Event,
        EventTime,
        EventId,
        CalendarView,
        CalendarOptions,
        CalendarColors,
        Selection,
        Area,
    ),
    _: NaiveTime,
) {
}
