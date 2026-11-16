//! Slider: values fit the range and step; keys step it; a press on the track jumps there; a drag
//! follows the pointer past the ends and ends with a release; disabled it ignores everything.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, KeyDownEvent,
    KeyUpEvent, Keystroke, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ParentElement as _, Render, Styled as _, TestAppContext, VisualTestContext, Window, div, point,
    px,
};
use gpui_core::{Disableable as _, Root, Slider, SliderEvent, SliderState};

struct Panel {
    state: Entity<SliderState>,
    disabled: bool,
    draws: Rc<std::cell::Cell<usize>>,
}

impl Render for Panel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        div().size_full().p(px(20.)).child(
            div()
                .debug_selector(|| "slider".into())
                .w(px(200.))
                .child(Slider::new(&self.state).disabled(self.disabled).w_full()),
        )
    }
}

type Events = Rc<RefCell<Vec<SliderEvent>>>;

thread_local! {
    static DRAWS: Rc<std::cell::Cell<usize>> = Rc::default();
}

fn open(
    cx: &mut TestAppContext,
    disabled: bool,
) -> (Entity<SliderState>, Events, &mut VisualTestContext) {
    cx.update(gpui_core::init);
    let events: Events = Rc::default();
    let e = events.clone();
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let state = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(100.)
                .step(10.)
                .default_value(50.)
        });
        cx.subscribe(&state, move |_, _, ev: &SliderEvent, _| {
            e.borrow_mut().push(*ev)
        })
        .detach();
        *m.borrow_mut() = Some(state.clone());
        let panel = cx.new(|_| Panel {
            state,
            disabled,
            draws: DRAWS.with(|d| d.clone()),
        });
        Root::new(panel, window, cx)
    });
    vcx.run_until_parked();
    let state = made.borrow().clone().unwrap();
    (state, events, vcx)
}

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

#[test]
fn values_fit_the_range_and_step() {
    let s = SliderState::new()
        .min(0.)
        .max(1.)
        .step(0.1)
        .default_value(0.34);
    assert_eq!(s.value(), 0.3);
    let s = SliderState::new().min(10.).max(0.).default_value(99.);
    assert_eq!(s.value(), 10., "a reversed range still holds");
    let s = SliderState::new()
        .min(0.)
        .max(10.)
        .step(f32::NAN)
        .default_value(f32::INFINITY);
    assert_eq!(s.value(), 0., "nonsense is the minimum");
    let s = SliderState::new()
        .min(0.)
        .max(10.)
        .step(0.)
        .default_value(3.33);
    assert_eq!(s.value(), 3.33, "no step: any value in range");
}

#[gpui::test]
fn keys_step_it(cx: &mut TestAppContext) {
    let (state, events, vcx) = open(cx, false);
    let before = DRAWS.with(|d| d.get());
    keys(vcx, "tab right right");
    assert_eq!(state.read_with(vcx, |s, _| s.value()), 70.);
    assert!(
        DRAWS.with(|d| d.get()) > before,
        "the view holding the slider didn't redraw"
    );
    keys(vcx, "home");
    assert_eq!(state.read_with(vcx, |s, _| s.value()), 0.);
    keys(vcx, "left");
    keys(vcx, "pageup");
    assert_eq!(state.read_with(vcx, |s, _| s.value()), 100.);
    let ev = events.borrow();
    assert_eq!(ev[0], SliderEvent::Change(60.));
    assert_eq!(ev[1], SliderEvent::Release(60.));
    // At the minimum, left changes nothing and says nothing.
    assert!(!ev.contains(&SliderEvent::Change(-10.)));
}

#[gpui::test]
fn a_press_jumps_and_a_drag_follows_past_the_ends(cx: &mut TestAppContext) {
    let (state, events, vcx) = open(cx, false);
    let b = vcx.debug_bounds("slider").unwrap();
    let y = b.center().y;
    let at = |f: f32| point(b.origin.x + b.size.width * f, y);
    vcx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: at(0.2),
        modifiers: Modifiers::none(),
        click_count: 1,
        first_mouse: false,
    });
    vcx.run_until_parked();
    assert_eq!(state.read_with(vcx, |s, _| s.value()), 20.);
    vcx.simulate_event(MouseMoveEvent {
        position: at(1.5),
        pressed_button: Some(MouseButton::Left),
        modifiers: Modifiers::none(),
    });
    vcx.run_until_parked();
    assert_eq!(
        state.read_with(vcx, |s, _| s.value()),
        100.,
        "past the end is the end"
    );
    vcx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: at(1.5),
        modifiers: Modifiers::none(),
        click_count: 1,
    });
    vcx.run_until_parked();
    assert_eq!(events.borrow().last(), Some(&SliderEvent::Release(100.)));
    // After the release, moving doesn't drag.
    vcx.simulate_event(MouseMoveEvent {
        position: at(0.),
        pressed_button: None,
        modifiers: Modifiers::none(),
    });
    vcx.run_until_parked();
    assert_eq!(state.read_with(vcx, |s, _| s.value()), 100.);
}

#[gpui::test]
fn disabled_it_ignores_everything(cx: &mut TestAppContext) {
    let (state, events, vcx) = open(cx, true);
    let b = vcx.debug_bounds("slider").unwrap();
    vcx.simulate_click(point(b.origin.x + px(5.), b.center().y), Modifiers::none());
    keys(vcx, "tab right");
    assert_eq!(state.read_with(vcx, |s, _| s.value()), 50.);
    assert!(events.borrow().is_empty());
}
