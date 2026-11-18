//! A node dragged only by its handle, as React Flow's `dragHandle`.

mod common;

use common::*;
use gpui::{
    InteractiveElement as _, IntoElement as _, Modifiers, ParentElement as _, Styled as _,
    TestAppContext, div, px,
};
use gpui_flow::*;

/// A card with a grip along its top: the only place it's dragged by.
fn cards(flow: Flow) -> Flow {
    flow.with_node_kind("card", |_, ctx, _, _, _| {
        div()
            .w(px(150.))
            .child(ctx.drag_handle(div().debug_selector(|| "grip".into()).h(px(12.)).w_full()))
            .child(
                div()
                    .debug_selector(|| "body".into())
                    .h(px(40.))
                    .child("Notes"),
            )
            .into_any_element()
    })
    .with_nodes(vec![
        Node::new("a", xy(100., 100.))
            .kind("card")
            .drag_handle(".grip"),
        Node::new("b", xy(400., 100.)).label("B"),
    ])
}

#[gpui::test]
fn a_node_with_a_handle_moves_only_by_it(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, cards);
    // By its body: selected, not moved.
    let body = cx.debug_bounds("body").unwrap().center();
    drag(cx, body, offset(body, 80., 40.), Modifiers::none());
    assert_eq!(position(&flow, cx, "a"), xy(100., 100.));
    assert!(flow.read_with(cx, |f, _| f.node("a").unwrap().selected));
    // By its grip: moved.
    let grip = cx.debug_bounds("grip").unwrap().center();
    drag(cx, grip, offset(grip, 80., 40.), Modifiers::none());
    assert_eq!(position(&flow, cx, "a"), xy(180., 140.));
    // A node without one still moves by anywhere on it.
    let b = center(&flow, cx, "b");
    drag(cx, b, offset(b, 20., 0.), Modifiers::none());
    assert_eq!(position(&flow, cx, "b"), xy(420., 100.));
}

#[gpui::test]
fn the_handle_is_saved_as_react_flows_drag_handle(cx: &mut TestAppContext) {
    let node = Node::new("a", xy(0., 0.)).drag_handle(".grip");
    let json = serde_json::to_value(&node).unwrap();
    assert_eq!(json["dragHandle"], ".grip");
    let back: Node = serde_json::from_value(json).unwrap();
    assert!(back.has_drag_handle());
    assert!(!Node::new("b", xy(0., 0.)).has_drag_handle());
    let _ = cx;
}
