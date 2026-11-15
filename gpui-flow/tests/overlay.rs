//! A native view laid over the canvas (a PencilKit canvas on iPad): it follows the canvas's
//! transform, and takes pointer input from it when the person draws.

mod common;

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use common::*;
use gpui_flow::*;
use gpui_kit::{Modifiers, MouseButton, TestAppContext, px, size};

type Seen = Rc<RefCell<Vec<CanvasTransform>>>;

fn listening(seen: &Seen) -> impl FnOnce(Flow) -> Flow + 'static {
    let seen = seen.clone();
    move |flow| with_three(flow).with_transform_listener(move |t, _, _| seen.borrow_mut().push(t))
}

#[gpui_kit::test]
fn the_listener_hears_each_change_of_the_transform_and_nothing_else(cx: &mut TestAppContext) {
    let seen: Seen = Rc::default();
    let (flow, _, cx) = open(cx, listening(&seen));
    // The first frame, once.
    assert_eq!(seen.borrow().len(), 1);
    let first = seen.borrow()[0];
    assert_eq!(first.viewport, Viewport::default());
    assert!(first.pane.size.width > px(0.));
    for _ in 0..3 {
        draw(cx);
    }
    assert_eq!(seen.borrow().len(), 1, "nothing changed");

    // A pan.
    let at = screen(&flow, cx, xy(600., 60.));
    drag(cx, at, offset(at, -80., 45.), Modifiers::none());
    let last = *seen.borrow().last().unwrap();
    assert_eq!(last.viewport, Viewport::new(-80., 45., 1.));
    // It agrees with the flow about where flow points are.
    flow.read_with(cx, |f, _| {
        let p = xy(123., 456.);
        assert_eq!(last.flow_to_window(p), f.flow_to_screen(p));
        assert_eq!(last.window_to_flow(f.flow_to_screen(p)), p);
        assert_eq!(f.canvas_transform(), last);
        assert_eq!(last.translation(), f.flow_to_screen(xy(0., 0.)));
    });

    // A zoom.
    let n = seen.borrow().len();
    scroll(cx, at, 0., 100., Modifiers::none());
    assert_eq!(seen.borrow().len(), n + 1);
    assert!(seen.borrow().last().unwrap().scale() > 1.);

    // The pane changing size in the window.
    let n = seen.borrow().len();
    cx.simulate_resize(size(px(700.), px(500.)));
    draw(cx);
    assert!(seen.borrow().len() > n);
    assert_eq!(
        seen.borrow().last().unwrap().pane.size,
        size(px(700.), px(500.))
    );
}

#[gpui_kit::test]
fn each_step_of_an_animated_zoom_is_heard(cx: &mut TestAppContext) {
    let seen: Seen = Rc::default();
    let listen = listening(&seen);
    let (flow, _, cx) = open(cx, move |f| {
        listen(f).with_options(|o| o.transition = Duration::from_millis(200))
    });
    flow.update(cx, |f, cx| f.zoom_to(2., cx));
    for _ in 0..4 {
        std::thread::sleep(Duration::from_millis(60));
        draw(cx);
    }
    let zooms: Vec<f32> = seen.borrow().iter().map(|t| t.scale()).collect();
    // Every frame of the transition, rising to 2.
    assert!(zooms.len() >= 4, "{zooms:?}");
    assert!(zooms.windows(2).all(|w| w[1] > w[0]), "{zooms:?}");
    assert_eq!(*zooms.last().unwrap(), 2.);
}

#[gpui_kit::test]
fn navigating_only_moves_about(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, with_three);
    flow.update(cx, |f, cx| f.set_pointer_mode(PointerMode::Navigate, cx));
    // A drag that starts on a node pans.
    let at = center(&flow, cx, "b");
    drag(cx, at, offset(at, 40., 30.), Modifiers::none());
    flow.read_with(cx, |f, _| {
        assert_eq!(f.node("b").unwrap().position, xy(100., 250.));
        assert_eq!(f.viewport(), Viewport::new(40., 30., 1.));
        assert!(f.selected_nodes().is_empty());
    });
    // So does one from a handle: nothing's connected.
    let from = handle(&flow, cx, "a", HandleKind::Source);
    drag(cx, from, offset(from, 20., 0.), Modifiers::none());
    assert_eq!(flow.read_with(cx, |f, _| f.edges().len()), 1);
    assert_eq!(
        flow.read_with(cx, |f, _| f.viewport()),
        Viewport::new(60., 30., 1.)
    );
    // A tap on a node or an edge selects nothing; a double tap still zooms.
    cx.simulate_click(center(&flow, cx, "c"), Modifiers::none());
    draw(cx);
    assert!(flow.read_with(cx, |f, _| f.selected_nodes()).is_empty());
    assert!(!events.borrow().iter().any(|e| matches!(
        e,
        FlowEvent::NodeClick(_) | FlowEvent::EdgeClick(_) | FlowEvent::NodeDragStart(_)
    )));
    let c = center(&flow, cx, "c");
    cx.simulate_event(gpui_kit::MouseDownEvent {
        button: MouseButton::Left,
        position: c,
        modifiers: Modifiers::none(),
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_mouse_up(c, MouseButton::Left, Modifiers::none());
    draw(cx);
    assert_eq!(flow.read_with(cx, |f, _| f.viewport().zoom), 2.);
}

#[gpui_kit::test]
fn off_leaves_every_press_and_scroll_alone(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, with_three);
    flow.update(cx, |f, cx| f.set_pointer_mode(PointerMode::Off, cx));
    let at = center(&flow, cx, "b");
    drag(cx, at, offset(at, 40., 30.), Modifiers::none());
    let pane = screen(&flow, cx, xy(600., 60.));
    drag(cx, pane, offset(pane, 40., 30.), Modifiers::none());
    scroll(cx, pane, 0., 100., Modifiers::none());
    cx.simulate_event(gpui_kit::PinchEvent {
        position: pane,
        delta: 0.5,
        ..Default::default()
    });
    draw(cx);
    flow.read_with(cx, |f, _| {
        assert_eq!(f.viewport(), Viewport::default());
        assert_eq!(f.node("b").unwrap().position, xy(100., 250.));
        assert!(f.selected_nodes().is_empty());
    });
    assert!(events.borrow().is_empty(), "{:?}", events.borrow());
    // The host still moves it.
    flow.update(cx, |f, cx| f.set_viewport(Viewport::new(5., 5., 1.5), cx));
    assert_eq!(
        flow.read_with(cx, |f, _| f.viewport()),
        Viewport::new(5., 5., 1.5)
    );
}

#[gpui_kit::test]
fn changing_the_mode_mid_drag_puts_the_drag_back(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, with_three);
    let at = center(&flow, cx, "b");
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(offset(at, 30., 0.), MouseButton::Left, Modifiers::none());
    draw(cx);
    flow.update(cx, |f, cx| f.set_pointer_mode(PointerMode::Navigate, cx));
    cx.simulate_mouse_move(offset(at, 60., 0.), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(offset(at, 60., 0.), MouseButton::Left, Modifiers::none());
    draw(cx);
    assert_eq!(position(&flow, cx, "b"), xy(100., 250.));
    assert!(
        events
            .borrow()
            .iter()
            .any(|e| matches!(e, FlowEvent::NodeDragStop(ids) if ids == &["b"]))
    );
}
