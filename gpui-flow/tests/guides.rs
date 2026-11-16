//! Lining nodes up: guides while dragging, and align and distribute commands.

mod common;

use common::*;
use gpui::{Modifiers, MouseButton, TestAppContext};
use gpui_flow::*;

/// `a` and `b` side by side, `c` below, with guides on.
fn row(guides: bool) -> impl FnOnce(Flow) -> Flow {
    move |flow| {
        flow.with_nodes(vec![
            Node::new("a", xy(100., 100.)).label("A").size(120., 40.),
            Node::new("b", xy(400., 160.)).label("B").size(120., 40.),
            Node::new("c", xy(100., 400.)).label("C").size(80., 60.),
        ])
        .with_options(move |o| o.alignment_guides = guides)
    }
}

#[gpui::test]
fn a_node_dragged_near_lining_up_lines_up_and_shows_a_guide(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, row(true));
    // b's top is 60 below a's: dragged up 56, it's 4 off, within reach, and lines up.
    let at = center(&flow, cx, "b");
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(offset(at, 0., -30.), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(offset(at, 0., -56.), MouseButton::Left, Modifiers::none());
    draw(cx);
    flow.read_with(cx, |f, _| {
        assert_eq!(f.node("b").unwrap().position.y, 100.);
        let g = f.alignment_guides();
        assert_eq!(g.len(), 1, "{g:?}");
        // Along the shared top, from a's left to b's right.
        assert_eq!((g[0].from, g[0].to), (xy(100., 100.), xy(520., 100.)));
    });
    cx.simulate_mouse_up(offset(at, 0., -56.), MouseButton::Left, Modifiers::none());
    draw(cx);
    flow.read_with(cx, |f, _| {
        assert_eq!(f.node("b").unwrap().position, xy(400., 100.));
        assert!(f.alignment_guides().is_empty());
    });
}

#[gpui::test]
fn out_of_reach_nothing_lines_up(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, row(true));
    let at = center(&flow, cx, "b");
    // Up 30: every edge and middle is 10 or more from any of a's.
    drag(cx, at, offset(at, 0., -30.), Modifiers::none());
    assert_eq!(position(&flow, cx, "b"), xy(400., 130.));
}

#[gpui::test]
fn with_guides_off_nothing_lines_up(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, row(false));
    let at = center(&flow, cx, "b");
    drag(cx, at, offset(at, 0., -56.), Modifiers::none());
    assert_eq!(position(&flow, cx, "b"), xy(400., 104.));
}

fn select_all(flow: &gpui::Entity<Flow>, cx: &mut gpui::VisualTestContext) {
    flow.update(cx, |f, cx| f.select_all(cx));
}

#[gpui::test]
fn align_lines_up_the_selection(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, row(false));
    select_all(&flow, cx);
    let cases = [
        (Align::Left, [("a", 100.), ("b", 100.), ("c", 100.)], true),
        (Align::Right, [("a", 400.), ("b", 400.), ("c", 440.)], true),
        (Align::Top, [("a", 100.), ("b", 100.), ("c", 100.)], false),
        (
            Align::Bottom,
            [("a", 420.), ("b", 420.), ("c", 400.)],
            false,
        ),
    ];
    for (align, want, across) in cases {
        flow.update(cx, |f, cx| {
            f.update_node("a", |n| n.position = xy(100., 100.), cx);
            f.update_node("b", |n| n.position = xy(400., 160.), cx);
            f.update_node("c", |n| n.position = xy(100., 400.), cx);
            assert!(f.align_selected(align, cx));
        });
        flow.read_with(cx, |f, _| {
            for (id, v) in want {
                let p = f.node(id).unwrap().position;
                assert_eq!(if across { p.x } else { p.y }, v, "{align:?} {id}");
            }
        });
    }
    // Middles: centred on the selection's middle, 100 to 520 across.
    flow.update(cx, |f, cx| {
        f.align_selected(Align::Center, cx);
    });
    flow.read_with(cx, |f, _| {
        assert_eq!(f.node("a").unwrap().position.x, 250.);
        assert_eq!(f.node("c").unwrap().position.x, 270.);
    });
    assert!(
        events
            .borrow()
            .iter()
            .any(|e| matches!(e, FlowEvent::NodeMove(ids) if ids.len() == 3))
    );
    // With one selected there's nothing to line up with.
    flow.update(cx, |f, cx| {
        f.deselect_all(cx);
        f.update_node("a", |n| n.selected = true, cx);
        assert!(!f.align_selected(Align::Left, cx));
    });
}

#[gpui::test]
fn distribute_spaces_the_selection_evenly(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, |f| {
        f.with_nodes(vec![
            Node::new("a", xy(0., 0.)).size(100., 40.),
            Node::new("b", xy(130., 200.)).size(60., 40.),
            Node::new("c", xy(500., 50.)).size(100., 40.),
        ])
    });
    select_all(&flow, cx);
    flow.update(cx, |f, cx| {
        assert!(f.distribute_selected(Distribute::Horizontally, cx));
    });
    flow.read_with(cx, |f, _| {
        // 0 to 600, 260 of nodes: two gaps of 170.
        assert_eq!(f.node("a").unwrap().position.x, 0.);
        assert_eq!(f.node("b").unwrap().position.x, 270.);
        assert_eq!(f.node("c").unwrap().position.x, 500.);
        // Only across: up and down they stay.
        assert_eq!(f.node("b").unwrap().position.y, 200.);
    });
}

#[gpui::test]
fn a_node_in_a_frame_lines_up_by_where_it_is_in_the_flow(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, |f| {
        f.with_nodes(vec![
            Node::new("frame", xy(300., 300.))
                .kind("frame")
                .size(300., 200.),
            Node::new("in", xy(40., 60.)).parent("frame").size(80., 40.),
            Node::new("out", xy(100., 100.)).size(80., 40.),
        ])
    });
    flow.update(cx, |f, cx| {
        f.update_node("in", |n| n.selected = true, cx);
        f.update_node("out", |n| n.selected = true, cx);
        f.align_selected(Align::Left, cx);
    });
    flow.read_with(cx, |f, _| {
        assert_eq!(f.absolute_position("in").unwrap().x, 100.);
        assert_eq!(f.node("in").unwrap().position.x, -200.);
    });
}
