//! What assistive technology gets of a flow: the canvas, each node and each edge as a group
//! with a role and a name, as React Flow's are. GPUI's test windows build no accessibility
//! tree, so the flow reports what it draws to gpui-core's audit instead.

mod common;

use common::*;
use gpui::{Focusable as _, Role, TestAppContext};
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

/// What the canvas says in its live region, which assistive technology reads out as it changes:
/// a node moved by keyboard and where to, a connection made, what was deleted.
#[gpui::test]
fn moves_connections_and_deletions_are_announced(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    let said =
        |cx: &gpui::VisualTestContext| flow.read_with(cx, |f, _| f.announcement().to_string());
    assert_eq!(said(cx), "");
    cx.simulate_keystrokes("tab right");
    assert_eq!(said(cx), "Moved A. New position, x: 105, y: 100");
    let from = handle(&flow, cx, "a", HandleKind::Source);
    let to = handle(&flow, cx, "c", HandleKind::Target);
    drag(cx, from, to, gpui::Modifiers::none());
    assert_eq!(said(cx), "Connected A to C");
    // B, and the edge from A to it.
    cx.simulate_keystrokes("escape tab tab delete");
    assert_eq!(said(cx), "Deleted 1 node and 1 edge");
}

/// Every control the flow draws is named: each button (zoom, fit, lock, a tree's fold chip, a
/// node's link button, a link card's open button, the edit toolbar's), and the minimap. A
/// button drawn without a name fails here.
#[gpui::test]
fn every_control_the_flow_draws_has_a_name(cx: &mut TestAppContext) {
    gpui_core::audit_start();
    let (flow, _, cx) = open(cx, |f| {
        let topic = |id: &str| {
            Node::new(id, xy(0., 0.))
                .kind("topic")
                .label(id.to_uppercase())
        };
        f.with_nodes(vec![
            topic("root").link("doc:42"),
            topic("a"),
            Node::new("card", xy(0., 0.)).link_card("https://example.com"),
        ])
        .with_edges(vec![
            Edge::new("root-a", "root", "a"),
            Edge::new("root-card", "root", "card"),
        ])
        .with_viewport(Viewport::new(150., 250., 1.))
        .with_controls()
        .with_minimap()
        .with_options(|o| {
            o.layout = Layout::Tree(TreeLayout {
                motion: std::time::Duration::ZERO,
                ..TreeLayout::default()
            });
            o.edit_toolbar = true;
        })
    });
    flow.update(cx, |f, cx| f.update_node("a", |n| n.selected = true, cx));
    draw(cx);
    let seen = gpui_core::audit_take();
    let unnamed: Vec<&gpui_core::Audited> = seen
        .iter()
        .filter(|a| a.role == Role::Button)
        .filter(|a| a.label.as_ref().is_none_or(|l| l.trim().is_empty()))
        .collect();
    assert!(unnamed.is_empty(), "buttons with no name: {unnamed:?}");
    let named = |what: &str| {
        let mut n: Vec<String> = seen
            .iter()
            .filter(|a| a.what == what)
            .filter_map(|a| a.label.as_ref().map(|l| l.to_string()))
            .collect();
        n.sort();
        n.dedup();
        n
    };
    assert_eq!(
        named("FlowControl"),
        ["Fit view", "Lock the canvas", "Zoom in", "Zoom out"]
    );
    assert_eq!(named("FlowMinimap"), ["Mini map"]);
    assert_eq!(
        named("FlowNodeButton"),
        [
            "Fold away what's under ROOT",
            "Open the link",
            "Open the page"
        ]
    );
    // The edit toolbar's, gpui-core buttons.
    for name in ["Copy", "Paste", "Duplicate", "Delete"] {
        assert!(named("Press").iter().any(|n| n == name), "{name}");
    }

    // Locked, the lock says it unlocks; folded, the chip says it shows what's under.
    flow.update(cx, |f, cx| {
        f.set_interactive(false, cx);
        f.set_collapsed("root", true, cx);
    });
    gpui_core::audit_start();
    draw(cx);
    let seen = gpui_core::audit_take();
    let labels: Vec<String> = seen
        .iter()
        .filter_map(|a| a.label.as_ref().map(|l| l.to_string()))
        .collect();
    assert!(
        labels.iter().any(|l| l == "Unlock the canvas"),
        "{labels:?}"
    );
    assert!(
        labels.iter().any(|l| l == "Show what's under ROOT"),
        "{labels:?}"
    );
}

/// The controls are pressed as buttons: by a click, which leaves the canvas its keys.
#[gpui::test]
fn the_controls_zoom_by_a_click_and_the_canvas_keeps_the_keys(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, |f| with_three(f).with_controls());
    let before = flow.read_with(cx, |f, _| f.viewport().zoom);
    let at = cx
        .debug_bounds("flow-control-Zoom in")
        .expect("the zoom-in button")
        .center();
    cx.simulate_click(at, gpui::Modifiers::none());
    // Past any zoom transition.
    for _ in 0..5 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(200));
        draw(cx);
    }
    let after = flow.read_with(cx, |f, _| f.viewport().zoom);
    assert!(after > before, "{before} {after}");
    let focused = cx.update(|window, cx| flow.read(cx).focus_handle(cx).is_focused(window));
    assert!(focused, "the canvas keeps the focus");
}
