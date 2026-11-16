//! A date picker's and a month's colours given by the host: the field's fill, border and
//! placeholder, the month's popover, a picked day, a range's middle, today, and the quiet days.
//! Laid out with Core Text, as on screen.

#![cfg(target_os = "macos")]

use std::sync::Arc;

use chrono::NaiveDate;
use gpui::{
    AppContext as _, Context, Entity, Hsla, InteractiveElement as _, IntoElement, Modifiers,
    ParentElement as _, Render, Styled as _, TestAppContext, TestDispatcher, VisualTestContext,
    Window, div, px,
};
use gpui_core::{DatePicker, DatePickerColors, DatePickerState, MonthGrid, PopoverColors, Root};

const fn hue(h: f32) -> Hsla {
    Hsla {
        h,
        s: 0.9,
        l: 0.5,
        a: 1.,
    }
}

const RED: Hsla = hue(0.);
const ORANGE: Hsla = hue(0.08);
const YELLOW: Hsla = hue(0.16);
const GREEN: Hsla = hue(0.33);
const CYAN: Hsla = hue(0.5);
const BLUE: Hsla = hue(0.62);
const PURPLE: Hsla = hue(0.78);
const PINK: Hsla = hue(0.9);

fn near(a: Hsla, b: Hsla) -> bool {
    (a.h - b.h).abs() < 0.02 && (a.l - b.l).abs() < 0.06 && a.s > 0.5
}

struct Drawn {
    glyphs: Vec<Hsla>,
    fills: Vec<Hsla>,
    borders: Vec<Hsla>,
}

fn draw(vcx: &mut VisualTestContext) -> Drawn {
    vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let scene = window.rendered_scene();
        Drawn {
            glyphs: scene
                .monochrome_sprites
                .iter()
                .map(|g| g.color)
                .chain(scene.subpixel_sprites.iter().map(|g| g.color))
                .collect(),
            fills: scene
                .quads
                .iter()
                .filter_map(|q| q.background.as_solid())
                .collect(),
            borders: scene
                .quads
                .iter()
                .filter(|q| q.border_widths.left.0 > 0.)
                .map(|q| q.border_color)
                .collect(),
        }
    })
}

fn app() -> TestAppContext {
    TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        Arc::new(gpui_apple::AppleTextSystem::new()),
    )
}

const COLORS: DatePickerColors = DatePickerColors {
    field: Some(GREEN),
    border: Some(RED),
    text: None,
    placeholder: Some(ORANGE),
    focus_ring: None,
    focus_border: None,
    popover: Some(PopoverColors {
        background: BLUE,
        foreground: hue(0.0),
        border: RED,
    }),
    selected: Some(PURPLE),
    selected_text: Some(CYAN),
    range: Some(PINK),
    hover: None,
    today: None,
    today_ring: None,
    muted: Some(YELLOW),
    cursor: None,
};

struct Field(Entity<DatePickerState>);

impl Render for Field {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().p(px(20.)).w(px(300.)).child(
            div()
                .debug_selector(|| "picker".into())
                .child(DatePicker::new(&self.0).colors(COLORS)),
        )
    }
}

#[test]
fn a_fields_and_its_months_colours_are_the_hosts() {
    let mut cx = app();
    cx.update(gpui_core::init);
    let made = std::rc::Rc::new(std::cell::RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let state = cx.new(|cx| DatePickerState::new(window, cx));
        *m.borrow_mut() = Some(state.clone());
        let v = cx.new(|_| Field(state));
        Root::new(v, window, cx)
    });
    vcx.run_until_parked();
    let state = made.borrow().clone().unwrap();

    // Closed, with nothing picked: the field's fill, border and placeholder.
    let d = draw(vcx);
    assert!(d.fills.iter().any(|c| near(*c, GREEN)), "the field's fill");
    assert!(
        d.borders.iter().any(|c| near(*c, RED)),
        "the field's border"
    );
    assert!(d.glyphs.iter().any(|c| near(*c, ORANGE)), "the placeholder");

    // A day picked, and the month open: the popover, the picked day, the quiet days.
    let day = NaiveDate::from_ymd_opt(2030, 6, 12).unwrap();
    vcx.update(|_, cx| {
        state.update(cx, |s, cx| {
            s.set_value(Some(day), None, cx);
            s.show_month(day, cx);
        })
    });
    let b = vcx.debug_bounds("picker").unwrap();
    vcx.simulate_click(b.center(), Modifiers::none());
    vcx.run_until_parked();
    let d = draw(vcx);
    assert!(d.fills.iter().any(|c| near(*c, BLUE)), "the popover");
    assert!(d.fills.iter().any(|c| near(*c, PURPLE)), "the picked day");
    assert!(
        d.glyphs.iter().any(|c| near(*c, CYAN)),
        "the picked day's number"
    );
    assert!(
        d.glyphs.iter().any(|c| near(*c, YELLOW)),
        "days outside the month and the weekdays"
    );
}

struct Grid(Entity<DatePickerState>);

impl Render for Grid {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().child(MonthGrid::new(&self.0).colors(COLORS))
    }
}

#[test]
fn a_months_range_takes_the_hosts_colours() {
    let mut cx = app();
    cx.update(gpui_core::init);
    let (_, vcx) = cx.add_window_view(|window, cx| {
        let a = NaiveDate::from_ymd_opt(2030, 6, 10).unwrap();
        let b = NaiveDate::from_ymd_opt(2030, 6, 14).unwrap();
        let state = cx.new(|cx| {
            let mut s = DatePickerState::range(window, cx);
            s.set_value(Some(a), Some(b), cx);
            s.show_month(a, cx);
            s
        });
        let v = cx.new(|_| Grid(state));
        Root::new(v, window, cx)
    });
    vcx.run_until_parked();
    let d = draw(vcx);
    assert!(d.fills.iter().any(|c| near(*c, PINK)), "the range's middle");
    assert!(d.fills.iter().any(|c| near(*c, PURPLE)), "its ends");
}

#[test]
fn without_colours_the_theme_draws_it() {
    let mut cx = app();
    cx.update(gpui_core::init);
    struct Plain(Entity<DatePickerState>);
    impl Render for Plain {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().child(MonthGrid::new(&self.0))
        }
    }
    let (_, vcx) = cx.add_window_view(|window, cx| {
        let state = cx.new(|cx| DatePickerState::new(window, cx));
        let v = cx.new(|_| Plain(state));
        Root::new(v, window, cx)
    });
    vcx.run_until_parked();
    let d = draw(vcx);
    for c in [GREEN, RED, ORANGE, BLUE, PURPLE, CYAN, PINK, YELLOW] {
        assert!(!d.fills.iter().chain(&d.glyphs).any(|x| near(*x, c)));
    }
}

struct Shown(Entity<DatePickerState>);

impl Render for Shown {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let a = NaiveDate::from_ymd_opt(2030, 6, 10).unwrap();
        let b = NaiveDate::from_ymd_opt(2030, 6, 14).unwrap();
        div().child(MonthGrid::new(&self.0).colors(COLORS).shown_range(b, a))
    }
}

#[test]
fn a_single_pickers_month_shows_a_range_kept_elsewhere() {
    let mut cx = app();
    cx.update(gpui_core::init);
    let (_, vcx) = cx.add_window_view(|window, cx| {
        let state = cx.new(|cx| {
            let mut s = DatePickerState::new(window, cx);
            s.show_month(NaiveDate::from_ymd_opt(2030, 6, 1).unwrap(), cx);
            s
        });
        let v = cx.new(|_| Shown(state));
        Root::new(v, window, cx)
    });
    vcx.run_until_parked();
    let d = draw(vcx);
    assert!(d.fills.iter().any(|c| near(*c, PINK)), "the days between");
    assert!(d.fills.iter().any(|c| near(*c, PURPLE)), "its ends");
}

struct Focused(Entity<DatePickerState>);

impl Render for Focused {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .p(px(20.))
            .w(px(300.))
            .child(DatePicker::new(&self.0).colors(DatePickerColors {
                focus_ring: Some(BLUE),
                focus_border: Some(GREEN),
                ..Default::default()
            }))
    }
}

#[test]
fn the_focused_fields_border_and_ring_take_their_own_colours() {
    let mut cx = app();
    cx.update(gpui_core::init);
    let (_, vcx) = cx.add_window_view(|window, cx| {
        let state = cx.new(|cx| DatePickerState::new(window, cx));
        let v = cx.new(|_| Focused(state));
        Root::new(v, window, cx)
    });
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    // Focused from the keyboard, as the ring shows only then.
    vcx.simulate_keystrokes("tab");
    vcx.run_until_parked();
    let (borders, shadows) = vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let scene = window.rendered_scene();
        let borders: Vec<Hsla> = scene
            .quads
            .iter()
            .filter(|q| q.border_widths.left.0 > 0.)
            .map(|q| q.border_color)
            .collect();
        let shadows: Vec<Hsla> = scene.shadows.iter().map(|s| s.color).collect();
        (borders, shadows)
    });
    assert!(borders.iter().any(|c| near(*c, GREEN)), "the border");
    assert!(shadows.iter().any(|c| near(*c, BLUE)), "the ring");
}

struct Ringed(Entity<DatePickerState>);

impl Render for Ringed {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().child(MonthGrid::new(&self.0).colors(DatePickerColors {
            today_ring: Some(PINK),
            ..Default::default()
        }))
    }
}

#[test]
fn today_takes_the_hosts_ring_and_the_keyboards_day_keeps_its_own() {
    let today = NaiveDate::from_ymd_opt(2030, 6, 12).unwrap();
    let ringed = |cursor: NaiveDate| {
        let mut cx = app();
        cx.update(gpui_core::init);
        let (_, vcx) = cx.add_window_view(move |window, cx| {
            let state = cx.new(|cx| {
                let mut s = DatePickerState::new(window, cx).today(today);
                s.show_month(cursor, cx);
                s
            });
            let v = cx.new(|_| Ringed(state));
            Root::new(v, window, cx)
        });
        vcx.run_until_parked();
        draw(vcx).borders.iter().any(|c| near(*c, PINK))
    };
    // The keyboard on the 20th: today is ringed.
    assert!(ringed(NaiveDate::from_ymd_opt(2030, 6, 20).unwrap()));
    // The keyboard on today: its own border, not the ring.
    assert!(!ringed(today));
}
