//! SegmentedControl: a press turns an option on (not a disabled one), Tab stops once (on the
//! option that's on), the arrows, Home and End move along and turn on what they reach (passing
//! over a disabled one, stopping at the ends), touch options are 44 points, and a narrow space
//! cuts the labels short rather than running out of it.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, ParentElement as _, Render, SharedString,
    Styled as _, TestAppContext, VisualTestContext, Window, div, px,
};
use gpui_core::{Press, Root, SegmentedControl};

struct Host {
    theme: SharedString,
    touch: bool,
    width: f32,
    changes: Rc<RefCell<Vec<SharedString>>>,
    before: FocusHandle,
    after: FocusHandle,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let changes = self.changes.clone();
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
            .child(
                div()
                    .w(px(self.width))
                    .debug_selector(|| "box".into())
                    .child(
                        SegmentedControl::new("theme")
                            // The box's width, split between the options.
                            .w_full()
                            .accessibility_label("Theme")
                            .segment("system", "System")
                            .segment("light", "Light")
                            .segment("custom", "Custom")
                            .segment("dark", "Dark")
                            .disable("custom")
                            .selected(self.theme.clone())
                            .touch(self.touch)
                            .on_change(cx.listener(move |this, id: &SharedString, _, cx| {
                                changes.borrow_mut().push(id.clone());
                                this.theme = id.clone();
                                cx.notify();
                            })),
                    ),
            )
            .child(Press::new("after").track_focus(&self.after).child("After"))
    }
}

fn open(
    cx: &mut TestAppContext,
    touch: bool,
    width: f32,
) -> (Entity<Host>, &mut VisualTestContext) {
    cx.update(gpui_core::init);
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let h = cx.new(|cx| Host {
            theme: "light".into(),
            touch,
            width,
            changes: Rc::default(),
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

fn theme(h: &Entity<Host>, vcx: &mut VisualTestContext) -> String {
    h.read_with(vcx, |h, _| h.theme.to_string())
}

#[gpui::test]
fn a_press_turns_an_option_on_but_not_a_disabled_one(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, false, 400.);
    let dark = vcx.debug_bounds("segment-dark").unwrap().center();
    vcx.simulate_click(dark, Modifiers::none());
    vcx.run_until_parked();
    assert_eq!(theme(&h, vcx), "dark");
    let custom = vcx.debug_bounds("segment-custom").unwrap().center();
    vcx.simulate_click(custom, Modifiers::none());
    vcx.run_until_parked();
    assert_eq!(theme(&h, vcx), "dark");
    // A press on the one that's on tells the host nothing.
    vcx.simulate_click(dark, Modifiers::none());
    vcx.run_until_parked();
    assert_eq!(h.read_with(vcx, |h, _| h.changes.borrow().len()), 1);
}

#[gpui::test]
fn tab_stops_once_on_the_option_thats_on(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, false, 400.);
    let before = h.read_with(vcx, |h, _| h.before.clone());
    vcx.update(|window, cx| window.focus(&before, cx));
    keys(vcx, "tab");
    // On Light, the one that's on: the arrows work from it.
    keys(vcx, "right");
    assert_eq!(theme(&h, vcx), "dark", "Custom, disabled, passed over");
    // Tab again leaves the control.
    keys(vcx, "tab");
    let after = h.read_with(vcx, |h, _| h.after.clone());
    assert!(vcx.update(|window, _| after.is_focused(window)));
}

#[gpui::test]
fn the_arrows_home_and_end_move_and_stop_at_the_ends(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, false, 400.);
    let before = h.read_with(vcx, |h, _| h.before.clone());
    vcx.update(|window, cx| window.focus(&before, cx));
    keys(vcx, "tab left");
    assert_eq!(theme(&h, vcx), "system");
    keys(vcx, "left");
    assert_eq!(theme(&h, vcx), "system", "the first: no further");
    keys(vcx, "end");
    assert_eq!(theme(&h, vcx), "dark");
    keys(vcx, "down");
    assert_eq!(theme(&h, vcx), "dark", "the last: no further");
    keys(vcx, "home up");
    assert_eq!(theme(&h, vcx), "system");
    assert_eq!(
        h.read_with(vcx, |h, _| h.changes.borrow().len()),
        3,
        "only real moves are told"
    );
}

#[gpui::test]
fn touch_options_are_44_points(cx: &mut TestAppContext) {
    let (_, vcx) = open(cx, true, 400.);
    let b = vcx.debug_bounds("segment-light").unwrap();
    assert!(b.size.height >= px(38.), "{b:?}"); // 44 less the track's padding
}

#[gpui::test]
fn a_narrow_space_cuts_the_labels_short(cx: &mut TestAppContext) {
    let (_, vcx) = open(cx, false, 120.);
    let b = vcx.debug_bounds("box").unwrap();
    for id in ["segment-system", "segment-light", "segment-dark"] {
        let s = vcx.debug_bounds(id).unwrap();
        assert!(s.right() <= b.right() + px(1.), "{id} {s:?} in {b:?}");
    }
}
