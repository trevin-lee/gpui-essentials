//! Separator: a horizontal one is a pixel tall across its parent, a vertical one a pixel wide
//! down its row, both in the border colour; decorative ones are nothing to assistive
//! technology and the others are separators with their orientation; nothing redraws.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AppContext as _, Context, Element as _, InteractiveElement as _, IntoElement, Orientation,
    ParentElement as _, Render, RenderOnce as _, Role, Styled as _, TestAppContext,
    VisualTestContext, Window, accesskit, div, px,
};
use gpui_core::{Root, Separator};

struct Host {
    draws: Rc<Cell<usize>>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        div()
            .p(px(20.))
            .w(px(300.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .debug_selector(|| "across".into())
                    .child(Separator::horizontal()),
            )
            .child(
                div()
                    .debug_selector(|| "row".into())
                    .flex()
                    .items_center()
                    .h(px(24.))
                    .gap(px(8.))
                    .child(div().w(px(40.)).child("Blog"))
                    .child(
                        // A row of its own, its height the row's, that centres what's in it:
                        // the separator still runs its full height.
                        div()
                            .flex()
                            .items_center()
                            .h_full()
                            .debug_selector(|| "down".into())
                            .child(Separator::vertical()),
                    )
                    .child(div().w(px(40.)).child("Docs")),
            )
    }
}

fn open(cx: &mut TestAppContext) -> (Rc<Cell<usize>>, &mut VisualTestContext) {
    cx.update(gpui_core::init);
    let draws = Rc::new(Cell::new(0));
    let d = draws.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let host = cx.new(|_| Host { draws: d });
        Root::new(host, window, cx)
    });
    vcx.run_until_parked();
    (draws, vcx)
}

#[gpui::test]
fn horizontal_is_a_pixel_across_and_vertical_a_pixel_down(cx: &mut TestAppContext) {
    let (_, vcx) = open(cx);
    let across = vcx.debug_bounds("across").unwrap();
    assert_eq!(across.size.height, px(1.));
    assert_eq!(across.size.width, px(300. - 40.));
    let down = vcx.debug_bounds("down").unwrap();
    assert_eq!(down.size.width, px(1.));
}

/// What assistive technology is told of a separator: its role, if any, and its orientation.
fn told(vcx: &mut VisualTestContext, separator: Separator) -> (Option<Role>, Option<Orientation>) {
    vcx.update(|window, cx| {
        let el = separator.render(window, cx).into_element();
        let role = el.a11y_role();
        let mut node = accesskit::Node::new(role.unwrap_or(Role::Unknown));
        el.write_a11y_info(&mut node);
        (role, node.orientation())
    })
}

#[gpui::test]
fn decorative_ones_are_nothing_to_assistive_technology(cx: &mut TestAppContext) {
    let (_, vcx) = open(cx);
    assert_eq!(told(vcx, Separator::horizontal()), (None, None));
    assert_eq!(told(vcx, Separator::vertical()), (None, None));
    assert_eq!(
        told(vcx, Separator::horizontal().decorative(false)),
        (Some(Role::Splitter), Some(Orientation::Horizontal))
    );
    assert_eq!(
        told(vcx, Separator::vertical().decorative(false).id("tools")),
        (Some(Role::Splitter), Some(Orientation::Vertical))
    );
}

#[gpui::test]
fn nothing_redraws_while_it_sits_there(cx: &mut TestAppContext) {
    let (draws, vcx) = open(cx);
    let settled = draws.get();
    for _ in 0..30 {
        vcx.executor().advance_clock(Duration::from_millis(16));
        let asked = vcx.update(|window, cx| window.simulate_next_frame(cx));
        vcx.run_until_parked();
        assert_eq!(asked, 0, "asked for a frame");
    }
    assert_eq!(draws.get(), settled);
}

#[cfg(target_os = "macos")]
#[gpui::test]
fn it_is_drawn_in_the_border_colour_its_full_length(cx: &mut TestAppContext) {
    let (_, vcx) = open(cx);
    let across = vcx.debug_bounds("across").unwrap();
    let down = vcx.debug_bounds("down").unwrap();
    let border = gpui_core::Theme::dark().border;
    vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let k = window.scale_factor();
        let scene = window.rendered_scene();
        // The border-coloured lines drawn in each box, as (width, height).
        let lines = |b: gpui::Bounds<gpui::Pixels>| -> Vec<(f32, f32)> {
            scene
                .quads
                .iter()
                .filter(|q| q.background.as_solid() == Some(border))
                .filter(|q| {
                    let c = q.bounds.center();
                    b.contains(&gpui::point(px(c.x.0 / k), px(c.y.0 / k)))
                })
                .map(|q| (q.bounds.size.width.0 / k, q.bounds.size.height.0 / k))
                .collect()
        };
        assert_eq!(lines(across), [(260., 1.)]);
        assert_eq!(lines(down), [(1., 24.)], "it doesn't run down its row");
    });
}
