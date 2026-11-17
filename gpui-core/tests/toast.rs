//! Toasts: shown at the bottom of the window and gone by themselves (an error later), one with
//! the same id replacing the one shown, at most three, an action run and the toast closed, the
//! close button closing it, and nothing redrawn while one shows but its one timer.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AppContext as _, Context, IntoElement, Modifiers, ParentElement as _, Render, Styled as _,
    TestAppContext, VisualTestContext, Window, div, px, size,
};
use gpui_core::{Root, TOAST_DURATION, TOAST_DURATION_LONG, Toast};

struct Page {
    draws: Rc<Cell<usize>>,
}

impl Render for Page {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        div().size_full().child("A page")
    }
}

fn open(cx: &mut TestAppContext) -> (Rc<Cell<usize>>, &mut VisualTestContext) {
    cx.update(gpui_core::init);
    let draws = Rc::new(Cell::new(0));
    let d = draws.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let page = cx.new(|_| Page { draws: d });
        Root::new(page, window, cx)
    });
    vcx.simulate_resize(size(px(800.), px(600.)));
    vcx.run_until_parked();
    (draws, vcx)
}

fn show(vcx: &mut VisualTestContext, toast: Toast) {
    vcx.update(|window, cx| toast.show(window, cx));
    vcx.run_until_parked();
}

fn wait(vcx: &mut VisualTestContext, d: Duration) {
    vcx.executor().advance_clock(d);
    vcx.run_until_parked();
}

#[gpui::test]
fn a_toast_shows_at_the_bottom_and_goes_by_itself(cx: &mut TestAppContext) {
    let (_, vcx) = open(cx);
    show(vcx, Toast::success("Saved.").id("saved"));
    let b = vcx.debug_bounds("toast-saved").expect("shown");
    assert!(
        b.bottom() > px(540.) && b.bottom() <= px(600.),
        "at the bottom: {b:?}"
    );
    assert!(
        (f32::from(b.center().x) - 400.).abs() < 2.,
        "in the middle: {b:?}"
    );
    wait(vcx, TOAST_DURATION - Duration::from_millis(100));
    assert!(vcx.debug_bounds("toast-saved").is_some(), "not yet");
    wait(vcx, Duration::from_millis(200));
    assert!(vcx.debug_bounds("toast-saved").is_none(), "gone");
    // An error stays longer.
    show(vcx, Toast::error("It didn't save.").id("oops"));
    wait(vcx, TOAST_DURATION + Duration::from_millis(100));
    assert!(vcx.debug_bounds("toast-oops").is_some());
    wait(vcx, TOAST_DURATION_LONG);
    assert!(vcx.debug_bounds("toast-oops").is_none());
}

#[gpui::test]
fn one_with_the_same_id_replaces_it_and_at_most_three_show(cx: &mut TestAppContext) {
    let (_, vcx) = open(cx);
    show(vcx, Toast::new("Copied.").id("copy"));
    let first = vcx.debug_bounds("toast-copy").unwrap();
    show(vcx, Toast::new("Copied.").id("copy"));
    assert_eq!(
        vcx.debug_bounds("toast-copy").unwrap(),
        first,
        "one, where it was"
    );
    for id in ["a", "b", "c"] {
        show(vcx, Toast::new(format!("Toast {id}")).id(id));
    }
    assert!(vcx.debug_bounds("toast-copy").is_none(), "the oldest went");
    for id in ["toast-a", "toast-b", "toast-c"] {
        assert!(vcx.debug_bounds(id).is_some(), "{id}");
    }
    // The newest at the bottom.
    let (a, c) = (
        vcx.debug_bounds("toast-a").unwrap(),
        vcx.debug_bounds("toast-c").unwrap(),
    );
    assert!(c.top() > a.top());
}

#[gpui::test]
fn its_action_runs_and_closes_it_and_so_does_close(cx: &mut TestAppContext) {
    let (_, vcx) = open(cx);
    let undone = Rc::new(Cell::new(false));
    let u = undone.clone();
    show(
        vcx,
        Toast::new("Deleted the rule.")
            .id("deleted")
            .action("Undo", move |_, _| u.set(true)),
    );
    let action = vcx.debug_bounds("toast-deleted").unwrap();
    // The action is the button towards the right; press along the toast until it's run.
    let mut x = action.right() - px(20.);
    while !undone.get() && x > action.left() {
        vcx.simulate_click(gpui::point(x, action.center().y), Modifiers::none());
        vcx.run_until_parked();
        x -= px(8.);
    }
    assert!(undone.get(), "the action ran");
    assert!(vcx.debug_bounds("toast-deleted").is_none(), "and closed it");
    show(vcx, Toast::new("Hello.").id("hi"));
    let close = vcx.debug_bounds("toast-close-2").unwrap().center();
    vcx.simulate_click(close, Modifiers::none());
    vcx.run_until_parked();
    assert!(vcx.debug_bounds("toast-hi").is_none());
}

#[gpui::test]
fn nothing_redraws_while_a_toast_shows_but_its_timer(cx: &mut TestAppContext) {
    let (draws, vcx) = open(cx);
    show(vcx, Toast::new("Saved.").id("saved"));
    let before = draws.get();
    wait(vcx, Duration::from_secs(1));
    wait(vcx, Duration::from_secs(1));
    assert_eq!(draws.get(), before, "idle while it shows");
}
