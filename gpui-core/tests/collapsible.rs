//! Collapsible: kept by itself it opens and closes on a press, Enter or Space and starts as
//! told; kept by the host it changes only when the host passes the new state; disabled it
//! ignores presses; its label trigger is 44 points tall for touch and cuts a long label short
//! in a narrow box; and once it has changed nothing redraws.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, ParentElement as _, Render, Styled as _,
    TestAppContext, VisualTestContext, Window, div, px,
};
use gpui_core::{Collapsible, Disableable as _, Press, Root};

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// The collapsible keeps the state, starting open or not.
    Kept(bool),
    /// The host keeps it; it obeys `on_toggle` or ignores it.
    Host { obeys: bool },
    /// The host's own trigger, made for the state.
    HostTrigger,
}

struct Panel {
    kind: Kind,
    open: bool,
    disabled: bool,
    touch: bool,
    width: f32,
    label: &'static str,
    told: Rc<RefCell<Vec<bool>>>,
    draws: Rc<Cell<usize>>,
    before: FocusHandle,
}

impl Render for Panel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let told = self.told.clone();
        let obeys = matches!(self.kind, Kind::Host { obeys: true });
        let mut c = Collapsible::new("c")
            .disabled(self.disabled)
            .touch(self.touch)
            .on_toggle(cx.listener(move |this, open: &bool, _, cx| {
                told.borrow_mut().push(*open);
                if obeys {
                    this.open = *open;
                    cx.notify();
                }
            }))
            .child(div().debug_selector(|| "content".into()).child("Content"));
        c = match self.kind {
            Kind::Kept(open) => c.label(self.label).default_open(open),
            Kind::Host { .. } => c.label(self.label).open(self.open),
            Kind::HostTrigger => c.trigger_with(|open| {
                Press::new("mine")
                    .debug_selector(|| "collapsible-trigger".into())
                    .child(if open { "Hide" } else { "Show" })
            }),
        };
        div()
            .p(px(20.))
            .child(
                Press::new("before")
                    .track_focus(&self.before)
                    .child("Before"),
            )
            .child(
                div()
                    .w(px(self.width))
                    .debug_selector(|| "box".into())
                    .child(c),
            )
    }
}

struct Opened<'a> {
    panel: Entity<Panel>,
    told: Rc<RefCell<Vec<bool>>>,
    draws: Rc<Cell<usize>>,
    vcx: &'a mut VisualTestContext,
}

fn open_with(cx: &mut TestAppContext, kind: Kind, setup: impl FnOnce(&mut Panel)) -> Opened<'_> {
    cx.update(gpui_core::init);
    let told = Rc::new(RefCell::new(Vec::new()));
    let draws = Rc::new(Cell::new(0));
    let (t, d) = (told.clone(), draws.clone());
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let panel = cx.new(|cx| {
            let mut p = Panel {
                kind,
                open: false,
                disabled: false,
                touch: false,
                width: 300.,
                label: "Details",
                told: t,
                draws: d,
                before: cx.focus_handle(),
            };
            setup(&mut p);
            p
        });
        *m.borrow_mut() = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let panel = made.borrow().clone().unwrap();
    Opened {
        panel,
        told,
        draws,
        vcx,
    }
}

fn open(cx: &mut TestAppContext, kind: Kind) -> Opened<'_> {
    open_with(cx, kind, |_| {})
}

fn press(vcx: &mut VisualTestContext) {
    let at = vcx
        .debug_bounds("collapsible-trigger")
        .expect("a trigger")
        .center();
    vcx.simulate_click(at, Modifiers::none());
    vcx.run_until_parked();
}

fn shown(vcx: &mut VisualTestContext) -> bool {
    vcx.debug_bounds("content").is_some()
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

#[gpui::test]
fn kept_by_itself_a_press_opens_and_closes_it(cx: &mut TestAppContext) {
    let o = open(cx, Kind::Kept(false));
    assert!(!shown(o.vcx), "closed content isn't drawn");
    press(o.vcx);
    assert!(shown(o.vcx));
    press(o.vcx);
    assert!(!shown(o.vcx));
    assert_eq!(*o.told.borrow(), [true, false], "and the host is told");
}

#[gpui::test]
fn it_starts_open_when_told_to(cx: &mut TestAppContext) {
    let o = open(cx, Kind::Kept(true));
    assert!(shown(o.vcx));
    press(o.vcx);
    assert!(!shown(o.vcx));
}

#[gpui::test]
fn kept_by_the_host_it_changes_only_when_the_host_says(cx: &mut TestAppContext) {
    {
        let o = open(cx, Kind::Host { obeys: false });
        press(o.vcx);
        assert_eq!(*o.told.borrow(), [true]);
        assert!(!shown(o.vcx), "the host didn't open it");
    }
    let o = open(cx, Kind::Host { obeys: true });
    press(o.vcx);
    assert!(shown(o.vcx));
    o.panel.update(o.vcx, |p, cx| {
        p.open = false;
        cx.notify();
    });
    o.vcx.run_until_parked();
    assert!(!shown(o.vcx), "the host closed it");
}

#[gpui::test]
fn enter_and_space_toggle_it(cx: &mut TestAppContext) {
    let o = open(cx, Kind::Kept(false));
    let before = o.panel.read_with(o.vcx, |p, _| p.before.clone());
    o.vcx.update(|window, cx| window.focus(&before, cx));
    keys(o.vcx, "tab enter");
    assert!(shown(o.vcx));
    keys(o.vcx, "space");
    assert!(!shown(o.vcx));
}

#[gpui::test]
fn a_host_trigger_is_made_for_the_state(cx: &mut TestAppContext) {
    let o = open(cx, Kind::HostTrigger);
    press(o.vcx);
    assert!(shown(o.vcx));
    assert_eq!(*o.told.borrow(), [true]);
}

#[gpui::test]
fn disabled_it_ignores_presses(cx: &mut TestAppContext) {
    let o = open_with(cx, Kind::Kept(false), |p| p.disabled = true);
    press(o.vcx);
    assert!(!shown(o.vcx));
    assert!(o.told.borrow().is_empty());
}

#[gpui::test]
fn the_touch_trigger_is_44_points_tall(cx: &mut TestAppContext) {
    let o = open_with(cx, Kind::Kept(false), |p| p.touch = true);
    let b = o.vcx.debug_bounds("collapsible-trigger").unwrap();
    assert!(b.size.height >= px(44.), "{b:?}");
}

#[gpui::test]
fn a_narrow_box_keeps_the_trigger_inside_it(cx: &mut TestAppContext) {
    let o = open_with(cx, Kind::Kept(true), |p| {
        p.width = 60.;
        p.label = "A label far too long for the box it's in, with nowhere to go";
    });
    let b = o.vcx.debug_bounds("box").unwrap();
    let t = o.vcx.debug_bounds("collapsible-trigger").unwrap();
    assert!(t.right() <= b.right() + px(0.5), "{t:?} in {b:?}");
}

/// Delivers one display frame and says how many things asked for it.
fn frame(vcx: &mut VisualTestContext) -> usize {
    vcx.executor().advance_clock(Duration::from_millis(16));
    let asked = vcx.update(|window, cx| window.simulate_next_frame(cx));
    vcx.run_until_parked();
    asked
}

#[gpui::test]
fn once_it_has_changed_nothing_redraws(cx: &mut TestAppContext) {
    let o = open(cx, Kind::Kept(false));
    press(o.vcx);
    frame(o.vcx);
    let settled = o.draws.get();
    for _ in 0..30 {
        assert_eq!(frame(o.vcx), 0, "asked for a frame while idle");
    }
    assert_eq!(o.draws.get(), settled);
}
