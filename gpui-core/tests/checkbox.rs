//! Checkbox: a press on the box or on its label asks for the opposite state (on, from
//! indeterminate), Space works from the keyboard, disabled it takes neither presses nor the
//! focus, it's a check box to assistive tech with its state (mixed when indeterminate) and its
//! label's name, a touch one takes presses 44 points tall, a long label wraps inside a narrow
//! box, and nothing redraws while nothing changes.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, ParentElement as _, Render, RenderOnce,
    Styled as _, TestAppContext, Toggled, VisualTestContext, Window, div, point, px,
};
use gpui_core::{Checkbox, Disableable as _, Press, Root, Sizable as _};

struct Panel {
    on: bool,
    mixed: bool,
    disabled: bool,
    touch: bool,
    width: f32,
    label: &'static str,
    asked: Rc<RefCell<Vec<bool>>>,
    draws: Rc<Cell<usize>>,
    before: FocusHandle,
}

impl Render for Panel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let asked = self.asked.clone();
        let mut cb = Checkbox::new("terms")
            .label(self.label)
            .checked(self.on)
            .indeterminate(self.mixed)
            .disabled(self.disabled)
            .on_click(cx.listener(move |this, on: &bool, _, cx| {
                asked.borrow_mut().push(*on);
                this.on = *on;
                this.mixed = false;
                cx.notify();
            }));
        if self.touch {
            cb = cb.touch();
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
                    .w(px(self.width))
                    .debug_selector(|| "box".into())
                    .child(cb),
            )
    }
}

struct Blank;

impl Render for Blank {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// Draws `cb` in a view of its own and gives what it tells assistive technology: its role and
/// its node's properties.
fn a11y(vcx: &mut VisualTestContext, cb: Checkbox) -> gpui::accesskit::Node {
    struct Probe {
        cb: Option<Checkbox>,
        out: Rc<RefCell<Option<gpui::accesskit::Node>>>,
    }
    impl Render for Probe {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            if let Some(cb) = self.cb.take() {
                let el = RenderOnce::render(cb, window, cx).into_element();
                let role = gpui::Element::a11y_role(&el).unwrap_or(gpui::Role::Unknown);
                let mut node = gpui::accesskit::Node::new(role);
                gpui::Element::write_a11y_info(&el, &mut node);
                *self.out.borrow_mut() = Some(node);
            }
            div()
        }
    }
    let out = Rc::new(RefCell::new(None));
    let o = out.clone();
    let probe = vcx.new(|_| Probe {
        cb: Some(cb),
        out: o,
    });
    vcx.draw(
        point(px(0.), px(0.)),
        gpui::size(px(200.), px(100.)),
        |_, _| gpui::AnyView::from(probe).into_any_element(),
    );
    out.take().expect("drawn")
}

struct Opened<'a> {
    panel: Entity<Panel>,
    asked: Rc<RefCell<Vec<bool>>>,
    draws: Rc<Cell<usize>>,
    vcx: &'a mut VisualTestContext,
}

#[derive(Clone, Copy)]
struct Setup {
    mixed: bool,
    disabled: bool,
    touch: bool,
    width: f32,
    label: &'static str,
}

const PLAIN: Setup = Setup {
    mixed: false,
    disabled: false,
    touch: false,
    width: 300.,
    label: "Accept terms",
};

fn open(cx: &mut TestAppContext, s: Setup) -> Opened<'_> {
    cx.update(gpui_core::init);
    let asked = Rc::new(RefCell::new(Vec::new()));
    let draws = Rc::new(Cell::new(0));
    let (a, d) = (asked.clone(), draws.clone());
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let panel = cx.new(|cx| Panel {
            on: false,
            mixed: s.mixed,
            disabled: s.disabled,
            touch: s.touch,
            width: s.width,
            label: s.label,
            asked: a,
            draws: d,
            before: cx.focus_handle(),
        });
        *m.borrow_mut() = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let panel = made.borrow().clone().unwrap();
    Opened {
        panel,
        asked,
        draws,
        vcx,
    }
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

/// A press on the box (at the row's left, 8 points in) or on the label's middle.
fn press(vcx: &mut VisualTestContext, on_label: bool) {
    let at = if on_label {
        vcx.debug_bounds("checkbox-label-terms").unwrap().center()
    } else {
        let row = vcx.debug_bounds("checkbox-terms").unwrap();
        point(row.left() + px(8.), row.top() + px(8.))
    };
    vcx.simulate_click(at, Modifiers::none());
    vcx.run_until_parked();
}

#[gpui::test]
fn a_press_on_the_box_or_its_label_asks_for_the_opposite(cx: &mut TestAppContext) {
    let o = open(cx, PLAIN);
    press(o.vcx, false);
    press(o.vcx, true);
    press(o.vcx, true);
    assert_eq!(*o.asked.borrow(), [true, false, true]);
}

#[gpui::test]
fn indeterminate_a_press_turns_it_on(cx: &mut TestAppContext) {
    let o = open(
        cx,
        Setup {
            mixed: true,
            ..PLAIN
        },
    );
    press(o.vcx, false);
    assert_eq!(*o.asked.borrow(), [true]);
}

#[gpui::test]
fn space_toggles_it_from_the_keyboard(cx: &mut TestAppContext) {
    let o = open(cx, PLAIN);
    let before = o.panel.read_with(o.vcx, |p, _| p.before.clone());
    o.vcx.update(|window, cx| window.focus(&before, cx));
    keys(o.vcx, "tab space space");
    assert_eq!(*o.asked.borrow(), [true, false]);
}

#[gpui::test]
fn disabled_it_takes_neither_presses_nor_the_focus(cx: &mut TestAppContext) {
    let o = open(
        cx,
        Setup {
            disabled: true,
            ..PLAIN
        },
    );
    press(o.vcx, false);
    press(o.vcx, true);
    let before = o.panel.read_with(o.vcx, |p, _| p.before.clone());
    o.vcx.update(|window, cx| window.focus(&before, cx));
    keys(o.vcx, "tab space");
    assert!(o.asked.borrow().is_empty());
}

#[gpui::test]
fn its_a_check_box_named_by_its_label_with_its_state(cx: &mut TestAppContext) {
    cx.update(gpui_core::init);
    let (_, vcx) = cx.add_window_view(|_, _| Blank);
    let node = |cb: Checkbox, vcx: &mut VisualTestContext| a11y(vcx, cb);
    let off = node(Checkbox::new("a").label("Accept terms"), vcx);
    assert_eq!(off.role(), gpui::Role::CheckBox);
    assert_eq!(off.toggled(), Some(Toggled::False));
    assert_eq!(off.label(), Some("Accept terms"));
    let on = node(Checkbox::new("b").checked(true), vcx);
    assert_eq!(on.toggled(), Some(Toggled::True));
    let mixed = node(Checkbox::new("c").checked(true).indeterminate(true), vcx);
    assert_eq!(mixed.toggled(), Some(Toggled::Mixed));
    let disabled = node(Checkbox::new("d").disabled(true), vcx);
    assert!(disabled.is_disabled());
    let named = node(
        Checkbox::new("e")
            .label("Terms")
            .accessibility_label("Accept the terms"),
        vcx,
    );
    assert_eq!(named.label(), Some("Accept the terms"));
}

#[gpui::test]
fn a_touch_one_takes_presses_44_points_tall(cx: &mut TestAppContext) {
    let o = open(
        cx,
        Setup {
            touch: true,
            ..PLAIN
        },
    );
    let row = o.vcx.debug_bounds("checkbox-terms").unwrap();
    assert!(row.size.height >= px(44.), "{row:?}");
    // A press near its top edge, well above the 16 point box, still toggles it.
    o.vcx.simulate_click(
        point(row.left() + px(8.), row.top() + px(3.)),
        Modifiers::none(),
    );
    o.vcx.run_until_parked();
    assert_eq!(*o.asked.borrow(), [true]);
}

#[cfg(target_os = "macos")]
fn app_with_text() -> TestAppContext {
    TestAppContext::build_with_text_system(
        gpui::TestDispatcher::new(0),
        None,
        std::sync::Arc::new(gpui_apple::AppleTextSystem::new()),
    )
}

#[cfg(target_os = "macos")]
#[test]
fn a_long_label_wraps_inside_a_narrow_box() {
    let mut cx = app_with_text();
    let o = open(
        &mut cx,
        Setup {
            width: 120.,
            label: "Send me a summary of everything that happened overnight by email",
            ..PLAIN
        },
    );
    let outer = o.vcx.debug_bounds("box").unwrap();
    let label = o.vcx.debug_bounds("checkbox-label-terms").unwrap();
    assert!(
        label.right() <= outer.right() + px(1.),
        "{label:?} in {outer:?}"
    );
    assert!(
        label.size.height > px(30.),
        "it didn't wrap onto more lines: {label:?}"
    );
    // Every glyph drawn is inside the box.
    o.vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let scale = window.scale_factor();
        let right = outer.right().as_f32() * scale;
        let scene = window.rendered_scene();
        for s in &scene.monochrome_sprites {
            let r = s.bounds.origin.x.as_f32() + s.bounds.size.width.as_f32();
            assert!(r <= right + 2. * scale, "a glyph out at {r} past {right}");
        }
    });
}

/// Delivers one display frame and says how many things had asked for it.
fn frame(vcx: &mut VisualTestContext) -> usize {
    vcx.executor().advance_clock(Duration::from_millis(16));
    let asked = vcx.update(|window, cx| window.simulate_next_frame(cx));
    vcx.run_until_parked();
    asked
}

#[gpui::test]
fn nothing_redraws_while_nothing_changes(cx: &mut TestAppContext) {
    let o = open(cx, PLAIN);
    press(o.vcx, false);
    let settled = o.draws.get();
    for _ in 0..30 {
        assert_eq!(frame(o.vcx), 0, "asked for a frame while idle");
    }
    assert_eq!(o.draws.get(), settled, "kept drawing while idle");
}
