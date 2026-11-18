//! The calendar's drags on gpui-dnd: what they gained over following the window's mouse
//! itself. A release beside the calendar commits at the clamped spot; the hours scroll on timed
//! frames while a drag holds still near (or past) the grid's edge, and nothing runs once it's
//! over; the cursor says what's dragged; two calendars in one window keep their own drags.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration as StdDuration;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use gpui::{
    AppContext as _, Context, CursorStyle, Entity, Focusable as _, IntoElement, Modifiers,
    MouseButton, ParentElement as _, Pixels, Point, Render, Styled as _, TestAppContext,
    VisualTestContext, Window, div, point, px, size,
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
    vec![Event::timed("a", "Planning", at(day(2), 11, 0), at(day(2), 12, 0)).calendar("work")]
}

fn calendar(cx: &mut Context<Calendar>) -> Calendar {
    let mut c = Calendar::new(cx)
        .with_options(|o| {
            o.time_zone = TZ;
            o.week_start = chrono::Weekday::Mon;
        })
        .with_header(false)
        .with_now(at(day(2), 10, 40))
        .with_date(day(2))
        .with_calendars(vec![CalendarSource::new("work", "Work", 0x5b8def)])
        .with_events(events());
    c.scroll_to_minute(8. * 60.);
    c
}

/// A window with a 200-wide aside at the left and one calendar beside it, or two calendars
/// side by side.
struct Host {
    left: Entity<Calendar>,
    right: Option<Entity<Calendar>>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let row = div().flex().size_full();
        match &self.right {
            Some(right) => row
                .child(div().w_1_2().h_full().child(self.left.clone()))
                .child(div().w_1_2().h_full().child(right.clone())),
            None => row
                .child(div().w(px(200.)).h_full())
                .child(div().flex_1().h_full().child(self.left.clone())),
        }
    }
}

struct Opened<'a> {
    left: Entity<Calendar>,
    right: Option<Entity<Calendar>>,
    seen: Seen,
    cx: &'a mut VisualTestContext,
}

fn open(cx: &mut TestAppContext, two: bool) -> Opened<'_> {
    cx.update(|cx| {
        gpui_core::init(cx);
        gpui_calendar::init(cx);
    });
    let (host, vcx) = cx.add_window_view(|_, cx| Host {
        left: cx.new(calendar),
        right: two.then(|| cx.new(calendar)),
    });
    vcx.simulate_resize(size(px(1400.), px(700.)));
    let (left, right) = host.read_with(vcx, |h, _| (h.left.clone(), h.right.clone()));
    let seen: Seen = Rc::default();
    vcx.update(|window, cx| {
        let s = seen.clone();
        cx.subscribe(&left, move |_, e: &CalendarEvent, _| {
            s.borrow_mut().push(e.clone())
        })
        .detach();
        if let Some(right) = &right {
            let s = seen.clone();
            cx.subscribe(right, move |_, e: &CalendarEvent, _| {
                s.borrow_mut().push(e.clone())
            })
            .detach();
        }
        left.read(cx).focus_handle(cx).focus(window, cx);
    });
    let mut o = Opened {
        left,
        right,
        seen,
        cx: vcx,
    };
    o.draw();
    o.draw();
    o
}

impl Opened<'_> {
    fn draw(&mut self) {
        self.cx.run_until_parked();
        self.cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        self.cx.run_until_parked();
    }

    fn spot(&mut self, c: &Entity<Calendar>, d: NaiveDate, h: u32, m: u32) -> Point<Pixels> {
        c.read_with(self.cx, |c, _| c.point_at(d, (h * 60 + m) as f32))
            .expect("on screen")
    }

    fn press(&mut self, from: Point<Pixels>) {
        self.cx.simulate_mouse_move(from, None, Modifiers::none());
        self.cx
            .simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
        self.draw();
    }

    fn move_to(&mut self, to: Point<Pixels>) {
        self.cx
            .simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
        self.draw();
    }

    fn up(&mut self, at: Point<Pixels>) {
        self.cx
            .simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
        self.draw();
    }

    /// Lets `n` frames of 16 ms go by, as the display would.
    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            self.cx
                .executor()
                .advance_clock(StdDuration::from_millis(16));
            self.cx.update(|window, cx| {
                window.simulate_next_frame(cx);
            });
            self.cx.run_until_parked();
        }
    }

    fn frames_wanted(&mut self) -> usize {
        self.cx.update(|window, cx| window.simulate_next_frame(cx))
    }

    fn scroll(&mut self) -> f32 {
        self.left.read_with(self.cx, |c, _| c.scroll_minute())
    }

    fn moves(&self) -> Vec<EventTime> {
        self.seen
            .borrow()
            .iter()
            .filter_map(|e| match e {
                CalendarEvent::Move { time, .. } => Some(*time),
                _ => None,
            })
            .collect()
    }
}

#[gpui::test]
fn a_release_beside_the_calendar_commits_at_the_clamped_spot(cx: &mut TestAppContext) {
    let mut o = open(cx, false);
    let left = o.left.clone();
    let from = o.spot(&left, day(2), 11, 15);
    let to = o.spot(&left, day(0), 12, 45);
    o.press(from);
    o.move_to(from + point(px(0.), px(20.)));
    // Out over the aside, level with Monday 12:45: clamped to Monday.
    let beside = point(px(100.), to.y);
    o.move_to(beside);
    assert!(
        left.read_with(o.cx, |c, _| c.is_dragging()),
        "still the calendar's drag"
    );
    o.up(beside);
    assert_eq!(
        o.moves(),
        [EventTime::timed(at(day(0), 12, 30), at(day(0), 13, 30))]
    );
    assert!(!left.read_with(o.cx, |c, _| c.is_dragging()));
}

#[gpui::test]
fn a_drag_held_near_the_bottom_scrolls_the_hours_until_it_ends(cx: &mut TestAppContext) {
    let mut o = open(cx, false);
    let left = o.left.clone();
    let from = o.spot(&left, day(2), 11, 15);
    let bottom = o.cx.update(|window, _| window.viewport_size().height);
    o.press(from);
    o.move_to(from + point(px(0.), px(20.)));
    o.move_to(point(from.x, bottom - px(4.)));
    let before = o.scroll();
    o.frames(10);
    let after = o.scroll();
    assert!(
        after > before,
        "scrolled down while held still: {before} → {after}"
    );
    o.frames(10);
    assert!(o.scroll() > after, "and goes on");

    // Let go: it stops, and nothing runs.
    let p = point(from.x, bottom - px(4.));
    o.up(p);
    let stopped = o.scroll();
    o.frames(3);
    assert_eq!(o.scroll(), stopped);
    assert_eq!(o.frames_wanted(), 0);
}

#[gpui::test]
fn a_drag_past_the_top_of_the_grid_scrolls_the_hours_up(cx: &mut TestAppContext) {
    let mut o = open(cx, false);
    let left = o.left.clone();
    left.update(o.cx, |c, cx| {
        c.scroll_to_minute(12. * 60.);
        cx.notify();
    });
    o.draw();
    let from = o.spot(&left, day(2), 13, 0);
    o.press(from);
    o.move_to(from + point(px(0.), px(20.)));
    // Above the window's top edge, past the grid's.
    o.move_to(point(from.x, px(-10.)));
    let before = o.scroll();
    o.frames(10);
    assert!(
        o.scroll() < before,
        "scrolled up: {before} → {}",
        o.scroll()
    );
    o.up(point(from.x, px(-10.)));
}

#[gpui::test]
fn the_cursor_says_what_is_dragged(cx: &mut TestAppContext) {
    let mut o = open(cx, false);
    let left = o.left.clone();
    let cursor = |o: &mut Opened| o.cx.update(|_, cx| gpui_dnd::drag_cursor(cx));

    // Moving the event.
    let from = o.spot(&left, day(2), 11, 15);
    o.press(from);
    o.move_to(from + point(px(0.), px(30.)));
    assert_eq!(cursor(&mut o), Some(CursorStyle::ClosedHand));
    o.cx.simulate_keystrokes("escape");
    o.draw();
    o.up(from);

    // Its bottom edge.
    let edge = o.spot(&left, day(2), 12, 0) - point(px(0.), px(2.));
    o.press(edge);
    o.move_to(edge + point(px(0.), px(30.)));
    assert_eq!(cursor(&mut o), Some(CursorStyle::ResizeUpDown));
    o.cx.simulate_keystrokes("escape");
    o.draw();
    o.up(edge);

    // Empty time.
    let empty = o.spot(&left, day(3), 15, 0);
    o.press(empty);
    o.move_to(empty + point(px(0.), px(30.)));
    assert_eq!(cursor(&mut o), Some(CursorStyle::Arrow));
    o.cx.simulate_keystrokes("escape");
    o.draw();
    o.up(empty);
    assert_eq!(cursor(&mut o), None);
}

#[gpui::test]
fn two_calendars_in_a_window_keep_their_own_drags(cx: &mut TestAppContext) {
    let mut o = open(cx, true);
    let (left, right) = (o.left.clone(), o.right.clone().unwrap());
    let from = o.spot(&left, day(2), 11, 15);
    let over_right = o.spot(&right, day(4), 12, 45);
    o.press(from);
    o.move_to(from + point(px(0.), px(20.)));
    o.move_to(over_right);
    assert!(left.read_with(o.cx, |c, _| c.is_dragging()));
    assert!(
        !right.read_with(o.cx, |c, _| c.is_dragging()),
        "the other calendar doesn't take it"
    );
    o.up(over_right);
    // The left calendar's move, clamped to its last day, Sunday; one move only.
    assert_eq!(
        o.moves(),
        [EventTime::timed(at(day(6), 12, 30), at(day(6), 13, 30))]
    );
}
