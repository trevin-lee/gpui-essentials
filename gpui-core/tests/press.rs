//! Press: a click, a tap, Enter and Space press it; Tab reaches it; disabled, it takes neither
//! the focus nor a press.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    AppContext as _, Context, InteractiveElement as _, IntoElement, KeyDownEvent, KeyUpEvent,
    Keystroke, Modifiers, ParentElement as _, Render, Styled as _, TestAppContext,
    VisualTestContext, Window, div, px,
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
