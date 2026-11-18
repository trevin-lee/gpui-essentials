//! A focus ring is drawn round a control, never under it. GPUI draws an outer shadow under its
//! element as well as round it, so a ring drawn as the control's own shadow showed through a
//! see-through fill (a focused outline, ghost or destructive button turned grey). Each control
//! here, focused from the keyboard, must have no shadow under its middle, and a ring (an inset
//! shadow on a box three points bigger all round) just outside its edge.

use gpui::{
    AppContext as _, Bounds, Context, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, KeyUpEvent, Keystroke, ParentElement as _, Pixels, Render, Styled as _,
    TestAppContext, VisualTestContext, Window, div, point, px, size,
};
use gpui_core::{Button, ButtonVariants as _, Input, InputState, Press, Root, Select};

struct Host {
    before: FocusHandle,
    name: gpui::Entity<InputState>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                Press::new("before")
                    .track_focus(&self.before)
                    .child("Before"),
            )
            .child(Button::new("outline").outline().label("Cancel"))
            .child(Button::new("ghost").ghost().label("Archive"))
            .child(Button::new("danger").destructive().label("Delete"))
            .child(
                div().w(px(200.)).child(
                    Select::new("fruit")
                        .option("a", "Apple")
                        .accessibility_label("Fruit"),
                ),
            )
            .child(
                div()
                    .w(px(200.))
                    .debug_selector(|| "name-box".into())
                    .child(Input::new(&self.name).accessibility_label("Name")),
            )
    }
}

fn tab(vcx: &mut VisualTestContext) {
    let keystroke = Keystroke::parse("tab").unwrap();
    vcx.simulate_event(KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    vcx.simulate_event(KeyUpEvent { keystroke });
    vcx.run_until_parked();
}

/// The shadows drawn now, in the window's points.
fn shadows(vcx: &mut VisualTestContext) -> Vec<(bool, Bounds<Pixels>, Bounds<Pixels>)> {
    vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let s = window.scale_factor();
        let pt = |b: gpui::Bounds<gpui::ScaledPixels>| {
            Bounds::new(
                point(px(b.origin.x.as_f32() / s), px(b.origin.y.as_f32() / s)),
                size(
                    px(b.size.width.as_f32() / s),
                    px(b.size.height.as_f32() / s),
                ),
            )
        };
        window
            .rendered_scene()
            .shadows
            .iter()
            .map(|sh| (sh.inset != 0, pt(sh.bounds), pt(sh.element_bounds)))
            .collect()
    })
}

fn near(a: Pixels, b: Pixels) -> bool {
    (a - b).abs() <= px(0.6)
}

#[gpui::test]
fn a_focused_controls_ring_is_round_it_not_under_it(cx: &mut TestAppContext) {
    cx.update(gpui_core::init);
    let made = std::rc::Rc::new(std::cell::RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let h = cx.new(|cx| Host {
            before: cx.focus_handle(),
            name: cx.new(|cx| InputState::new(window, cx)),
        });
        *m.borrow_mut() = Some(h.clone());
        Root::new(h, window, cx)
    });
    vcx.simulate_resize(size(px(400.), px(500.)));
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let h = made.borrow().clone().unwrap();
    let before = h.read_with(vcx, |h, _| h.before.clone());
    vcx.update(|window, cx| window.focus(&before, cx));

    let mut checked = 0;
    for which in ["outline", "ghost", "danger", "select-fruit", "name-box"] {
        tab(vcx);
        let control = vcx.debug_bounds(which).expect(which);
        let middle = control.center();
        let drawn = shadows(vcx);
        for (inset, bounds, _) in &drawn {
            assert!(
                *inset || !bounds.contains(&middle),
                "{which}: a shadow is drawn under its middle ({bounds:?}), which a see-through \
                 fill shows"
            );
        }
        // The ring: an inset shadow on a box three points outside the control's edge (for the
        // input, the field inside its box: its left, right and top edges).
        let ring = drawn.iter().any(|(inset, _, el)| {
            *inset
                && near(el.origin.x, control.origin.x - px(3.))
                && near(el.origin.y, control.origin.y - px(3.))
                && near(el.size.width, control.size.width + px(6.))
        });
        assert!(
            ring,
            "{which}: no ring just outside its edge {control:?}, of {drawn:?}"
        );
        checked += 1;
    }
    assert_eq!(checked, 5);
}
