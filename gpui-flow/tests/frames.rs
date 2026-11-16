//! Frames: areas that hold nodes and move them together, under a title.

mod common;

use common::*;
use gpui::{Modifiers, TestAppContext, TouchEvent, TouchId, TouchPhase};
use gpui_flow::*;

/// A frame at (100, 100), 400 × 260, holding `in`, with `out` loose beside it.
fn framed(flow: Flow) -> Flow {
    flow.with_nodes(vec![
        Node::new("frame", xy(100., 100.))
            .kind("frame")
            .label("Payments")
            .size(400., 260.),
        Node::new("in", xy(40., 60.)).parent("frame").label("In"),
        Node::new("out", xy(700., 120.)).label("Out"),
    ])
}

fn parent_changes(events: &Events) -> Vec<(String, Option<String>)> {
    events
        .borrow()
        .iter()
        .filter_map(|e| match e {
            FlowEvent::NodeParentChange { node, parent } => {
                Some((node.to_string(), parent.as_ref().map(|p| p.to_string())))
            }
            _ => None,
        })
        .collect()
}

#[gpui::test]
fn a_press_inside_a_frame_is_the_panes(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, framed);
    // Inside, well clear of the title and the border: a drag pans.
    let inside = screen(&flow, cx, xy(400., 300.));
    drag(cx, inside, offset(inside, 30., 20.), Modifiers::none());
    flow.read_with(cx, |f, _| {
        assert_eq!(f.viewport(), Viewport::new(30., 20., 1.));
        assert!(f.selected_nodes().is_empty());
        assert_eq!(f.node("frame").unwrap().position, xy(100., 100.));
    });
    // Shift and drag inside it draws a rectangle that takes what's in it.
    let a = screen(&flow, cx, xy(120., 140.));
    let b = screen(&flow, cx, xy(380., 280.));
    drag(cx, a, b, Modifiers::shift());
    assert_eq!(flow.read_with(cx, |f, _| f.selected_nodes()), ["in"]);
}

#[gpui::test]
fn the_title_takes_the_frame_and_what_it_holds(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, framed);
    let title = screen(&flow, cx, xy(200., 112.));
    drag(cx, title, offset(title, 50., 40.), Modifiers::none());
    flow.read_with(cx, |f, _| {
        assert_eq!(f.selected_nodes(), ["frame"]);
        assert_eq!(f.node("frame").unwrap().position, xy(150., 140.));
        // The child moved with it, and is still its child.
        assert_eq!(f.absolute_position("in"), Some(xy(190., 200.)));
        assert_eq!(f.node("in").unwrap().parent.as_deref(), Some("frame"));
    });
}

#[gpui::test]
fn a_node_under_a_frame_can_still_be_pressed(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, |f| {
        // Added after the frame, and a loose node under its middle.
        framed(f).with_nodes(vec![
            Node::new("under", xy(300., 250.)).label("Under"),
            Node::new("frame", xy(100., 100.))
                .kind("frame")
                .label("Payments")
                .size(400., 260.),
        ])
    });
    cx.simulate_click(center(&flow, cx, "under"), Modifiers::none());
    draw(cx);
    assert_eq!(flow.read_with(cx, |f, _| f.selected_nodes()), ["under"]);
}

#[gpui::test]
fn dropped_in_a_frame_a_node_joins_it_and_dragged_out_it_leaves(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, framed);
    let out = center(&flow, cx, "out");
    let into = screen(&flow, cx, xy(330., 280.));
    drag(cx, out, into, Modifiers::none());
    flow.read_with(cx, |f, _| {
        let n = f.node("out").unwrap();
        assert_eq!(n.parent.as_deref(), Some("frame"));
        // Where it was dropped in the flow, now counted from the frame.
        let abs = f.absolute_position("out").unwrap();
        assert_eq!(n.position, abs - xy(100., 100.));
    });
    // It moves with the frame now.
    let title = screen(&flow, cx, xy(200., 112.));
    drag(cx, title, offset(title, 0., 30.), Modifiers::none());
    let before = flow.read_with(cx, |f, _| f.absolute_position("in").unwrap());
    assert_eq!(before, xy(140., 190.));
    // Dragged wholly out, `in` leaves, where it is.
    let at = center(&flow, cx, "in");
    drag(cx, at, offset(at, 600., 0.), Modifiers::none());
    flow.read_with(cx, |f, _| {
        let n = f.node("in").unwrap();
        assert_eq!(n.parent, None);
        assert_eq!(n.position, xy(740., 190.));
    });
    assert_eq!(
        parent_changes(&events),
        [("out".into(), Some("frame".into())), ("in".into(), None)]
    );
}

#[gpui::test]
fn a_frame_dropped_over_loose_nodes_takes_them_in(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, |f| {
        f.with_nodes(vec![
            Node::new("frame", xy(0., 0.))
                .kind("frame")
                .label("Empty")
                .size(300., 200.),
            Node::new("a", xy(520., 80.)).label("A"),
            Node::new("far", xy(1200., 80.)).label("Far"),
        ])
    });
    let title = screen(&flow, cx, xy(100., 12.));
    // Over `a`: the frame's body from (450, 30) to (750, 200) holds it.
    drag(cx, title, offset(title, 450., 0.), Modifiers::none());
    flow.read_with(cx, |f, _| {
        assert_eq!(f.node("a").unwrap().parent.as_deref(), Some("frame"));
        assert_eq!(f.node("far").unwrap().parent, None);
    });
    assert_eq!(
        parent_changes(&events),
        [("a".into(), Some("frame".into()))]
    );
}

#[gpui::test]
fn command_g_frames_the_selection_and_shift_command_g_unframes(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, with_three);
    flow.update(cx, |f, cx| {
        f.update_node("a", |n| n.selected = true, cx);
        f.update_node("b", |n| n.selected = true, cx);
    });
    cx.simulate_keystrokes("cmd-g");
    draw(cx);
    let frame = flow.read_with(cx, |f, _| {
        let id = f.selected_nodes()[0].clone();
        let frame = f.node(&id).unwrap();
        assert_eq!(frame.kind, "frame");
        let area = f.node_rect(&id).unwrap();
        for child in ["a", "b"] {
            assert_eq!(f.node(child).unwrap().parent.as_ref(), Some(&id));
            assert!(area.encloses(&f.node_rect(child).unwrap()));
        }
        // a and b stayed where they were in the flow.
        assert_eq!(f.absolute_position("a"), Some(xy(100., 100.)));
        id
    });
    assert!(
        events
            .borrow()
            .iter()
            .any(|e| matches!(e, FlowEvent::NodeAdd { node, .. } if *node == frame))
    );
    cx.simulate_keystrokes("cmd-shift-g");
    draw(cx);
    flow.read_with(cx, |f, _| {
        assert!(f.node(&frame).is_none());
        assert_eq!(f.node("a").unwrap().parent, None);
        assert_eq!(f.node("a").unwrap().position, xy(100., 100.));
        assert_eq!(f.nodes().len(), 3);
    });
}

#[gpui::test]
fn a_finger_inside_a_frame_pans_and_on_its_title_drags_it(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, framed);
    let touch = |cx: &mut gpui::VisualTestContext, phase, at| {
        cx.simulate_event(TouchEvent {
            id: TouchId(1),
            phase,
            position: at,
            ..Default::default()
        });
        draw(cx);
    };
    let inside = screen(&flow, cx, xy(400., 300.));
    touch(cx, TouchPhase::Started, inside);
    touch(cx, TouchPhase::Moved, offset(inside, 40., 0.));
    touch(cx, TouchPhase::Ended, offset(inside, 40., 0.));
    flow.read_with(cx, |f, _| {
        assert_eq!(f.viewport().x, 40.);
        assert_eq!(f.node("frame").unwrap().position, xy(100., 100.));
    });
    let title = screen(&flow, cx, xy(200., 112.));
    touch(cx, TouchPhase::Started, title);
    touch(cx, TouchPhase::Moved, offset(title, 0., 40.));
    touch(cx, TouchPhase::Ended, offset(title, 0., 40.));
    assert_eq!(position(&flow, cx, "frame"), xy(100., 140.));
}
