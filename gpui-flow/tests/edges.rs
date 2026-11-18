//! Edges on a whiteboard: routes round what's in the way, dashes, palette colors, arrowheads
//! and labels.

mod common;

use common::*;
use gpui::TestAppContext;
use gpui_flow::*;

/// Two nodes in a row with a third square between them, joined past it by a step edge.
fn blocked(avoid: bool) -> impl FnOnce(Flow) -> Flow {
    move |flow| {
        flow.with_nodes(vec![
            Node::new("a", xy(0., 100.))
                .label("A")
                .source_position(Side::Right),
            Node::new("wall", xy(260., 40.))
                .label("Wall")
                .size(120., 160.),
            Node::new("b", xy(560., 100.))
                .label("B")
                .target_position(Side::Left),
        ])
        .with_edges(vec![Edge::new("a-b", "a", "b").kind(EdgeKind::SmoothStep)])
        .with_options(move |o| o.avoid_nodes = avoid)
    }
}

fn crosses(line: &[XY], r: Rect) -> bool {
    line.windows(2).any(|w| {
        (0..=40).any(|i| {
            let p = w[0] + (w[1] - w[0]) * (i as f32 / 40.);
            p.x > r.x && p.x < r.right() && p.y > r.y && p.y < r.bottom()
        })
    })
}

#[gpui::test]
fn a_routed_edge_goes_round_a_node_in_its_way(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, blocked(true));
    draw(cx);
    flow.read_with(cx, |f, _| {
        let wall = f.node_rect("wall").unwrap();
        let line = f.edge_line("a-b").expect("drawn");
        assert!(!crosses(&line, wall), "{line:?}");
        // It still leaves a's handle and reaches b's.
        let (from, to) = (
            f.handle_point("a", HandleKind::Source, None).unwrap(),
            f.handle_point("b", HandleKind::Target, None).unwrap(),
        );
        assert!(line[0].distance(from) < 0.5 && line[line.len() - 1].distance(to) < 0.5);
    });
}

#[gpui::test]
fn without_routing_an_edge_takes_its_usual_way(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, blocked(false));
    draw(cx);
    flow.read_with(cx, |f, _| {
        let wall = f.node_rect("wall").unwrap();
        assert!(crosses(&f.edge_line("a-b").unwrap(), wall));
    });
}

#[gpui::test]
fn a_route_follows_a_node_that_moves_into_the_way(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, blocked(true));
    // Out of the way first: the edge runs straight across.
    flow.update(cx, |f, cx| {
        f.update_node("wall", |n| n.position = xy(260., 600.), cx);
    });
    draw(cx);
    let straight = flow.read_with(cx, |f, _| f.edge_line("a-b").unwrap());
    assert!(straight.iter().all(|p| (p.y - straight[0].y).abs() < 0.5));
    // Back in the way: it goes round again.
    flow.update(cx, |f, cx| {
        f.update_node("wall", |n| n.position = xy(260., 40.), cx);
    });
    draw(cx);
    draw(cx);
    flow.read_with(cx, |f, _| {
        assert!(!crosses(
            &f.edge_line("a-b").unwrap(),
            f.node_rect("wall").unwrap()
        ));
    });
}

#[test]
fn dashes_and_palette_colors_save_as_react_flow_and_subcanvas_do() {
    let edge = Edge::new("e", "a", "b")
        .dash(EdgeDash::Dashed)
        .pencil("blue");
    let json = serde_json::to_value(&edge).unwrap();
    assert_eq!(json["style"]["strokeDasharray"], "6 4");
    assert_eq!(json["color"], "blue");
    let back: Edge = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(
        (back.dash, back.pencil.as_deref()),
        (EdgeDash::Dashed, Some("blue"))
    );
    // And saved again, the same.
    assert_eq!(serde_json::to_value(&back).unwrap(), json);
    // React Flow's own dash arrays: a short dash and a long gap is dotted.
    for (array, dash) in [
        ("5 5", EdgeDash::Dashed),
        ("2,6", EdgeDash::Dotted),
        ("0", EdgeDash::Solid),
        ("", EdgeDash::Solid),
    ] {
        let e: Edge = serde_json::from_value(serde_json::json!({
            "id": "e", "source": "a", "target": "b", "style": { "strokeDasharray": array }
        }))
        .unwrap();
        assert_eq!(e.dash, dash, "{array:?}");
    }
}

#[test]
fn arrows_are_put_at_the_ends_asked_for() {
    let both = Edge::new("e", "a", "b").arrows(Arrows::Both);
    assert!(both.marker_start.is_some() && both.marker_end.is_some());
    let reverse = Edge::new("e", "a", "b").arrows(Arrows::Reverse);
    assert!(reverse.marker_start.is_some() && reverse.marker_end.is_none());
    let none = Edge::new("e", "a", "b").arrows(Arrows::None);
    assert!(none.marker_start.is_none() && none.marker_end.is_none());
}

#[gpui::test]
fn a_label_sits_on_its_edge(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, |f| {
        with_three(f).with_edges(vec![Edge::new("a-b", "a", "b").label("then")])
    });
    flow.read_with(cx, |f, _| {
        let at = f.edge_label_point("a-b").unwrap();
        let line = f.edge_line("a-b").unwrap();
        assert!(geometry::distance_to_polyline(at, &line) < 1., "{at:?}");
    });
}

/// A kind of the host's runs where the host says, as React Flow's `edgeTypes`: drawn, taken
/// by clicks and exported along its own path.
#[gpui::test]
fn a_hosts_kind_of_edge_runs_along_its_own_path(cx: &mut TestAppContext) {
    use gpui_flow::geometry::{EdgePath, Segment};
    let (flow, events, cx) = open(cx, |f| {
        with_three(f)
            .with_edge_kind("detour", |ends: &EdgeEnds| {
                // Out to x 300 and back, whatever the bezier would do.
                let mid = (ends.source.y + ends.target.y) / 2.;
                EdgePath {
                    start: ends.source,
                    segments: vec![
                        Segment::Line(xy(300., ends.source.y)),
                        Segment::Line(xy(300., ends.target.y)),
                        Segment::Line(ends.target),
                    ],
                    label: xy(300., mid),
                }
            })
            .with_edges(vec![
                Edge::new("a-b", "a", "b").kind(EdgeKind::Custom("detour".into())),
            ])
    });
    let (a, b) = flow.read_with(cx, |f, _| {
        (
            f.handle_point("a", HandleKind::Source, None).unwrap(),
            f.handle_point("b", HandleKind::Target, None).unwrap(),
        )
    });
    // A click on the detour, far from where a bezier would run, takes the edge.
    let on = screen(&flow, cx, xy(300., (a.y + b.y) / 2.));
    cx.simulate_mouse_down(on, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(on, gpui::MouseButton::Left, gpui::Modifiers::none());
    draw(cx);
    assert!(
        events
            .borrow()
            .iter()
            .any(|e| matches!(e, FlowEvent::EdgeClick(id) if id == "a-b"))
    );
    // A click where the bezier would run, between the two, doesn't.
    events.borrow_mut().clear();
    let off = screen(&flow, cx, xy(a.x, (a.y + b.y) / 2.));
    cx.simulate_mouse_down(off, gpui::MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(off, gpui::MouseButton::Left, gpui::Modifiers::none());
    draw(cx);
    assert!(
        !events
            .borrow()
            .iter()
            .any(|e| matches!(e, FlowEvent::EdgeClick(_)))
    );
}
