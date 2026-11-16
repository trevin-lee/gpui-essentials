//! Nodes that link to something of the host's (a document, in Docs): the flow shows a button
//! and tells the host, which opens it. The flow never opens anything itself.

mod common;

use common::*;
use gpui::{Modifiers, TestAppContext, TouchEvent, TouchId, TouchPhase};
use gpui_flow::*;

fn linked(flow: Flow) -> Flow {
    flow.with_nodes(vec![
        Node::new("doc", xy(100., 100.))
            .label("Trip notes")
            .link("doc:42"),
        Node::new("plain", xy(100., 250.)).label("Plain"),
        Node::new("topic", xy(400., 100.))
            .kind("topic")
            .label("Topic")
            .link("doc:7"),
    ])
}

fn opened(events: &Events) -> Vec<(String, String)> {
    events
        .borrow()
        .iter()
        .filter_map(|e| match e {
            FlowEvent::OpenLink { node, link } => Some((node.to_string(), link.to_string())),
            _ => None,
        })
        .collect()
}

#[gpui::test]
fn the_button_asks_the_host_to_open_the_link(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, linked);
    // Look for the button along the middle of the node: the first point that opens the link.
    let r = flow.read_with(cx, |f, _| f.node_rect("doc").unwrap());
    let mut found = false;
    for step in 0..40 {
        let x = r.right() - 4. - step as f32 * 3.;
        // A miss selects the node; only the click that hits the button counts.
        flow.update(cx, |f, cx| f.deselect_all(cx));
        draw(cx);
        events.borrow_mut().clear();
        cx.simulate_click(screen(&flow, cx, xy(x, r.y + r.h / 2.)), Modifiers::none());
        draw(cx);
        if !opened(&events).is_empty() {
            found = true;
            break;
        }
    }
    assert!(found, "no link button on the node");
    assert_eq!(opened(&events), [("doc".into(), "doc:42".into())]);
    // The press was the button's: the node wasn't selected, clicked or dragged.
    assert!(!events.borrow().iter().any(|e| matches!(
        e,
        FlowEvent::NodeClick(_) | FlowEvent::NodeDragStart(_) | FlowEvent::SelectionChange { .. }
    )));
}

#[gpui::test]
fn command_enter_opens_the_selected_nodes_link(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, linked);
    flow.update(cx, |f, cx| {
        f.update_node("topic", |n| n.selected = true, cx);
    });
    cx.simulate_keystrokes("cmd-enter");
    draw(cx);
    assert_eq!(opened(&events), [("topic".into(), "doc:7".into())]);
    // A node without a link: nothing, and the key goes on.
    flow.update(cx, |f, cx| {
        f.update_node("topic", |n| n.selected = false, cx);
        f.update_node("plain", |n| n.selected = true, cx);
    });
    cx.simulate_keystrokes("cmd-enter");
    draw(cx);
    assert_eq!(opened(&events).len(), 1);
    // Nodes without a link have no button: a click in the same place selects the node.
    let r = flow.read_with(cx, |f, _| f.node_rect("plain").unwrap());
    cx.simulate_click(
        screen(&flow, cx, xy(r.right() - 20., r.y + r.h / 2.)),
        Modifiers::none(),
    );
    draw(cx);
    assert_eq!(flow.read_with(cx, |f, _| f.selected_nodes()), ["plain"]);
}

#[gpui::test]
fn a_finger_taps_the_button(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, linked);
    let r = flow.read_with(cx, |f, _| f.node_rect("topic").unwrap());
    // The topic's row is its label then the button, so the button ends near its right side.
    for step in 0..30 {
        let at = screen(
            &flow,
            cx,
            xy(r.right() - 6. - step as f32 * 3., r.y + r.h / 2.),
        );
        for phase in [TouchPhase::Started, TouchPhase::Ended] {
            cx.simulate_event(TouchEvent {
                id: TouchId(1),
                phase,
                position: at,
                ..Default::default()
            });
            draw(cx);
        }
        if !opened(&events).is_empty() {
            break;
        }
    }
    assert_eq!(opened(&events), [("topic".into(), "doc:7".into())]);
}

#[test]
fn a_link_saves_and_loads() {
    let node = Node::new("n", xy(0., 0.)).link("doc:42");
    let json = serde_json::to_value(&node).unwrap();
    assert_eq!(json["link"], "doc:42");
    let back: Node = serde_json::from_value(json).unwrap();
    assert_eq!(back, node);
}
