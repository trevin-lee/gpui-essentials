//! A flow under a finger, as on iPhone and iPad: raw touches go through GPUI's own gesture
//! recogniser, as the iOS platform's do.

mod common;

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use common::*;
use gpui::{
    InteractiveElement as _, IntoElement as _, Modifiers, MouseButton, ParentElement as _, Pixels,
    Point, StatefulInteractiveElement as _, Styled as _, TestAppContext, TouchEvent, TouchId,
    TouchPhase, VisualTestContext, div, point,
};
use gpui_flow::*;

/// One finger down at `from`, moved to `to` in a few steps, and lifted.
fn swipe(cx: &mut VisualTestContext, from: Point<Pixels>, to: Point<Pixels>) {
    touch(cx, TouchPhase::Started, from);
    for i in 1..=4 {
        let t = i as f32 / 4.;
        touch(
            cx,
            TouchPhase::Moved,
            point(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t),
        );
    }
    touch(cx, TouchPhase::Ended, to);
}

fn touch(cx: &mut VisualTestContext, phase: TouchPhase, at: Point<Pixels>) {
    cx.simulate_event(TouchEvent {
        id: TouchId(1),
        phase,
        position: at,
        ..Default::default()
    });
    draw(cx);
}

fn tap(cx: &mut VisualTestContext, at: Point<Pixels>) {
    touch(cx, TouchPhase::Started, at);
    touch(cx, TouchPhase::Ended, at);
}

fn hold(cx: &mut VisualTestContext, at: Point<Pixels>) {
    touch(cx, TouchPhase::Started, at);
    cx.executor().advance_clock(Duration::from_millis(600));
    draw(cx);
    touch(cx, TouchPhase::Ended, at);
}

fn menus(events: &Events) -> Vec<ContextTarget> {
    events
        .borrow()
        .iter()
        .filter_map(|e| match e {
            FlowEvent::ContextMenu { target, .. } => Some(target.clone()),
            _ => None,
        })
        .collect()
}

#[gpui::test]
fn a_finger_drags_a_node_and_pans_the_pane(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, with_three);
    let at = center(&flow, cx, "b");
    swipe(cx, at, offset(at, 60., 40.));
    flow.read_with(cx, |f, _| {
        assert_eq!(f.node("b").unwrap().position, xy(160., 290.));
        assert_eq!(f.selected_nodes(), ["b"]);
        assert_eq!(f.viewport(), Viewport::default());
    });
    assert!(
        events
            .borrow()
            .iter()
            .any(|e| matches!(e, FlowEvent::NodeDragStop(ids) if ids == &["b"]))
    );
    // On the empty pane, the flow follows the finger.
    let pane = screen(&flow, cx, xy(600., 60.));
    swipe(cx, pane, offset(pane, -50., 30.));
    flow.read_with(cx, |f, _| {
        let v = f.viewport();
        assert_eq!(v.zoom, 1.);
        assert!((v.x + 50.).abs() < 0.5 && (v.y - 30.).abs() < 0.5, "{v:?}");
        assert_eq!(f.node("b").unwrap().position, xy(160., 290.));
    });
}

#[gpui::test]
fn a_tap_selects_and_a_second_tap_is_a_double_tap(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, with_three);
    let at = center(&flow, cx, "c");
    // A finger that wobbles less than the slop still taps, and leaves the node where it was.
    touch(cx, TouchPhase::Started, at);
    touch(cx, TouchPhase::Moved, offset(at, 3., 2.));
    touch(cx, TouchPhase::Ended, offset(at, 3., 2.));
    assert_eq!(flow.read_with(cx, |f, _| f.selected_nodes()), ["c"]);
    assert_eq!(position(&flow, cx, "c"), xy(400., 250.));
    tap(cx, at);
    let events = events.borrow();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, FlowEvent::NodeClick(id) if id == "c"))
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, FlowEvent::NodeDoubleClick(id) if id == "c"))
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, FlowEvent::NodeDragStart(_)))
    );
}

#[gpui::test]
fn a_finger_connects_handles_and_resizes(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, |f| {
        with_three(f).with_node_kind("box", |_, ctx, _, _, _| {
            div()
                .size_full()
                .relative()
                .child(ctx.resizer(50., 50.))
                .into_any_element()
        })
    });
    let from = handle(&flow, cx, "a", HandleKind::Source);
    let to = handle(&flow, cx, "c", HandleKind::Target);
    swipe(cx, from, to);
    assert_eq!(flow.read_with(cx, |f, _| f.edges().len()), 2);
    assert!(
        events
            .borrow()
            .iter()
            .any(|e| matches!(e, FlowEvent::Connect(_)))
    );

    // A grip is a 44-point target: a finger 15 points off its corner still takes it.
    flow.update(cx, |f, cx| {
        f.add_node(
            Node::new("box", xy(100., 450.))
                .kind("box")
                .size(150., 100.),
            cx,
        );
        f.update_node("box", |n| n.selected = true, cx);
    });
    draw(cx);
    let grip = screen(&flow, cx, xy(265., 565.));
    swipe(cx, grip, offset(grip, 40., 20.));
    flow.read_with(cx, |f, _| {
        let n = f.node("box").unwrap();
        assert_eq!((n.width, n.height), (Some(190.), Some(120.)));
    });
}

#[gpui::test]
fn holding_still_asks_for_a_context_menu(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, with_three);
    let at = center(&flow, cx, "b");
    hold(cx, at);
    assert_eq!(menus(&events), [ContextTarget::Node("b".into())]);
    // It didn't move, and the lift isn't a click.
    assert_eq!(position(&flow, cx, "b"), xy(100., 250.));
    assert!(
        !events
            .borrow()
            .iter()
            .any(|e| matches!(e, FlowEvent::NodeClick(_)))
    );

    // On an edge, from a finger's distance (15 points off the line).
    let (s, t) = flow.read_with(cx, |f, _| {
        (
            f.handle_point("a", HandleKind::Source, None).unwrap(),
            f.handle_point("b", HandleKind::Target, None).unwrap(),
        )
    });
    hold(cx, offset(screen(&flow, cx, (s + t) * 0.5), 15., 0.));
    hold(cx, screen(&flow, cx, xy(600., 60.)));
    let menus = menus(&events);
    assert_eq!(menus[1], ContextTarget::Edge("a-b".into()));
    assert_eq!(menus[2], ContextTarget::Pane);
    let position = events.borrow().iter().rev().find_map(|e| match e {
        FlowEvent::ContextMenu { position, .. } => Some(*position),
        _ => None,
    });
    let p = position.unwrap();
    assert!((p.x - 600.).abs() < 0.5 && (p.y - 60.).abs() < 0.5, "{p:?}");
}

#[gpui::test]
fn a_right_click_asks_for_a_context_menu_too(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, with_three);
    cx.simulate_mouse_down(
        center(&flow, cx, "c"),
        MouseButton::Right,
        Modifiers::none(),
    );
    cx.simulate_mouse_up(
        center(&flow, cx, "c"),
        MouseButton::Right,
        Modifiers::none(),
    );
    let pane = screen(&flow, cx, xy(600., 60.));
    cx.simulate_mouse_down(pane, MouseButton::Right, Modifiers::none());
    cx.simulate_mouse_up(pane, MouseButton::Right, Modifiers::none());
    draw(cx);
    assert_eq!(
        menus(&events),
        [ContextTarget::Node("c".into()), ContextTarget::Pane]
    );
    assert!(flow.read_with(cx, |f, _| f.selected_nodes()).is_empty());
}

#[gpui::test]
fn a_finger_taps_a_nodrag_button_without_dragging_its_node(cx: &mut TestAppContext) {
    let clicks = Rc::new(Cell::new(0));
    let counted = clicks.clone();
    let (flow, _, cx) = open(cx, move |f| {
        with_three(f).with_node_kind("button", move |_, _, _, _, _| {
            let counted = counted.clone();
            div()
                .w(units(150.))
                .p(units(10.))
                .child(
                    div()
                        .id("go")
                        .h(units(30.))
                        .nodrag()
                        .on_click(move |_, _, _| counted.set(counted.get() + 1)),
                )
                .into_any_element()
        })
    });
    flow.update(cx, |f, cx| {
        f.update_node("b", |n| n.kind = "button".into(), cx);
    });
    draw(cx);
    draw(cx);
    let at = center(&flow, cx, "b");
    tap(cx, at);
    assert_eq!(clicks.get(), 1);
    flow.read_with(cx, |f, _| assert!(f.selected_nodes().is_empty()));
}

#[gpui::test]
fn navigating_a_finger_only_pans(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    flow.update(cx, |f, cx| f.set_pointer_mode(PointerMode::Navigate, cx));
    let at = center(&flow, cx, "b");
    swipe(cx, at, offset(at, 60., 40.));
    flow.read_with(cx, |f, _| {
        assert_eq!(f.node("b").unwrap().position, xy(100., 250.));
        let v = f.viewport();
        assert!((v.x - 60.).abs() < 0.5 && (v.y - 40.).abs() < 0.5, "{v:?}");
    });
}

#[gpui::test]
fn a_finger_takes_an_edge_from_further_than_the_mouse(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    let (s, t) = flow.read_with(cx, |f, _| {
        (
            f.handle_point("a", HandleKind::Source, None).unwrap(),
            f.handle_point("b", HandleKind::Target, None).unwrap(),
        )
    });
    let near = offset(screen(&flow, cx, (s + t) * 0.5), 18., 0.);
    // The mouse, 18 points off, misses it.
    cx.simulate_click(near, Modifiers::none());
    draw(cx);
    assert!(flow.read_with(cx, |f, _| f.selected_edges()).is_empty());
    tap(cx, near);
    assert_eq!(flow.read_with(cx, |f, _| f.selected_edges()), ["a-b"]);
}

#[gpui::test]
fn a_hosts_pinch_zooms_about_the_fingers(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    let at = screen(&flow, cx, xy(300., 200.));
    flow.update(cx, |f, cx| f.zoom_by(1.5, at, cx));
    flow.read_with(cx, |f, _| {
        assert_eq!(f.viewport().zoom, 1.5);
        let under = f.screen_to_flow(at);
        assert!((under.x - 300.).abs() < 0.01 && (under.y - 200.).abs() < 0.01);
    });
    // Or as GPUI's own pinch event, dispatched into the window.
    cx.simulate_event(gpui::PinchEvent {
        position: at,
        delta: -0.25,
        ..Default::default()
    });
    assert!((flow.read_with(cx, |f, _| f.viewport().zoom) - 1.125).abs() < 1e-5);
}
