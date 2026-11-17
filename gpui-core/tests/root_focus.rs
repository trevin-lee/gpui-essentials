//! Root's first focus: with `Root::focusing`, the host's view takes it, so a key binding in the
//! view's own key context (a ⌘K palette) works from the first key; with `Root::new`, the Root
//! takes it and that binding reaches nothing until something in the view has the focus.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    AppContext as _, Context, FocusHandle, Focusable, InteractiveElement as _, IntoElement,
    KeyBinding, KeyDownEvent, KeyUpEvent, Keystroke, ParentElement as _, Render, TestAppContext,
    VisualTestContext, Window, actions, div,
};
use gpui_core::Root;

actions!(root_focus_test, [OpenPalette]);

struct Host {
    focus: FocusHandle,
    /// A tab stop inside the host.
    field: FocusHandle,
    opened: Rc<Cell<usize>>,
}

impl Focusable for Host {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let opened = self.opened.clone();
        div()
            .key_context("Host")
            .track_focus(&self.focus)
            .on_action(move |_: &OpenPalette, _, _| opened.set(opened.get() + 1))
            .child(
                div()
                    .id("field")
                    .track_focus(&self.field.clone().tab_stop(true)),
            )
            .child("host")
    }
}

fn press(vcx: &mut VisualTestContext, key: &'static str) {
    let keystroke = Keystroke::parse(key).unwrap();
    vcx.simulate_event(KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    vcx.simulate_event(KeyUpEvent { keystroke });
    vcx.run_until_parked();
}

fn open(
    cx: &mut TestAppContext,
    focusing: bool,
) -> (Rc<Cell<usize>>, gpui::Entity<Host>, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_core::init(cx);
        cx.bind_keys([KeyBinding::new("cmd-k", OpenPalette, Some("Host"))]);
    });
    let opened = Rc::new(Cell::new(0));
    let o = opened.clone();
    let made = Rc::new(std::cell::RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let host = cx.new(|cx| Host {
            focus: cx.focus_handle(),
            field: cx.focus_handle(),
            opened: o,
        });
        *m.borrow_mut() = Some(host.clone());
        if focusing {
            Root::focusing(host, window, cx)
        } else {
            Root::new(host, window, cx)
        }
    });
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let host = made.borrow().clone().unwrap();
    (opened, host, vcx)
}

#[gpui::test]
fn a_focusing_root_gives_the_hosts_key_bindings_the_first_key(cx: &mut TestAppContext) {
    let (opened, host, vcx) = open(cx, true);
    press(vcx, "cmd-k");
    assert_eq!(opened.get(), 1);
    // Tab still moves the focus (the Root's binding is above the host), and the host's binding
    // still works from inside it.
    press(vcx, "tab");
    let field = host.read_with(vcx, |h, _| h.field.clone());
    assert!(vcx.update(|window, _| field.is_focused(window)));
    press(vcx, "cmd-k");
    assert_eq!(opened.get(), 2);
}

#[gpui::test]
fn a_plain_root_takes_the_first_focus_itself(cx: &mut TestAppContext) {
    // What `Root::focusing` is for: the Root has the focus, above the host's key context.
    let (opened, _, vcx) = open(cx, false);
    press(vcx, "cmd-k");
    assert_eq!(opened.get(), 0);
}
