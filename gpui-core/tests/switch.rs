//! Switch: a press asks for the opposite state, disabled it ignores presses, Space works from
//! the keyboard, and once its thumb has slid nothing redraws.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, KeyDownEvent,
    KeyUpEvent, Keystroke, Modifiers, ParentElement as _, Render, Styled as _, TestAppContext,
    VisualTestContext, Window, div,
};
use gpui_core::{Disableable as _, Root, Switch};

struct Panel {
    on: bool,
    disabled: bool,
    asked: Rc<RefCell<Vec<bool>>>,
    draws: Rc<Cell<usize>>,
}

impl Render for Panel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let asked = self.asked.clone();
        div().size_full().child(
            div().debug_selector(|| "switch".into()).child(
                Switch::new("s")
                    .checked(self.on)
                    .disabled(self.disabled)
                    .on_click(cx.listener(move |this, on: &bool, _, cx| {
                        asked.borrow_mut().push(*on);
                        this.on = *on;
                        cx.notify();
                    })),
            ),
        )
    }
}

struct Opened<'a> {
    panel: Entity<Panel>,
    asked: Rc<RefCell<Vec<bool>>>,
    draws: Rc<Cell<usize>>,
    vcx: &'a mut VisualTestContext,
}

fn open(cx: &mut TestAppContext, disabled: bool) -> Opened<'_> {
    cx.update(gpui_core::init);
    let asked = Rc::new(RefCell::new(Vec::new()));
    let draws = Rc::new(Cell::new(0));
    let (a, d) = (asked.clone(), draws.clone());
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let panel = cx.new(|_| Panel {
            on: false,
            disabled,
            asked: a,
            draws: d,
        });
        *m.borrow_mut() = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    vcx.run_until_parked();
    let panel = made.borrow().clone().unwrap();
    Opened {
        panel,
        asked,
        draws,
        vcx,
    }
}

fn click(vcx: &mut VisualTestContext) {
    let at = vcx.debug_bounds("switch").expect("drawn").center();
    vcx.simulate_click(at, Modifiers::none());
    vcx.run_until_parked();
}

#[gpui::test]
fn a_press_asks_for_the_opposite_state(cx: &mut TestAppContext) {
    let o = open(cx, false);
    click(o.vcx);
    click(o.vcx);
    assert_eq!(*o.asked.borrow(), [true, false]);
}

#[gpui::test]
fn disabled_it_ignores_presses(cx: &mut TestAppContext) {
    let o = open(cx, true);
    click(o.vcx);
    assert!(o.asked.borrow().is_empty());
}

#[gpui::test]
fn space_turns_it_from_the_keyboard(cx: &mut TestAppContext) {
    let o = open(cx, false);
    for k in ["tab", "space"] {
        let keystroke = Keystroke::parse(k).unwrap();
        o.vcx.simulate_event(KeyDownEvent {
            keystroke: keystroke.clone(),
            is_held: false,
            prefer_character_input: false,
        });
        o.vcx.simulate_event(KeyUpEvent { keystroke });
    }
    o.vcx.run_until_parked();
    assert_eq!(*o.asked.borrow(), [true]);
    assert!(o.panel.read_with(o.vcx, |p, _| p.on));
}

/// Delivers one display frame (the test platform has no frame loop of its own) and says how
/// many things had asked for it.
fn frame(vcx: &mut VisualTestContext) -> usize {
    vcx.executor().advance_clock(Duration::from_millis(16));
    let asked = vcx.update(|window, cx| window.simulate_next_frame(cx));
    vcx.run_until_parked();
    asked
}

#[gpui::test]
fn once_the_thumb_has_slid_nothing_redraws(cx: &mut TestAppContext) {
    let o = open(cx, false);
    click(o.vcx);
    // The slide takes 150 ms and asks for frames while it runs.
    let mut asked = 0;
    for _ in 0..30 {
        asked += frame(o.vcx);
    }
    assert!(asked > 0, "the thumb didn't animate");
    let settled = o.draws.get();
    for _ in 0..60 {
        assert_eq!(frame(o.vcx), 0, "asked for a frame after the thumb settled");
    }
    assert_eq!(
        o.draws.get(),
        settled,
        "kept drawing after the thumb settled"
    );
}
