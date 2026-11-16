//! A large flow draws only what's in view: the work of a frame follows what's on screen, not
//! how much there is. (How long frames take is `benches/scale.rs`'s to say.) Text is laid out
//! with Core Text, as on screen, so what's drawn of it can be counted.

#![cfg(target_os = "macos")]

use std::sync::Arc;

use gpui::{Entity, TestDispatcher, VisualTestContext};
use gpui_flow::*;

fn open(count: usize) -> (Entity<Flow>, &'static mut VisualTestContext) {
    let cx = Box::leak(Box::new(gpui::TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        Arc::new(gpui_apple::AppleTextSystem::new()),
    )));
    cx.update(|cx| {
        gpui_core::init(cx);
        gpui_flow::init(cx);
    });
    let (flow, cx) = cx.add_window_view(move |_, cx| grid(count)(Flow::new(cx)));
    draw(cx);
    draw(cx);
    (flow, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.refresh();
        let _ = window.draw(cx);
    });
    cx.run_until_parked();
}

fn glyphs(cx: &mut VisualTestContext) -> usize {
    cx.update(|window, _| {
        let scene = window.rendered_scene();
        scene.monochrome_sprites.len() + scene.subpixel_sprites.len()
    })
}

/// A grid of `count` nodes, each joined to the next, a node every 200 across and 110 down.
fn grid(count: usize) -> impl FnOnce(Flow) -> Flow {
    move |flow| {
        let columns = (count as f32).sqrt().ceil() as usize;
        let nodes = (0..count)
            .map(|i| {
                let at = xy((i % columns) as f32 * 200., (i / columns) as f32 * 110.);
                Node::new(format!("n{i}"), at).label(format!("Node {i}"))
            })
            .collect();
        let edges = (1..count)
            .map(|i| Edge::new(format!("e{i}"), format!("n{}", i - 1), format!("n{i}")))
            .collect();
        flow.with_nodes(nodes)
            .with_edges(edges)
            .with_options(|o| o.min_zoom = 0.02)
    }
}

/// How many nodes have been drawn (and so measured) so far.
fn drawn(flow: &Entity<Flow>, cx: &VisualTestContext) -> usize {
    flow.read_with(cx, |f, _| {
        f.nodes()
            .iter()
            .filter(|n| f.node_rect(&n.id).is_some())
            .count()
    })
}

#[test]
fn of_5000_nodes_only_those_in_view_are_drawn() {
    let (flow, cx) = open(5000);
    // A few frames: what's in view, and a few hundred more each to measure, never all at
    // once.
    let first = drawn(&flow, cx);
    assert!(first > 0 && first < 2000, "{first} of 5000 drawn");
    // The rest are measured a few hundred a frame, then frames stop.
    for _ in 0..20 {
        draw(cx);
    }
    assert_eq!(drawn(&flow, cx), 5000);
    // Then a frame lays out only what's on screen: its text is a screen's worth.
    let glyphs = glyphs(cx);
    assert!(
        glyphs > 100 && glyphs < 2000,
        "{glyphs} glyphs for one screen"
    );
    let paths = cx.update(|window, _| window.rendered_scene().paths.len());
    assert!(paths < 400, "{paths} paths for one screen");
}

#[test]
fn zoomed_right_out_nodes_are_boxes_with_no_text() {
    let (flow, cx) = open(2000);
    assert!(glyphs(cx) > 100, "text drawn at 100%");
    flow.update(cx, |f, cx| f.set_viewport(Viewport::new(10., 10., 0.1), cx));
    // Once to measure what's new in view, then drawn as boxes.
    draw(cx);
    draw(cx);
    draw(cx);
    assert_eq!(glyphs(cx), 0, "text laid out while too small to read");
}
