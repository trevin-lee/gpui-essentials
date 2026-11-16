//! What a flow does as React Flow does it, and the events that let a host follow along.

mod common;

use common::*;
use gpui::{
    IntoElement as _, Modifiers, MouseButton, ParentElement as _, Styled as _, TestAppContext, div,
};
use gpui_flow::*;

fn count(events: &Events, f: impl Fn(&FlowEvent) -> bool) -> usize {
    events.borrow().iter().filter(|e| f(e)).count()
}

#[gpui::test]
fn another_button_doesnt_end_or_replace_a_drag(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, with_three);
    let at = center(&flow, cx, "b");
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(offset(at, 20., 0.), MouseButton::Left, Modifiers::none());
    draw(cx);
    // A middle press (which would pan) and a right release, mid-drag.
    cx.simulate_mouse_down(offset(at, 20., 0.), MouseButton::Middle, Modifiers::none());
    cx.simulate_mouse_up(offset(at, 20., 0.), MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_move(offset(at, 60., 0.), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(offset(at, 60., 0.), MouseButton::Left, Modifiers::none());
    draw(cx);
    assert_eq!(position(&flow, cx, "b"), xy(160., 250.));
    assert_eq!(
        count(&events, |e| matches!(e, FlowEvent::NodeDragStart(_))),
        1
    );
    assert_eq!(
        count(&events, |e| matches!(e, FlowEvent::NodeDragStop(_))),
        1
    );
    assert_eq!(flow.read_with(cx, |f, _| f.viewport()), Viewport::default());
}

#[gpui::test]
fn a_node_stays_under_the_pointer_when_the_view_zooms_mid_drag(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    let at = center(&flow, cx, "b");
    let grab = flow.read_with(cx, |f, _| {
        f.screen_to_flow(at) - f.node("b").unwrap().position
    });
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(offset(at, 10., 0.), MouseButton::Left, Modifiers::none());
    draw(cx);
    scroll(cx, offset(at, 10., 0.), 0., 150., Modifiers::none());
    let to = offset(at, 40., 30.);
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    draw(cx);
    flow.read_with(cx, |f, _| {
        let now = f.screen_to_flow(to) - f.node("b").unwrap().position;
        assert!(
            (now.x - grab.x).abs() < 0.01 && (now.y - grab.y).abs() < 0.01,
            "{now:?} {grab:?}"
        );
    });
}

#[gpui::test]
fn a_zoom_mid_pan_isnt_undone_by_the_next_move(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    let at = screen(&flow, cx, xy(600., 60.));
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(offset(at, 10., 0.), MouseButton::Left, Modifiers::none());
    draw(cx);
    scroll(cx, offset(at, 10., 0.), 0., 150., Modifiers::none());
    let zoomed = flow.read_with(cx, |f, _| f.viewport().zoom);
    assert!(zoomed > 1.);
    cx.simulate_mouse_move(offset(at, 30., 0.), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(offset(at, 30., 0.), MouseButton::Left, Modifiers::none());
    draw(cx);
    assert_eq!(flow.read_with(cx, |f, _| f.viewport().zoom), zoomed);
}

#[gpui::test]
fn snapping_puts_a_child_on_the_flows_grid_and_keeps_a_groups_spacing(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, |f| {
        f.with_nodes(vec![
            Node::new("group", xy(107., 103.))
                .kind("group")
                .size(400., 300.),
            Node::new("child", xy(20., 30.))
                .parent("group")
                .size(80., 40.)
                .label("C"),
            Node::new("x", xy(600., 0.)).label("X"),
            Node::new("y", xy(607., 200.)).label("Y"),
        ])
        .with_options(|o| {
            o.snap_to_grid = true;
            o.snap_grid = (20., 20.);
        })
    });
    let at = center(&flow, cx, "child");
    drag(cx, at, offset(at, 33., 14.), Modifiers::none());
    let abs = flow.read_with(cx, |f, _| f.absolute_position("child").unwrap());
    assert_eq!((abs.x % 20., abs.y % 20.), (0., 0.), "{abs:?}");

    // Two nodes 7 apart stay 7 apart: the pressed one snaps, the other follows.
    flow.update(cx, |f, cx| {
        f.update_node("x", |n| n.selected = true, cx);
        f.update_node("y", |n| n.selected = true, cx);
    });
    let at = center(&flow, cx, "x");
    drag(cx, at, offset(at, 21., 0.), Modifiers::none());
    assert_eq!(position(&flow, cx, "x").x, 620.);
    assert_eq!(position(&flow, cx, "y").x, 627.);
}

#[gpui::test]
fn clearing_a_selection_the_flow_started_with_is_reported(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, |f| {
        let (mut nodes, edges) = three();
        nodes[0].selected = true;
        f.with_nodes(nodes).with_edges(edges)
    });
    cx.simulate_click(screen(&flow, cx, xy(600., 60.)), Modifiers::none());
    draw(cx);
    assert!(
        events
            .borrow()
            .iter()
            .any(|e| matches!(e, FlowEvent::SelectionChange { nodes, .. } if nodes.is_empty()))
    );
}

#[gpui::test]
fn escape_puts_back_a_drag_and_says_it_stopped(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, with_three);
    let at = center(&flow, cx, "b");
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(offset(at, 50., 50.), MouseButton::Left, Modifiers::none());
    draw(cx);
    cx.simulate_keystrokes("escape");
    cx.simulate_mouse_up(offset(at, 50., 50.), MouseButton::Left, Modifiers::none());
    draw(cx);
    assert_eq!(position(&flow, cx, "b"), xy(100., 250.));
    assert_eq!(
        count(
            &events,
            |e| matches!(e, FlowEvent::NodeDragStop(ids) if ids == &["b"])
        ),
        1
    );
}

#[gpui::test]
fn escape_puts_back_a_resize(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, |f| {
        f.with_nodes(vec![
            Node::new("n", xy(100., 100.))
                .kind("sized")
                .size(150., 100.)
                .selected_now(),
        ])
        .with_node_kind("sized", |_, ctx, _, _, _| {
            div()
                .size_full()
                .relative()
                .child(ctx.resizer(50., 50.))
                .into_any_element()
        })
    });
    // The bottom right grip sits on the corner.
    let grip = screen(&flow, cx, xy(250., 200.));
    cx.simulate_mouse_down(grip, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(offset(grip, 40., 30.), MouseButton::Left, Modifiers::none());
    draw(cx);
    assert_eq!(
        flow.read_with(cx, |f, _| f.node("n").unwrap().width),
        Some(190.)
    );
    cx.simulate_keystrokes("escape");
    cx.simulate_mouse_up(offset(grip, 40., 30.), MouseButton::Left, Modifiers::none());
    draw(cx);
    flow.read_with(cx, |f, _| {
        let n = f.node("n").unwrap();
        assert_eq!(
            (n.width, n.height, n.position),
            (Some(150.), Some(100.), xy(100., 100.))
        );
    });
    assert_eq!(
        count(&events, |e| matches!(e, FlowEvent::NodeResizeStart(_))),
        1
    );
    assert_eq!(
        count(&events, |e| matches!(e, FlowEvent::NodeResizeEnd(_))),
        1
    );
}

trait Selected {
    fn selected_now(self) -> Self;
}

impl Selected for Node {
    fn selected_now(mut self) -> Self {
        self.selected = true;
        self
    }
}

#[gpui::test]
fn delete_reports_once_and_keeps_what_may_not_go(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, |f| {
        f.with_nodes(vec![
            Node::new("g", xy(100., 100.))
                .kind("group")
                .size(300., 200.)
                .selected_now(),
            Node::new("keep", xy(20., 30.))
                .parent("g")
                .extent_parent(true)
                .deletable(false)
                .label("K"),
            Node::new("go", xy(150., 30.)).parent("g").label("G"),
            Node::new("x", xy(600., 100.)).label("X"),
        ])
        .with_edges(vec![
            Edge::new("keep-x", "keep", "x"),
            Edge::new("go-x", "go", "x").deletable(false),
        ])
    });
    events.borrow_mut().clear();
    cx.simulate_keystrokes("backspace");
    draw(cx);
    flow.read_with(cx, |f, _| {
        let ids: Vec<&str> = f.nodes().iter().map(|n| n.id.as_ref()).collect();
        assert_eq!(ids, ["keep", "x"]);
        // It stays where it was in the flow, no longer anyone's child.
        let keep = f.node("keep").unwrap();
        assert_eq!(
            (keep.parent.clone(), keep.position, keep.extent),
            (None, xy(120., 130.), None)
        );
        // An edge can't outlive its node, deletable or not.
        let edges: Vec<&str> = f.edges().iter().map(|e| e.id.as_ref()).collect();
        assert_eq!(edges, ["keep-x"]);
    });
    let events = events.borrow();
    assert!(
        matches!(&events[0], FlowEvent::Delete { nodes, edges } if nodes.len() == 2 && edges.len() == 1)
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, FlowEvent::SelectionChange { .. }))
            .count(),
        1
    );
}

#[gpui::test]
fn the_hosts_delete_check_can_keep_everything(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, |f| {
        with_three(f).with_delete_check(|nodes, _, _| !nodes.iter().any(|n| n.id == "b"))
    });
    flow.update(cx, |f, cx| f.select_all(cx));
    cx.simulate_keystrokes("backspace");
    draw(cx);
    assert_eq!(flow.read_with(cx, |f, _| f.nodes().len()), 3);
    assert_eq!(count(&events, |e| matches!(e, FlowEvent::Delete { .. })), 0);
}

#[gpui::test]
fn the_connection_radius_is_in_flow_units(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, |f| {
        with_three(f).with_viewport(Viewport::new(0., 0., 2.))
    });
    let from = handle(&flow, cx, "a", HandleKind::Source);
    let target = flow.read_with(cx, |f, _| {
        f.handle_point("c", HandleKind::Target, None).unwrap()
    });
    // 15 flow units (30 points at zoom 2) above the handle: within React Flow's 20.
    drag(
        cx,
        from,
        screen(&flow, cx, target - xy(0., 15.)),
        Modifiers::none(),
    );
    assert_eq!(flow.read_with(cx, |f, _| f.edges().len()), 2);
}

#[gpui::test]
fn a_selection_rectangle_takes_the_edges_of_what_it_takes(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    // Round a alone.
    drag(
        cx,
        screen(&flow, cx, xy(60., 60.)),
        screen(&flow, cx, xy(300., 180.)),
        Modifiers::shift(),
    );
    flow.read_with(cx, |f, _| {
        assert_eq!(f.selected_nodes(), ["a"]);
        assert_eq!(f.selected_edges(), ["a-b"]);
    });
}

#[gpui::test]
fn a_shift_drag_from_a_node_selects_rather_than_moves(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, |f| {
        with_three(f).with_options(|o| o.selection_mode = SelectionMode::Partial)
    });
    let at = center(&flow, cx, "b");
    drag(
        cx,
        at,
        screen(&flow, cx, xy(500., 320.)),
        Modifiers::shift(),
    );
    flow.read_with(cx, |f, _| {
        assert_eq!(f.node("b").unwrap().position, xy(100., 250.));
        assert_eq!(f.selected_nodes(), ["b", "c"]);
    });
}

#[gpui::test]
fn a_child_is_above_its_parent_whatever_their_z(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, |f| {
        f.with_nodes(vec![
            Node::new("g", xy(100., 100.))
                .kind("group")
                .size(400., 300.)
                .z_index(5),
            Node::new("a", xy(20., 30.)).parent("g").label("A"),
            Node::new("b", xy(220., 200.)).parent("g").label("B"),
        ])
        .with_edges(vec![Edge::new("a-b", "a", "b").kind(EdgeKind::Straight)])
    });
    cx.simulate_click(center(&flow, cx, "a"), Modifiers::none());
    draw(cx);
    assert_eq!(flow.read_with(cx, |f, _| f.selected_nodes()), ["a"]);
    // The edge between the two children is over the group, and takes a click there.
    let (s, t) = flow.read_with(cx, |f, _| {
        (
            f.handle_point("a", HandleKind::Source, None).unwrap(),
            f.handle_point("b", HandleKind::Target, None).unwrap(),
        )
    });
    cx.simulate_click(screen(&flow, cx, (s + t) * 0.5), Modifiers::none());
    draw(cx);
    assert_eq!(flow.read_with(cx, |f, _| f.selected_edges()), ["a-b"]);
    assert_eq!(
        count(
            &events,
            |e| matches!(e, FlowEvent::EdgeClick(id) if id == "a-b")
        ),
        1
    );
}

#[gpui::test]
fn a_selected_parent_that_cant_move_doesnt_hold_its_child(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, |f| {
        f.with_nodes(vec![
            Node::new("g", xy(100., 100.))
                .kind("group")
                .size(400., 300.)
                .draggable(false)
                .selected_now(),
            Node::new("a", xy(20., 30.))
                .parent("g")
                .label("A")
                .selected_now(),
        ])
    });
    cx.simulate_keystrokes("right");
    draw(cx);
    assert_eq!(position(&flow, cx, "a"), xy(25., 30.));
}

#[gpui::test]
fn a_node_added_off_screen_is_measured_so_its_edges_draw(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    flow.update(cx, |f, cx| {
        f.add_node(
            Node::new("far", xy(3000., 3000.))
                .size(100., 50.)
                .label("Far"),
            cx,
        );
        f.add_edge(Edge::new("a-far", "a", "far"), cx);
    });
    draw(cx);
    draw(cx);
    assert!(
        flow.read_with(cx, |f, _| f.handle_point("far", HandleKind::Target, None))
            .is_some()
    );
    // Made wider while off screen, its handle follows.
    flow.update(cx, |f, cx| {
        f.update_node("far", |n| n.width = Some(400.), cx);
    });
    draw(cx);
    draw(cx);
    let p = flow.read_with(cx, |f, _| {
        f.handle_point("far", HandleKind::Target, None).unwrap()
    });
    assert_eq!(p.x, 3200.);
}

#[gpui::test]
fn nodes_too_small_to_read_are_still_pressed_and_dragged(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, |f| with_three(f).with_options(|o| o.min_zoom = 0.1));
    flow.update(cx, |f, cx| f.set_viewport(Viewport::new(0., 0., 0.2), cx));
    draw(cx);
    let at = center(&flow, cx, "c");
    drag(cx, at, offset(at, 10., 10.), Modifiers::none());
    flow.read_with(cx, |f, _| {
        assert_eq!(f.selected_nodes(), ["c"]);
        assert_eq!(f.node("c").unwrap().position, xy(450., 300.));
        // Edges still meet the handles they measured when last drawn whole.
        assert!(f.handle_point("b", HandleKind::Target, None).is_some());
    });
}

#[gpui::test]
fn a_child_saved_outside_its_parent_is_put_inside(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, |f| {
        f.with_nodes(vec![
            Node::new("g", xy(0., 0.)).kind("group").size(100., 100.),
            Node::new("c", xy(500., 500.))
                .parent("g")
                .extent_parent(true)
                .size(40., 20.),
        ])
    });
    draw(cx);
    assert_eq!(position(&flow, cx, "c"), xy(60., 80.));
}

#[gpui::test]
fn set_center_zooms_to_the_largest_unless_told(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    flow.update(cx, |f, cx| f.set_center(xy(0., 0.), None, cx));
    assert_eq!(flow.read_with(cx, |f, _| f.viewport().zoom), 2.);
    flow.update(cx, |f, cx| f.set_center(xy(0., 0.), Some(1.), cx));
    assert_eq!(flow.read_with(cx, |f, _| f.viewport().zoom), 1.);
}

#[gpui::test]
fn a_connection_dropped_on_the_pane_says_where(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, with_three);
    let from = handle(&flow, cx, "a", HandleKind::Source);
    drag(
        cx,
        from,
        screen(&flow, cx, xy(600., 500.)),
        Modifiers::none(),
    );
    assert!(events.borrow().iter().any(|e| matches!(
        e,
        FlowEvent::ConnectEnd { from, to: None, connected: false, pointer }
            if from.node == "a" && (pointer.x - 600.).abs() < 0.01 && (pointer.y - 500.).abs() < 0.01
    )));
    // A connection made by the host isn't reported back to it.
    flow.update(cx, |f, cx| {
        f.connect(
            Connection {
                source: "b".into(),
                source_handle: None,
                target: "c".into(),
                target_handle: None,
            },
            cx,
        );
    });
    assert_eq!(count(&events, |e| matches!(e, FlowEvent::Connect(_))), 0);
    assert_eq!(flow.read_with(cx, |f, _| f.edges().len()), 2);
}

#[gpui::test]
fn loose_connections_dont_join_a_handle_to_its_own_id(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, |f| {
        with_three(f).with_options(|o| o.connection_mode = ConnectionMode::Loose)
    });
    // b's source and target are both unnamed.
    let from = handle(&flow, cx, "b", HandleKind::Source);
    let to = handle(&flow, cx, "b", HandleKind::Target);
    drag(cx, from, to, Modifiers::none());
    assert_eq!(flow.read_with(cx, |f, _| f.edges().len()), 1);
}

#[gpui::test]
fn the_arrow_keys_report_their_moves_and_shift_goes_further(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, with_three);
    flow.update(cx, |f, cx| {
        f.update_node("c", |n| n.selected = true, cx);
    });
    cx.simulate_keystrokes("right shift-down");
    draw(cx);
    assert_eq!(position(&flow, cx, "c"), xy(405., 270.));
    assert_eq!(
        count(
            &events,
            |e| matches!(e, FlowEvent::NodeMove(ids) if ids == &["c"])
        ),
        2
    );
    assert_eq!(
        count(&events, |e| matches!(e, FlowEvent::NodeDragStop(_))),
        0
    );
}

#[gpui::test]
fn a_transition_reports_the_viewport_as_it_goes(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, |f| {
        with_three(f).with_options(|o| o.transition = std::time::Duration::from_millis(400))
    });
    events.borrow_mut().clear();
    flow.update(cx, |f, cx| f.zoom_to(2., cx));
    draw(cx);
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(120));
    draw(cx);
    // Pressing the pane stops it part way, where the last report left it.
    cx.simulate_click(screen(&flow, cx, xy(600., 60.)), Modifiers::none());
    draw(cx);
    let at = flow.read_with(cx, |f, _| f.viewport());
    let last = events.borrow().iter().rev().find_map(|e| match e {
        FlowEvent::ViewportChange(v) => Some(*v),
        _ => None,
    });
    assert!(at.zoom > 1. && at.zoom < 2., "{at:?}");
    assert_eq!(last, Some(at));
}
