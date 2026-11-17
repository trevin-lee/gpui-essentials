//! Drawer: against each edge, the keyboard taken on opening and given back on closing, Tab
//! going round inside, Escape and the scrim dismissing it, a bottom drawer dragged down past a
//! quarter of its height dismissing it (and put back short of that, or by a tap), and nothing
//! redrawn while idle.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{
    AppContext as _, Bounds, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, MouseButton, ParentElement as _, Pixels, Point,
    Render, Styled as _, TestAppContext, VisualTestContext, Window, div, point, px, size,
};
use gpui_core::{DismissReason, Drawer, DrawerSide, Press, Root};

struct Panel {
    side: DrawerSide,
    open: bool,
    opener: FocusHandle,
    first: FocusHandle,
    second: FocusHandle,
    reasons: Rc<RefCell<Vec<DismissReason>>>,
    draws: Rc<Cell<usize>>,
}

fn press(id: &'static str, focus: &FocusHandle) -> Press {
    Press::new(id)
        .track_focus(focus)
        .debug_selector(move || id.into())
        .h(px(30.))
        .child(id)
}

impl Render for Panel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let reasons = self.reasons.clone();
        div()
            .size_full()
            .p(px(10.))
            .child(
                press("opener", &self.opener)
                    .w(px(40.))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.open = true;
                        cx.notify();
                    })),
            )
            .child(
                Drawer::new("drawer")
                    .open(self.open)
                    .side(self.side)
                    .title("Move goal")
                    .description("Set your daily activity goal.")
                    .on_dismiss(cx.listener(move |this, reason: &DismissReason, _, cx| {
                        reasons.borrow_mut().push(*reason);
                        this.open = false;
                        cx.notify();
                    }))
                    .child(div().h(px(100.)).child("350 calories"))
                    .footer(press("first", &self.first))
                    .footer(press("second", &self.second)),
            )
    }
}

struct Opened<'a> {
    panel: Entity<Panel>,
    reasons: Rc<RefCell<Vec<DismissReason>>>,
    draws: Rc<Cell<usize>>,
    vcx: &'a mut VisualTestContext,
}

fn open(cx: &mut TestAppContext, side: DrawerSide) -> Opened<'_> {
    cx.update(gpui_core::init);
    let reasons: Rc<RefCell<Vec<DismissReason>>> = Rc::default();
    let draws = Rc::new(Cell::new(0));
    let (r, d) = (reasons.clone(), draws.clone());
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let panel = cx.new(|cx| Panel {
            side,
            open: false,
            opener: cx.focus_handle(),
            first: cx.focus_handle(),
            second: cx.focus_handle(),
            reasons: r,
            draws: d,
        });
        *m.borrow_mut() = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    vcx.simulate_resize(size(px(800.), px(600.)));
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let panel = made.borrow().clone().unwrap();
    let mut o = Opened {
        panel,
        reasons,
        draws,
        vcx,
    };
    let at = bounds(o.vcx, "opener").center();
    click(o.vcx, at);
    o.vcx.run_until_parked();
    let _ = &mut o;
    o
}

fn drawer(vcx: &mut VisualTestContext) -> Option<Bounds<Pixels>> {
    vcx.debug_bounds("gpui-core-drawer")
}

fn bounds(vcx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    vcx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} drawn"))
}

fn click(vcx: &mut VisualTestContext, at: Point<Pixels>) {
    vcx.simulate_click(at, Modifiers::none());
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

fn focused(o: &mut Opened<'_>, pick: impl Fn(&Panel) -> &FocusHandle) -> bool {
    let handle = o.panel.read_with(o.vcx, |p, _| pick(p).clone());
    o.vcx.update(|w, _| handle.is_focused(w))
}

/// Presses on the grip, moves down by `dy` in steps, and lets go.
fn drag(vcx: &mut VisualTestContext, dy: f32) {
    let from = bounds(vcx, "gpui-core-drawer-grip").center();
    vcx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    vcx.run_until_parked();
    for i in 1..=4 {
        let at = point(from.x, from.y + px(dy * i as f32 / 4.));
        vcx.simulate_mouse_move(at, MouseButton::Left, Modifiers::none());
        vcx.run_until_parked();
    }
    let to = point(from.x, from.y + px(dy));
    vcx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    vcx.run_until_parked();
}

#[gpui::test]
fn it_comes_from_each_edge(cx: &mut TestAppContext) {
    for (side, check) in [
        (
            DrawerSide::Bottom,
            (|d: Bounds<Pixels>| d.bottom() == px(600.) && d.size.width == px(800.))
                as fn(Bounds<Pixels>) -> bool,
        ),
        (DrawerSide::Top, |d| {
            d.top() == px(0.) && d.size.width == px(800.)
        }),
        (DrawerSide::Left, |d| {
            d.left() == px(0.) && d.size.height == px(600.) && d.size.width == px(384.)
        }),
        (DrawerSide::Right, |d| {
            d.right() == px(800.) && d.size.height == px(600.) && d.size.width == px(384.)
        }),
    ] {
        let mut cx = cx.clone();
        let o = open(&mut cx, side);
        let d = drawer(o.vcx).expect("open");
        assert!(check(d), "{side:?}: {d:?}");
        let has_grip = o.vcx.debug_bounds("gpui-core-drawer-handle").is_some();
        assert_eq!(has_grip, side == DrawerSide::Bottom, "{side:?}");
    }
}

#[gpui::test]
fn it_takes_the_keyboard_keeps_tab_inside_and_gives_it_back(cx: &mut TestAppContext) {
    let mut o = open(cx, DrawerSide::Bottom);
    assert!(focused(&mut o, |p| &p.first));
    key(o.vcx, "tab");
    assert!(focused(&mut o, |p| &p.second));
    key(o.vcx, "tab");
    assert!(focused(&mut o, |p| &p.first), "round, never out");
    key(o.vcx, "shift-tab");
    assert!(focused(&mut o, |p| &p.second));
    key(o.vcx, "escape");
    assert_eq!(*o.reasons.borrow(), [DismissReason::Escape]);
    assert!(drawer(o.vcx).is_none());
    assert!(focused(&mut o, |p| &p.opener));
}

#[gpui::test]
fn the_scrim_dismisses_it(cx: &mut TestAppContext) {
    let o = open(cx, DrawerSide::Right);
    let d = drawer(o.vcx).unwrap();
    click(o.vcx, d.center());
    assert!(o.reasons.borrow().is_empty());
    click(o.vcx, point(px(100.), px(300.)));
    assert_eq!(*o.reasons.borrow(), [DismissReason::ClickOutside]);
}

#[gpui::test]
fn dragged_down_far_enough_it_goes(cx: &mut TestAppContext) {
    let o = open(cx, DrawerSide::Bottom);
    let d = drawer(o.vcx).unwrap();
    // Held part way down, it follows.
    let from = bounds(o.vcx, "gpui-core-drawer-grip").center();
    o.vcx
        .simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    o.vcx.simulate_mouse_move(
        point(from.x, from.y + px(30.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    o.vcx.run_until_parked();
    assert_eq!(drawer(o.vcx).unwrap().top(), d.top() + px(30.));
    // Let go short of a quarter: back where it was.
    o.vcx.simulate_mouse_up(
        point(from.x, from.y + px(30.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    o.vcx.run_until_parked();
    assert!(o.reasons.borrow().is_empty());
    assert_eq!(drawer(o.vcx).unwrap(), d);
    // Past a quarter of its height: dismissed.
    drag(o.vcx, f32::from(d.size.height) / 4. + 20.);
    assert_eq!(*o.reasons.borrow(), [DismissReason::Swiped]);
    assert!(drawer(o.vcx).is_none());
}

#[gpui::test]
fn a_side_drawer_isnt_dragged(cx: &mut TestAppContext) {
    let o = open(cx, DrawerSide::Left);
    drag(o.vcx, 400.);
    assert!(o.reasons.borrow().is_empty());
    assert!(drawer(o.vcx).is_some());
}

#[gpui::test]
fn open_and_idle_it_doesnt_redraw(cx: &mut TestAppContext) {
    let o = open(cx, DrawerSide::Bottom);
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
