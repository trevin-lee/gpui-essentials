//! Whiteboard nodes: every outline, palette colors, badges and text alignment.

mod common;

use common::*;
use gpui::{Modifiers, TestAppContext};
use gpui_flow::shapes::geometry;
use gpui_flow::*;

fn board(flow: Flow) -> Flow {
    let nodes = NodeShape::ALL
        .iter()
        .enumerate()
        .map(|(i, shape)| {
            Node::new(
                format!("{shape:?}").to_lowercase(),
                xy((i % 3) as f32 * 240., (i / 3) as f32 * 160.),
            )
            .shape(*shape)
            .label(format!("{shape:?}"))
        })
        .collect();
    flow.with_nodes(nodes)
}

#[gpui::test]
fn every_shape_takes_its_size_and_edges_meet_its_outline(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, board);
    flow.read_with(cx, |f, _| {
        for shape in NodeShape::ALL {
            let id = format!("{shape:?}").to_lowercase();
            let r = f.node_rect(&id).unwrap();
            let (w, h) = shape.default_size();
            assert_eq!((r.w, r.h), (w, h), "{shape:?}");
            let g = geometry(shape, w, h);
            // Each side has a handle of each kind, named after the side, where the outline is.
            for (side, name) in [
                (Side::Top, "top"),
                (Side::Right, "right"),
                (Side::Bottom, "bottom"),
                (Side::Left, "left"),
            ] {
                for kind in [HandleKind::Source, HandleKind::Target] {
                    let p = f.handle_point(&id, kind, Some(name)).unwrap();
                    let want = xy(r.x, r.y) + g.anchors.on(side);
                    assert!(
                        p.distance(want) < 0.6,
                        "{shape:?} {name} {kind:?}: {p:?}, want {want:?}"
                    );
                }
            }
        }
    });
}

#[gpui::test]
fn two_shapes_connect_from_side_to_side(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, board);
    // Handles aren't drawn until a shape is selected, but take a drag all the same.
    let (from, to) = flow.read_with(cx, |f, _| {
        (
            f.flow_to_screen(
                f.handle_point("rectangle", HandleKind::Source, Some("right"))
                    .unwrap(),
            ),
            f.flow_to_screen(
                f.handle_point("rounded", HandleKind::Target, Some("left"))
                    .unwrap(),
            ),
        )
    });
    drag(cx, offset(from, -2., 0.), to, Modifiers::none());
    flow.read_with(cx, |f, _| {
        let e = f.edges().last().expect("an edge");
        assert_eq!(
            (e.source.as_ref(), e.target.as_ref()),
            ("rectangle", "rounded")
        );
        assert_eq!(
            (e.source_handle.as_deref(), e.target_handle.as_deref()),
            (Some("right"), Some("left"))
        );
    });
    assert!(
        events
            .borrow()
            .iter()
            .any(|e| matches!(e, FlowEvent::Connect(_)))
    );
}

#[gpui::test]
fn a_shape_is_pressed_and_dragged_like_any_node(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, board);
    let at = center(&flow, cx, "diamond");
    drag(cx, at, offset(at, 30., 20.), Modifiers::none());
    assert_eq!(position(&flow, cx, "diamond"), xy(30., 180.));
    assert_eq!(flow.read_with(cx, |f, _| f.selected_nodes()), ["diamond"]);
}

#[test]
fn palette_colors_come_by_name() {
    let colors = FlowColors {
        palette: Palette::default(),
        ..dark_colors()
    };
    let blue = colors.pencil(Some("blue"));
    assert_eq!(blue.stroke, colors.palette.blue);
    // The fill is the pencil mixed into the pane, and opaque.
    assert_eq!(blue.fill.a, 1.);
    assert_ne!(blue.fill, colors.background);
    // On a dark pane the text is a light shade of it.
    assert!(blue.text.l >= 0.75);
    // `default`, and a name the palette doesn't know, are the node's own colors.
    for key in [None, Some("default"), Some("chartreuse")] {
        let p = colors.pencil(key);
        assert_eq!(
            (p.fill, p.stroke, p.text),
            (colors.node, colors.node_border, colors.node_text)
        );
    }
}

fn dark_colors() -> FlowColors {
    let gray = |l: f32| gpui::Hsla {
        h: 0.,
        s: 0.,
        l,
        a: 1.,
    };
    FlowColors {
        background: gray(0.06),
        pattern: gray(0.3),
        node: gray(0.1),
        node_border: gray(0.25),
        node_text: gray(0.93),
        muted_text: gray(0.55),
        group: gray(0.12),
        selected: gray(0.6),
        edge: gray(0.55),
        edge_label: gray(0.93),
        edge_label_background: gray(0.06),
        handle: gray(0.93),
        handle_border: gray(0.06),
        invalid: gray(0.5),
        selection_fill: gray(0.2),
        panel: gray(0.1),
        panel_border: gray(0.2),
        panel_hover: gray(0.15),
        panel_icon: gray(0.93),
        minimap_node: gray(0.5),
        minimap_mask: gray(0.06),
        minimap_view: gray(0.6),
        guide: gray(0.7),
        palette: Palette::default(),
    }
}

#[test]
fn styles_save_and_load_by_subcanvas_names() {
    let node = Node::new("n", xy(0., 0.))
        .shape(NodeShape::Cylinder)
        .color("teal")
        .icon("database")
        .emoji("🗄️")
        .text_align(LabelAlign::Left);
    let json = serde_json::to_value(&node).unwrap();
    assert_eq!(json["type"], "shape");
    assert_eq!(json["shape"], "cylinder");
    assert_eq!(json["color"], "teal");
    assert_eq!(json["icon"], "database");
    assert_eq!(json["emoji"], "🗄️");
    assert_eq!(json["textAlign"], "left");
    let back: Node = serde_json::from_value(json).unwrap();
    assert_eq!(back, node);
}
