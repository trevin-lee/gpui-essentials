//! HoverCard: shown after the pointer rests on its trigger (not before), kept while the pointer
//! crosses to the card, hidden a moment after it leaves both; shown at once while the trigger
//! has the focus (which stays there) and hidden by Escape; and nothing redrawn while idle.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, ParentElement as _, Pixels, Point, Render,
    Styled as _, TestAppContext, VisualTestContext, Window, div, point, px, size,
};
use gpui_core::{HOVER_CARD_CLOSE_DELAY, HOVER_CARD_OPEN_DELAY, HoverCard, Press, Root};

struct Panel {
    trigger: FocusHandle,
    after: FocusHandle,
    escapes: Rc<Cell<usize>>,
    draws: Rc<Cell<usize>>,
}

impl Render for Panel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let escapes = self.escapes.clone();
        div()
            .size_full()
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .key_context("Host")
            .on_key_down(move |e, _, _| {
                if e.keystroke.key == "escape" {
                    escapes.set(escapes.get() + 1);
                }
            })
            .child(
                HoverCard::new("card")
                    .trigger(
                        Press::new("trigger")
                            .track_focus(&self.trigger)
                            .debug_selector(|| "trigger".into())
                            .w(px(80.))
                            .h(px(20.))
                            .child("@nextjs"),
                    )
                    .child("The React Framework"),
            )
            .child(
                Press::new("after")
                    .track_focus(&self.after)
                    .debug_selector(|| "after".into())
                    .w(px(80.))
                    .h(px(20.))
                    .child("After"),
            )
    }
}

struct Opened<'a> {
    panel: Entity<Panel>,
    escapes: Rc<Cell<usize>>,
    draws: Rc<Cell<usize>>,
    vcx: &'a mut VisualTestContext,
}

fn open(cx: &mut TestAppContext) -> Opened<'_> {
    cx.update(gpui_core::init);
    let escapes = Rc::new(Cell::new(0));
    let draws = Rc::new(Cell::new(0));
    let (e, d) = (escapes.clone(), draws.clone());
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let panel = cx.new(|cx| Panel {
            trigger: cx.focus_handle(),
            after: cx.focus_handle(),
            escapes: e,
            draws: d,
        });
        *m.borrow_mut() = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    vcx.simulate_resize(size(px(500.), px(400.)));
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let panel = made.borrow().clone().unwrap();
    Opened {
        panel,
        escapes,
        draws,
        vcx,
    }
}

fn card(vcx: &mut VisualTestContext) -> bool {
    vcx.debug_bounds("gpui-core-hover-card").is_some()
}

fn center(vcx: &mut VisualTestContext, selector: &'static str) -> Point<Pixels> {
    vcx.debug_bounds(selector).unwrap().center()
}

fn move_to(vcx: &mut VisualTestContext, at: Point<Pixels>) {
    vcx.simulate_mouse_move(at, None, Modifiers::none());
    vcx.run_until_parked();
}

fn wait(vcx: &mut VisualTestContext, d: Duration) {
    vcx.executor().advance_clock(d);
    vcx.run_until_parked();
}

fn key(vcx: &mut VisualTestContext, keys: &'static str) {
    for k in keys.split(' ') {
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

const NOWHERE: Point<Pixels> = point(px(450.), px(350.));
const MS: Duration = Duration::from_millis(1);

#[gpui::test]
fn it_shows_after_the_pointer_rests_and_hides_after_it_leaves(cx: &mut TestAppContext) {
    let o = open(cx);
    let at = center(o.vcx, "trigger");
    move_to(o.vcx, at);
    wait(o.vcx, HOVER_CARD_OPEN_DELAY - MS * 50);
    assert!(!card(o.vcx), "not before the delay");
    wait(o.vcx, MS * 60);
    assert!(card(o.vcx), "after it");
    move_to(o.vcx, NOWHERE);
    wait(o.vcx, HOVER_CARD_CLOSE_DELAY - MS * 50);
    assert!(card(o.vcx), "still, for a moment");
    wait(o.vcx, MS * 60);
    assert!(!card(o.vcx));
}

#[gpui::test]
fn passing_over_it_shows_nothing(cx: &mut TestAppContext) {
    let o = open(cx);
    let at = center(o.vcx, "trigger");
    move_to(o.vcx, at);
    wait(o.vcx, HOVER_CARD_OPEN_DELAY / 2);
    move_to(o.vcx, NOWHERE);
    wait(o.vcx, HOVER_CARD_OPEN_DELAY * 2);
    assert!(!card(o.vcx));
}

#[gpui::test]
fn the_pointer_can_cross_to_the_card(cx: &mut TestAppContext) {
    let o = open(cx);
    let at = center(o.vcx, "trigger");
    move_to(o.vcx, at);
    wait(o.vcx, HOVER_CARD_OPEN_DELAY + MS);
    let on_card = center(o.vcx, "gpui-core-hover-card");
    move_to(o.vcx, on_card);
    wait(o.vcx, HOVER_CARD_CLOSE_DELAY * 3);
    assert!(card(o.vcx), "kept while the pointer is on the card");
    move_to(o.vcx, NOWHERE);
    wait(o.vcx, HOVER_CARD_CLOSE_DELAY + MS);
    assert!(!card(o.vcx));
}

#[gpui::test]
fn the_focus_shows_it_at_once_and_escape_hides_it(cx: &mut TestAppContext) {
    let o = open(cx);
    key(o.vcx, "tab");
    assert!(card(o.vcx), "the focus shows it, with no delay");
    let trigger = o.panel.read_with(o.vcx, |p, _| p.trigger.clone());
    assert!(
        o.vcx.update(|w, _| trigger.is_focused(w)),
        "the trigger keeps the focus"
    );
    key(o.vcx, "escape");
    assert!(!card(o.vcx));
    assert_eq!(o.escapes.get(), 0, "its Escape, not the host's");
    // With it hidden, Escape is the host's again.
    key(o.vcx, "escape");
    assert_eq!(o.escapes.get(), 1);
    // The focus leaving and coming back shows it again.
    key(o.vcx, "tab");
    assert!(!card(o.vcx));
    key(o.vcx, "shift-tab");
    assert!(card(o.vcx));
}

#[gpui::test]
fn shown_and_idle_it_doesnt_redraw(cx: &mut TestAppContext) {
    let o = open(cx);
    let at = center(o.vcx, "trigger");
    move_to(o.vcx, at);
    wait(o.vcx, HOVER_CARD_OPEN_DELAY + MS);
    assert!(card(o.vcx));
    let before = o.draws.get();
    for _ in 0..30 {
        o.vcx.update(|window, cx| window.simulate_next_frame(cx));
        wait(o.vcx, MS * 16);
    }
    assert_eq!(o.draws.get(), before);
}
