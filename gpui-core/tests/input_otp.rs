//! InputOtp: typing fills the slots in turn (only digits by default), Backspace empties the
//! last, a paste fills them all with what it takes of the clipboard, the code is told complete
//! once per code (again after an edit), groups have a dash between them, a press on it gives it
//! the keyboard, Tab reaches it, touch slots are 44 points, a narrow box keeps the slots inside,
//! and nothing redraws while nothing changes.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AppContext as _, ClipboardItem, Context, Entity, FocusHandle, InteractiveElement as _,
    IntoElement, Modifiers, ParentElement as _, Render, SharedString, Styled as _, TestAppContext,
    VisualTestContext, Window, div, px,
};
use gpui_core::{InputOtp, InputOtpEvent, InputOtpState, Press, Root, Sizable as _};

#[derive(Clone, Copy)]
struct Setup {
    touch: bool,
    width: f32,
    letters: bool,
}

const PLAIN: Setup = Setup {
    touch: false,
    width: 400.,
    letters: false,
};

struct Host {
    s: Setup,
    code: Entity<InputOtpState>,
    completed: Rc<RefCell<Vec<SharedString>>>,
    events: Rc<RefCell<Vec<InputOtpEvent>>>,
    draws: Rc<Cell<usize>>,
    before: FocusHandle,
    _sub: gpui::Subscription,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let completed = self.completed.clone();
        let mut otp = InputOtp::new(&self.code)
            .groups([3, 3])
            .on_complete(move |code, _, _| completed.borrow_mut().push(code.clone()));
        if self.s.touch {
            otp = otp.touch();
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
            .child(
                div()
                    .w(px(self.s.width))
                    .debug_selector(|| "box".into())
                    .child(otp),
            )
    }
}

fn open(cx: &mut TestAppContext, s: Setup) -> (Entity<Host>, &mut VisualTestContext) {
    cx.update(gpui_core::init);
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let code = cx.new(|cx| InputOtpState::new(window, cx).digits_only(!s.letters));
        let h = cx.new(|cx| {
            let events: Rc<RefCell<Vec<InputOtpEvent>>> = Rc::default();
            let e = events.clone();
            let sub = cx.subscribe(&code, move |_, _, ev: &InputOtpEvent, _| {
                e.borrow_mut().push(ev.clone())
            });
            Host {
                s,
                code,
                completed: Rc::default(),
                events,
                draws: Rc::default(),
                before: cx.focus_handle(),
                _sub: sub,
            }
        });
        *m.borrow_mut() = Some(h.clone());
        Root::new(h, window, cx)
    });
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let h = made.borrow().clone().unwrap();
    (h, vcx)
}

fn otp_id(h: &Entity<Host>, vcx: &mut VisualTestContext) -> u64 {
    h.read_with(vcx, |h, _| h.code.entity_id().as_u64())
}

fn bounds(vcx: &mut VisualTestContext, name: String) -> gpui::Bounds<gpui::Pixels> {
    let name: &'static str = Box::leak(name.into_boxed_str());
    vcx.debug_bounds(name)
        .unwrap_or_else(|| panic!("{name} drawn"))
}

/// A press on the field, which gives it the keyboard.
fn press(h: &Entity<Host>, vcx: &mut VisualTestContext) {
    let id = otp_id(h, vcx);
    let at = bounds(vcx, format!("otp-slot-{id}-2")).center();
    vcx.simulate_click(at, Modifiers::none());
    vcx.run_until_parked();
}

fn value(h: &Entity<Host>, vcx: &mut VisualTestContext) -> String {
    h.read_with(vcx, |h, cx| h.code.read(cx).value(cx).to_string())
}

fn completed(h: &Entity<Host>, vcx: &mut VisualTestContext) -> Vec<String> {
    h.read_with(vcx, |h, _| {
        h.completed.borrow().iter().map(|c| c.to_string()).collect()
    })
}

#[gpui::test]
fn typing_fills_the_slots_with_digits_only(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, PLAIN);
    press(&h, vcx);
    vcx.simulate_input("1a2 3");
    vcx.run_until_parked();
    assert_eq!(value(&h, vcx), "123");
    vcx.simulate_keystrokes("backspace");
    vcx.run_until_parked();
    assert_eq!(value(&h, vcx), "12");
    assert!(completed(&h, vcx).is_empty());
}

#[gpui::test]
fn the_code_is_told_complete_once_per_code(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, PLAIN);
    press(&h, vcx);
    vcx.simulate_input("123456");
    vcx.run_until_parked();
    assert_eq!(completed(&h, vcx), ["123456"]);
    // Past the last slot, typing does nothing.
    vcx.simulate_input("7");
    vcx.run_until_parked();
    assert_eq!(value(&h, vcx), "123456");
    assert_eq!(completed(&h, vcx), ["123456"]);
    // An edit and the same code again tells it again.
    vcx.simulate_keystrokes("backspace");
    vcx.simulate_input("6");
    vcx.run_until_parked();
    assert_eq!(completed(&h, vcx), ["123456", "123456"]);
    let events = h.read_with(vcx, |h, _| h.events.borrow().clone());
    assert!(events.contains(&InputOtpEvent::Complete("123456".into())));
    assert!(events.contains(&InputOtpEvent::Change("12".into())));
}

#[gpui::test]
fn a_paste_fills_every_slot(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, PLAIN);
    press(&h, vcx);
    vcx.simulate_input("9");
    vcx.write_to_clipboard(ClipboardItem::new_string("Your code: 123-456.".into()));
    // The platform's paste: ⌘V on Apple's, Ctrl-V elsewhere (as Input binds it).
    vcx.simulate_keystrokes(if cfg!(any(target_os = "macos", target_os = "ios")) {
        "cmd-v"
    } else {
        "ctrl-v"
    });
    vcx.run_until_parked();
    assert_eq!(value(&h, vcx), "123456", "the paste replaced the code");
    assert_eq!(completed(&h, vcx), ["123456"]);
}

#[gpui::test]
fn letters_when_told(cx: &mut TestAppContext) {
    let (h, vcx) = open(
        cx,
        Setup {
            letters: true,
            ..PLAIN
        },
    );
    press(&h, vcx);
    vcx.simulate_input("ab 1C");
    vcx.run_until_parked();
    assert_eq!(value(&h, vcx), "ab1C");
}

#[gpui::test]
fn tab_reaches_it_and_its_groups_have_a_dash_between(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, PLAIN);
    let before = h.read_with(vcx, |h, _| h.before.clone());
    vcx.update(|window, cx| window.focus(&before, cx));
    vcx.simulate_keystrokes("tab");
    vcx.simulate_input("42");
    vcx.run_until_parked();
    assert_eq!(value(&h, vcx), "42");
    let id = otp_id(&h, vcx);
    let third = bounds(vcx, format!("otp-slot-{id}-2"));
    let dash = bounds(vcx, format!("otp-separator-{id}-1"));
    let fourth = bounds(vcx, format!("otp-slot-{id}-3"));
    assert!(third.right() <= dash.left() && dash.right() <= fourth.left());
}

#[gpui::test]
fn touch_slots_are_44_points(cx: &mut TestAppContext) {
    let (h, vcx) = open(
        cx,
        Setup {
            touch: true,
            ..PLAIN
        },
    );
    let id = otp_id(&h, vcx);
    let b = bounds(vcx, format!("otp-slot-{id}-0"));
    assert!(b.size.height >= px(44.) && b.size.width >= px(44.), "{b:?}");
}

#[gpui::test]
fn a_narrow_box_keeps_the_slots_inside(cx: &mut TestAppContext) {
    let (h, vcx) = open(
        cx,
        Setup {
            width: 180.,
            ..PLAIN
        },
    );
    let id = otp_id(&h, vcx);
    let outer = vcx.debug_bounds("box").unwrap();
    let last = bounds(vcx, format!("otp-slot-{id}-5"));
    assert!(
        last.right() <= outer.right() + px(1.),
        "{last:?} in {outer:?}"
    );
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
    press(&h, vcx);
    vcx.simulate_input("12");
    vcx.run_until_parked();
    let settled = h.read_with(vcx, |h, _| h.draws.get());
    for _ in 0..30 {
        assert_eq!(frame(vcx), 0, "asked for a frame while idle");
    }
    assert_eq!(h.read_with(vcx, |h, _| h.draws.get()), settled);
}
