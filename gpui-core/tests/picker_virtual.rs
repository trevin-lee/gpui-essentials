//! A long picker draws only the rows in view: a thousand people open with a window's worth of
//! rows made, the keys scroll the highlight into view (to the last and back), a pick is the
//! right one, and filtering under the limit goes back to the plain list.

#![cfg(target_os = "macos")]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    AppContext as _, Context, IntoElement, KeyDownEvent, KeyUpEvent, Keystroke, ParentElement as _,
    Render, SharedString, Styled as _, TestAppContext, TestDispatcher, VisualTestContext, Window,
    div, px,
};
use gpui_core::{MenuSelect, Picker, PopupMenuItem, Root};

type Log = Rc<RefCell<Vec<SharedString>>>;

struct Panel {
    log: Log,
}

impl Render for Panel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let log = self.log.clone();
        div().size_full().p(px(20.)).w(px(300.)).child(
            Picker::new("people")
                .items(
                    (0..1000).map(|i| PopupMenuItem::new(format!("p{i}"), format!("Person {i}"))),
                )
                .on_select(move |pick: &MenuSelect, _, _| log.borrow_mut().push(pick.id.clone())),
        )
    }
}

fn open(cx: &mut TestAppContext) -> (Log, &mut VisualTestContext) {
    cx.update(gpui_core::init);
    let log: Log = Rc::default();
    let l = log.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let panel = cx.new(|_| Panel { log: l });
        Root::new(panel, window, cx)
    });
    vcx.simulate_resize(gpui::size(px(400.), px(800.)));
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    (log, vcx)
}

fn app() -> TestAppContext {
    TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        Arc::new(gpui_apple::AppleTextSystem::new()),
    )
}

fn keys(vcx: &mut VisualTestContext, keys: &str) {
    for k in keys.split(' ') {
        let k: &'static str = Box::leak(k.to_string().into_boxed_str());
        let keystroke = Keystroke::parse(k).unwrap();
        vcx.simulate_event(KeyDownEvent {
            keystroke: keystroke.clone(),
            is_held: false,
            prefer_character_input: false,
        });
        vcx.simulate_event(KeyUpEvent { keystroke });
        vcx.run_until_parked();
    }
}

fn drawn(vcx: &mut VisualTestContext, i: usize) -> bool {
    let sel: &'static str = Box::leak(format!("menu-item-p{i}").into_boxed_str());
    vcx.debug_bounds(sel).is_some()
}

#[test]
fn only_the_rows_in_view_are_made() {
    let mut cx = app();
    let (_, vcx) = open(&mut cx);
    assert!(drawn(vcx, 0));
    let made = (0..1000).filter(|i| drawn(vcx, *i)).count();
    assert!((5..=15).contains(&made), "{made} rows made");
}

#[test]
fn the_keys_scroll_the_highlight_into_view() {
    let mut cx = app();
    let (log, vcx) = open(&mut cx);
    // Up from the first wraps to the last, which scrolls into view.
    keys(vcx, "up");
    assert!(drawn(vcx, 999));
    assert!(!drawn(vcx, 0));
    keys(vcx, "enter");
    assert_eq!(
        log.borrow().last().map(|s| s.to_string()),
        Some("p999".into())
    );
}

#[test]
fn going_down_a_window_and_more_picks_the_right_one() {
    let mut cx = app();
    let (log, vcx) = open(&mut cx);
    keys(vcx, &["down"; 15].join(" "));
    assert!(drawn(vcx, 15));
    keys(vcx, "enter");
    assert_eq!(
        log.borrow().last().map(|s| s.to_string()),
        Some("p15".into())
    );
}

#[test]
fn filtering_under_the_limit_draws_every_match() {
    let mut cx = app();
    let (_, vcx) = open(&mut cx);
    vcx.simulate_input("Person 77");
    // Every person with "77" in their number: nineteen, all drawn.
    let made: Vec<usize> = (0..1000).filter(|i| drawn(vcx, *i)).collect();
    let want: Vec<usize> = (0..1000).filter(|i| i.to_string().contains("77")).collect();
    assert_eq!(made, want);
}
