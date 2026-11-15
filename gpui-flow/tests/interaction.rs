//! The flow driven as a person drives it: a window, the mouse and the keyboard.

mod common;

use common::*;
use gpui_flow::*;
use gpui_kit::{Modifiers, TestAppContext, point, px};

#[gpui_kit::test]
fn nodes_are_measured_and_the_built_in_kinds_have_their_handles(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    flow.read_with(cx, |f, _| {
        let a = f.node_rect("a").unwrap();
        // The default box is 150 wide, with ten of padding round a line of 12-point text.
        assert_eq!(a.w, 150.);
        assert!(a.h > 30. && a.h < 45., "{a:?}");
        // An input has a source under it and no target; a default node has both.
        assert_eq!(
            f.handle_point("a", HandleKind::Source, None),
            Some(xy(175., 100. + a.h + 4.))
        );
        assert_eq!(
            f.handle_point("b", HandleKind::Target, None),
            Some(xy(175., 246.))
        );
        // With no target handle, nothing can end at `a`: as in React Flow, such an edge isn't
        // drawn.
        assert_eq!(f.handle_point("a", HandleKind::Target, None), None);
    });
}

#[gpui_kit::test]
fn a_fit_on_start_shows_every_node(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, |f| with_three(f).fit_view_on_start());
    draw(cx);
    flow.read_with(cx, |f, _| {
        let bounds = f.nodes_bounds().unwrap();
        let pane = gpui_kit::Bounds::from_corners(
            f.flow_to_screen(xy(bounds.x, bounds.y)),
            f.flow_to_screen(xy(bounds.right(), bounds.bottom())),
        );
        let v = f.viewport();
        assert!(v.zoom > 1. && v.zoom <= 2., "{v:?}");
        assert!(pane.origin.x > px(0.) && pane.origin.y > px(0.), "{pane:?}");
        // The middle of the nodes is the middle of the pane.
        let mid = f.screen_to_flow(point(
            pane.origin.x + pane.size.width / 2.,
            pane.origin.y + pane.size.height / 2.,
        ));
        assert!((mid.x - bounds.center().x).abs() < 0.5 && (mid.y - bounds.center().y).abs() < 0.5);
    });
}

#[gpui_kit::test]
fn dragging_a_node_moves_it_by_the_distance_in_the_flow(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, |f| {
        with_three(f).with_viewport(Viewport::new(40., 20., 2.))
    });
    let at = center(&flow, cx, "b");
    drag(cx, at, offset(at, 100., 60.), Modifiers::none());
    // At zoom 2 a hundred points on screen is fifty in the flow.
    assert_eq!(position(&flow, cx, "b"), xy(150., 280.));
    assert_eq!(position(&flow, cx, "a"), xy(100., 100.));
    assert_eq!(flow.read_with(cx, |f, _| f.selected_nodes()), ["b"]);
    let events = events.borrow();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, FlowEvent::NodeDragStart(ids) if ids == &["b"]))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, FlowEvent::NodeDragStop(ids) if ids == &["b"]))
    );
    assert!(!events.iter().any(|e| matches!(e, FlowEvent::NodeClick(_))));
}

#[gpui_kit::test]
fn a_dragged_node_snaps_to_the_grid_when_asked(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, |f| {
        with_three(f).with_options(|o| {
            o.snap_to_grid = true;
            o.snap_grid = (20., 20.);
        })
    });
    let at = center(&flow, cx, "c");
    drag(cx, at, offset(at, 33., 14.), Modifiers::none());
    assert_eq!(position(&flow, cx, "c"), xy(440., 260.));
}

#[gpui_kit::test]
fn a_click_selects_one_node_and_command_click_adds_another(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, with_three);
    let (a, c) = (center(&flow, cx, "a"), center(&flow, cx, "c"));
    cx.simulate_click(a, Modifiers::none());
    draw(cx);
    assert_eq!(flow.read_with(cx, |f, _| f.selected_nodes()), ["a"]);
    cx.simulate_click(c, Modifiers::command());
    draw(cx);
    assert_eq!(flow.read_with(cx, |f, _| f.selected_nodes()), ["a", "c"]);
    assert!(
        events
            .borrow()
            .iter()
            .any(|e| matches!(e, FlowEvent::NodeClick(id) if id == "a"))
    );

    // Dragging either moves both.
    let c = center(&flow, cx, "c");
    drag(cx, c, offset(c, 30., 30.), Modifiers::none());
    assert_eq!(position(&flow, cx, "a"), xy(130., 130.));
    assert_eq!(position(&flow, cx, "c"), xy(430., 280.));

    // As in React Flow, a plain click on one of them keeps both; ⌘-click takes one away; a
    // click on the pane clears what's left.
    cx.simulate_click(center(&flow, cx, "c"), Modifiers::none());
    draw(cx);
    assert_eq!(flow.read_with(cx, |f, _| f.selected_nodes()), ["a", "c"]);
    cx.simulate_click(center(&flow, cx, "a"), Modifiers::command());
    draw(cx);
    assert_eq!(flow.read_with(cx, |f, _| f.selected_nodes()), ["c"]);
    cx.simulate_click(screen(&flow, cx, xy(600., 60.)), Modifiers::none());
    draw(cx);
    assert!(flow.read_with(cx, |f, _| f.selected_nodes()).is_empty());
    assert!(
        events
            .borrow()
            .iter()
            .any(|e| matches!(e, FlowEvent::PaneClick(at) if *at == xy(600., 60.)))
    );
}

#[gpui_kit::test]
fn dragging_from_a_source_handle_to_a_target_handle_connects_them(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, with_three);
    let from = handle(&flow, cx, "a", HandleKind::Source);
    let to = handle(&flow, cx, "c", HandleKind::Target);
    // Let go near the handle, not on it: within reach it snaps.
    drag(cx, from, offset(to, 9., -8.), Modifiers::none());
    flow.read_with(cx, |f, _| {
        let edge = f.edges().last().unwrap();
        assert_eq!((edge.source.as_ref(), edge.target.as_ref()), ("a", "c"));
        assert_eq!(f.edges().len(), 2);
        // The node the drag started on didn't move.
        assert_eq!(f.node("a").unwrap().position, xy(100., 100.));
    });
    assert!(
        events
            .borrow()
            .iter()
            .any(|e| matches!(e, FlowEvent::Connect(edge) if edge.target == "c"))
    );
    assert!(
        events
            .borrow()
            .iter()
            .any(|e| matches!(e, FlowEvent::ConnectEnd { connected: true, to: Some(to), .. } if to.node == "c"))
    );

    // The same again adds nothing, and neither does letting go over nothing.
    drag(cx, from, to, Modifiers::none());
    drag(cx, from, offset(to, 150., -120.), Modifiers::none());
    assert_eq!(flow.read_with(cx, |f, _| f.edges().len()), 2);
}

#[gpui_kit::test]
fn a_connection_can_be_dragged_backwards_from_the_target(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    let from = handle(&flow, cx, "c", HandleKind::Target);
    let to = handle(&flow, cx, "b", HandleKind::Source);
    drag(cx, from, to, Modifiers::none());
    flow.read_with(cx, |f, _| {
        let edge = f.edges().last().unwrap();
        assert_eq!((edge.source.as_ref(), edge.target.as_ref()), ("b", "c"));
    });
}

#[gpui_kit::test]
fn two_sources_connect_only_when_connections_are_loose(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    let from = handle(&flow, cx, "b", HandleKind::Source);
    let to = handle(&flow, cx, "c", HandleKind::Source);
    drag(cx, from, to, Modifiers::none());
    assert_eq!(flow.read_with(cx, |f, _| f.edges().len()), 1);
    flow.update(cx, |f, _| f.options.connection_mode = ConnectionMode::Loose);
    drag(cx, from, to, Modifiers::none());
    assert_eq!(flow.read_with(cx, |f, _| f.edges().len()), 2);
}

#[gpui_kit::test]
fn the_hosts_check_can_refuse_a_connection(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, |f| {
        with_three(f).with_connection_check(|c, _| c.target != "c")
    });
    let from = handle(&flow, cx, "a", HandleKind::Source);
    let to = handle(&flow, cx, "c", HandleKind::Target);
    drag(cx, from, to, Modifiers::none());
    assert_eq!(flow.read_with(cx, |f, _| f.edges().len()), 1);
    assert!(events.borrow().iter().any(|e| matches!(
        e,
        FlowEvent::ConnectEnd {
            connected: false,
            ..
        }
    )));
}

#[gpui_kit::test]
fn dragging_the_pane_pans_and_the_nodes_stay_put_in_the_flow(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, with_three);
    let at = screen(&flow, cx, xy(600., 60.));
    drag(cx, at, offset(at, -80., 45.), Modifiers::none());
    assert_eq!(
        flow.read_with(cx, |f, _| f.viewport()),
        Viewport::new(-80., 45., 1.)
    );
    assert_eq!(position(&flow, cx, "a"), xy(100., 100.));
    assert!(
        events
            .borrow()
            .iter()
            .any(|e| matches!(e, FlowEvent::ViewportChange(_)))
    );
    assert!(
        !events
            .borrow()
            .iter()
            .any(|e| matches!(e, FlowEvent::PaneClick(_)))
    );
}

#[gpui_kit::test]
fn a_shift_drag_selects_the_nodes_inside_it_and_the_edges_between_them(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    // Round a and b whole, and through c.
    let (from, to) = (
        screen(&flow, cx, xy(60., 60.)),
        screen(&flow, cx, xy(450., 320.)),
    );
    drag(cx, from, to, Modifiers::shift());
    flow.read_with(cx, |f, _| {
        assert_eq!(f.selected_nodes(), ["a", "b"]);
        assert_eq!(f.selected_edges(), ["a-b"]);
        assert_eq!(f.viewport(), Viewport::default());
    });
    // Taking what it touches takes c as well.
    flow.update(cx, |f, _| f.options.selection_mode = SelectionMode::Partial);
    drag(cx, from, to, Modifiers::shift());
    assert_eq!(
        flow.read_with(cx, |f, _| f.selected_nodes()),
        ["a", "b", "c"]
    );
}

#[gpui_kit::test]
fn delete_removes_what_is_selected_and_the_edges_it_held(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, with_three);
    cx.simulate_click(center(&flow, cx, "b"), Modifiers::none());
    draw(cx);
    cx.simulate_keystrokes("backspace");
    draw(cx);
    flow.read_with(cx, |f, _| {
        assert_eq!(
            f.nodes().iter().map(|n| n.id.as_ref()).collect::<Vec<_>>(),
            ["a", "c"]
        );
        assert!(f.edges().is_empty());
    });
    assert!(events.borrow().iter().any(
        |e| matches!(e, FlowEvent::Delete { nodes, edges } if nodes.len() == 1 && edges.len() == 1)
    ));

    // A node that says it can't be deleted stays.
    flow.update(cx, |f, cx| {
        f.update_node("a", |n| n.deletable = Some(false), cx);
        f.select_all(cx);
    });
    cx.simulate_keystrokes("backspace");
    draw(cx);
    assert_eq!(flow.read_with(cx, |f, _| f.nodes().len()), 1);
}

#[gpui_kit::test]
fn a_click_on_an_edges_line_selects_it(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, with_three);
    // Half way down the straight drop from a to b.
    let (from, to) = flow.read_with(cx, |f, _| {
        (
            f.handle_point("a", HandleKind::Source, None).unwrap(),
            f.handle_point("b", HandleKind::Target, None).unwrap(),
        )
    });
    let mid = screen(&flow, cx, (from + to) * 0.5);
    cx.simulate_click(offset(mid, 3., 0.), Modifiers::none());
    draw(cx);
    assert_eq!(flow.read_with(cx, |f, _| f.selected_edges()), ["a-b"]);
    assert!(
        events
            .borrow()
            .iter()
            .any(|e| matches!(e, FlowEvent::EdgeClick(id) if id == "a-b"))
    );
    cx.simulate_keystrokes("backspace");
    draw(cx);
    assert!(flow.read_with(cx, |f, _| f.edges().is_empty()));
    assert_eq!(flow.read_with(cx, |f, _| f.nodes().len()), 3);
}

#[gpui_kit::test]
fn scrolling_zooms_about_the_pointer_within_the_limits(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    let at = screen(&flow, cx, xy(300., 200.));
    // React Flow's wheel: 2^(100 * 0.002), about 1.15.
    scroll(cx, at, 0., 100., Modifiers::none());
    flow.read_with(cx, |f, _| {
        assert!(
            (f.viewport().zoom - 2f32.powf(0.2)).abs() < 1e-4,
            "{:?}",
            f.viewport()
        );
        let under = f.screen_to_flow(at);
        assert!(
            (under.x - 300.).abs() < 0.01 && (under.y - 200.).abs() < 0.01,
            "{under:?}"
        );
    });
    scroll(cx, at, 0., 5000., Modifiers::none());
    assert_eq!(flow.read_with(cx, |f, _| f.viewport().zoom), 2.);
    scroll(cx, at, 0., -50_000., Modifiers::none());
    assert_eq!(flow.read_with(cx, |f, _| f.viewport().zoom), 0.5);
}

#[gpui_kit::test]
fn scrolling_pans_instead_when_asked_and_command_still_zooms(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, |f| {
        with_three(f).with_options(|o| o.pan_on_scroll = true)
    });
    let at = screen(&flow, cx, xy(300., 200.));
    // At React Flow's speed, half a point for a point.
    scroll(cx, at, -30., 12., Modifiers::none());
    assert_eq!(
        flow.read_with(cx, |f, _| f.viewport()),
        Viewport::new(-15., 6., 1.)
    );
    scroll(cx, at, 0., 100., Modifiers::command());
    assert!(flow.read_with(cx, |f, _| f.viewport().zoom) > 1.1);
}

#[gpui_kit::test]
fn children_move_with_their_parent_and_stay_inside_it(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, |f| {
        f.with_nodes(vec![
            Node::new("group", xy(100., 100.))
                .kind("group")
                .size(300., 200.),
            Node::new("child", xy(20., 30.))
                .parent("group")
                .extent_parent(true)
                .size(100., 40.)
                .label("Child"),
        ])
    });
    assert_eq!(
        flow.read_with(cx, |f, _| f.absolute_position("child")),
        Some(xy(120., 130.))
    );
    // By an empty part of the group.
    let at = screen(&flow, cx, xy(350., 280.));
    drag(cx, at, offset(at, 50., 40.), Modifiers::none());
    flow.read_with(cx, |f, _| {
        assert_eq!(f.node("group").unwrap().position, xy(150., 140.));
        assert_eq!(f.node("child").unwrap().position, xy(20., 30.));
        assert_eq!(f.absolute_position("child"), Some(xy(170., 170.)));
    });
    // The child can't be dragged out through the group's right and bottom sides.
    let at = center(&flow, cx, "child");
    drag(cx, at, offset(at, 500., 500.), Modifiers::none());
    assert_eq!(position(&flow, cx, "child"), xy(200., 160.));
    // Deleting the group takes the child with it.
    flow.update(cx, |f, cx| {
        f.remove_nodes(&["group".into()], cx);
    });
    assert!(flow.read_with(cx, |f, _| f.nodes().is_empty()));
}

#[gpui_kit::test]
fn the_keyboard_selects_all_nudges_and_zooms(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    cx.simulate_keystrokes("cmd-a");
    draw(cx);
    flow.read_with(cx, |f, _| {
        assert_eq!(f.selected_nodes().len(), 3);
        assert_eq!(f.selected_edges().len(), 1);
    });
    cx.simulate_keystrokes("right down");
    assert_eq!(position(&flow, cx, "a"), xy(105., 105.));
    cx.simulate_keystrokes("escape");
    assert!(flow.read_with(cx, |f, _| f.selected_nodes()).is_empty());
    cx.simulate_keystrokes("cmd-=");
    assert!((flow.read_with(cx, |f, _| f.viewport().zoom) - 1.2).abs() < 1e-5);
    cx.simulate_keystrokes("cmd-0");
    assert_eq!(flow.read_with(cx, |f, _| f.viewport().zoom), 1.);
}

#[gpui_kit::test]
fn a_locked_flow_still_pans_but_nothing_in_it_moves_or_connects(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    flow.update(cx, |f, cx| f.set_interactive(false, cx));
    draw(cx);
    let at = center(&flow, cx, "b");
    drag(cx, at, offset(at, 60., 60.), Modifiers::none());
    let from = handle(&flow, cx, "a", HandleKind::Source);
    let to = handle(&flow, cx, "c", HandleKind::Target);
    drag(cx, from, to, Modifiers::none());
    flow.read_with(cx, |f, _| {
        assert_eq!(f.node("b").unwrap().position, xy(100., 250.));
        assert!(f.selected_nodes().is_empty());
        assert_eq!(f.edges().len(), 1);
    });
}

#[gpui_kit::test]
fn a_flow_saves_to_json_and_comes_back_the_same(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    let saved = flow.read_with(cx, |f, _| serde_json::to_string(&f.to_object()).unwrap());
    flow.update(cx, |f, cx| {
        f.remove_nodes(&["a".into()], cx);
        f.set_viewport(Viewport::new(5., 6., 1.5), cx);
    });
    let object: FlowObject = serde_json::from_str(&saved).unwrap();
    flow.update(cx, |f, cx| f.restore(object, cx));
    draw(cx);
    flow.read_with(cx, |f, _| {
        let (nodes, edges) = three();
        assert_eq!(f.nodes(), nodes);
        assert_eq!(f.edges(), edges);
        assert_eq!(f.viewport(), Viewport::default());
    });
}

#[test]
fn react_flows_own_json_loads() {
    // As React Flow's `toObject()` writes it, camelCase and all, with things this crate doesn't
    // draw (a custom edge type, classes, styles) that must survive a save.
    let json = r##"{
        "nodes": [
            { "id": "1", "type": "input", "position": { "x": 0, "y": 0 }, "data": { "label": "One" },
              "sourcePosition": "right", "className": "big", "measured": { "width": 150, "height": 40 } },
            { "id": "g", "type": "group", "position": { "x": 200, "y": 0 }, "width": 300, "height": 200,
              "zIndex": 5, "style": { "background": "#123" } },
            { "id": "2", "position": { "x": 10, "y": 90 }, "data": { "label": "Two" }, "width": 120,
              "parentId": "g", "extent": "parent", "targetPosition": "left" },
            { "id": "3", "position": { "x": 0, "y": 300 }, "extent": [[0, 0], [500, 500]] }
        ],
        "edges": [
            { "id": "e1-2", "source": "1", "target": "2", "type": "smoothstep", "animated": true,
              "label": "to", "sourceHandle": "a", "targetHandle": null,
              "markerEnd": { "type": "arrowclosed", "color": "#ff0072", "width": 20, "height": 20 },
              "style": { "stroke": "#f6ab6c", "strokeWidth": 2, "strokeDasharray": "4" },
              "zIndex": 2, "interactionWidth": 30 },
            { "id": "e2-3", "source": "2", "target": "3", "type": "buttonedge", "label": 7,
              "markerStart": "custom-marker-id", "data": { "weight": 1 } }
        ],
        "viewport": { "x": 3, "y": 4, "zoom": 1.5 }
    }"##;
    let object: FlowObject = serde_json::from_str(json).unwrap();
    let [one, g, two, three] = &object.nodes[..] else {
        panic!("four nodes");
    };
    assert_eq!(one.kind, "input");
    assert_eq!(one.source_position, Side::Right);
    assert_eq!(one.extra["className"], "big");
    assert_eq!(g.z_index, 5);
    assert_eq!(two.kind, "default");
    assert_eq!(two.label_text(), "Two");
    assert_eq!(two.width, Some(120.));
    assert_eq!(two.parent.as_deref(), Some("g"));
    assert_eq!(two.extent, Some(Extent::Parent));
    assert_eq!(two.target_position, Side::Left);
    assert_eq!(
        three.extent,
        Some(Extent::Area(Rect::new(0., 0., 500., 500.)))
    );

    let [e12, e23] = &object.edges[..] else {
        panic!("two edges");
    };
    assert_eq!(e12.kind, EdgeKind::SmoothStep);
    assert!(e12.animated);
    assert_eq!(e12.source_handle.as_deref(), Some("a"));
    assert_eq!(e12.target_handle, None);
    let marker = e12.marker_end.unwrap();
    assert_eq!(
        (marker.kind, marker.color, marker.size),
        (MarkerKind::ArrowClosed, Some(0xff0072), 16.)
    );
    assert_eq!((e12.color, e12.stroke_width), (Some(0xf6ab6c), Some(2.)));
    assert_eq!((e12.z_index, e12.interaction_width), (2, Some(30.)));
    assert_eq!(e23.kind, EdgeKind::Custom("buttonedge".into()));
    assert_eq!(e23.label.as_deref(), Some("7"));
    assert_eq!(e23.marker_start, None);
    assert_eq!(object.viewport, Viewport::new(3., 4., 1.5));

    // Saved again, it's React Flow's JSON still, with what this crate didn't read kept.
    let saved: serde_json::Value = serde_json::to_value(&object).unwrap();
    let edge = &saved["edges"][0];
    assert_eq!(edge["sourceHandle"], "a");
    assert_eq!(edge["type"], "smoothstep");
    assert_eq!(edge["style"]["stroke"], "#f6ab6c");
    assert_eq!(edge["style"]["strokeWidth"], 2.0);
    assert_eq!(edge["style"]["strokeDasharray"], "4");
    assert_eq!(edge["markerEnd"]["type"], "arrowclosed");
    assert_eq!(saved["edges"][1]["type"], "buttonedge");
    assert_eq!(saved["edges"][1]["data"]["weight"], 1);
    assert_eq!(saved["nodes"][2]["parentId"], "g");
    assert_eq!(saved["nodes"][2]["extent"], "parent");
    assert_eq!(saved["nodes"][1]["zIndex"], 5);
    assert_eq!(saved["nodes"][1]["style"]["background"], "#123");
    assert_eq!(
        saved["nodes"][3]["extent"],
        serde_json::json!([[0.0, 0.0], [500.0, 500.0]])
    );
    let again: FlowObject = serde_json::from_value(saved).unwrap();
    assert_eq!(again, object);

    // An edge saved by this crate has React Flow's names for the built-in kinds.
    let bezier = serde_json::to_value(Edge::new("x", "1", "2")).unwrap();
    assert_eq!(bezier["type"], "default");
}
