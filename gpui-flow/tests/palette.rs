//! Palette drops on gpui-dnd: a host's palette item dragged onto the pane lands where the
//! pointer is in the flow, whatever the zoom, and on the grid when snapping is on; the flow
//! draws where it would land as it's dragged; one held near the pane's edge pans it; one
//! dropped off the pane springs back, adding nothing; and by keyboard, Tab and Space pick an
//! item up, the arrows move it onto the pane and a grid step at a time, each step said in the
//! flow's words, and Space drops it.

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::draw;
use gpui::{
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, Modifiers, MouseButton,
    ParentElement as _, Pixels, Point, Render, Styled as _, TestAppContext, VisualTestContext,
    Window, div, point, px, size,
};
use gpui_dnd::{DndProvider, DragMonitor, dnd};
use gpui_flow::*;

/// How wide the palette column is: the pane starts this far in.
const PALETTE: f32 = 120.;

struct Host {
    flow: Entity<Flow>,
}

fn note() -> FlowPaletteItem {
    FlowPaletteItem::new(
        "Note",
        Node::new("", xy(0., 0.)).kind("note").size(100., 40.),
    )
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        DndProvider::new(
            div()
                .size_full()
                .flex()
                .child(
                    div().w(px(PALETTE)).h_full().p(px(10.)).child(
                        dnd("note")
                            .debug_selector(|| "note".into())
                            .label("Note")
                            .w(px(100.))
                            .h(px(30.))
                            .source(note().source()),
                    ),
                )
                .child(div().flex_1().h_full().child(self.flow.clone())),
        )
    }
}

type Dropped = Rc<RefCell<Vec<(FlowPaletteItem, XY)>>>;

fn open(
    cx: &mut TestAppContext,
    build: impl FnOnce(Flow) -> Flow + 'static,
) -> (Entity<Flow>, Dropped, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_core::init(cx);
        gpui_flow::init(cx);
    });
    let (host, vcx) = cx.add_window_view(move |_, cx| Host {
        flow: cx.new(|cx| build(Flow::new(cx))),
    });
    vcx.simulate_resize(size(px(800.), px(600.)));
    let flow = host.read_with(vcx, |h, _| h.flow.clone());
    let dropped: Dropped = Rc::default();
    vcx.update(|_, cx| {
        let seen = dropped.clone();
        cx.subscribe(&flow, move |_, event: &FlowEvent, _| {
            if let FlowEvent::NodeDropped { item, position } = event {
                seen.borrow_mut().push((item.clone(), *position));
            }
        })
        .detach();
    });
    draw(vcx);
    draw(vcx);
    (flow, dropped, vcx)
}

/// As `open`, inside gpui-core's `Root`, which moves the focus with Tab, as in an app.
fn open_in_root(
    cx: &mut TestAppContext,
    build: impl FnOnce(Flow) -> Flow + 'static,
) -> (Entity<Flow>, Dropped, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_core::init(cx);
        gpui_flow::init(cx);
    });
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let flow = cx.new(|cx| build(Flow::new(cx)));
        *m.borrow_mut() = Some(flow.clone());
        gpui_core::Root::new(cx.new(|_| Host { flow }), window, cx)
    });
    vcx.simulate_resize(size(px(800.), px(600.)));
    vcx.update(|window, _| window.activate_window());
    let flow = made.borrow().clone().expect("made");
    let dropped: Dropped = Rc::default();
    vcx.update(|_, cx| {
        let seen = dropped.clone();
        cx.subscribe(&flow, move |_, event: &FlowEvent, _| {
            if let FlowEvent::NodeDropped { item, position } = event {
                seen.borrow_mut().push((item.clone(), *position));
            }
        })
        .detach();
    });
    draw(vcx);
    draw(vcx);
    (flow, dropped, vcx)
}

fn keys(cx: &mut VisualTestContext, keys: &str) {
    cx.simulate_keystrokes(keys);
    draw(cx);
}

fn said(cx: &mut VisualTestContext) -> Option<String> {
    cx.update(|window, cx| gpui_dnd::announcement(window, cx).map(|s| s.to_string()))
}

/// Presses on the palette's note and drags it to `to`, without letting go.
fn hold(cx: &mut VisualTestContext, to: Point<Pixels>) {
    let from = cx.debug_bounds("note").expect("the palette").center();
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
    draw(cx);
    cx.simulate_mouse_move(
        from + point(px(12.), px(0.)),
        MouseButton::Left,
        Modifiers::none(),
    );
    draw(cx);
    cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
    draw(cx);
}

fn release(cx: &mut VisualTestContext, at: Point<Pixels>) {
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    draw(cx);
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}

#[gpui::test]
fn a_palette_item_lands_under_the_pointer_in_flow_coordinates(cx: &mut TestAppContext) {
    let (flow, dropped, cx) = open(cx, |f| f.with_viewport(Viewport::new(0., 0., 1.)));
    let at = point(px(420.), px(320.));
    hold(cx, at);
    // Its middle under the pointer: the pointer is at (300, 320) in the flow.
    let landing = xy(300. - 50., 320. - 20.);
    let ghost = cx
        .debug_bounds("flow-palette-ghost")
        .expect("drawn where it would land");
    let corner = flow.read_with(cx, |f, _| f.flow_to_screen(landing));
    assert_eq!(ghost.origin, corner);
    assert_eq!(ghost.size, size(px(100.), px(40.)));
    release(cx, at);
    let dropped = dropped.borrow();
    assert_eq!(dropped.len(), 1);
    assert_eq!(dropped[0].0, note());
    assert_eq!(dropped[0].1, landing);
    assert!(
        cx.debug_bounds("flow-palette-ghost").is_none(),
        "gone once it's dropped"
    );
    assert!(
        flow.read_with(cx, |f, _| f.nodes().is_empty()),
        "the host adds the node, not the flow"
    );
}

#[gpui::test]
fn zoomed_in_and_panned_it_still_lands_under_the_pointer(cx: &mut TestAppContext) {
    let (flow, dropped, cx) = open(cx, |f| f.with_viewport(Viewport::new(10., 20., 2.)));
    let at = point(px(420.), px(320.));
    hold(cx, at);
    // The pointer is (300 - 10, 320 - 20) / 2 into the flow.
    let landing = xy(145. - 50., 150. - 20.);
    let ghost = cx.debug_bounds("flow-palette-ghost").expect("drawn");
    assert_eq!(
        ghost.size,
        size(px(200.), px(80.)),
        "drawn at the canvas's zoom"
    );
    assert_eq!(
        ghost.origin,
        flow.read_with(cx, |f, _| f.flow_to_screen(landing))
    );
    release(cx, at);
    let (_, position) = dropped.borrow()[0].clone();
    assert!(
        close(position.x, landing.x) && close(position.y, landing.y),
        "{position:?}"
    );

    // Zoomed out it's the same, at half the size.
    flow.update(cx, |f, cx| f.set_viewport(Viewport::new(0., 0., 0.5), cx));
    draw(cx);
    hold(cx, at);
    let landing = xy(600. - 50., 640. - 20.);
    let ghost = cx.debug_bounds("flow-palette-ghost").expect("drawn");
    assert_eq!(ghost.size, size(px(50.), px(20.)));
    release(cx, at);
    assert_eq!(dropped.borrow()[1].1, landing);
}

#[gpui::test]
fn with_snapping_on_it_lands_on_the_grid(cx: &mut TestAppContext) {
    let (_, dropped, cx) = open(cx, |f| {
        f.with_viewport(Viewport::new(0., 0., 1.))
            .with_options(|o| o.snap_to_grid = true)
    });
    let at = point(px(427.), px(318.));
    hold(cx, at);
    release(cx, at);
    // (307 - 50, 318 - 20) to the nearest 15.
    assert_eq!(dropped.borrow()[0].1, xy(255., 300.));
}

#[gpui::test]
fn dropped_off_the_pane_it_springs_back_and_adds_nothing(cx: &mut TestAppContext) {
    let (_, dropped, cx) = open(cx, |f| f);
    // Over the pane first, then back over the palette, below the item.
    hold(cx, point(px(420.), px(320.)));
    assert!(cx.debug_bounds("flow-palette-ghost").is_some());
    let off = point(px(60.), px(400.));
    cx.simulate_mouse_move(off, MouseButton::Left, Modifiers::none());
    draw(cx);
    assert!(
        cx.debug_bounds("flow-palette-ghost").is_none(),
        "the ghost goes as the pointer leaves the pane"
    );
    release(cx, off);
    assert!(dropped.borrow().is_empty());
    assert!(
        cx.update(|_, cx| DragMonitor::of(cx).is_returning()),
        "the preview slides back to the palette"
    );
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(300));
    draw(cx);
    assert!(!cx.update(|_, cx| DragMonitor::of(cx).is_dragging()));
    assert!(dropped.borrow().is_empty());
}

#[gpui::test]
fn a_flow_that_takes_no_pointer_takes_no_drops(cx: &mut TestAppContext) {
    let (_, dropped, cx) = open(cx, |f| {
        f.with_options(|o| o.pointer_mode = PointerMode::Off)
    });
    let at = point(px(420.), px(320.));
    hold(cx, at);
    release(cx, at);
    assert!(dropped.borrow().is_empty());
}

#[gpui::test]
fn held_near_the_panes_edge_it_pans_the_canvas(cx: &mut TestAppContext) {
    let (flow, dropped, cx) = open(cx, |f| f.with_viewport(Viewport::new(0., 0., 1.)));
    // Near the pane's right edge, halfway down.
    let at = point(px(795.), px(300.));
    hold(cx, at);
    for _ in 0..10 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(16));
        cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
    }
    let v = flow.read_with(cx, |f, _| f.viewport());
    assert!(v.x < -20., "panned toward what's right of the pane: {v:?}");
    assert_eq!(v.y, 0., "not up or down");
    release(cx, at);
    let v = flow.read_with(cx, |f, _| f.viewport());
    let (_, position) = dropped.borrow()[0].clone();
    let landing = xy(795. - PALETTE - v.x - 50., 300. - 20.);
    assert!(
        close(position.x, landing.x) && close(position.y, landing.y),
        "under the pointer in the panned flow: {position:?}, {landing:?}"
    );
}

#[gpui::test]
fn by_keyboard_the_arrows_step_it_over_the_pane_and_say_where(cx: &mut TestAppContext) {
    let (_, dropped, cx) = open_in_root(cx, |f| f.with_viewport(Viewport::new(0., 0., 1.)));
    keys(cx, "tab space");
    assert_eq!(
        said(cx).as_deref(),
        Some("Picked up Note. Arrow keys move it, Space drops it, Escape cancels.")
    );
    let pane = cx.debug_bounds("flow-palette-ghost");
    assert!(pane.is_none(), "not over the pane yet");

    // Onto the pane, at its middle: (460, 300) in the window, (340, 300) in the flow.
    keys(cx, "right");
    assert_eq!(said(cx).as_deref(), Some("Note. Position, x: 290, y: 280"));
    assert!(
        cx.debug_bounds("flow-palette-ghost").is_some(),
        "drawn there"
    );

    // A grid step (15) at a time.
    keys(cx, "right");
    assert_eq!(said(cx).as_deref(), Some("Note. Position, x: 305, y: 280"));
    keys(cx, "down");
    assert_eq!(said(cx).as_deref(), Some("Note. Position, x: 305, y: 295"));
    keys(cx, "space");
    assert_eq!(
        said(cx).as_deref(),
        Some("Dropped Note. Position, x: 305, y: 295")
    );
    assert_eq!(dropped.borrow()[0].1, xy(305., 295.));
}

#[gpui::test]
fn zoomed_in_a_keyboard_step_is_still_a_grid_step_of_the_flow(cx: &mut TestAppContext) {
    let (_, dropped, cx) = open_in_root(cx, |f| f.with_viewport(Viewport::new(0., 0., 2.)));
    // The pane's middle is (170, 150) in the flow.
    keys(cx, "tab space right");
    assert_eq!(said(cx).as_deref(), Some("Note. Position, x: 120, y: 130"));
    keys(cx, "left up space");
    assert_eq!(dropped.borrow()[0].1, xy(105., 115.));
}

#[gpui::test]
fn a_keyboard_step_past_the_panes_edge_pans_it(cx: &mut TestAppContext) {
    let (flow, dropped, cx) = open_in_root(cx, |f| f.with_viewport(Viewport::new(0., 0., 1.)));
    keys(cx, "tab space right");
    // 300 px to the pane's bottom: twenty steps reach it, and the rest pan.
    for _ in 0..25 {
        keys(cx, "down");
    }
    let v = flow.read_with(cx, |f, _| f.viewport());
    assert!(v.y < 0., "panned up to show what's below: {v:?}");
    keys(cx, "space");
    let (_, position) = dropped.borrow()[0].clone();
    assert_eq!(position, xy(290., 280. + 25. * 15.));
}

#[gpui::test]
fn escape_puts_it_back_and_adds_nothing(cx: &mut TestAppContext) {
    let (_, dropped, cx) = open_in_root(cx, |f| f);
    keys(cx, "tab space right escape");
    // Said once the card has slid back.
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(250));
    draw(cx);
    assert!(dropped.borrow().is_empty());
    assert_eq!(
        said(cx).as_deref(),
        Some("Cancelled. Note is back where it was."),
        "the provider's own sentence"
    );
    assert!(cx.debug_bounds("flow-palette-ghost").is_none());
}
