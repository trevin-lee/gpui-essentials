//! Copy, cut, paste and duplicate: the selection on the clipboard as gpui-flow's JSON, and back
//! with new ids, a step along, as one edit.

mod common;

use std::time::Duration;

use common::*;
use gpui::{ClipboardItem, Entity, TestAppContext, VisualTestContext};
use gpui_flow::ink::{InkPoint, InkStroke, InkTool};
use gpui_flow::json::{Document, Op};
use gpui_flow::*;
use serde_json::json;

fn select(flow: &Entity<Flow>, cx: &mut VisualTestContext, ids: &[&str]) {
    flow.update(cx, |f, cx| {
        f.deselect_all(cx);
        for id in ids {
            f.update_node(id, |n| n.selected = true, cx);
        }
    });
    draw(cx);
}

fn keys(cx: &mut VisualTestContext, keys: &str) {
    cx.simulate_keystrokes(keys);
    draw(cx);
}

fn edits(events: &Events) -> Vec<Vec<Op>> {
    events
        .borrow()
        .iter()
        .filter_map(|e| match e {
            FlowEvent::Edits(ops) => Some(ops.clone()),
            _ => None,
        })
        .collect()
}

/// The nodes that aren't in `before`, in the flow's order.
fn new_nodes(flow: &Entity<Flow>, cx: &VisualTestContext, before: &[&str]) -> Vec<Node> {
    flow.read_with(cx, |f, _| {
        f.nodes()
            .iter()
            .filter(|n| !before.contains(&n.id.as_ref()))
            .cloned()
            .collect()
    })
}

fn clipboard(cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()))
}

#[gpui::test]
fn a_copy_pastes_a_step_along_with_new_ids_and_its_edges(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, with_three);
    select(&flow, cx, &["a", "b"]);
    keys(cx, "cmd-c");
    // On the clipboard: gpui-flow's JSON, the two nodes and the edge between them.
    let doc = Document::from_json(&clipboard(cx).unwrap()).unwrap();
    let ids: Vec<&str> = doc.nodes.iter().map(|n| n.id.as_ref()).collect();
    assert_eq!(ids, ["a", "b"]);
    assert_eq!(doc.edges.len(), 1);
    assert!(edits(&events).is_empty(), "a copy isn't an edit");

    events.borrow_mut().clear();
    keys(cx, "cmd-v");
    let pasted = new_nodes(&flow, cx, &["a", "b", "c"]);
    assert_eq!(pasted.len(), 2);
    assert_eq!(pasted[0].position, xy(120., 120.));
    assert_eq!(pasted[1].position, xy(120., 270.));
    assert_eq!(pasted[0].label_text().as_ref(), "A");
    let new: Vec<NodeId> = pasted.iter().map(|n| n.id.clone()).collect();
    flow.read_with(cx, |f, _| {
        // Selected in place of the originals, the edge between the copies too.
        assert_eq!(f.selected_nodes(), new);
        let edge = f.edge(&f.selected_edges()[0]).unwrap();
        assert_eq!((&edge.source, &edge.target), (&new[0], &new[1]));
        assert_ne!(edge.id.as_ref(), "a-b");
        assert_eq!(f.announcement(), "Pasted 2 nodes");
    });
    // One batch: the two nodes and the edge added.
    let batches = edits(&events);
    assert_eq!(batches.len(), 1, "{batches:?}");
    let kinds: Vec<&str> = batches[0]
        .iter()
        .map(|op| match op {
            Op::AddNode { .. } => "addNode",
            Op::AddEdge { .. } => "addEdge",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, ["addNode", "addNode", "addEdge"]);

    // Again: a step further.
    keys(cx, "cmd-v");
    let again = new_nodes(&flow, cx, &["a", "b", "c", &new[0], &new[1]]);
    assert_eq!(again[0].position, xy(140., 140.));

    // Undo takes each paste back in one step.
    flow.update(cx, |f, cx| f.undo(cx));
    draw(cx);
    assert_eq!(flow.read_with(cx, |f, _| f.nodes().len()), 5);
    flow.update(cx, |f, cx| f.undo(cx));
    draw(cx);
    flow.read_with(cx, |f, _| {
        assert_eq!(f.nodes().len(), 3);
        assert_eq!(f.edges().len(), 1);
    });
}

#[gpui::test]
fn a_cut_pastes_back_where_it_was(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    select(&flow, cx, &["c"]);
    keys(cx, "cmd-x");
    flow.read_with(cx, |f, _| {
        assert!(f.node("c").is_none());
        assert_eq!(f.announcement(), "Cut 1 node");
    });
    keys(cx, "cmd-v");
    let pasted = new_nodes(&flow, cx, &["a", "b"]);
    assert_eq!(pasted.len(), 1);
    assert_ne!(pasted[0].id.as_ref(), "c");
    assert_eq!(pasted[0].position, xy(400., 250.));
    // The next lands a step along.
    keys(cx, "cmd-v");
    let next = new_nodes(&flow, cx, &["a", "b", &pasted[0].id]);
    assert_eq!(next[0].position, xy(420., 270.));
}

#[gpui::test]
fn what_isnt_a_flow_on_the_clipboard_goes_on_to_the_host(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, with_three);
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("Call Theo".into())));
    let pasted = flow.update(cx, |f, cx| f.paste(cx));
    draw(cx);
    assert!(!pasted);
    assert!(edits(&events).is_empty());
    // With nothing selected, there's nothing to copy, cut or duplicate.
    select(&flow, cx, &[]);
    flow.update(cx, |f, cx| {
        assert!(!f.copy_selected(cx));
        assert!(!f.cut_selected(cx));
        assert!(!f.duplicate_selected(cx));
    });
    assert_eq!(clipboard(cx).as_deref(), Some("Call Theo"));
}

/// A frame at (100, 100) holding `in` at (40, 60) inside it, and `out` loose.
fn framed(flow: Flow) -> Flow {
    flow.with_nodes(vec![
        Node::new("frame", xy(100., 100.))
            .kind("frame")
            .label("Payments")
            .size(400., 260.),
        Node::new("in", xy(40., 60.)).parent("frame").label("In"),
        Node::new("out", xy(700., 120.)).label("Out"),
    ])
    .with_edges(vec![Edge::new("in-out", "in", "out")])
}

#[gpui::test]
fn a_duplicated_frame_takes_what_it_holds_and_leaves_the_clipboard(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, framed);
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("kept".into())));
    select(&flow, cx, &["frame"]);
    keys(cx, "cmd-d");
    let copies = new_nodes(&flow, cx, &["frame", "in", "out"]);
    assert_eq!(copies.len(), 2);
    let (frame, inside) = (&copies[0], &copies[1]);
    assert_eq!(frame.position, xy(120., 120.));
    // The child is the new frame's, where it was inside it.
    assert_eq!(inside.parent.as_ref(), Some(&frame.id));
    assert_eq!(inside.position, xy(40., 60.));
    flow.read_with(cx, |f, _| {
        // The edge to the node left behind isn't copied.
        assert_eq!(f.edges().len(), 1);
        assert_eq!(f.announcement(), "Duplicated 2 nodes");
    });
    assert_eq!(clipboard(cx).as_deref(), Some("kept"));
}

#[gpui::test]
fn a_child_copied_without_its_frame_comes_out_where_it_is(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, framed);
    select(&flow, cx, &["in"]);
    let doc = flow.read_with(cx, |f, _| f.selection_document()).unwrap();
    assert_eq!(doc.nodes.len(), 1);
    assert_eq!(doc.nodes[0].parent, None);
    assert_eq!(doc.nodes[0].position, xy(140., 160.));
}

#[gpui::test]
fn a_paste_out_of_view_goes_in_the_middle_of_it(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    select(&flow, cx, &["c"]);
    keys(cx, "cmd-c");
    // Far off to the right.
    flow.update(cx, |f, cx| {
        f.set_viewport(Viewport::new(-5000., 0., 1.), cx)
    });
    draw(cx);
    keys(cx, "cmd-v");
    let pasted = new_nodes(&flow, cx, &["a", "b", "c"]);
    // The view shows x from 5000 on.
    let at = pasted[0].position;
    assert!(at.x > 5000. && at.x < 6000., "{at:?}");
}

#[gpui::test]
fn a_duplicated_stroke_keeps_its_ink_with_it(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, |f| {
        let points = (0..=10)
            .map(|i| InkPoint::at(100. + i as f32 * 10., 100., i as f32 / 100., 4.))
            .collect();
        let s = InkStroke::new(InkTool::Ball, 0x2040_80ff, 4., points);
        f.with_nodes(vec![Node::ink("ink", &s)])
    });
    select(&flow, cx, &["ink"]);
    events.borrow_mut().clear();
    keys(cx, "cmd-d");
    let copy = &new_nodes(&flow, cx, &["ink"])[0];
    assert_eq!(
        copy.data["ink"]["transform"],
        json!([1.0, 0.0, 0.0, 1.0, 20.0, 20.0])
    );
    flow.read_with(cx, |f, _| {
        assert_eq!(
            f.node("ink").unwrap().data["ink"]["points"],
            copy.data["ink"]["points"]
        );
    });
}

/// A root with two children, the first with one of its own, laid out to the right.
fn map(flow: Flow) -> Flow {
    let topic = |id: &str| {
        Node::new(id, xy(0., 0.))
            .kind("topic")
            .label(id.to_uppercase())
    };
    flow.with_nodes(vec![topic("root"), topic("a"), topic("b"), topic("a1")])
        .with_edges(vec![
            Edge::new("root-a", "root", "a"),
            Edge::new("root-b", "root", "b"),
            Edge::new("a-a1", "a", "a1"),
        ])
        .with_viewport(Viewport::new(150., 250., 1.))
        .with_options(|o| {
            o.layout = Layout::Tree(TreeLayout {
                motion: Duration::ZERO,
                ..TreeLayout::default()
            })
        })
}

fn children(flow: &Entity<Flow>, cx: &VisualTestContext, id: &str) -> Vec<NodeId> {
    flow.read_with(cx, |f, _| f.tree().children_of(&id.into()).to_vec())
}

#[gpui::test]
fn in_a_mind_map_a_duplicate_is_the_next_sibling_with_its_subtree(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, map);
    select(&flow, cx, &["a"]);
    keys(cx, "cmd-d");
    let copies = new_nodes(&flow, cx, &["root", "a", "b", "a1"]);
    assert_eq!(copies.len(), 2);
    let (top, under) = (copies[0].id.clone(), copies[1].id.clone());
    // Right after "a", before "b"; its own child copied under it; only it selected.
    assert_eq!(
        children(&flow, cx, "root"),
        ["a".into(), top.clone(), "b".into()]
    );
    assert_eq!(children(&flow, cx, &top), [under]);
    assert_eq!(flow.read_with(cx, |f, _| f.selected_nodes()), [top]);
}

#[gpui::test]
fn in_a_mind_map_a_paste_goes_under_the_selected_node(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, map);
    select(&flow, cx, &["a"]);
    keys(cx, "cmd-c");
    select(&flow, cx, &["b"]);
    events.borrow_mut().clear();
    keys(cx, "cmd-v");
    let copies = new_nodes(&flow, cx, &["root", "a", "b", "a1"]);
    assert_eq!(children(&flow, cx, "b"), [copies[0].id.clone()]);
    assert_eq!(children(&flow, cx, &copies[0].id), [copies[1].id.clone()]);
    // One batch, one undo step.
    assert_eq!(edits(&events).len(), 1);
    flow.update(cx, |f, cx| f.undo(cx));
    draw(cx);
    assert!(children(&flow, cx, "b").is_empty());
}
