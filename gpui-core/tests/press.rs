//! Press: a click, a tap, Enter and Space press it; Tab reaches it; disabled, it takes neither
//! the focus nor a press.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{
    AppContext as _, Context, InteractiveElement as _, IntoElement, KeyDownEvent, KeyUpEvent,
    Keystroke, Modifiers, ParentElement as _, Render, StatefulInteractiveElement as _, Styled as _,
    TestAppContext, VisualTestContext, Window, div, px,
};
use gpui_core::{Disableable as _, Press, Root};

struct Two {
    pressed: Rc<Cell<[usize; 2]>>,
    disabled: bool,
}

impl Render for Two {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let press = |i: usize, p: Rc<Cell<[usize; 2]>>| {
            Press::new(("press", i))
                .w(px(80.))
                .h(px(30.))
                .debug_selector(move || format!("press{i}"))
                .child("Press")
                .on_click(move |_, _, _| {
                    let mut n = p.get();
                    n[i] += 1;
                    p.set(n);
                })
        };
        div()
            .size_full()
            .child(press(0, self.pressed.clone()))
            .child(press(1, self.pressed.clone()).disabled(self.disabled))
    }
}

fn open(cx: &mut TestAppContext, disabled: bool) -> (Rc<Cell<[usize; 2]>>, &mut VisualTestContext) {
    cx.update(gpui_core::init);
    let pressed = Rc::new(Cell::new([0, 0]));
    let p = pressed.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let two = cx.new(|_| Two {
            pressed: p,
            disabled,
        });
        Root::new(two, window, cx)
    });
    vcx.run_until_parked();
    (pressed, vcx)
}

/// A key pressed and let go: a keyboard press is a key down and a key up on the same element.
fn key(vcx: &mut VisualTestContext, keys: &'static str) {
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

fn click(vcx: &mut VisualTestContext, which: &'static str) {
    let at = vcx.debug_bounds(which).expect("drawn").center();
    vcx.simulate_click(at, Modifiers::none());
    vcx.run_until_parked();
}

#[gpui::test]
fn a_click_presses_it(cx: &mut TestAppContext) {
    let (pressed, vcx) = open(cx, false);
    click(vcx, "press0");
    click(vcx, "press1");
    assert_eq!(pressed.get(), [1, 1]);
}

#[gpui::test]
fn tab_reaches_it_and_enter_and_space_press_it(cx: &mut TestAppContext) {
    let (pressed, vcx) = open(cx, false);
    key(vcx, "tab enter tab space");
    assert_eq!(pressed.get(), [1, 1]);
}

#[gpui::test]
fn disabled_it_takes_neither_the_focus_nor_a_press(cx: &mut TestAppContext) {
    let (pressed, vcx) = open(cx, true);
    click(vcx, "press1");
    assert_eq!(pressed.get(), [0, 0]);
    // Tab stops only on the first; Enter there presses it, not the disabled one.
    key(vcx, "tab tab enter");
    assert_eq!(pressed.get(), [1, 0]);
}

#[gpui::test]
fn disabled_is_announced(cx: &mut TestAppContext) {
    cx.update(gpui_core::init);
    let (_, vcx) = cx.add_window_view(|_, _| Two {
        pressed: Rc::default(),
        disabled: false,
    });
    let node = |disabled: bool, vcx: &mut VisualTestContext| {
        vcx.update(|window, cx| {
            let focus = cx.focus_handle();
            let press = Press::new("p").track_focus(&focus).disabled(disabled);
            let el = gpui::RenderOnce::render(press, window, cx).into_element();
            let mut node = gpui::accesskit::Node::new(gpui::Role::Button);
            gpui::Element::write_a11y_info(&el, &mut node);
            node
        })
    };
    assert!(node(true, vcx).is_disabled());
    assert!(!node(false, vcx).is_disabled());
}

/// Three presses; pressing the middle one disables it.
struct Three {
    off: Rc<Cell<bool>>,
    focus: Vec<gpui::FocusHandle>,
}

impl Render for Three {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let off = self.off.clone();
        div()
            .size_full()
            .child(
                Press::new("a")
                    .track_focus(&self.focus[0])
                    .w(px(40.))
                    .h(px(20.))
                    .child("A"),
            )
            .child(
                Press::new("b")
                    .track_focus(&self.focus[1])
                    .w(px(40.))
                    .h(px(20.))
                    .child("B")
                    .disabled(self.off.get())
                    .on_click(move |_, window, _| {
                        off.set(true);
                        window.refresh();
                    }),
            )
            .child(
                Press::new("c")
                    .track_focus(&self.focus[2])
                    .w(px(40.))
                    .h(px(20.))
                    .child("C"),
            )
    }
}

#[gpui::test]
fn a_press_that_disables_itself_keeps_its_place_in_tab_order(cx: &mut TestAppContext) {
    cx.update(gpui_core::init);
    let made = Rc::new(RefCell::new(Vec::new()));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let focus: Vec<_> = (0..3).map(|_| cx.focus_handle()).collect();
        *m.borrow_mut() = focus.clone();
        let three = cx.new(|_| Three {
            off: Rc::default(),
            focus,
        });
        Root::new(three, window, cx)
    });
    vcx.run_until_parked();
    let focus = made.borrow().clone();
    key(vcx, "tab tab enter");
    vcx.update(|window, _| {
        assert!(
            focus[1].is_focused(window),
            "B lost the focus when it was disabled"
        )
    });
    key(vcx, "tab");
    vcx.update(|window, _| assert!(focus[2].is_focused(window), "Tab didn't move on from B"));
    // And it's no longer a stop: back from C is A.
    key(vcx, "shift-tab");
    vcx.update(|window, _| assert!(focus[0].is_focused(window)));
}

struct Inside {
    outer: Rc<Cell<usize>>,
}

impl Render for Inside {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let outer = self.outer.clone();
        div()
            .id("outer")
            .size_full()
            .on_click(move |_, _, _| outer.set(outer.get() + 1))
            .child(
                div().child(
                    Press::new("off")
                        .debug_selector(|| "off".into())
                        .w(px(40.))
                        .h(px(20.))
                        .disabled(true),
                ),
            )
    }
}

#[gpui::test]
fn a_press_on_a_disabled_one_doesnt_fall_through(cx: &mut TestAppContext) {
    cx.update(gpui_core::init);
    let outer = Rc::new(Cell::new(0));
    let o = outer.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let inside = cx.new(|_| Inside { outer: o });
        Root::new(inside, window, cx)
    });
    vcx.run_until_parked();
    click(vcx, "off");
    assert_eq!(outer.get(), 0);
}
