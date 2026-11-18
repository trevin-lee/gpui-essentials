//! Connecting two nodes from the keyboard: C on one, Tab to another, Enter, each step said.

mod common;

use common::*;
use gpui::{Entity, TestAppContext, VisualTestContext};
use gpui_flow::json::Op;
use gpui_flow::*;

fn keys(cx: &mut VisualTestContext, keys: &str) {
    cx.simulate_keystrokes(keys);
    draw(cx);
}

fn said(flow: &Entity<Flow>, cx: &VisualTestContext) -> String {
    flow.read_with(cx, |f, _| f.announcement().to_string())
}

fn on(flow: &Entity<Flow>, cx: &mut VisualTestContext, id: &str) {
    let id = NodeId::from(id.to_string());
    flow.update(cx, |f, cx| f.set_keyboard_on(Some(FlowItem::Node(id)), cx));
    draw(cx);
}

/// A, B and C as `three` has them (A to B joined), and a topic, which has no handles.
fn board(flow: Flow) -> Flow {
    let (mut nodes, edges) = three();
    nodes.push(Node::new("t", xy(700., 250.)).kind("topic").label("T"));
    flow.with_nodes(nodes).with_edges(edges)
}

#[gpui::test]
fn c_tab_and_enter_connect_two_nodes(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, board);
    on(&flow, cx, "b");
    keys(cx, "c");
    assert_eq!(
        flow.read_with(cx, |f, _| f.keyboard_connecting()),
        Some("b".into())
    );
    assert!(
        said(&flow, cx).starts_with("Connecting from B."),
        "{}",
        said(&flow, cx)
    );
    events.borrow_mut().clear();
    // Tab on to C, then Enter.
    keys(cx, "tab");
    assert_eq!(
        flow.read_with(cx, |f, _| f.keyboard_on().cloned()),
        Some(FlowItem::Node("c".into()))
    );
    keys(cx, "enter");
    assert_eq!(said(&flow, cx), "Connected B to C");
    assert_eq!(flow.read_with(cx, |f, _| f.keyboard_connecting()), None);
    let edge = flow.read_with(cx, |f, _| {
        f.edges()
            .iter()
            .find(|e| e.source == "b" && e.target == "c")
            .cloned()
            .expect("an edge from B to C")
    });
    // Told as a connection is, and sent as one edit.
    let seen = events.borrow();
    assert!(
        seen.iter()
            .any(|e| matches!(e, FlowEvent::Connect(made) if made.id == edge.id))
    );
    let batches: Vec<&Vec<Op>> = seen
        .iter()
        .filter_map(|e| match e {
            FlowEvent::Edits(ops) => Some(ops),
            _ => None,
        })
        .collect();
    assert_eq!(batches.len(), 1, "{batches:?}");
    assert!(matches!(&batches[0][..], [Op::AddEdge { .. }]));
}

#[gpui::test]
fn escape_stops_and_a_refused_or_handleless_connection_is_said(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, board);
    let edges = flow.read_with(cx, |f, _| f.edges().len());
    // Escape lets it go.
    on(&flow, cx, "b");
    keys(cx, "c");
    keys(cx, "escape");
    assert_eq!(said(&flow, cx), "Stopped connecting from B");
    assert_eq!(flow.read_with(cx, |f, _| f.keyboard_connecting()), None);
    // A to B is already joined: refused, and said.
    on(&flow, cx, "a");
    keys(cx, "c");
    on(&flow, cx, "b");
    keys(cx, "enter");
    assert_eq!(said(&flow, cx), "Can't connect A to B");
    assert_eq!(flow.read_with(cx, |f, _| f.edges().len()), edges);
    // A topic has no handles.
    on(&flow, cx, "t");
    keys(cx, "c");
    assert_eq!(said(&flow, cx), "T can't be connected");
    assert_eq!(flow.read_with(cx, |f, _| f.keyboard_connecting()), None);
}
