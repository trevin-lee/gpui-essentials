//! What assistive technology gets of a flow: the canvas, each node and each edge as a group
//! with a role and a name, as React Flow's are. GPUI's test windows build no accessibility
//! tree, so the flow reports what it draws to gpui-core's audit instead.

mod common;

use common::*;
use gpui::{Role, TestAppContext};
use gpui_flow::*;

fn names(what: &str, seen: &[gpui_core::Audited]) -> Vec<String> {
    let mut names: Vec<String> = seen
        .iter()
        .filter(|a| a.what == what)
        .map(|a| {
            assert_eq!(a.role, Role::Group, "{what} {:?}", a.label);
            a.label.as_ref().map(|l| l.to_string()).expect("a name")
        })
        .collect();
    names.sort();
    names.dedup();
    names
}

#[gpui::test]
fn the_canvas_its_nodes_and_its_edges_are_named_groups(cx: &mut TestAppContext) {
    gpui_core::audit_start();
    let (_flow, _, cx) = open(cx, |f| {
        f.with_nodes(vec![
            Node::new("a", xy(100., 100.)).kind("input").label("Start"),
            Node::new("b", xy(100., 250.)).label("Check the order"),
            Node::new("c", xy(400., 250.)).aria_label("Ship it"),
            Node::new("d", xy(400., 400.)),
        ])
        .with_edges(vec![
            Edge::new("a-b", "a", "b"),
            Edge::new("b-c", "b", "c").label("if paid"),
            Edge::new("c-d", "c", "d").aria_label("Then archive"),
        ])
        .with_options(|o| o.aria_label = "Order flow".into())
    });
    draw(cx);
    let seen = gpui_core::audit_take();
    assert_eq!(names("Flow", &seen), ["Order flow"]);
    // Their own name (ariaLabel), else their label, else their kind.
    assert_eq!(
        names("FlowNode", &seen),
        ["Check the order", "Ship it", "Start", "default node"]
    );
    assert_eq!(
        names("FlowEdge", &seen),
        [
            "Edge from Start to Check the order",
            "Then archive",
            "if paid, edge from Check the order to Ship it",
        ]
    );
}

#[gpui::test]
fn names_are_saved_as_react_flows_aria_label(cx: &mut TestAppContext) {
    let node = Node::new("c", xy(0., 0.)).aria_label("Ship it");
    let json = serde_json::to_value(&node).unwrap();
    assert_eq!(json["ariaLabel"], "Ship it");
    let back: Node = serde_json::from_value(json).unwrap();
    assert_eq!(back.accessible_name(), "Ship it");
    let edge = Edge::new("e", "a", "b").aria_label("Then archive");
    assert_eq!(
        serde_json::to_value(&edge).unwrap()["ariaLabel"],
        "Then archive"
    );
    let _ = cx;
}
