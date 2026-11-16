//! Ink: strokes stored as the iPad's Notes stores PencilKit's, drawn, moved and pressed.

mod common;

use common::*;
use gpui::{Modifiers, MouseButton, TestAppContext};
use gpui_flow::ink::{self, InkPoint, InkStroke, InkTool};
use gpui_flow::json::Op;
use gpui_flow::*;
use serde_json::{Value, json};

/// The first point of the iPad's own example stroke, as its `InkPoints.encode` writes it.
const IPAD_POINT: &str = "AACQQgAA8EIAAAAAmpkZP83MTD/NzIw/mpkZQA==";

#[test]
fn points_read_and_write_as_the_ipad_does() {
    let points = ink::decode_points(IPAD_POINT).unwrap();
    assert_eq!(points.len(), 1);
    let p = points[0];
    assert_eq!((p.x, p.y, p.t), (72., 120., 0.));
    assert_eq!(
        (p.force, p.azimuth, p.altitude, p.size),
        (0.6, 0.8, 1.1, 2.4)
    );
    assert_eq!(ink::encode_points(&points), IPAD_POINT);
    // Not whole points, or not base64: not points.
    assert!(ink::decode_points("AAAA").is_none());
    assert!(ink::decode_points("not base64!").is_none());
}

#[test]
fn a_stroke_is_stored_in_the_ipads_shape() {
    let stroke = InkStroke::new(
        InkTool::Fountain,
        0x0000_00ff,
        2.4,
        ink::decode_points(IPAD_POINT).unwrap(),
    );
    let node = Node::ink("s1", &stroke);
    let v = serde_json::to_value(&node).unwrap();
    assert_eq!(v["type"], "ink");
    assert_eq!(
        v["data"]["ink"],
        json!({ "tool": "fountain", "color": "#000000FF", "width": 2.4,
                "points": IPAD_POINT, "transform": null })
    );
    // Its box is the stroke's bounds, with its width: the tip is 2.4 wide at (72, 120).
    let close = |a: f32, b: f32| (a - b).abs() < 1e-4;
    assert!(close(node.position.x, 70.8) && close(node.position.y, 118.8));
    assert!(close(node.width.unwrap(), 2.4) && close(node.height.unwrap(), 2.4));
    // And back.
    let back: Node = serde_json::from_value(v).unwrap();
    assert_eq!(InkStroke::of(&back), Some(stroke));
    // `#RRGGBB` is opaque; anything else isn't a colour.
    assert_eq!(ink::parse_color_hex("#FF8000"), Some(0xff80_00ff));
    assert_eq!(ink::parse_color_hex("#ff800080"), Some(0xff80_0080));
    assert_eq!(ink::parse_color_hex("orange"), None);
}

#[test]
fn a_transform_places_and_scales_the_stroke() {
    let mut s = InkStroke::new(
        InkTool::Ball,
        0xff,
        4.,
        vec![InkPoint::at(0., 0., 0., 4.), InkPoint::at(10., 0., 0.1, 4.)],
    );
    s.transform = Some([2., 0., 0., 2., 100., 50.]);
    let b = s.bounds().unwrap();
    // Twice as long and wide, moved to (100, 50).
    assert_eq!((b.x, b.y, b.w, b.h), (96., 46., 28., 8.));
    s.translate(5., -5.);
    assert_eq!(s.transform, Some([2., 0., 0., 2., 105., 45.]));
}

#[test]
fn a_mouse_line_is_smoothed_and_thinned() {
    // A straight line with a hand's shake, a point every unit.
    let raw: Vec<InkPoint> = (0..=100)
        .map(|i| {
            InkPoint::at(
                i as f32,
                if i % 2 == 0 { 0.3 } else { -0.3 },
                i as f32 / 100.,
                3.,
            )
        })
        .collect();
    let smooth = ink::smooth(&raw, 0.5);
    assert!(smooth.len() < 10, "{}", smooth.len());
    assert_eq!(smooth.first().map(|p| (p.x, p.y)), Some((0., 0.3)));
    assert_eq!(smooth.last().map(|p| (p.x, p.y)), Some((100., 0.3)));
}

fn board(flow: Flow) -> Flow {
    // An L of ink from (100, 100) to (300, 100) to (300, 300), a point every 10 as a pen
    // gives them, and a node beside it.
    let across = (0..=20).map(|i| (100. + i as f32 * 10., 100.));
    let down = (1..=20).map(|i| (300., 100. + i as f32 * 10.));
    let points = across
        .chain(down)
        .enumerate()
        .map(|(i, (x, y))| InkPoint::at(x, y, i as f32 / 100., 4.))
        .collect();
    let s = InkStroke::new(InkTool::Ball, 0x2040_80ff, 4., points);
    flow.with_nodes(vec![
        Node::ink("ink", &s),
        Node::new("n", xy(500., 100.)).label("N"),
    ])
}

#[gpui::test]
fn a_press_takes_a_stroke_on_its_ink_not_across_its_box(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, board);
    // On the ink.
    cx.simulate_click(screen(&flow, cx, xy(200., 101.)), Modifiers::none());
    draw(cx);
    assert_eq!(flow.read_with(cx, |f, _| f.selected_nodes()), ["ink"]);
    // Inside the L's box but far from its ink: the pane's.
    cx.simulate_click(screen(&flow, cx, xy(150., 250.)), Modifiers::none());
    draw(cx);
    assert!(flow.read_with(cx, |f, _| f.selected_nodes()).is_empty());
}

#[gpui::test]
fn moving_a_stroke_moves_its_transform_and_never_its_points(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, board);
    let (points, start) = flow.read_with(cx, |f, _| {
        let n = f.node("ink").unwrap();
        (n.data["ink"]["points"].clone(), n.position)
    });
    let at = screen(&flow, cx, xy(200., 101.));
    drag(cx, at, offset(at, 30., 20.), Modifiers::none());
    let ops: Vec<Op> = events
        .borrow()
        .iter()
        .filter_map(|e| match e {
            FlowEvent::Edits(ops) => Some(ops.clone()),
            _ => None,
        })
        .flatten()
        .collect();
    let [Op::PatchNode { id, patch }] = &ops[..] else {
        panic!("{ops:?}");
    };
    assert_eq!(id, "ink");
    assert_eq!(
        patch["data"]["ink"]["transform"],
        json!([1.0, 0.0, 0.0, 1.0, 30.0, 20.0])
    );
    assert!(patch.contains_key("position"));
    assert!(patch["data"]["ink"].get("points").is_none());
    flow.read_with(cx, |f, _| {
        assert_eq!(f.node("ink").unwrap().data["ink"]["points"], points);
    });

    // Undone, both come back, and the transform isn't moved again.
    flow.update(cx, |f, cx| f.undo(cx));
    draw(cx);
    flow.read_with(cx, |f, _| {
        let n = f.node("ink").unwrap();
        assert_eq!(n.data["ink"]["transform"], Value::Null);
        assert_eq!(n.position, start);
    });
}

#[gpui::test]
fn a_move_from_the_ipad_is_drawn_and_not_sent_back(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, board);
    events.borrow_mut().clear();
    // The iPad moves a stroke by its transform and its frame.
    flow.update(cx, |f, cx| {
        f.apply(
            &[Op::PatchNode {
                id: "ink".into(),
                patch: json!({
                    "position": { "x": 148.0, "y": 98.0 },
                    "data": { "ink": { "transform": [1, 0, 0, 1, 50, 0] } }
                })
                .as_object()
                .unwrap()
                .clone(),
            }],
            cx,
        )
    });
    draw(cx);
    assert!(
        !events
            .borrow()
            .iter()
            .any(|e| matches!(e, FlowEvent::Edits(_)))
    );
    // Now the ink is 50 to the right.
    cx.simulate_click(screen(&flow, cx, xy(250., 101.)), Modifiers::none());
    draw(cx);
    assert_eq!(flow.read_with(cx, |f, _| f.selected_nodes()), ["ink"]);
}

#[gpui::test]
fn a_stroke_that_doesnt_read_draws_nothing(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, |f| {
        f.with_nodes(vec![
            Node::new("bad", xy(0., 0.))
                .kind("ink")
                .data(json!({ "ink": { "tool": "crayon", "points": "!!" } })),
            Node::new("none", xy(50., 0.)).kind("ink"),
        ])
    });
    cx.simulate_mouse_down(
        screen(&flow, cx, xy(1., 1.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    draw(cx);
    assert_eq!(flow.read_with(cx, |f, _| f.nodes().len()), 2);
}
