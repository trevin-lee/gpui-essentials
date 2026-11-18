//! Resizing a node from the keyboard: R, the arrows a step (shift for four), Enter keeps it as
//! one edit and Escape puts it back, each said.

mod common;

use common::*;
use gpui::{Entity, TestAppContext, VisualTestContext};
use gpui_flow::json::Op;
use gpui_flow::*;

/// A frame (which has a resizer, at least 120 wide) and a plain node (which hasn't).
fn board(flow: Flow) -> Flow {
    flow.with_nodes(vec![
        Node::new("frame", xy(100., 100.))
            .kind("frame")
            .label("Payments")
            .size(240., 160.),
        Node::new("plain", xy(500., 100.)).label("Plain"),
    ])
}

fn keys(cx: &mut VisualTestContext, keys: &str) {
    cx.simulate_keystrokes(keys);
    draw(cx);
}

fn size(flow: &Entity<Flow>, cx: &VisualTestContext, id: &str) -> (Option<f32>, Option<f32>) {
    flow.read_with(cx, |f, _| {
        let n = f.node(id).unwrap();
        (n.width, n.height)
    })
}

fn said(flow: &Entity<Flow>, cx: &VisualTestContext) -> String {
    flow.read_with(cx, |f, _| f.announcement().to_string())
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
fn the_arrows_resize_and_enter_keeps_it_as_one_edit(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, board);
    // The keyboard on the frame, not selected: R selects it and begins.
    flow.update(cx, |f, cx| {
        f.set_keyboard_on(Some(FlowItem::Node("frame".into())), cx)
    });
    draw(cx);
    keys(cx, "r");
    draw(cx);
    assert_eq!(
        flow.read_with(cx, |f, _| f.keyboard_resizing().cloned()),
        Some("frame".into())
    );
    assert!(
        said(&flow, cx).starts_with("Resizing Payments, 240 by 160."),
        "{}",
        said(&flow, cx)
    );
    events.borrow_mut().clear();

    // Right and down grow it a step; shift-right four; left and up shrink it.
    keys(cx, "right");
    assert_eq!(size(&flow, cx, "frame"), (Some(250.), Some(160.)));
    assert_eq!(said(&flow, cx), "Payments, 250 by 160");
    keys(cx, "shift-right down down up");
    assert_eq!(size(&flow, cx, "frame"), (Some(290.), Some(170.)));
    // The node doesn't move, and nothing is sent yet.
    assert_eq!(position(&flow, cx, "frame"), xy(100., 100.));
    assert!(edits(&events).is_empty());

    // Enter keeps it: one batch, one undo step.
    keys(cx, "enter");
    assert!(flow.read_with(cx, |f, _| f.keyboard_resizing().is_none()));
    assert_eq!(said(&flow, cx), "Payments resized to 290 by 170");
    let batches = edits(&events);
    assert_eq!(batches.len(), 1, "{batches:?}");
    assert!(matches!(&batches[0][..], [Op::PatchNode { id, .. }] if id == "frame"));
    // The arrows move the node again.
    keys(cx, "right");
    assert_eq!(position(&flow, cx, "frame"), xy(105., 100.));
    flow.update(cx, |f, cx| {
        f.undo(cx);
        f.undo(cx);
    });
    draw(cx);
    assert_eq!(size(&flow, cx, "frame"), (Some(240.), Some(160.)));
}

#[gpui::test]
fn escape_puts_the_size_back_and_sends_nothing(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, board);
    flow.update(cx, |f, cx| {
        f.update_node("frame", |n| n.selected = true, cx)
    });
    draw(cx);
    events.borrow_mut().clear();
    keys(cx, "r");
    keys(
        cx,
        "left left left left left left left left left left left left left left",
    );
    // Not below the frame's least width.
    assert_eq!(size(&flow, cx, "frame"), (Some(120.), Some(160.)));
    keys(cx, "escape");
    assert_eq!(size(&flow, cx, "frame"), (Some(240.), Some(160.)));
    assert_eq!(said(&flow, cx), "Payments back to 240 by 160");
    assert!(edits(&events).is_empty());
    // Still selected: Escape ended the resize, not the selection.
    assert_eq!(flow.read_with(cx, |f, _| f.selected_nodes()), ["frame"]);
}

#[gpui::test]
fn a_node_without_a_resizer_says_so(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, board);
    flow.update(cx, |f, cx| {
        f.set_keyboard_on(Some(FlowItem::Node("plain".into())), cx)
    });
    draw(cx);
    keys(cx, "r");
    draw(cx);
    assert!(flow.read_with(cx, |f, _| f.keyboard_resizing().is_none()));
    assert_eq!(said(&flow, cx), "Plain can't be resized");
    // The arrows still move it.
    keys(cx, "right");
    assert_eq!(position(&flow, cx, "plain"), xy(505., 100.));
}

#[gpui::test]
fn a_picture_keeps_its_proportions_from_the_keyboard(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, |f| {
        f.with_nodes(vec![
            Node::new("pic", xy(0., 0.))
                .image("a.png", 400., 200., "")
                .size(200., 100.),
        ])
    });
    flow.update(cx, |f, cx| f.update_node("pic", |n| n.selected = true, cx));
    draw(cx);
    keys(cx, "r right");
    assert_eq!(size(&flow, cx, "pic"), (Some(210.), Some(105.)));
    keys(cx, "down enter");
    assert_eq!(size(&flow, cx, "pic"), (Some(230.), Some(115.)));
}
