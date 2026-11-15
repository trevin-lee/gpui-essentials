//! Inputs that once crashed, hung or corrupted a flow. Each must now be drawn and driven
//! without a panic, and leave the flow in a state that makes sense.

mod common;

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use common::*;
use gpui_flow::*;
use gpui_kit::{
    AppContext as _, IntoElement as _, Modifiers, MouseButton, ParentElement as _, TestAppContext,
    div,
};

#[gpui_kit::test]
fn an_animated_edge_between_far_apart_nodes_draws_quickly(cx: &mut TestAppContext) {
    // A billion units apart: the dashes were once walked along the whole line, and past
    // about 4e7 an f32 running total stops growing, so the loop never ended.
    let started = Instant::now();
    let (flow, _, cx) = open(cx, |f| {
        f.with_nodes(vec![
            Node::new("a", xy(0., 0.)).label("A"),
            Node::new("b", xy(1e9, 0.)).label("B"),
        ])
        .with_edges(vec![
            Edge::new("a-b", "a", "b")
                .kind(EdgeKind::Straight)
                .animated(true),
        ])
    });
    for _ in 0..3 {
        draw(cx);
    }
    flow.update(cx, |f, cx| f.set_viewport(Viewport::new(0., 0., 2.), cx));
    draw(cx);
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "{:?}",
        started.elapsed()
    );
}

#[gpui_kit::test]
fn zoom_limits_the_wrong_way_round_or_not_numbers_dont_panic(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, |f| {
        with_three(f)
            .with_options(|o| {
                o.min_zoom = 3.;
                o.max_zoom = 2.;
            })
            .fit_view_on_start()
    });
    draw(cx);
    let at = center(&flow, cx, "a");
    scroll(cx, at, 0., 100., Modifiers::none());
    flow.update(cx, |f, cx| {
        f.options.min_zoom = f32::NAN;
        f.zoom_in(cx);
        f.set_viewport(Viewport::new(f32::NAN, 1., f32::NAN), cx);
    });
    cx.simulate_keystrokes("cmd-= cmd-0 shift-1");
    draw(cx);
    let v = flow.read_with(cx, |f, _| f.viewport());
    assert!(
        v.x.is_finite() && v.y.is_finite() && v.zoom.is_finite() && v.zoom > 0.,
        "{v:?}"
    );
}

#[gpui_kit::test]
fn a_zero_snap_grid_leaves_positions_numbers(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, |f| {
        with_three(f).with_options(|o| {
            o.snap_to_grid = true;
            o.snap_grid = (0., 15.);
        })
    });
    let at = center(&flow, cx, "b");
    drag(cx, at, offset(at, 33., 14.), Modifiers::none());
    // x doesn't snap; y does.
    assert_eq!(position(&flow, cx, "b"), xy(133., 270.));
}

#[gpui_kit::test]
fn positions_and_sizes_that_arent_numbers_are_put_right(cx: &mut TestAppContext) {
    // 1e39 overflows an f32 to infinity as JSON loads.
    let object: FlowObject = serde_json::from_str(
        r#"{ "nodes": [
                { "id": "a", "position": { "x": 1e39, "y": 0 }, "data": { "label": "A" } },
                { "id": "b", "position": { "x": 0, "y": 200 }, "width": -5, "data": { "label": "B" } }
             ],
             "edges": [ { "id": "a-b", "source": "a", "target": "b", "animated": true } ],
             "viewport": { "x": 0, "y": 0, "zoom": -1 } }"#,
    )
    .unwrap();
    assert!(object.nodes[0].position.x.is_infinite());
    let (flow, _, cx) = open(cx, |f| f);
    flow.update(cx, |f, cx| f.restore(object, cx));
    draw(cx);
    draw(cx);
    flow.read_with(cx, |f, _| {
        assert_eq!(f.node("a").unwrap().position, xy(0., 0.));
        assert_eq!(f.node("b").unwrap().width, None);
        assert!(f.viewport().zoom > 0.);
    });
}

#[gpui_kit::test]
fn a_node_renderer_reads_the_flow_it_is_given(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, |f| {
        with_three(f).with_node_kind("counted", |node, ctx, flow, _, _| {
            // Reading the flow while it's drawn, as a "3 connections" badge would.
            let edges = flow
                .edges()
                .iter()
                .filter(|e| e.source == node.id || e.target == node.id)
                .count();
            div()
                .w(units(120.))
                .child(format!("{edges} edges"))
                .child(ctx.handle(HandleKind::Target, Side::Top))
                .into_any_element()
        })
    });
    flow.update(cx, |f, cx| {
        f.update_node("b", |n| n.kind = "counted".into(), cx);
    });
    draw(cx);
    assert_eq!(flow.read_with(cx, |f, _| f.node_rect("b").unwrap().w), 120.);
}

use gpui_kit::Styled as _;

#[gpui_kit::test]
fn two_nodes_with_one_id_settle(cx: &mut TestAppContext) {
    // Both copies once wrote their different sizes over one measurement, and each frame asked
    // for another.
    let (flow, _, cx) = open(cx, |f| {
        f.with_nodes(vec![
            Node::new("a", xy(0., 0.)).label("A"),
            Node::new("a", xy(300., 0.)).kind("group"),
        ])
    });
    let notified = Rc::new(Cell::new(0));
    let seen = notified.clone();
    let _watch = cx.update(|_, cx| cx.observe(&flow, move |_, _| seen.set(seen.get() + 1)));
    for _ in 0..4 {
        draw(cx);
    }
    assert_eq!(notified.get(), 0);
}

#[gpui_kit::test]
fn a_huge_z_index_doesnt_overflow(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, |f| {
        f.with_nodes(vec![
            Node::new("a", xy(100., 100.))
                .label("A")
                .z_index(i32::MAX - 10),
        ])
    });
    cx.simulate_click(center(&flow, cx, "a"), Modifiers::none());
    draw(cx);
    assert_eq!(flow.read_with(cx, |f, _| f.selected_nodes()), ["a"]);
}

#[gpui_kit::test]
fn a_node_that_is_its_own_parent_stays_where_it_is_and_moves(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, |f| {
        f.with_nodes(vec![
            Node::new("self", xy(2., 3.)).parent("self").label("Self"),
            Node::new("p", xy(100., 100.)).parent("q").label("P"),
            Node::new("q", xy(1., 1.)).parent("p").label("Q"),
        ])
    });
    flow.read_with(cx, |f, _| {
        assert_eq!(f.absolute_position("self"), Some(xy(2., 3.)));
        // A loop is cut where it comes back round: p is in q, and q in nothing further.
        assert_eq!(f.absolute_position("p"), Some(xy(101., 101.)));
    });
    flow.update(cx, |f, cx| {
        f.update_node("self", |n| n.selected = true, cx);
    });
    cx.simulate_keystrokes("right");
    draw(cx);
    assert_eq!(position(&flow, cx, "self"), xy(7., 3.));
}

#[gpui_kit::test]
fn removing_the_target_mid_connection_makes_no_edge(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, with_three);
    let from = handle(&flow, cx, "a", HandleKind::Source);
    let to = handle(&flow, cx, "c", HandleKind::Target);
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    draw(cx);
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    draw(cx);
    flow.update(cx, |f, cx| {
        f.remove_nodes(&["c".into()], cx);
    });
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::none());
    draw(cx);
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
fn an_animated_edge_off_screen_doesnt_keep_redrawing(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, |f| {
        f.with_nodes(vec![
            Node::new("a", xy(5000., 5000.)).label("A"),
            Node::new("b", xy(5000., 5200.)).label("B"),
        ])
        .with_edges(vec![Edge::new("a-b", "a", "b").animated(true)])
    });
    let notified = Rc::new(Cell::new(0));
    let seen = notified.clone();
    let _watch = cx.update(|_, cx| cx.observe(&flow, move |_, _| seen.set(seen.get() + 1)));
    cx.executor().advance_clock(Duration::from_secs(1));
    draw(cx);
    assert_eq!(notified.get(), 0);
    // On screen, it ticks.
    flow.update(cx, |f, cx| {
        f.set_viewport(Viewport::new(-4900., -4900., 1.), cx)
    });
    draw(cx);
    draw(cx);
    notified.set(0);
    cx.executor().advance_clock(Duration::from_secs(1));
    draw(cx);
    assert!(notified.get() >= 10, "{}", notified.get());
}

#[gpui_kit::test]
fn the_node_index_is_fresh_after_building(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_flow::init(cx);
    });
    let flow = cx.new(|cx| {
        let f = Flow::new(cx);
        assert!(f.node("a").is_none());
        f.with_nodes(vec![Node::new("a", xy(0., 0.))])
    });
    assert!(flow.read_with(cx, |f, _| f.node("a").is_some()));
}
