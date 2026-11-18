//! The canvas by keyboard, as React Flow's: Tab and shift-Tab move between the nodes (as they
//! read) and then the edges, and out of the canvas past the last; enter or space selects what
//! the keyboard is on, the arrows move it, delete removes it, and escape leaves it.

mod common;

use common::*;
use gpui::{Modifiers, MouseButton, TestAppContext};
use gpui_flow::*;

fn on(flow: &gpui::Entity<Flow>, cx: &gpui::VisualTestContext) -> Option<FlowItem> {
    flow.read_with(cx, |f, _| f.keyboard_on().cloned())
}

fn node(id: &str) -> Option<FlowItem> {
    Some(FlowItem::Node(id.into()))
}

#[gpui::test]
fn tab_goes_through_the_nodes_as_they_read_then_the_edges_then_out(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    let mut seen = Vec::new();
    for _ in 0..5 {
        cx.simulate_keystrokes("tab");
        draw(cx);
        seen.push(on(&flow, cx));
    }
    assert_eq!(
        seen,
        [
            node("a"),
            node("b"),
            node("c"),
            Some(FlowItem::Edge("a-b".into())),
            None
        ]
    );
    // Shift-tab goes back from the end.
    cx.simulate_keystrokes("shift-tab");
    assert_eq!(on(&flow, cx), Some(FlowItem::Edge("a-b".into())));
    cx.simulate_keystrokes("shift-tab");
    assert_eq!(on(&flow, cx), node("c"));
}

#[gpui::test]
fn enter_or_space_selects_what_the_keyboard_is_on(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    cx.simulate_keystrokes("tab tab enter");
    let selected = |cx: &gpui::VisualTestContext| {
        flow.read_with(cx, |f, _| {
            f.nodes()
                .iter()
                .filter(|n| n.selected)
                .map(|n| n.id.to_string())
                .collect::<Vec<_>>()
        })
    };
    assert_eq!(selected(cx), ["b"]);
    cx.simulate_keystrokes("tab space");
    assert_eq!(selected(cx), ["c"]);
}

#[gpui::test]
fn the_arrows_move_the_node_the_keyboard_is_on(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, with_three);
    cx.simulate_keystrokes("tab right right down shift-left");
    // Two steps right, one down, four left: 5 a step.
    assert_eq!(position(&flow, cx, "a"), xy(100. + 10. - 20., 105.));
    assert!(
        events
            .borrow()
            .iter()
            .any(|e| matches!(e, FlowEvent::NodeMove(ids) if ids.len() == 1 && ids[0] == "a"))
    );
    // Nothing else moved, and nothing was selected for it.
    assert_eq!(position(&flow, cx, "b"), xy(100., 250.));
    assert!(flow.read_with(cx, |f, _| f.nodes().iter().all(|n| !n.selected)));
}

#[gpui::test]
fn delete_removes_what_the_keyboard_is_on(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    cx.simulate_keystrokes("tab tab tab delete");
    flow.read_with(cx, |f, _| {
        assert!(f.node("c").is_none());
        assert_eq!(f.nodes().len(), 2);
        assert!(f.keyboard_on().is_none());
    });
}

#[gpui::test]
fn escape_leaves_and_a_click_takes_the_keyboard_off(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    cx.simulate_keystrokes("tab escape");
    assert_eq!(on(&flow, cx), None);
    cx.simulate_keystrokes("tab");
    assert_eq!(on(&flow, cx), node("a"));
    let at = center(&flow, cx, "c");
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    draw(cx);
    assert_eq!(on(&flow, cx), None);
}

#[gpui::test]
fn the_keyboard_leaves_a_node_that_goes(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    cx.simulate_keystrokes("tab");
    flow.update(cx, |f, cx| {
        f.set_nodes(
            f.nodes().iter().filter(|n| n.id != "a").cloned().collect(),
            cx,
        )
    });
    draw(cx);
    assert_eq!(on(&flow, cx), None);
}
