//! Edits as small operations by id: what the flow sends, and what it takes from elsewhere.

mod common;

use common::*;
use gpui::{AppContext as _, Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext};
use gpui_flow::json::{self, Document, Op};
use gpui_flow::*;
use serde_json::{Map, Value, json};

/// The batches of edits sent so far, oldest first.
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

fn patch(v: Value) -> Map<String, Value> {
    v.as_object().unwrap().clone()
}

fn select(flow: &Entity<Flow>, cx: &mut VisualTestContext, ids: &[&str]) {
    flow.update(cx, |f, cx| {
        f.deselect_all(cx);
        for id in ids {
            f.update_node(id, |n| n.selected = true, cx);
        }
    });
    draw(cx);
}

#[gpui::test]
fn a_drag_of_many_nodes_is_one_batch_sent_at_its_end(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, with_three);
    select(&flow, cx, &["a", "c"]);
    events.borrow_mut().clear();
    let at = center(&flow, cx, "a");
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(offset(at, 20., 0.), MouseButton::Left, Modifiers::none());
    draw(cx);
    cx.simulate_mouse_move(offset(at, 50., 10.), MouseButton::Left, Modifiers::none());
    draw(cx);
    assert!(edits(&events).is_empty(), "nothing is sent mid-drag");
    cx.simulate_mouse_up(offset(at, 50., 10.), MouseButton::Left, Modifiers::none());
    draw(cx);
    assert_eq!(
        edits(&events),
        [vec![
            Op::PatchNode {
                id: "a".into(),
                patch: patch(json!({ "position": { "x": 150.0, "y": 110.0 } })),
            },
            Op::PatchNode {
                id: "c".into(),
                patch: patch(json!({ "position": { "x": 450.0, "y": 260.0 } })),
            },
        ]]
    );
}

#[gpui::test]
fn a_patch_has_only_what_changed(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, with_three);
    flow.update(cx, |f, cx| {
        f.update_node(
            "b",
            |n| {
                n.data["label"] = "Bee".into();
                n.color = Some("teal".into());
            },
            cx,
        );
        f.update_edge("a-b", |e| e.label = Some("then".into()), cx);
    });
    draw(cx);
    assert_eq!(
        edits(&events),
        [vec![
            Op::PatchNode {
                id: "b".into(),
                patch: patch(json!({ "data": { "label": "Bee" }, "color": "teal" })),
            },
            Op::PatchEdge {
                id: "a-b".into(),
                patch: patch(json!({ "label": "then" })),
            },
        ]]
    );
    // Taking a field away is a null.
    flow.update(cx, |f, cx| f.update_node("b", |n| n.color = None, cx));
    draw(cx);
    assert_eq!(
        edits(&events)[1],
        [Op::PatchNode {
            id: "b".into(),
            patch: patch(json!({ "color": null })),
        }]
    );
}

#[gpui::test]
fn selecting_panning_and_zooming_are_not_edits(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, with_three);
    select(&flow, cx, &["a", "b"]);
    flow.update(cx, |f, cx| {
        f.select_all(cx);
        f.set_viewport(Viewport::new(30., 40., 1.5), cx);
    });
    let at = center(&flow, cx, "c");
    scroll(cx, offset(at, 300., 0.), 0., 40., Modifiers::none());
    draw(cx);
    assert!(edits(&events).is_empty(), "{:?}", edits(&events));
}

#[gpui::test]
fn connecting_and_deleting_send_adds_and_removals(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, with_three);
    let edge = flow.update(cx, |f, cx| {
        f.connect(
            Connection {
                source: "b".into(),
                source_handle: None,
                target: "c".into(),
                target_handle: None,
            },
            cx,
        )
        .unwrap()
    });
    draw(cx);
    assert_eq!(
        edits(&events),
        [vec![Op::AddEdge {
            edge: Box::new(edge.clone())
        }]]
    );

    // Deleting b takes both its edges, before b itself.
    select(&flow, cx, &["b"]);
    flow.update(cx, |f, cx| f.delete_selected(cx));
    draw(cx);
    let mut want = vec![
        Op::RemoveEdge { id: "a-b".into() },
        Op::RemoveEdge {
            id: edge.id.clone(),
        },
    ];
    want.sort_by(|a, b| a.id().cmp(b.id()));
    want.push(Op::RemoveNode { id: "b".into() });
    assert_eq!(edits(&events)[1], want);
}

#[gpui::test]
fn framing_is_one_batch(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, with_three);
    select(&flow, cx, &["b", "c"]);
    events.borrow_mut().clear();
    let frame = flow.update(cx, |f, cx| f.frame_selection("Group", cx).unwrap());
    draw(cx);
    let batches = edits(&events);
    assert_eq!(batches.len(), 1, "{batches:?}");
    let ops = &batches[0];
    assert!(matches!(&ops[0], Op::AddNode { node } if node.id == frame && !node.selected));
    for id in ["b", "c"] {
        assert!(ops.iter().any(|op| matches!(op,
            Op::PatchNode { id: i, patch } if i == id && patch["parentId"] == frame.as_ref())));
    }
}

#[gpui::test]
fn edits_from_elsewhere_are_applied_and_not_sent_back(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, with_three);
    select(&flow, cx, &["a", "c"]);
    let ops = vec![
        Op::PatchNode {
            id: "a".into(),
            patch: patch(json!({ "position": { "x": 5 }, "data": { "label": "Start" } })),
        },
        Op::AddNode {
            node: Box::new(Node::new("d", xy(700., 100.)).label("D")),
        },
        Op::AddEdge {
            edge: Box::new(Edge::new("c-d", "c", "d")),
        },
        Op::RemoveNode { id: "c".into() },
        Op::PatchNode {
            id: "nowhere".into(),
            patch: patch(json!({ "hidden": true })),
        },
    ];
    flow.update(cx, |f, cx| f.apply(&ops, cx));
    draw(cx);
    flow.read_with(cx, |f, _| {
        let a = f.node("a").unwrap();
        assert_eq!(a.position, xy(5., 100.));
        assert_eq!(a.label_text(), "Start");
        assert!(f.node("c").is_none());
        assert!(f.node("d").is_some());
        // a stays selected; c went, so it isn't.
        assert_eq!(f.selected_nodes(), ["a"]);
    });
    assert!(edits(&events).is_empty(), "{:?}", edits(&events));
}

#[gpui::test]
fn an_edit_from_elsewhere_and_ones_own_unsent_edit_both_survive(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, with_three);
    select(&flow, cx, &["b"]);
    let at = center(&flow, cx, "b");
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(offset(at, 30., 0.), MouseButton::Left, Modifiers::none());
    draw(cx);
    // Mid-drag, someone renames b.
    flow.update(cx, |f, cx| {
        f.apply(
            &[Op::PatchNode {
                id: "b".into(),
                patch: patch(json!({ "data": { "label": "Renamed" } })),
            }],
            cx,
        )
    });
    cx.simulate_mouse_move(offset(at, 60., 0.), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(offset(at, 60., 0.), MouseButton::Left, Modifiers::none());
    draw(cx);
    // Only the move is sent: the rename was theirs.
    assert_eq!(
        edits(&events),
        [vec![Op::PatchNode {
            id: "b".into(),
            patch: patch(json!({ "position": { "x": 160.0 } })),
        }]]
    );
    assert_eq!(
        flow.read_with(cx, |f, _| f.node("b").unwrap().label_text()),
        "Renamed"
    );
}

#[gpui::test]
fn two_people_editing_one_node_end_up_with_the_same_flow(cx: &mut TestAppContext) {
    let (mine, my_events, cx) = open(cx, with_three);
    let theirs = cx.update(|_, cx| {
        let (nodes, edges) = three();
        cx.new(|cx| Flow::new(cx).with_nodes(nodes).with_edges(edges))
    });
    let their_events: Events = Default::default();
    cx.update(|_, cx| {
        let seen = their_events.clone();
        cx.subscribe(&theirs, move |_, e: &FlowEvent, _| {
            seen.borrow_mut().push(e.clone())
        })
        .detach();
    });
    let mut server = Document::new(three().0, three().1);
    // I move b; they recolour it and add a node, at the same time.
    mine.update(cx, |f, cx| f.update_node("b", |n| n.position.x = 180., cx));
    theirs.update(cx, |f, cx| {
        f.update_node("b", |n| n.color = Some("red".into()), cx);
        f.add_node(Node::new("e", xy(0., 0.)).label("E"), cx);
    });
    cx.run_until_parked();
    let (my_ops, their_ops) = (edits(&my_events).concat(), edits(&their_events).concat());
    assert!(!my_ops.is_empty() && !their_ops.is_empty());
    // Each is stored, and each flow is given the other's.
    server.apply(&my_ops);
    server.apply(&their_ops);
    mine.update(cx, |f, cx| f.apply(&their_ops, cx));
    theirs.update(cx, |f, cx| f.apply(&my_ops, cx));
    cx.run_until_parked();
    let a = mine.read_with(cx, |f, _| f.document());
    let b = theirs.read_with(cx, |f, _| f.document());
    assert_eq!(a.nodes, b.nodes);
    assert_eq!(a.nodes, server.nodes);
    let node = a.nodes.iter().find(|n| n.id == "b").unwrap();
    assert_eq!(
        (node.position.x, node.color.as_deref()),
        (180., Some("red"))
    );
    // And nothing was sent back for what was applied.
    assert_eq!(edits(&my_events).concat(), my_ops);
}

#[gpui::test]
fn a_new_mind_map_child_is_an_add_and_its_place_is_the_layouts(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, |f| {
        with_three(f).with_options(|o| {
            o.layout = Layout::Tree(TreeLayout {
                motion: std::time::Duration::ZERO,
                ..TreeLayout::default()
            })
        })
    });
    select(&flow, cx, &["b"]);
    events.borrow_mut().clear();
    cx.simulate_keystrokes("tab");
    draw(cx);
    draw(cx);
    let ops = edits(&events).concat();
    let child = flow.read_with(cx, |f, _| f.selected_nodes()[0].clone());
    assert!(
        matches!(&ops[0], Op::AddNode { node } if node.id == child),
        "{ops:?}"
    );
    assert!(matches!(&ops[1], Op::AddEdge { edge } if edge.target == child));
    // Laying the tree out again moves nodes, but where a node under a root goes isn't an edit.
    assert!(
        !ops.iter().any(|op| matches!(op, Op::PatchNode { patch, .. }
            if patch.contains_key("position"))),
        "{ops:?}"
    );
}

#[test]
fn operations_are_written_with_an_op_tag() {
    let op = Op::PatchNode {
        id: "k3W9cQe2LfAp".into(),
        patch: patch(json!({ "position": { "x": 40 } })),
    };
    assert_eq!(
        serde_json::to_value(&op).unwrap(),
        json!({ "op": "patchNode", "id": "k3W9cQe2LfAp", "patch": { "position": { "x": 40 } } })
    );
    let back: Op = serde_json::from_value(json!({ "op": "removeEdge", "id": "e1" })).unwrap();
    assert_eq!(back, Op::RemoveEdge { id: "e1".into() });
    let add: Op = serde_json::from_value(json!({
        "op": "addNode", "node": { "id": "n", "position": { "x": 1, "y": 2 } }
    }))
    .unwrap();
    assert!(matches!(add, Op::AddNode { node } if node.position == xy(1., 2.)));
}

#[test]
fn merge_patches_follow_rfc_7386() {
    // The RFC's own example.
    let mut doc = json!({
        "title": "Goodbye!",
        "author": { "givenName": "John", "familyName": "Doe" },
        "tags": ["example", "sample"],
        "content": "This will be unchanged"
    });
    let p = json!({
        "title": "Hello!",
        "phoneNumber": "+01-555-555-5555",
        "author": { "familyName": null },
        "tags": ["example"]
    });
    let want = json!({
        "title": "Hello!",
        "author": { "givenName": "John" },
        "tags": ["example"],
        "content": "This will be unchanged",
        "phoneNumber": "+01-555-555-5555"
    });
    let before = doc.clone();
    json::merge_patch(&mut doc, &p);
    assert_eq!(doc, want);
    // And the diff of the two is a patch that does the same.
    let diff = json::merge_diff(&before, &want).unwrap();
    let mut again = before.clone();
    json::merge_patch(&mut again, &diff);
    assert_eq!(again, want);
    assert_eq!(json::merge_diff(&want, &want), None);
}

#[test]
fn a_server_applies_operations_as_the_flow_does() {
    let mut doc = Document::new(three().0, three().1);
    doc.apply(&[
        Op::PatchNode {
            id: "a".into(),
            patch: patch(json!({ "id": "hijack", "position": { "y": 0 }, "selected": true })),
        },
        Op::AddNode {
            node: Box::new(Node::new("bad id", xy(0., 0.))),
        },
        Op::RemoveEdge { id: "a-b".into() },
        Op::AddNode {
            node: Box::new(Node::new("c", xy(1., 1.)).label("C2")),
        },
    ]);
    let a = doc.nodes.iter().find(|n| n.id == "a").unwrap();
    // A patch can't change an id or select anything.
    assert_eq!((a.position, a.selected), (xy(100., 0.), false));
    assert!(
        doc.nodes
            .iter()
            .all(|n| n.id != "bad id" && n.id != "hijack")
    );
    assert!(doc.edges.is_empty());
    let c: Vec<&Node> = doc.nodes.iter().filter(|n| n.id == "c").collect();
    assert_eq!((c.len(), c[0].label_text().as_ref()), (1, "C2"));
}

#[gpui::test]
fn a_node_removed_beside_repeated_ids_is_sent_as_removed(cx: &mut TestAppContext) {
    // Two nodes share an id (a bad import, say): only the first is stored. Removing another
    // node still says so, and undo brings it back.
    let (flow, events, cx) = open(cx, |f| {
        f.with_nodes(vec![
            Node::new("a", xy(0., 0.)),
            Node::new("a", xy(80., 0.)),
            Node::new("b", xy(160., 0.)),
        ])
    });
    flow.update(cx, |f, cx| {
        f.remove_nodes(&["b".into()], cx);
    });
    draw(cx);
    assert_eq!(edits(&events).concat(), [Op::RemoveNode { id: "b".into() }]);
}
