//! A button's states: a ghost button's hover is the hover token (the foreground at 5% in the
//! dark), drawn at once; and the one-pixel press is the pointer's alone: it shows while the
//! button is held down, never for Enter or Space.

use gpui::{
    AppContext as _, Context, FocusHandle, IntoElement, KeyDownEvent, KeyUpEvent, Keystroke,
    Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement as _, Render,
    Styled as _, TestAppContext, VisualTestContext, Window, div, px, size,
};
use gpui_core::{Button, ButtonVariants as _, Press, Root, Theme};

struct Host {
    before: FocusHandle,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                Press::new("before")
                    .track_focus(&self.before)
                    .child("Before"),
            )
            .child(Button::new("ghost").ghost().label("Ghost"))
            .child(Button::new("go").label("Go").on_click(|_, _, _| {}))
    }
}

fn open(cx: &mut TestAppContext) -> (gpui::Entity<Host>, &mut VisualTestContext) {
    cx.update(gpui_core::init);
    let made = std::rc::Rc::new(std::cell::RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let h = cx.new(|cx| Host {
            before: cx.focus_handle(),
        });
        *m.borrow_mut() = Some(h.clone());
        Root::new(h, window, cx)
    });
    vcx.simulate_resize(size(px(400.), px(300.)));
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let h = made.borrow().clone().unwrap();
    (h, vcx)
}

fn key(vcx: &mut VisualTestContext, k: &'static str, up: bool) {
    let keystroke = Keystroke::parse(k).unwrap();
    vcx.simulate_event(KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    if up {
        vcx.simulate_event(KeyUpEvent { keystroke });
    }
    vcx.run_until_parked();
}

#[gpui::test]
fn a_ghost_buttons_hover_is_the_hover_token(cx: &mut TestAppContext) {
    let (_, vcx) = open(cx);
    let at = vcx.debug_bounds("ghost").unwrap().center();
    vcx.simulate_event(MouseMoveEvent {
        position: at,
        pressed_button: None,
        modifiers: Modifiers::none(),
    });
    vcx.run_until_parked();
    let accent = vcx.update(|_, cx| gpui_core::ActiveTheme::theme(cx).accent);
    assert_eq!(accent, Theme::dark().accent);
    // Drawn at once: the hovered fill is in the very next frame's scene.
    let found = vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        window
            .rendered_scene()
            .quads
            .iter()
            .any(|q| q.background == gpui::Background::from(accent))
    });
    assert!(
        found,
        "a quad filled with the hover token is drawn under the pointer"
    );
}

#[gpui::test]
fn the_press_nudge_is_the_pointers_alone(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx);
    let rest = vcx.debug_bounds("go").unwrap();
    // Held down by the pointer: a pixel lower.
    vcx.simulate_event(MouseDownEvent {
        position: rest.center(),
        button: MouseButton::Left,
        modifiers: Modifiers::none(),
        click_count: 1,
        first_mouse: false,
    });
    vcx.run_until_parked();
    let held = vcx.debug_bounds("go").unwrap();
    assert_eq!(held.origin.y, rest.origin.y + px(1.), "held by the pointer");
    vcx.simulate_event(gpui::MouseUpEvent {
        position: rest.center(),
        button: MouseButton::Left,
        modifiers: Modifiers::none(),
        click_count: 1,
    });
    vcx.run_until_parked();
    assert_eq!(vcx.debug_bounds("go").unwrap().origin, rest.origin);
    // Enter and Space, held down: it stays where it is.
    let before = h.read_with(vcx, |h, _| h.before.clone());
    vcx.update(|window, cx| window.focus(&before, cx));
    key(vcx, "tab", true);
    key(vcx, "tab", true);
    for k in ["enter", "space"] {
        key(vcx, k, false);
        assert_eq!(
            vcx.debug_bounds("go").unwrap().origin,
            rest.origin,
            "{k} held down doesn't nudge it"
        );
        vcx.simulate_event(KeyUpEvent {
            keystroke: Keystroke::parse(k).unwrap(),
        });
        vcx.run_until_parked();
    }
}
