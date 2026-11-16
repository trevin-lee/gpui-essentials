//! Undo and redo, over the edits the flow sends: each batch is one step.

mod common;

use common::*;
use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};
use gpui_flow::json::Op;
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

fn undo(flow: &Entity<Flow>, cx: &mut VisualTestContext) -> bool {
    let done = flow.update(cx, |f, cx| f.undo(cx));
    draw(cx);
    done
}

fn redo(flow: &Entity<Flow>, cx: &mut VisualTestContext) -> bool {
    let done = flow.update(cx, |f, cx| f.redo(cx));
    draw(cx);
    done
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

#[gpui::test]
fn a_drag_of_many_nodes_is_one_undo(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, with_three);
    select(&flow, cx, &["a", "c"]);
    let at = center(&flow, cx, "a");
    drag(cx, at, offset(at, 50., 10.), Modifiers::none());
    assert_eq!(position(&flow, cx, "a"), xy(150., 110.));
    assert_eq!(position(&flow, cx, "c"), xy(450., 260.));
    events.borrow_mut().clear();

    assert!(undo(&flow, cx));
    assert_eq!(position(&flow, cx, "a"), xy(100., 100.));
    assert_eq!(position(&flow, cx, "c"), xy(400., 250.));
    // The undo is sent as an edit like any other.
    assert_eq!(
        edits(&events),
        [vec![
            Op::PatchNode {
                id: "a".into(),
                patch: json!({ "position": { "x": 100.0, "y": 100.0 } })
                    .as_object()
                    .unwrap()
                    .clone(),
            },
            Op::PatchNode {
                id: "c".into(),
                patch: json!({ "position": { "x": 400.0, "y": 250.0 } })
                    .as_object()
                    .unwrap()
                    .clone(),
            },
        ]]
    );
    assert!(!undo(&flow, cx), "nothing more to take back");

    assert!(redo(&flow, cx));
    assert_eq!(position(&flow, cx, "a"), xy(150., 110.));
    assert_eq!(position(&flow, cx, "c"), xy(450., 260.));
    assert!(!redo(&flow, cx));
}

#[gpui::test]
fn a_deletion_comes_back_whole_with_its_ids(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    let before = flow.read_with(cx, |f, _| f.document());
    select(&flow, cx, &["b"]);
    flow.update(cx, |f, cx| f.delete_selected(cx));
    draw(cx);
    assert!(flow.read_with(cx, |f, _| f.node("b").is_none() && f.edge("a-b").is_none()));

    assert!(undo(&flow, cx));
    let after = flow.read_with(cx, |f, _| f.document());
    let ids = |d: &json::Document| {
        let mut n: Vec<String> = d.nodes.iter().map(|n| n.id.to_string()).collect();
        n.sort();
        (
            n,
            d.edges.iter().map(|e| e.id.to_string()).collect::<Vec<_>>(),
        )
    };
    assert_eq!(ids(&after), ids(&before));
    let b = |d: &json::Document| d.nodes.iter().find(|n| n.id == "b").cloned();
    assert_eq!(b(&after), b(&before));

    assert!(redo(&flow, cx));
    assert!(flow.read_with(cx, |f, _| f.node("b").is_none() && f.edge("a-b").is_none()));
}

#[gpui::test]
fn each_call_is_a_step_and_one_update_is_one(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    // Two changes in one update: one step.
    flow.update(cx, |f, cx| {
        f.update_node("b", |n| n.data["label"] = "Bee".into(), cx);
        f.update_node("c", |n| n.color = Some("red".into()), cx);
    });
    draw(cx);
    // Then one more on its own.
    flow.update(cx, |f, cx| {
        f.update_node("b", |n| n.color = Some("blue".into()), cx)
    });
    draw(cx);

    assert!(undo(&flow, cx));
    flow.read_with(cx, |f, _| {
        let (b, c) = (f.node("b").unwrap(), f.node("c").unwrap());
        assert_eq!((b.label_text().as_ref(), b.color.as_deref()), ("Bee", None));
        assert_eq!(c.color.as_deref(), Some("red"));
    });
    assert!(undo(&flow, cx));
    flow.read_with(cx, |f, _| {
        assert_eq!(f.node("b").unwrap().label_text(), "B");
        assert_eq!(f.node("c").unwrap().color, None);
    });
}

#[gpui::test]
fn a_new_edit_forgets_what_was_taken_back(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    flow.update(cx, |f, cx| f.update_node("b", |n| n.hidden = true, cx));
    draw(cx);
    assert!(undo(&flow, cx));
    assert!(flow.read_with(cx, |f, _| f.can_redo()));
    flow.update(cx, |f, cx| f.update_node("c", |n| n.hidden = true, cx));
    draw(cx);
    assert!(flow.read_with(cx, |f, _| !f.can_redo()));
    assert!(!redo(&flow, cx));
}

#[gpui::test]
fn undo_takes_back_ones_own_fields_and_leaves_others_edits(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    flow.update(cx, |f, cx| f.update_node("a", |n| n.position.x = 300., cx));
    draw(cx);
    // Someone else renames a, and adds a node.
    flow.update(cx, |f, cx| {
        f.apply(
            &[
                Op::PatchNode {
                    id: "a".into(),
                    patch: json!({ "data": { "label": "Theirs" } })
                        .as_object()
                        .unwrap()
                        .clone(),
                },
                Op::AddNode {
                    node: Box::new(Node::new("z", xy(0., 0.))),
                },
            ],
            cx,
        )
    });
    draw(cx);
    assert!(undo(&flow, cx));
    flow.read_with(cx, |f, _| {
        let a = f.node("a").unwrap();
        assert_eq!(a.position.x, 100.);
        assert_eq!(a.label_text(), "Theirs");
        assert!(f.node("z").is_some());
    });
    assert!(!undo(&flow, cx), "their edits aren't mine to take back");
}

#[gpui::test]
fn an_undo_of_what_is_gone_is_passed_over(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    flow.update(cx, |f, cx| {
        f.update_node("b", |n| n.color = Some("red".into()), cx)
    });
    draw(cx);
    flow.update(cx, |f, cx| f.update_node("c", |n| n.position.x = 0., cx));
    draw(cx);
    flow.update(cx, |f, cx| {
        f.apply(&[Op::RemoveNode { id: "c".into() }], cx)
    });
    draw(cx);
    // Moving c can't be taken back now c is gone, so the undo takes back b's colour.
    assert!(undo(&flow, cx));
    assert_eq!(
        flow.read_with(cx, |f, _| f.node("b").unwrap().color.clone()),
        None
    );
    assert!(flow.read_with(cx, |f, _| f.node("c").is_none()));
}

#[gpui::test]
fn framing_is_taken_back_in_one(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    select(&flow, cx, &["b", "c"]);
    flow.update(cx, |f, cx| f.frame_selection("Group", cx).unwrap());
    draw(cx);
    assert!(undo(&flow, cx));
    flow.read_with(cx, |f, _| {
        assert_eq!(f.nodes().len(), 3);
        for (id, at) in [("b", xy(100., 250.)), ("c", xy(400., 250.))] {
            let n = f.node(id).unwrap();
            assert_eq!((n.parent.clone(), n.position), (None, at), "{id}");
        }
    });
}

#[gpui::test]
fn the_keys_undo_and_redo(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    select(&flow, cx, &["c"]);
    cx.simulate_keystrokes("right");
    draw(cx);
    let moved = position(&flow, cx, "c");
    assert_ne!(moved, xy(400., 250.));
    cx.simulate_keystrokes("cmd-z");
    draw(cx);
    assert_eq!(position(&flow, cx, "c"), xy(400., 250.));
    cx.simulate_keystrokes("cmd-shift-z");
    draw(cx);
    assert_eq!(position(&flow, cx, "c"), moved);
}

#[gpui::test]
fn loading_a_document_forgets_the_history(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    flow.update(cx, |f, cx| f.update_node("b", |n| n.hidden = true, cx));
    draw(cx);
    assert!(flow.read_with(cx, |f, _| f.can_undo()));
    let doc = flow.read_with(cx, |f, _| f.document());
    flow.update(cx, |f, cx| f.set_document(doc, cx));
    assert!(flow.read_with(cx, |f, _| !f.can_undo() && !f.can_redo()));
}
