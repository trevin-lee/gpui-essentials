//! What the tests share: a flow in a window, and the mouse and keyboard to drive it.

#![allow(dead_code)]

use std::cell::RefCell;
use std::rc::Rc;

use gpui_flow::*;
use gpui_kit::{
    Entity, Focusable as _, Modifiers, MouseButton, Pixels, Point, ScrollDelta, ScrollEvent,
    TestAppContext, VisualTestContext, point, px,
};

pub type Events = Rc<RefCell<Vec<FlowEvent>>>;

pub fn open(
    cx: &mut TestAppContext,
    build: impl FnOnce(Flow) -> Flow + 'static,
) -> (Entity<Flow>, Events, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_flow::init(cx);
    });
    let (flow, cx) = cx.add_window_view(move |_, cx| build(Flow::new(cx)));
    let events: Events = Rc::default();
    cx.update(|window, cx| {
        let seen = events.clone();
        cx.subscribe(&flow, move |_, event: &FlowEvent, _| {
            seen.borrow_mut().push(event.clone())
        })
        .detach();
        flow.read(cx).focus_handle(cx).focus(window, cx);
    });
    // Twice: the second frame is drawn from what the first one measured.
    draw(cx);
    draw(cx);
    (flow, events, cx)
}

pub fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.refresh();
        let _ = window.draw(cx);
    });
    cx.run_until_parked();
}

/// Three nodes in a column, a to b joined, c apart.
pub fn three() -> (Vec<Node>, Vec<Edge>) {
    (
        vec![
            Node::new("a", xy(100., 100.)).kind("input").label("A"),
            Node::new("b", xy(100., 250.)).label("B"),
            Node::new("c", xy(400., 250.)).label("C"),
        ],
        vec![Edge::new("a-b", "a", "b")],
    )
}

pub fn with_three(flow: Flow) -> Flow {
    let (nodes, edges) = three();
    flow.with_nodes(nodes).with_edges(edges)
}

pub fn center(flow: &Entity<Flow>, cx: &VisualTestContext, id: &str) -> Point<Pixels> {
    flow.read_with(cx, |f, _| {
        f.flow_to_screen(f.node_rect(id).expect("measured").center())
    })
}

pub fn screen(flow: &Entity<Flow>, cx: &VisualTestContext, at: XY) -> Point<Pixels> {
    flow.read_with(cx, |f, _| f.flow_to_screen(at))
}

pub fn handle(
    flow: &Entity<Flow>,
    cx: &VisualTestContext,
    node: &str,
    kind: HandleKind,
) -> Point<Pixels> {
    flow.read_with(cx, |f, _| {
        let at = f.handle_point(node, kind, None).expect("a handle");
        // A point inside the dot rather than on its outer edge.
        let inward = match (kind, f.node(node).unwrap()) {
            (HandleKind::Source, n) => n.source_position.direction(),
            (HandleKind::Target, n) => n.target_position.direction(),
        };
        f.flow_to_screen(at - inward * 3.)
    })
}

pub fn drag(
    cx: &mut VisualTestContext,
    from: Point<Pixels>,
    to: Point<Pixels>,
    modifiers: Modifiers,
) {
    cx.simulate_mouse_down(from, MouseButton::Left, modifiers);
    draw(cx);
    let mid = point((from.x + to.x) / 2., (from.y + to.y) / 2.);
    cx.simulate_mouse_move(mid, MouseButton::Left, modifiers);
    draw(cx);
    cx.simulate_mouse_move(to, MouseButton::Left, modifiers);
    draw(cx);
    cx.simulate_mouse_up(to, MouseButton::Left, modifiers);
    draw(cx);
}

pub fn offset(p: Point<Pixels>, dx: f32, dy: f32) -> Point<Pixels> {
    point(p.x + px(dx), p.y + px(dy))
}

pub fn position(flow: &Entity<Flow>, cx: &VisualTestContext, id: &str) -> XY {
    flow.read_with(cx, |f, _| f.node(id).unwrap().position)
}

pub fn scroll(
    cx: &mut VisualTestContext,
    at: Point<Pixels>,
    dx: f32,
    dy: f32,
    modifiers: Modifiers,
) {
    cx.simulate_event(ScrollEvent {
        position: at,
        delta: ScrollDelta::Pixels(point(px(dx), px(dy))),
        modifiers,
        ..Default::default()
    });
    draw(cx);
}
