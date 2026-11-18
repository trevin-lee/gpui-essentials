//! With gpui-core: the provider goes inside `Root`, as the view's content, and the preview is
//! drawn over Root's popovers and toasts.

use std::time::Duration;

use gpui::{
    AppContext as _, Bounds, Context, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, ScaledPixels, Styled as _, TestAppContext, Window, div, point, px,
    rgb, size,
};
use gpui_core::{Popover, Root, Toast};
use gpui_dnd::*;

#[derive(Clone)]
struct Card;

fn red() -> Hsla {
    rgb(0xff0000).into()
}

struct Host;

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        DndProvider::new(
            div()
                .size_full()
                .p(px(20.))
                .child(
                    dnd("a")
                        .debug_selector(|| "a".into())
                        .w(px(100.))
                        .h(px(40.))
                        .source(DragSource::item(Card).preview(Preview::build(|_, _, _| {
                            div().size(px(600.)).bg(red()).into_any_element()
                        }))),
                )
                .child(
                    Popover::new("pop")
                        .open(true)
                        .trigger(div().w(px(80.)).h(px(30.)).child("Menu"))
                        .child(
                            div()
                                .debug_selector(|| "popover-body".into())
                                .size(px(120.))
                                .child("Inside"),
                        ),
                ),
        )
    }
}

fn scaled(b: Bounds<gpui::Pixels>, scale: f32) -> Bounds<ScaledPixels> {
    b.scale(scale)
}

fn overlaps(a: &Bounds<ScaledPixels>, b: &Bounds<ScaledPixels>) -> bool {
    a.intersects(b)
}

#[gpui::test]
fn the_preview_draws_over_roots_popovers_and_toasts(cx: &mut TestAppContext) {
    cx.update(gpui_core::init);
    let (_, vcx) = cx.add_window_view(|window, cx| {
        let host = cx.new(|_| Host);
        Root::new(host, window, cx)
    });
    vcx.simulate_resize(size(px(800.), px(600.)));
    vcx.update(|window, cx| {
        Toast::new("Saved")
            .id("saved")
            .duration(Duration::from_secs(3600))
            .show(window, cx)
    });
    vcx.run_until_parked();
    let a = vcx.debug_bounds("a").unwrap().center();
    let popover = vcx
        .debug_bounds("popover-body")
        .expect("the popover is open");
    let toast = vcx.debug_bounds("toast-saved").expect("the toast shows");

    vcx.simulate_mouse_down(a, gpui::MouseButton::Left, gpui::Modifiers::none());
    vcx.simulate_mouse_move(
        a + point(px(10.), px(0.)),
        gpui::MouseButton::Left,
        gpui::Modifiers::none(),
    );
    // The preview's top-left near the window's: it covers the popover and the toast.
    vcx.simulate_mouse_move(
        a - point(px(30.), px(30.)),
        gpui::MouseButton::Left,
        gpui::Modifiers::none(),
    );
    vcx.run_until_parked();
    let preview = vcx
        .debug_bounds("dnd-preview")
        .expect("the preview is drawn");
    assert!(preview.intersects(&popover) && preview.intersects(&toast));

    vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let scale = window.scale_factor();
        let quads = window.painted_quads();
        let preview_order = quads
            .iter()
            .filter(|q| q.background.as_solid() == Some(red()))
            .map(|q| q.order)
            .max()
            .expect("the preview's quad is painted");
        for (name, bounds) in [("popover", popover), ("toast", toast)] {
            let under = scaled(bounds, scale);
            let highest = quads
                .iter()
                .filter(|q| q.background.as_solid() != Some(red()) && overlaps(&q.bounds, &under))
                .map(|q| q.order)
                .max()
                .unwrap_or_else(|| panic!("the {name} paints quads"));
            assert!(
                preview_order > highest,
                "the preview ({preview_order}) is over the {name} ({highest})"
            );
        }
    });
}
