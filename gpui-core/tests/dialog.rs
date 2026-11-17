//! Dialog and AlertDialog: centred over a scrim and inside the window however tall, the keyboard
//! taken on opening (by the first thing inside, or an alert's Cancel) and given back on closing,
//! Tab going round inside and never out, Escape, the scrim and the close button dismissing (not
//! the scrim of an alert), a popover in a dialog over it, and nothing redrawn while idle.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{
    AppContext as _, Bounds, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, ParentElement as _, Pixels, Render,
    Styled as _, TestAppContext, VisualTestContext, Window, div, point, px, size,
};
use gpui_core::{AlertDialog, Dialog, DismissReason, Popover, Press, Root};

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Dialog,
    Alert,
}

struct Panel {
    kind: Kind,
    open: bool,
    rows: usize,
    /// A popover inside the dialog, and whether it's open.
    inner_popover: Option<bool>,
    opener: FocusHandle,
    other: FocusHandle,
    /// In the dialog: a press before the footer, then the footer's two.
    first: FocusHandle,
    cancel: FocusHandle,
    ok: FocusHandle,
    reasons: Rc<RefCell<Vec<DismissReason>>>,
    pressed: Rc<RefCell<Vec<&'static str>>>,
    draws: Rc<Cell<usize>>,
}

fn press(id: &'static str, focus: &FocusHandle, pressed: &Rc<RefCell<Vec<&'static str>>>) -> Press {
    let pressed = pressed.clone();
    Press::new(id)
        .track_focus(focus)
        .debug_selector(move || id.into())
        .w(px(80.))
        .h(px(30.))
        .child(id)
        .on_click(move |_, _, _| pressed.borrow_mut().push(id))
}

impl Render for Panel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let reasons = self.reasons.clone();
        let on_dismiss = cx.listener(move |this, reason: &DismissReason, _, cx| {
            reasons.borrow_mut().push(*reason);
            this.open = false;
            cx.notify();
        });
        let rows = (0..self.rows).map(|i| div().h(px(40.)).child(format!("Row {i}")));
        let first = press("first", &self.first, &self.pressed);
        let cancel = press("cancel", &self.cancel, &self.pressed);
        let ok = press("ok", &self.ok, &self.pressed);
        let inner = self.inner_popover.map(|open| {
            Popover::new("inner")
                .open(open)
                .w(px(150.))
                .on_dismiss(cx.listener(|this, _, _, cx| {
                    this.inner_popover = Some(false);
                    cx.notify();
                }))
                .trigger(
                    Press::new("inner-trigger")
                        .debug_selector(|| "inner-trigger".into())
                        .w(px(80.))
                        .h(px(30.))
                        .child("More")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.inner_popover = Some(true);
                            cx.notify();
                        })),
                )
                .child(press("inner-item", &self.other, &self.pressed).w(px(150.)))
        });
        let dialog = match self.kind {
            Kind::Dialog => Dialog::new("dialog")
                .open(self.open)
                .title("Rename")
                .description("Pick a new name.")
                .on_dismiss(on_dismiss)
                .child(first)
                .children(inner)
                .children(rows)
                .footer(cancel)
                .footer(ok)
                .into_any_element(),
            Kind::Alert => AlertDialog::new("dialog")
                .open(self.open)
                .title("Delete it?")
                .description("This can't be undone.")
                .on_dismiss(on_dismiss)
                .child(first)
                .cancel(cancel)
                .action(ok)
                .into_any_element(),
        };
        div()
            .size_full()
            .p(px(10.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                Press::new("opener")
                    .track_focus(&self.opener)
                    .debug_selector(|| "opener".into())
                    .size(px(40.))
                    .child("Open")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.open = true;
                        cx.notify();
                    })),
            )
            .child(Press::new("behind").size(px(40.)).child("Behind"))
            .child(dialog)
    }
}

struct Opened<'a> {
    panel: Entity<Panel>,
    reasons: Rc<RefCell<Vec<DismissReason>>>,
    pressed: Rc<RefCell<Vec<&'static str>>>,
    draws: Rc<Cell<usize>>,
    vcx: &'a mut VisualTestContext,
}

fn open_with(
    cx: &mut TestAppContext,
    kind: Kind,
    width: f32,
    configure: impl FnOnce(&mut Panel) + 'static,
) -> Opened<'_> {
    cx.update(gpui_core::init);
    let reasons: Rc<RefCell<Vec<DismissReason>>> = Rc::default();
    let pressed: Rc<RefCell<Vec<&'static str>>> = Rc::default();
    let draws = Rc::new(Cell::new(0));
    let (r, p, d) = (reasons.clone(), pressed.clone(), draws.clone());
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let panel = cx.new(|cx| {
            let mut panel = Panel {
                kind,
                open: false,
                rows: 2,
                inner_popover: None,
                opener: cx.focus_handle(),
                other: cx.focus_handle(),
                first: cx.focus_handle(),
                cancel: cx.focus_handle(),
                ok: cx.focus_handle(),
                reasons: r,
                pressed: p,
                draws: d,
            };
            configure(&mut panel);
            panel
        });
        *m.borrow_mut() = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    vcx.simulate_resize(size(px(width), px(500.)));
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let panel = made.borrow().clone().unwrap();
    Opened {
        panel,
        reasons,
        pressed,
        draws,
        vcx,
    }
}

fn open(cx: &mut TestAppContext, kind: Kind) -> Opened<'_> {
    open_with(cx, kind, 800., |_| {})
}

fn dialog(vcx: &mut VisualTestContext) -> Option<Bounds<Pixels>> {
    vcx.debug_bounds("gpui-core-dialog")
}

fn bounds(vcx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    vcx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} drawn"))
}

fn click(vcx: &mut VisualTestContext, at: gpui::Point<Pixels>) {
    vcx.simulate_click(at, Modifiers::none());
    vcx.run_until_parked();
}

fn press_opener(o: &mut Opened<'_>) {
    let at = bounds(o.vcx, "opener").center();
    click(o.vcx, at);
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

fn focused(o: &mut Opened<'_>, pick: impl Fn(&Panel) -> &FocusHandle) -> bool {
    let handle = o.panel.read_with(o.vcx, |p, _| pick(p).clone());
    o.vcx.update(|w, _| handle.is_focused(w))
}

#[gpui::test]
fn it_sits_in_the_middle_and_draws_nothing_closed(cx: &mut TestAppContext) {
    let mut o = open(cx, Kind::Dialog);
    assert!(dialog(o.vcx).is_none(), "closed, nothing drawn");
    press_opener(&mut o);
    let d = dialog(o.vcx).expect("open");
    assert!((d.center().x - px(400.)).abs() < px(1.), "{d:?}");
    assert!((d.center().y - px(250.)).abs() < px(1.), "{d:?}");
    // The web's 384 CSS pixels.
    assert_eq!(d.size.width, px(384.));
    let title = bounds(o.vcx, "gpui-core-dialog-title");
    assert!(d.contains(&title.origin), "{title:?} in {d:?}");
}

#[gpui::test]
fn on_a_narrow_window_it_keeps_16_pixels_from_each_side(cx: &mut TestAppContext) {
    let mut o = open_with(cx, Kind::Dialog, 300., |_| {});
    press_opener(&mut o);
    let d = dialog(o.vcx).expect("open");
    assert_eq!(d.left(), px(16.));
    assert_eq!(d.right(), px(284.));
    // The footer's buttons stack, the last on top.
    let (cancel, ok) = (bounds(o.vcx, "cancel"), bounds(o.vcx, "ok"));
    assert!(ok.bottom() <= cancel.top(), "{ok:?} over {cancel:?}");
}

#[gpui::test]
fn taller_than_the_window_it_stays_inside_and_scrolls(cx: &mut TestAppContext) {
    let mut o = open_with(cx, Kind::Dialog, 800., |p| p.rows = 40);
    press_opener(&mut o);
    let d = dialog(o.vcx).expect("open");
    assert_eq!(d.top(), px(16.));
    assert_eq!(d.bottom(), px(484.));
    // The footer is below, scrolled to.
    let before = bounds(o.vcx, "ok");
    assert!(before.top() > px(500.), "{before:?}");
    o.vcx.simulate_event(gpui::ScrollEvent {
        position: d.center(),
        delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-5000.))),
        ..Default::default()
    });
    o.vcx.run_until_parked();
    let after = bounds(o.vcx, "ok");
    assert!(after.bottom() <= d.bottom(), "{after:?} in {d:?}");
}

#[gpui::test]
fn it_takes_the_keyboard_and_gives_it_back(cx: &mut TestAppContext) {
    let mut o = open(cx, Kind::Dialog);
    key(o.vcx, "tab");
    assert!(focused(&mut o, |p| &p.opener));
    key(o.vcx, "space");
    assert!(dialog(o.vcx).is_some());
    assert!(
        focused(&mut o, |p| &p.first),
        "the first thing inside has it"
    );
    key(o.vcx, "escape");
    assert_eq!(*o.reasons.borrow(), [DismissReason::Escape]);
    assert!(dialog(o.vcx).is_none());
    assert!(focused(&mut o, |p| &p.opener), "back where it was");
}

#[gpui::test]
fn tab_goes_round_inside_and_never_out(cx: &mut TestAppContext) {
    let mut o = open(cx, Kind::Dialog);
    press_opener(&mut o);
    assert!(focused(&mut o, |p| &p.first));
    key(o.vcx, "tab");
    assert!(focused(&mut o, |p| &p.cancel));
    key(o.vcx, "tab");
    assert!(focused(&mut o, |p| &p.ok));
    // The close button, last.
    key(o.vcx, "tab enter");
    assert_eq!(*o.reasons.borrow(), [DismissReason::CloseButton]);
    o.panel.update(o.vcx, |p, cx| {
        p.open = true;
        cx.notify();
    });
    o.vcx.run_until_parked();
    assert!(focused(&mut o, |p| &p.first));
    // Round past the close button to the first again, and back.
    key(o.vcx, "tab tab tab tab");
    assert!(focused(&mut o, |p| &p.first), "round to the first");
    key(o.vcx, "shift-tab shift-tab");
    assert!(focused(&mut o, |p| &p.ok), "back round to the last button");
    assert!(!focused(&mut o, |p| &p.opener));
}

#[gpui::test]
fn the_scrim_dismisses_it_and_a_press_inside_doesnt(cx: &mut TestAppContext) {
    let mut o = open(cx, Kind::Dialog);
    press_opener(&mut o);
    let d = dialog(o.vcx).unwrap();
    click(o.vcx, point(d.center().x, d.top() + px(4.)));
    assert!(o.reasons.borrow().is_empty());
    // The page's controls are under the scrim: the press is the scrim's.
    let behind = bounds(o.vcx, "opener").center();
    click(o.vcx, behind);
    assert_eq!(*o.reasons.borrow(), [DismissReason::ClickOutside]);
    assert!(dialog(o.vcx).is_none());
}

#[gpui::test]
fn an_alert_keeps_its_scrim_and_starts_on_cancel(cx: &mut TestAppContext) {
    let mut o = open(cx, Kind::Alert);
    key(o.vcx, "tab space");
    assert!(dialog(o.vcx).is_some());
    assert!(
        focused(&mut o, |p| &p.cancel),
        "Cancel, though something inside comes before it"
    );
    assert!(o.vcx.debug_bounds("gpui-core-dialog-close").is_none());
    click(o.vcx, point(px(5.), px(5.)));
    assert!(o.reasons.borrow().is_empty(), "the scrim doesn't close it");
    assert!(dialog(o.vcx).is_some());
    // Tab still goes round inside.
    key(o.vcx, "tab tab");
    assert!(focused(&mut o, |p| &p.first));
    key(o.vcx, "escape");
    assert_eq!(*o.reasons.borrow(), [DismissReason::Escape]);
    assert!(focused(&mut o, |p| &p.opener));
}

#[gpui::test]
fn a_popover_in_it_is_over_it(cx: &mut TestAppContext) {
    let mut o = open_with(cx, Kind::Dialog, 800., |p| p.inner_popover = Some(false));
    press_opener(&mut o);
    let at = bounds(o.vcx, "inner-trigger").center();
    click(o.vcx, at);
    let item = bounds(o.vcx, "inner-item").center();
    click(o.vcx, item);
    assert_eq!(*o.pressed.borrow(), ["inner-item"]);
    assert!(o.reasons.borrow().is_empty(), "the dialog stays");
    // Escape closes the popover, then the dialog.
    o.panel.update(o.vcx, |p, cx| {
        p.inner_popover = Some(true);
        cx.notify();
    });
    o.vcx.run_until_parked();
    key(o.vcx, "escape");
    assert!(o.vcx.debug_bounds("inner-item").is_none());
    assert!(dialog(o.vcx).is_some());
    key(o.vcx, "escape");
    assert_eq!(*o.reasons.borrow(), [DismissReason::Escape]);
}

#[gpui::test]
fn open_and_idle_it_doesnt_redraw(cx: &mut TestAppContext) {
    let mut o = open(cx, Kind::Dialog);
    press_opener(&mut o);
    let before = o.draws.get();
    for _ in 0..30 {
        o.vcx.update(|window, cx| window.simulate_next_frame(cx));
        o.vcx
            .executor()
            .advance_clock(std::time::Duration::from_millis(16));
        o.vcx.run_until_parked();
    }
    assert_eq!(o.draws.get(), before);
}

#[gpui::test]
fn a_toast_is_over_it(cx: &mut TestAppContext) {
    let mut o = open(cx, Kind::Dialog);
    press_opener(&mut o);
    o.vcx
        .update(|window, cx| gpui_core::Toast::new("Saved").id("saved").show(window, cx));
    o.vcx.run_until_parked();
    let toast = bounds(o.vcx, "toast-saved");
    // The toast takes the press, not the scrim under it.
    click(o.vcx, toast.center());
    assert!(o.reasons.borrow().is_empty(), "the dialog stays");
    assert!(dialog(o.vcx).is_some());
}
