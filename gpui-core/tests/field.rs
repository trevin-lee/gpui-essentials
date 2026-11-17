//! Label and Field: a press on a label focuses its control or does what the host says (and
//! nothing when disabled), a field lays its label, control, description and error out one
//! above the other or side by side (the control first or last), it's a group to assistive tech
//! named by its label and described by its error (else its description), long text wraps
//! inside a narrow box, and nothing redraws while nothing changes.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AppContext as _, Context, Entity, Focusable as _, InteractiveElement as _, IntoElement,
    Modifiers, ParentElement as _, Render, RenderOnce, Styled as _, TestAppContext,
    VisualTestContext, Window, div, px,
};
use gpui_core::{
    Disableable as _, Field, FieldGroup, FieldSet, Input, InputState, Label, Root, Switch,
};

const LONG: &str = "We use it only to sign you in and to send the receipts you ask for, never anything else, and you can take it out at any time";

#[derive(Clone, Copy)]
struct Setup {
    horizontal: bool,
    control_first: bool,
    disabled: bool,
    error: bool,
    width: f32,
}

const PLAIN: Setup = Setup {
    horizontal: false,
    control_first: false,
    disabled: false,
    error: false,
    width: 400.,
};

struct Host {
    s: Setup,
    email: Entity<InputState>,
    on: bool,
    toggles: Rc<Cell<usize>>,
    draws: Rc<Cell<usize>>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let toggles = self.toggles.clone();
        let email = Field::new("email")
            .label("Email")
            .for_focus(&self.email.focus_handle(cx))
            .description(LONG)
            .error(self.s.error.then_some("Enter an address with an @ in it"))
            .horizontal(self.s.horizontal)
            .control_first(self.s.control_first)
            .disabled(self.s.disabled)
            .child(
                div()
                    .w(px(160.))
                    .debug_selector(|| "control".into())
                    .child(Input::new(&self.email)),
            );
        let on = self.on;
        div()
            .p(px(20.))
            .w(px(self.s.width))
            .debug_selector(|| "box".into())
            .child(
                FieldGroup::new().child(email).child(
                    FieldSet::new("prefs")
                        .legend("Preferences")
                        .description("How we reach you.")
                        .child(
                            div()
                                .flex()
                                .gap(px(8.))
                                .child(Switch::new("news").checked(on))
                                .child(
                                    Label::new("news-label", "Newsletter")
                                        .disabled(self.s.disabled)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            toggles.set(toggles.get() + 1);
                                            this.on = !this.on;
                                            cx.notify();
                                        })),
                                ),
                        ),
                ),
            )
    }
}

fn open(cx: &mut TestAppContext, s: Setup) -> (Entity<Host>, &mut VisualTestContext) {
    cx.update(gpui_core::init);
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let email = cx.new(|cx| InputState::new(window, cx));
        let h = cx.new(|_| Host {
            s,
            email,
            on: false,
            toggles: Rc::default(),
            draws: Rc::default(),
        });
        *m.borrow_mut() = Some(h.clone());
        Root::new(h, window, cx)
    });
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let h = made.borrow().clone().unwrap();
    (h, vcx)
}

fn click(vcx: &mut VisualTestContext, selector: &'static str) {
    let at = vcx.debug_bounds(selector).unwrap().center();
    vcx.simulate_click(at, Modifiers::none());
    vcx.run_until_parked();
}

fn email_focused(h: &Entity<Host>, vcx: &mut VisualTestContext) -> bool {
    let email = h.read_with(vcx, |h, _| h.email.clone());
    vcx.update(|window, cx| email.read(cx).is_focused(window))
}

#[gpui::test]
fn a_press_on_a_label_focuses_or_toggles_its_control(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, PLAIN);
    assert!(!email_focused(&h, vcx));
    click(vcx, "label-email-label");
    assert!(
        email_focused(&h, vcx),
        "the field's label focused its input"
    );
    click(vcx, "label-news-label");
    click(vcx, "label-news-label");
    assert_eq!(h.read_with(vcx, |h, _| h.toggles.get()), 2);
    assert!(!h.read_with(vcx, |h, _| h.on));
}

#[gpui::test]
fn disabled_a_label_does_nothing(cx: &mut TestAppContext) {
    let (h, vcx) = open(
        cx,
        Setup {
            disabled: true,
            ..PLAIN
        },
    );
    click(vcx, "label-email-label");
    click(vcx, "label-news-label");
    assert!(!email_focused(&h, vcx));
    assert_eq!(h.read_with(vcx, |h, _| h.toggles.get()), 0);
}

#[gpui::test]
fn one_above_the_other_label_control_description_error(cx: &mut TestAppContext) {
    let (_, vcx) = open(
        cx,
        Setup {
            error: true,
            ..PLAIN
        },
    );
    let label = vcx.debug_bounds("label-email-label").unwrap();
    let control = vcx.debug_bounds("control").unwrap();
    let description = vcx.debug_bounds("field-description-email").unwrap();
    let error = vcx.debug_bounds("field-error-email").unwrap();
    assert!(label.bottom() <= control.top());
    assert!(control.bottom() <= description.top());
    assert!(description.bottom() <= error.top());
}

#[gpui::test]
fn side_by_side_the_control_last(cx: &mut TestAppContext) {
    let (_, vcx) = open(
        cx,
        Setup {
            horizontal: true,
            ..PLAIN
        },
    );
    let label = vcx.debug_bounds("label-email-label").unwrap();
    let control = vcx.debug_bounds("control").unwrap();
    assert!(label.right() <= control.left(), "{label:?} {control:?}");
}

#[gpui::test]
fn side_by_side_the_control_first(cx: &mut TestAppContext) {
    let (_, vcx) = open(
        cx,
        Setup {
            horizontal: true,
            control_first: true,
            ..PLAIN
        },
    );
    let label = vcx.debug_bounds("label-email-label").unwrap();
    let control = vcx.debug_bounds("control").unwrap();
    assert!(control.right() <= label.left(), "{label:?} {control:?}");
}

#[gpui::test]
fn its_a_group_named_by_its_label_and_described_by_its_error(cx: &mut TestAppContext) {
    struct Probe(Rc<RefCell<Vec<gpui::accesskit::Node>>>);
    impl Render for Probe {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            for error in [None, Some("Required")] {
                let field = Field::new("f")
                    .label("Name")
                    .description("As on your passport")
                    .error(error);
                let el = RenderOnce::render(field, window, cx).into_element();
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
    assert_eq!(nodes[0].role(), gpui::Role::Group);
    assert_eq!(nodes[0].label(), Some("Name"));
    assert_eq!(nodes[0].description(), Some("As on your passport"));
    assert_eq!(nodes[1].description(), Some("Required"));
}

#[cfg(target_os = "macos")]
#[test]
fn long_text_wraps_inside_a_narrow_box() {
    for horizontal in [false, true] {
        let mut cx = TestAppContext::build_with_text_system(
            gpui::TestDispatcher::new(0),
            None,
            std::sync::Arc::new(gpui_apple::AppleTextSystem::new()),
        );
        let (_, vcx) = open(
            &mut cx,
            Setup {
                horizontal,
                error: true,
                width: 320.,
                ..PLAIN
            },
        );
        let outer = vcx.debug_bounds("box").unwrap();
        let d = vcx.debug_bounds("field-description-email").unwrap();
        assert!(d.right() <= outer.right() + px(1.), "{d:?} in {outer:?}");
        assert!(d.size.height > px(30.), "it didn't wrap: {d:?}");
        vcx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
            let scale = window.scale_factor();
            let right = outer.right().as_f32() * scale;
            for s in &window.rendered_scene().monochrome_sprites {
                let r = s.bounds.origin.x.as_f32() + s.bounds.size.width.as_f32();
                assert!(r <= right + 2. * scale, "a glyph out at {r} past {right}");
            }
        });
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
    let (h, vcx) = open(
        cx,
        Setup {
            error: true,
            ..PLAIN
        },
    );
    click(vcx, "label-news-label");
    // The switch's thumb slides for 150 ms: past that, nothing should move.
    for _ in 0..20 {
        frame(vcx);
    }
    let settled = h.read_with(vcx, |h, _| h.draws.get());
    for _ in 0..30 {
        assert_eq!(frame(vcx), 0, "asked for a frame while idle");
    }
    assert_eq!(h.read_with(vcx, |h, _| h.draws.get()), settled);
}
