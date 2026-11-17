//! AspectRatio: as wide as its parent (or the width it's given) and as tall as its ratio makes
//! that; a ratio that isn't a positive number is a square; its children fill it, and tall
//! content neither stretches it nor shows past it; nothing redraws while it sits there.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AppContext as _, Context, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    Styled as _, TestAppContext, VisualTestContext, Window, div, px,
};
use gpui_core::{AspectRatio, Root};

struct Host {
    draws: Rc<Cell<usize>>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let boxed = |tag: &'static str, ratio: AspectRatio| {
            div().debug_selector(move || tag.into()).child(ratio)
        };
        div()
            .p(px(20.))
            .w(px(360.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(boxed(
                "wide",
                AspectRatio::new(16. / 9.).child(
                    div()
                        .debug_selector(|| "content".into())
                        // Content far taller than the box.
                        .child(div().h(px(1000.)).bg(gpui::red())),
                ),
            ))
            .child(boxed("given", AspectRatio::new(4. / 3.).w(px(120.))))
            .child(boxed("square", AspectRatio::new(1.)))
            .child(boxed("zero", AspectRatio::new(0.)))
            .child(boxed("nan", AspectRatio::new(f32::NAN)))
            .child(
                div()
                    .flex()
                    .child(boxed("in-row", AspectRatio::new(2.).w(px(100.)))),
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
    vcx.simulate_resize(gpui::size(px(800.), px(2000.)));
    vcx.run_until_parked();
    (draws, vcx)
}

fn size(vcx: &mut VisualTestContext, which: &'static str) -> (f32, f32) {
    let b = vcx.debug_bounds(which).unwrap();
    (f32::from(b.size.width), f32::from(b.size.height))
}

#[gpui::test]
fn it_keeps_its_ratio_at_its_width(cx: &mut TestAppContext) {
    let (_, vcx) = open(cx);
    assert_eq!(size(vcx, "wide"), (320., 180.));
    assert_eq!(size(vcx, "given").1, 90.);
    assert_eq!(size(vcx, "square"), (320., 320.));
    assert_eq!(size(vcx, "in-row"), (100., 50.));
}

#[gpui::test]
fn a_ratio_that_isnt_a_positive_number_is_a_square(cx: &mut TestAppContext) {
    let (_, vcx) = open(cx);
    assert_eq!(size(vcx, "zero"), (320., 320.));
    assert_eq!(size(vcx, "nan"), (320., 320.));
}

#[gpui::test]
fn its_content_fills_it_and_never_stretches_it(cx: &mut TestAppContext) {
    let (_, vcx) = open(cx);
    let wide = vcx.debug_bounds("wide").unwrap();
    let content = vcx.debug_bounds("content").unwrap();
    assert_eq!(content.origin, wide.origin);
    assert_eq!(content.size.width, wide.size.width);
    assert_eq!(wide.size.height, px(180.));
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
fn tall_content_doesnt_show_past_it(cx: &mut TestAppContext) {
    let (_, vcx) = open(cx);
    let wide = vcx.debug_bounds("wide").unwrap();
    vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let k = window.scale_factor();
        let red = window
            .rendered_scene()
            .quads
            .iter()
            .find(|q| q.background.as_solid() == Some(gpui::red()))
            .expect("the content drawn");
        let shown_bottom = (red.bounds.origin.y.0 + red.bounds.size.height.0)
            .min(red.content_mask.bounds.origin.y.0 + red.content_mask.bounds.size.height.0)
            / k;
        assert!(
            shown_bottom <= f32::from(wide.bottom()) + 0.5,
            "shows to {shown_bottom}, past {wide:?}"
        );
    });
}
