//! A flow as a mind map: a tree read from its edges and laid out automatically, grown with tab
//! and enter, folded with its chips, and rearranged by dropping one node on another.

mod common;

use std::time::Duration;

use common::*;
use gpui::{Modifiers, MouseButton, TestAppContext, VisualTestContext};
use gpui_flow::*;

/// A root with two children, the first with two of its own, laid out to the right at once.
fn map(flow: Flow) -> Flow {
    map_with(flow, TreeDirection::Right, Duration::ZERO)
}

fn map_with(flow: Flow, direction: TreeDirection, motion: Duration) -> Flow {
    let topic = |id: &str| {
        Node::new(id, xy(0., 0.))
            .kind("topic")
            .label(id.to_uppercase())
    };
    flow.with_nodes(vec![
        topic("root").selected_if(false),
        topic("a"),
        topic("b"),
        topic("a1"),
        topic("a2"),
    ])
    .with_edges(vec![
        Edge::new("root-a", "root", "a"),
        Edge::new("root-b", "root", "b"),
        Edge::new("a-a1", "a", "a1"),
        Edge::new("a-a2", "a", "a2"),
    ])
    // Down and in a little, so the whole tree (centred on the root's middle) is on screen.
    .with_viewport(Viewport::new(150., 250., 1.))
    .with_options(move |o| {
        o.layout = Layout::Tree(TreeLayout {
            direction,
            motion,
            ..TreeLayout::default()
        })
    })
}

trait SelectedIf {
    fn selected_if(self, on: bool) -> Self;
}

impl SelectedIf for Node {
    fn selected_if(mut self, on: bool) -> Self {
        self.selected = on;
        self
    }
}

fn rect(flow: &gpui::Entity<Flow>, cx: &VisualTestContext, id: &str) -> Rect {
    flow.read_with(cx, |f, _| f.node_rect(id).unwrap())
}

fn select(flow: &gpui::Entity<Flow>, cx: &mut VisualTestContext, id: &str) {
    flow.update(cx, |f, cx| {
        let ids: Vec<NodeId> = f.nodes().iter().map(|n| n.id.clone()).collect();
        for n in ids {
            f.update_node(&n, |n| n.selected = false, cx);
        }
        f.update_node(id, |n| n.selected = true, cx);
    });
    draw(cx);
}

#[gpui::test]
fn children_are_laid_out_beside_their_parent(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, map);
    draw(cx);
    let (root, a, b, a1, a2) = (
        rect(&flow, cx, "root"),
        rect(&flow, cx, "a"),
        rect(&flow, cx, "b"),
        rect(&flow, cx, "a1"),
        rect(&flow, cx, "a2"),
    );
    // The root stays where it was put; each level is to the right of the one before.
    assert_eq!((root.x, root.y), (0., 0.));
    assert!(a.x > root.right() && b.x == a.x, "{a:?} {b:?}");
    assert!(a1.x > a.right() && a2.x == a1.x);
    // Siblings in edge order, top to bottom, not overlapping; a parent centred on its children.
    assert!(a.bottom() < b.y);
    assert!(a1.bottom() < a2.y);
    let mid = |r: Rect| r.y + r.h / 2.;
    assert!((mid(a) - (a1.y + a2.bottom()) / 2.).abs() < 0.5);
    assert!((mid(root) - (a1.y + b.bottom()) / 2.).abs() < 0.5);
    // Edges join the boxes' sides, so a click between them takes the edge.
    let at = screen(
        &flow,
        cx,
        xy((root.right() + a.x) / 2., (mid(root) + mid(a)) / 2.),
    );
    cx.simulate_click(at, Modifiers::none());
    draw(cx);
    assert_eq!(flow.read_with(cx, |f, _| f.selected_edges()), ["root-a"]);
}

#[gpui::test]
fn tab_adds_a_child_and_enter_a_sibling(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, map);
    select(&flow, cx, "b");
    cx.simulate_keystrokes("tab");
    draw(cx);
    draw(cx);
    let child = flow.read_with(cx, |f, _| {
        let id = f.selected_nodes()[0].clone();
        assert_eq!(f.tree().parent[&id], "b");
        assert_eq!(f.node(&id).unwrap().label_text(), "New idea");
        id
    });
    assert!(events.borrow().iter().any(
        |e| matches!(e, FlowEvent::NodeAdd { node, parent: Some(p) } if *node == child && p == "b")
    ));
    assert!(rect(&flow, cx, &child).x > rect(&flow, cx, "b").right());

    // Enter after a1: a sibling between a1 and a2.
    select(&flow, cx, "a1");
    cx.simulate_keystrokes("enter");
    draw(cx);
    draw(cx);
    flow.read_with(cx, |f, _| {
        let new = f.selected_nodes()[0].clone();
        let kids = f.tree().children_of(&"a".into()).to_vec();
        assert_eq!(kids, [NodeId::from("a1"), new, NodeId::from("a2")]);
    });
    // Enter on the root gives it a child; with nothing selected the keys go on to the host.
    select(&flow, cx, "root");
    cx.simulate_keystrokes("enter");
    draw(cx);
    assert_eq!(
        flow.read_with(cx, |f, _| f.tree().children_of(&"root".into()).len()),
        3
    );
}

#[gpui::test]
fn the_arrows_move_the_selection_around_the_map(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, map);
    select(&flow, cx, "root");
    cx.simulate_keystrokes("right");
    draw(cx);
    // The nearest straight ahead of the root is between a and b; either is a child.
    let first = flow.read_with(cx, |f, _| f.selected_nodes()[0].clone());
    assert!(first == "a" || first == "b", "{first}");
    select(&flow, cx, "a");
    cx.simulate_keystrokes("down");
    draw(cx);
    assert_eq!(flow.read_with(cx, |f, _| f.selected_nodes()), ["b"]);
    cx.simulate_keystrokes("left");
    draw(cx);
    assert_eq!(flow.read_with(cx, |f, _| f.selected_nodes()), ["root"]);
    // Nothing moved.
    assert_eq!(rect(&flow, cx, "root").x, 0.);
}

#[gpui::test]
fn a_chip_folds_a_subtree_away_and_back(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, map);
    let a = rect(&flow, cx, "a");
    let b_before = rect(&flow, cx, "b");
    let a1 = rect(&flow, cx, "a1");
    // The chip sits just past a's right side, half way down.
    let chip = screen(&flow, cx, xy(a.right() + 10., a.y + a.h / 2.));
    cx.simulate_click(chip, Modifiers::none());
    draw(cx);
    draw(cx);
    flow.read_with(cx, |f, _| assert!(f.node("a").unwrap().collapsed));
    assert!(
        events
            .borrow()
            .iter()
            .any(|e| matches!(e, FlowEvent::NodeCollapse { node, collapsed: true } if node == "a"))
    );
    // a1 is gone from the canvas: a click where it was hits the pane.
    cx.simulate_click(screen(&flow, cx, a1.center()), Modifiers::none());
    draw(cx);
    assert!(!flow.read_with(cx, |f, _| f.selected_nodes().contains(&"a1".into())));
    // b closes up toward the root.
    assert!(rect(&flow, cx, "b").y < b_before.y);
    // And back.
    let a = rect(&flow, cx, "a");
    cx.simulate_click(
        screen(&flow, cx, xy(a.right() + 10., a.y + a.h / 2.)),
        Modifiers::none(),
    );
    draw(cx);
    draw(cx);
    assert_eq!(rect(&flow, cx, "b").y, b_before.y);
}

#[gpui::test]
fn dropping_a_node_on_another_makes_it_a_child(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, map);
    let a2 = rect(&flow, cx, "a2");
    let from = screen(&flow, cx, a2.center());
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    draw(cx);
    cx.simulate_mouse_move(offset(from, 20., 0.), MouseButton::Left, Modifiers::none());
    draw(cx);
    // With a2 lifted out, its siblings close up: b is aimed at where it is now.
    let to = screen(&flow, cx, rect(&flow, cx, "b").center());
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    draw(cx);
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    draw(cx);
    draw(cx);
    flow.read_with(cx, |f, _| {
        assert_eq!(f.tree().parent[&NodeId::from("a2")], "b");
        // The edge into it was moved, not added.
        assert_eq!(f.edges().len(), 4);
    });
    assert!(events.borrow().iter().any(
        |e| matches!(e, FlowEvent::NodeReparent { node, parent } if node == "a2" && parent == "b")
    ));
    assert!(rect(&flow, cx, "a2").x > rect(&flow, cx, "b").right());
}

#[gpui::test]
fn dropped_on_nothing_a_node_goes_back(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, map);
    let before = rect(&flow, cx, "a1");
    let from = screen(&flow, cx, before.center());
    drag(cx, from, offset(from, 0., 400.), Modifiers::none());
    draw(cx);
    assert_eq!(rect(&flow, cx, "a1"), before);
    assert_eq!(
        flow.read_with(cx, |f, _| f.tree().parent[&NodeId::from("a1")].clone()),
        "a"
    );
}

#[gpui::test]
fn delete_takes_the_subtree_and_selects_the_parent(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, map);
    select(&flow, cx, "a");
    cx.simulate_keystrokes("backspace");
    draw(cx);
    flow.read_with(cx, |f, _| {
        let ids: Vec<&str> = f.nodes().iter().map(|n| n.id.as_ref()).collect();
        assert_eq!(ids, ["root", "b"]);
        assert_eq!(f.selected_nodes(), ["root"]);
    });
}

#[gpui::test]
fn a_mind_map_grows_both_ways_from_its_root(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, |f| map_with(f, TreeDirection::Both, Duration::ZERO));
    let (root, a, b) = (
        rect(&flow, cx, "root"),
        rect(&flow, cx, "a"),
        rect(&flow, cx, "b"),
    );
    assert!(a.x > root.right());
    assert!(b.right() < root.x);
}

#[gpui::test]
fn nodes_move_to_new_places_smoothly(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, |f| {
        map_with(f, TreeDirection::Right, Duration::from_millis(300))
    });
    draw(cx);
    let b = rect(&flow, cx, "b");
    flow.update(cx, |f, cx| f.set_collapsed("a", true, cx));
    draw(cx);
    std::thread::sleep(Duration::from_millis(40));
    draw(cx);
    let mid = rect(&flow, cx, "b");
    // On its way up, but not there yet.
    assert!(mid.y < b.y, "{mid:?} {b:?}");
    for _ in 0..30 {
        std::thread::sleep(Duration::from_millis(20));
        draw(cx);
    }
    let end = rect(&flow, cx, "b");
    assert!(end.y < mid.y);
    // Settled: another frame moves nothing.
    draw(cx);
    assert_eq!(rect(&flow, cx, "b"), end);
}
