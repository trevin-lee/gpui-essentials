//! Toggle and ToggleGroup: a press asks for the opposite state, Space and Enter work from the
//! keyboard, disabled it takes nothing, it's announced as pressed or not; a one-choice group
//! presses one (and lets it out again), a several-choice one adds and takes away; a group is
//! one Tab stop whose arrows, Home and End move the keyboard (going round, passing over a
//! disabled toggle) without pressing anything; touch toggles are 44 points; a narrow space cuts
//! the labels short; and nothing redraws while nothing changes.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, ParentElement as _, Render, RenderOnce,
    SharedString, Styled as _, TestAppContext, Toggled, VisualTestContext, Window, div, px,
};
use gpui_core::{Disableable as _, IconName, Press, Root, Sizable as _, Toggle, ToggleGroup};

#[derive(Clone, Copy)]
struct Setup {
    multiple: bool,
    touch: bool,
    disabled: bool,
    width: f32,
}

const PLAIN: Setup = Setup {
    multiple: false,
    touch: false,
    disabled: false,
    width: 400.,
};

struct Host {
    s: Setup,
    bold: bool,
    picked: Vec<SharedString>,
    asked: Rc<RefCell<Vec<bool>>>,
    changes: Rc<RefCell<Vec<Vec<SharedString>>>>,
    draws: Rc<Cell<usize>>,
    before: FocusHandle,
    after: FocusHandle,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let asked = self.asked.clone();
        let changes = self.changes.clone();
        let mut bold = Toggle::new("bold")
            .icon(IconName::Italic)
            .accessibility_label("Italic")
            .pressed(self.bold)
            .disabled(self.s.disabled)
            .on_click(cx.listener(move |this, on: &bool, _, cx| {
                asked.borrow_mut().push(*on);
                this.bold = *on;
                cx.notify();
            }));
        let mut group = ToggleGroup::new("align")
            .accessibility_label("Alignment")
            .item("start", "Left aligned")
            .item("center", "Centred")
            .item("off", "Off")
            .item_with_icon("end", "Right aligned", IconName::TextAlignEnd)
            .disable("off")
            .multiple(self.s.multiple)
            .disabled(self.s.disabled)
            .selected(self.picked.clone())
            .on_change(cx.listener(move |this, ids: &Vec<SharedString>, _, cx| {
                changes.borrow_mut().push(ids.clone());
                this.picked = ids.clone();
                cx.notify();
            }));
        if self.s.touch {
            bold = bold.touch();
            group = group.touch();
        }
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
            .child(bold)
            .child(
                div()
                    .w(px(self.s.width))
                    .debug_selector(|| "box".into())
                    .child(group),
            )
            .child(Press::new("after").track_focus(&self.after).child("After"))
    }
}

fn open(cx: &mut TestAppContext, s: Setup) -> (Entity<Host>, &mut VisualTestContext) {
    cx.update(gpui_core::init);
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let h = cx.new(|cx| Host {
            s,
            bold: false,
            picked: vec!["center".into()],
            asked: Rc::default(),
            changes: Rc::default(),
            draws: Rc::default(),
            before: cx.focus_handle(),
            after: cx.focus_handle(),
        });
        *m.borrow_mut() = Some(h.clone());
        Root::new(h, window, cx)
    });
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let h = made.borrow().clone().unwrap();
    (h, vcx)
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
        vcx.run_until_parked();
    }
}

fn click(vcx: &mut VisualTestContext, selector: &'static str) {
    let at = vcx.debug_bounds(selector).unwrap().center();
    vcx.simulate_click(at, Modifiers::none());
    vcx.run_until_parked();
}

fn picked(h: &Entity<Host>, vcx: &mut VisualTestContext) -> Vec<String> {
    h.read_with(vcx, |h, _| h.picked.iter().map(|s| s.to_string()).collect())
}

fn focus_before(h: &Entity<Host>, vcx: &mut VisualTestContext) {
    let before = h.read_with(vcx, |h, _| h.before.clone());
    vcx.update(|window, cx| window.focus(&before, cx));
}

#[gpui::test]
fn a_toggle_asks_for_the_opposite_by_press_space_and_enter(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, PLAIN);
    click(vcx, "toggle-bold");
    click(vcx, "toggle-bold");
    focus_before(&h, vcx);
    keys(vcx, "tab space enter");
    assert_eq!(
        *h.read_with(vcx, |h, _| h.asked.clone()).borrow(),
        [true, false, true, false]
    );
}

#[gpui::test]
fn disabled_nothing_takes_a_press(cx: &mut TestAppContext) {
    let (h, vcx) = open(
        cx,
        Setup {
            disabled: true,
            ..PLAIN
        },
    );
    click(vcx, "toggle-bold");
    click(vcx, "toggle-toggle-item-start");
    focus_before(&h, vcx);
    keys(vcx, "tab space");
    assert!(h.read_with(vcx, |h, _| h.asked.borrow().is_empty()));
    assert!(h.read_with(vcx, |h, _| h.changes.borrow().is_empty()));
    let after = h.read_with(vcx, |h, _| h.after.clone());
    assert!(vcx.update(|window, _| after.is_focused(window)));
}

#[gpui::test]
fn one_choice_presses_one_and_lets_it_out_again(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, PLAIN);
    click(vcx, "toggle-toggle-item-start");
    assert_eq!(picked(&h, vcx), ["start"]);
    click(vcx, "toggle-toggle-item-start");
    assert!(picked(&h, vcx).is_empty());
    click(vcx, "toggle-toggle-item-off");
    assert!(picked(&h, vcx).is_empty(), "a disabled toggle");
}

#[gpui::test]
fn several_choice_adds_and_takes_away(cx: &mut TestAppContext) {
    let (h, vcx) = open(
        cx,
        Setup {
            multiple: true,
            ..PLAIN
        },
    );
    click(vcx, "toggle-toggle-item-end");
    assert_eq!(picked(&h, vcx), ["center", "end"]);
    click(vcx, "toggle-toggle-item-start");
    assert_eq!(picked(&h, vcx), ["start", "center", "end"]);
    click(vcx, "toggle-toggle-item-center");
    assert_eq!(picked(&h, vcx), ["start", "end"]);
}

#[gpui::test]
fn a_group_is_one_tab_stop_and_the_arrows_move_without_pressing(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, PLAIN);
    focus_before(&h, vcx);
    // Before, then the Italic toggle, then the group on its pressed toggle (Centred).
    keys(vcx, "tab tab right");
    assert_eq!(picked(&h, vcx), ["center"], "an arrow pressed something");
    // Off is disabled: Right went past it to Right aligned. Space presses it.
    keys(vcx, "space");
    assert_eq!(picked(&h, vcx), ["end"]);
    // Round from the last to the first, and from the first to the last.
    keys(vcx, "right space");
    assert_eq!(picked(&h, vcx), ["start"]);
    keys(vcx, "left space");
    assert_eq!(picked(&h, vcx), ["end"]);
    keys(vcx, "home enter");
    assert_eq!(picked(&h, vcx), ["start"]);
    // Tab leaves the group, and Shift-Tab comes back to where the keyboard was.
    keys(vcx, "tab");
    let after = h.read_with(vcx, |h, _| h.after.clone());
    assert!(vcx.update(|window, _| after.is_focused(window)));
    keys(vcx, "shift-tab end shift-tab tab space");
    assert_eq!(
        picked(&h, vcx),
        ["end"],
        "Tab came back to the last one moved to"
    );
}

#[gpui::test]
fn its_announced_as_pressed(cx: &mut TestAppContext) {
    struct Probe(Rc<RefCell<Vec<gpui::accesskit::Node>>>);
    impl Render for Probe {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            for (pressed, multiple) in [(true, false), (false, false), (true, true)] {
                let el = RenderOnce::render(
                    Toggle::new("t").icon(IconName::Italic).pressed(pressed),
                    window,
                    cx,
                )
                .into_element();
                let mut n = gpui::accesskit::Node::new(gpui::Element::a11y_role(&el).unwrap());
                gpui::Element::write_a11y_info(&el, &mut n);
                self.0.borrow_mut().push(n);
                let el = RenderOnce::render(
                    ToggleGroup::new("g").item("a", "A").multiple(multiple),
                    window,
                    cx,
                )
                .into_element();
                let mut n = gpui::accesskit::Node::new(gpui::Element::a11y_role(&el).unwrap());
                gpui::Element::write_a11y_info(&el, &mut n);
                self.0.borrow_mut().push(n);
            }
            div()
        }
    }
    cx.update(gpui_core::init);
    let out = Rc::new(RefCell::new(Vec::new()));
    let o = out.clone();
    let (_, vcx) = cx.add_window_view(move |_, _| Probe(o));
    vcx.run_until_parked();
    let nodes = out.borrow();
    assert_eq!(nodes[0].role(), gpui::Role::Button);
    assert_eq!(nodes[0].toggled(), Some(Toggled::True));
    assert_eq!(nodes[2].toggled(), Some(Toggled::False));
    assert_eq!(nodes[1].role(), gpui::Role::RadioGroup, "one choice");
    assert_eq!(nodes[5].role(), gpui::Role::Group, "several");
}

#[gpui::test]
fn touch_toggles_are_44_points(cx: &mut TestAppContext) {
    let (_, vcx) = open(
        cx,
        Setup {
            touch: true,
            ..PLAIN
        },
    );
    let b = vcx.debug_bounds("toggle-bold").unwrap();
    assert!(b.size.height >= px(44.) && b.size.width >= px(44.), "{b:?}");
    let g = vcx.debug_bounds("toggle-toggle-item-start").unwrap();
    assert!(g.size.height >= px(44.), "{g:?}");
}

#[gpui::test]
fn a_narrow_space_cuts_the_labels_short(cx: &mut TestAppContext) {
    let (_, vcx) = open(
        cx,
        Setup {
            width: 160.,
            ..PLAIN
        },
    );
    let b = vcx.debug_bounds("box").unwrap();
    for id in [
        "toggle-toggle-item-start",
        "toggle-toggle-item-center",
        "toggle-toggle-item-end",
    ] {
        let t = vcx.debug_bounds(id).unwrap();
        assert!(t.right() <= b.right() + px(1.), "{id} {t:?} in {b:?}");
    }
}

fn frame(vcx: &mut VisualTestContext) -> usize {
    vcx.executor().advance_clock(Duration::from_millis(16));
    let asked = vcx.update(|window, cx| window.simulate_next_frame(cx));
    vcx.run_until_parked();
    asked
}

#[gpui::test]
fn nothing_redraws_while_nothing_changes(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, PLAIN);
    click(vcx, "toggle-toggle-item-start");
    click(vcx, "toggle-bold");
    let settled = h.read_with(vcx, |h, _| h.draws.get());
    for _ in 0..30 {
        assert_eq!(frame(vcx), 0, "asked for a frame while idle");
    }
    assert_eq!(h.read_with(vcx, |h, _| h.draws.get()), settled);
}
