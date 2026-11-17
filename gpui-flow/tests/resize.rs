//! A pane that changes size (a window resized, a phone's keyboard up or down): a flow still as
//! its fit left it fits again; otherwise what's selected stays in view.

mod common;

use common::*;
use gpui::{TestAppContext, VisualTestContext, px, size};
use gpui_flow::*;

/// Whether the node is wholly in the pane as it's drawn now.
fn in_view(flow: &gpui::Entity<Flow>, cx: &VisualTestContext, id: &str, w: f32, h: f32) -> bool {
    flow.read_with(cx, |f, _| {
        let r = f.node_rect(id).expect("measured");
        let (a, b) = (
            f.flow_to_screen(xy(r.x, r.y)),
            f.flow_to_screen(xy(r.x + r.w, r.y + r.h)),
        );
        f32::from(a.x) >= 0. && f32::from(a.y) >= 0. && f32::from(b.x) <= w && f32::from(b.y) <= h
    })
}

#[gpui::test]
fn a_fitted_flow_fits_again_when_its_pane_shrinks(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, |f| with_three(f).fit_view_on_start());
    cx.simulate_resize(size(px(900.), px(700.)));
    draw(cx);
    draw(cx);
    let before = flow.read_with(cx, |f, _| f.viewport());
    // A keyboard takes most of the height.
    cx.simulate_resize(size(px(900.), px(260.)));
    draw(cx);
    draw(cx);
    let after = flow.read_with(cx, |f, _| f.viewport());
    assert_ne!(before, after);
    for id in ["a", "b", "c"] {
        assert!(
            in_view(&flow, cx, id, 900., 260.),
            "{id} out of view at {after:?}"
        );
    }
}

#[gpui::test]
fn a_moved_flow_keeps_the_selected_node_in_view_at_its_zoom(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    cx.simulate_resize(size(px(900.), px(700.)));
    draw(cx);
    // Moved by hand, then C selected near the foot of the pane.
    flow.update(cx, |f, cx| {
        f.set_viewport(Viewport::new(0., 300., 1.), cx);
        f.update_node("c", |n| n.selected = true, cx);
    });
    draw(cx);
    assert!(in_view(&flow, cx, "c", 900., 700.));
    cx.simulate_resize(size(px(900.), px(300.)));
    draw(cx);
    draw(cx);
    let v = flow.read_with(cx, |f, _| f.viewport());
    assert_eq!(v.zoom, 1.);
    assert!(in_view(&flow, cx, "c", 900., 300.), "{v:?}");
    // Back to its height: nothing moves (C is in view).
    cx.simulate_resize(size(px(900.), px(700.)));
    draw(cx);
    draw(cx);
    assert_eq!(flow.read_with(cx, |f, _| f.viewport()), v);
}
