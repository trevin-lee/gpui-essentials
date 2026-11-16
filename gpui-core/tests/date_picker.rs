//! DatePicker: Enter or a press opens the month, the arrows move through it, Enter picks; a
//! range takes two picks in either order; Escape and a press outside close it.

use std::cell::RefCell;
use std::rc::Rc;

use chrono::{Duration, Local, NaiveDate};
use gpui::{
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, KeyDownEvent,
    KeyUpEvent, Keystroke, Modifiers, ParentElement as _, Render, Styled as _, TestAppContext,
    VisualTestContext, Window, div, point, px,
};
use gpui_core::{DatePicker, DatePickerEvent, DatePickerState, Root};

struct Form {
    state: Entity<DatePickerState>,
}

impl Render for Form {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().p(px(20.)).child(
            div()
                .w(px(240.))
                .debug_selector(|| "picker".into())
                .child(DatePicker::new(&self.state)),
        )
    }
}

type Events = Rc<RefCell<Vec<DatePickerEvent>>>;

fn open(
    cx: &mut TestAppContext,
    range: bool,
) -> (Entity<DatePickerState>, Events, &mut VisualTestContext) {
    cx.update(gpui_core::init);
    let events: Events = Rc::default();
    let e = events.clone();
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let state = cx.new(|cx| {
            if range {
                DatePickerState::range(window, cx)
            } else {
                DatePickerState::new(window, cx)
            }
        });
        cx.subscribe(&state, move |_, _, ev: &DatePickerEvent, _| {
            e.borrow_mut().push(*ev)
        })
        .detach();
        *m.borrow_mut() = Some(state.clone());
        let form = cx.new(|_| Form { state });
        Root::new(form, window, cx)
    });
    vcx.run_until_parked();
    let state = made.borrow().clone().unwrap();
    (state, events, vcx)
}

/// Keys pressed and let go (a keyboard press needs both, as on screen).
fn keys(vcx: &mut VisualTestContext, keys: &'static str) {
    for k in keys.split(' ') {
        let keystroke = Keystroke::parse(k).unwrap();
        vcx.simulate_event(KeyDownEvent {
            keystroke: keystroke.clone(),
            is_held: false,
            prefer_character_input: false,
        });
        vcx.simulate_event(KeyUpEvent { keystroke });
    }
    vcx.run_until_parked();
}

fn today() -> NaiveDate {
    Local::now().date_naive()
}

#[gpui::test]
fn the_keyboard_opens_moves_and_picks(cx: &mut TestAppContext) {
    let (state, events, vcx) = open(cx, false);
    keys(vcx, "tab enter right right down enter");
    let want = today() + Duration::days(9);
    assert_eq!(state.read_with(vcx, |s, _| s.value().start()), Some(want));
    assert!(
        matches!(events.borrow().last(), Some(DatePickerEvent::Change(v)) if v.start() == Some(want))
    );
    // Picking closed it: Enter now opens it again rather than picking.
    keys(vcx, "enter left enter");
    assert_eq!(
        state.read_with(vcx, |s, _| s.value().start()),
        Some(want - Duration::days(1))
    );
}

#[gpui::test]
fn a_range_takes_two_picks_in_either_order(cx: &mut TestAppContext) {
    let (state, _, vcx) = open(cx, true);
    keys(vcx, "tab enter enter");
    assert_eq!(
        state.read_with(vcx, |s, _| (s.value().start(), s.value().end())),
        (Some(today()), None)
    );
    // The second pick is before the first: they swap.
    keys(vcx, "up enter");
    assert_eq!(
        state.read_with(vcx, |s, _| (s.value().start(), s.value().end())),
        (Some(today() - Duration::days(7)), Some(today()))
    );
}

#[gpui::test]
fn escape_and_a_press_outside_close_it(cx: &mut TestAppContext) {
    let (state, events, vcx) = open(cx, false);
    keys(vcx, "tab enter escape enter");
    // Escape closed it, so that Enter opened it again; nothing was picked.
    assert!(events.borrow().is_empty());
    let b = vcx.debug_bounds("picker").unwrap();
    vcx.simulate_click(point(b.right() + px(300.), b.top()), Modifiers::none());
    keys(vcx, "right enter");
    // Closed by the press outside: right does nothing, Enter opens.
    assert!(events.borrow().is_empty());
    let _ = state;
}

struct Side {
    state: Entity<DatePickerState>,
}

impl Render for Side {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(gpui_core::MonthGrid::new(&self.state))
    }
}

#[gpui::test]
fn a_month_grid_in_place_moves_and_picks_from_the_keyboard(cx: &mut TestAppContext) {
    cx.update(gpui_core::init);
    let events: Events = Rc::default();
    let e = events.clone();
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let state = cx.new(|cx| DatePickerState::new(window, cx));
        cx.subscribe(&state, move |_, _, ev: &DatePickerEvent, _| {
            e.borrow_mut().push(*ev)
        })
        .detach();
        *m.borrow_mut() = Some(state.clone());
        let side = cx.new(|_| Side { state });
        Root::new(side, window, cx)
    });
    vcx.run_until_parked();
    let state = made.borrow().clone().unwrap();
    keys(vcx, "tab right right enter");
    assert_eq!(
        state.read_with(vcx, |s, _| s.value().start()),
        Some(today() + Duration::days(2))
    );
    keys(vcx, "pagedown enter");
    let next = state.read_with(vcx, |s, _| s.value().start()).unwrap();
    assert!(next > today() + Duration::days(27), "{next}");
    assert_eq!(events.borrow().len(), 2);
}

struct Nav {
    state: Entity<DatePickerState>,
    touch: bool,
}

impl Render for Nav {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let marked = NaiveDate::from_ymd_opt(2026, 3, 10).unwrap();
        let grid = gpui_core::MonthGrid::new(&self.state).dots(move |d| d == marked);
        div()
            .size_full()
            .child(if self.touch { grid.touch() } else { grid })
    }
}

fn open_nav(
    cx: &mut TestAppContext,
    touch: bool,
) -> (Entity<DatePickerState>, Events, &mut VisualTestContext) {
    let events: Events = Rc::default();
    let e = events.clone();
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    // No init and no Root: the state's constructor sets the keys up.
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let state = cx.new(|cx| DatePickerState::new(window, cx).week_start(chrono::Weekday::Mon));
        cx.subscribe(&state, move |_, _, ev: &DatePickerEvent, _| {
            e.borrow_mut().push(*ev)
        })
        .detach();
        *m.borrow_mut() = Some(state.clone());
        Nav { state, touch }
    });
    vcx.run_until_parked();
    let state = made.borrow().clone().unwrap();
    (state, events, vcx)
}

#[gpui::test]
fn a_calendar_can_show_a_month_and_have_a_day_picked(cx: &mut TestAppContext) {
    let (state, events, vcx) = open_nav(cx, false);
    let day = NaiveDate::from_ymd_opt(2026, 3, 9).unwrap();
    vcx.update(|_, cx| state.update(cx, |s, cx| s.show_month(day, cx)));
    vcx.run_until_parked();
    // A Monday-first grid of March 2026 starts on 23 February, and shows the 9th.
    assert!(vcx.debug_bounds("day-2026-02-23").is_some());
    assert!(vcx.debug_bounds("day-2026-02-22").is_none());
    let at = vcx.debug_bounds("day-2026-03-09").unwrap().center();
    vcx.simulate_click(at, Modifiers::none());
    assert_eq!(state.read_with(vcx, |s, _| s.value().start()), Some(day));
    assert_eq!(events.borrow().len(), 1);
}

#[gpui::test]
fn touch_days_are_44_points_and_marked_days_have_dots(cx: &mut TestAppContext) {
    let (state, _, vcx) = open_nav(cx, true);
    let day = NaiveDate::from_ymd_opt(2026, 3, 9).unwrap();
    vcx.update(|_, cx| state.update(cx, |s, cx| s.show_month(day, cx)));
    vcx.run_until_parked();
    let b = vcx.debug_bounds("day-2026-03-09").unwrap();
    assert!(
        b.size.width >= px(44.) && b.size.height >= px(44.),
        "{:?}",
        b.size
    );
    // The 10th has a dot: a small round fill inside its day.
    let marked = vcx.debug_bounds("day-2026-03-10").unwrap();
    let dots = vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let s = window.scale_factor();
        window
            .rendered_scene()
            .quads
            .iter()
            .filter(|q| {
                let w = q.bounds.size.width.as_f32() / s;
                let c = gpui::point(
                    px(q.bounds.origin.x.as_f32() / s + w / 2.),
                    px(q.bounds.origin.y.as_f32() / s + w / 2.),
                );
                w > 2. && w < 8. && marked.contains(&c)
            })
            .count()
    });
    assert_eq!(dots, 1);
}
