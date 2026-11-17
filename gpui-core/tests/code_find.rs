//! A code editor's find bar: ⌘F opens it on the selection's text, typing finds (case aside) and
//! selects the match, Enter and Shift-Enter go round the matches, ⌘G and ⌘⇧G from the text,
//! the other matches are marked, Tab in the bar doesn't indent the text, and Escape or its close
//! button closes it with the keyboard back in the text. Laid out with Core Text.

#![cfg(target_os = "macos")]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, Modifiers,
    ParentElement as _, Render, Styled as _, TestAppContext, TestDispatcher, VisualTestContext,
    Window, div, px,
};
use gpui_core::{ActiveTheme as _, Input, InputState, Root};

const POLICY: &str = "rules:\n  - name: one\n    action: allow\n  - name: two\n    action: ask\n  - name: three\n    action: Allow\n";

struct Form {
    code: Entity<InputState>,
}

impl Render for Form {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().p(px(10.)).child(
            div()
                .w(px(460.))
                .h(px(240.))
                .debug_selector(|| "code".into())
                .child(Input::new(&self.code).size_full()),
        )
    }
}

fn open(cx: &mut TestAppContext) -> (Entity<InputState>, &mut VisualTestContext) {
    cx.update(gpui_core::init);
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let code = cx.new(|cx| InputState::code(window, cx).default_value(POLICY));
        *m.borrow_mut() = Some(code.clone());
        let form = cx.new(|_| Form { code });
        Root::new(form, window, cx)
    });
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let code = made.borrow().clone().unwrap();
    vcx.update(|window, cx| code.update(cx, |f, cx| f.focus(window, cx)));
    vcx.run_until_parked();
    (code, vcx)
}

fn app() -> TestAppContext {
    TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        Arc::new(gpui_apple::AppleTextSystem::new()),
    )
}

/// Where the selection is: its line (from 0) and its text.
fn selected(code: &Entity<InputState>, vcx: &mut VisualTestContext) -> (usize, String) {
    code.read_with(vcx, |c, _| {
        let v = c.value().to_string();
        let r = c.selection();
        (v[..r.start].matches('\n').count(), v[r].to_string())
    })
}

#[test]
fn typing_finds_case_aside_and_enter_goes_round_the_matches() {
    let mut cx = app();
    let (code, vcx) = open(&mut cx);
    vcx.simulate_keystrokes("cmd-f");
    assert!(vcx.debug_bounds("code-find").is_some(), "the bar is open");
    vcx.simulate_input("allow");
    assert_eq!(selected(&code, vcx), (2, "allow".into()));
    vcx.simulate_keystrokes("enter");
    assert_eq!(selected(&code, vcx), (6, "Allow".into()), "case aside");
    vcx.simulate_keystrokes("enter");
    assert_eq!(selected(&code, vcx), (2, "allow".into()), "round the end");
    vcx.simulate_keystrokes("shift-enter");
    assert_eq!(selected(&code, vcx), (6, "Allow".into()), "and back");
    // Nothing typed went into the text.
    assert_eq!(code.read_with(vcx, |c, _| c.value().to_string()), POLICY);
}

#[test]
fn escape_closes_it_and_cmd_g_goes_on_from_the_text() {
    let mut cx = app();
    let (code, vcx) = open(&mut cx);
    vcx.simulate_keystrokes("cmd-f");
    vcx.simulate_input("name");
    vcx.simulate_keystrokes("escape");
    assert!(vcx.debug_bounds("code-find").is_none(), "closed");
    assert_eq!(selected(&code, vcx), (1, "name".into()));
    vcx.simulate_keystrokes("cmd-g");
    assert_eq!(selected(&code, vcx), (3, "name".into()));
    vcx.simulate_keystrokes("cmd-shift-g cmd-shift-g");
    assert_eq!(selected(&code, vcx), (5, "name".into()), "round the start");
    // The keyboard is back in the text.
    vcx.simulate_keystrokes("right");
    vcx.simulate_input("!");
    assert!(code.read_with(vcx, |c, _| c.value().contains("name!: three")));
}

#[test]
fn the_selection_is_what_s_looked_for_and_tab_in_the_bar_doesnt_indent() {
    let mut cx = app();
    let (code, vcx) = open(&mut cx);
    // "rules" selected, then ⌘F.
    vcx.simulate_keystrokes("shift-alt-right cmd-f enter");
    assert_eq!(selected(&code, vcx), (0, "rules".into()), "the one match");
    // Tab in the bar moves the keyboard on, as from any field: it doesn't indent the text.
    vcx.simulate_keystrokes("cmd-f tab");
    assert_eq!(code.read_with(vcx, |c, _| c.value().to_string()), POLICY);
}

#[test]
fn the_other_matches_are_marked_and_the_close_button_closes() {
    let mut cx = app();
    let (code, vcx) = open(&mut cx);
    let attention = vcx.update(|_, cx| cx.theme().attention);
    vcx.simulate_keystrokes("cmd-f");
    vcx.simulate_input("action");
    let marked = vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        window
            .rendered_scene()
            .quads
            .iter()
            .filter(|q| {
                q.background.as_solid().is_some_and(|c| {
                    (c.h - attention.h).abs() < 0.02 && (c.a - attention.a * 0.3).abs() < 0.05
                })
            })
            .count()
    });
    assert_eq!(marked, 2, "three matches: one selected, two marked");
    let close = vcx.debug_bounds("code-find-close").unwrap().center();
    vcx.simulate_click(close, Modifiers::none());
    vcx.run_until_parked();
    assert!(vcx.debug_bounds("code-find").is_none());
    let _ = code;
}
