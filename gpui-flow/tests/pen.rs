//! Drawing on the Mac: a pen with the mouse or trackpad, and an eraser.

mod common;

use common::*;
use gpui::{Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext};
use gpui_flow::ink::{InkPoint, InkStroke, InkTool};
use gpui_flow::json::Op;
use gpui_flow::*;

fn edits(events: &Events) -> Vec<Vec<Op>> {
    events
        .borrow()
        .iter()
        .filter_map(|e| match e {
            FlowEvent::Edits(ops) => Some(ops.clone()),
            _ => None,
        })
        .collect()
}

fn inks(flow: &Entity<Flow>, cx: &VisualTestContext) -> Vec<Node> {
    flow.read_with(cx, |f, _| {
        f.nodes()
            .iter()
            .filter(|n| n.kind == "ink")
            .cloned()
            .collect()
    })
}

fn pen() -> Pen {
    Pen::new(InkTool::Ball, 0xd030_30ff, 3.)
}

/// A wavy line across the pane from `from`, in many small moves, as a hand draws one.
fn scribble(flow: &Entity<Flow>, cx: &mut VisualTestContext, from: XY, to: XY, lift: bool) {
    let at: Vec<_> = (0..=40)
        .map(|i| {
            let t = i as f32 / 40.;
            screen(
                flow,
                cx,
                xy(
                    from.x + (to.x - from.x) * t,
                    from.y + (to.y - from.y) * t + (t * 12.).sin() * 6.,
                ),
            )
        })
        .collect();
    cx.simulate_mouse_down(at[0], MouseButton::Left, Modifiers::none());
    for p in &at[1..] {
        cx.simulate_mouse_move(*p, MouseButton::Left, Modifiers::none());
    }
    draw(cx);
    if lift {
        cx.simulate_mouse_up(at[40], MouseButton::Left, Modifiers::none());
        draw(cx);
    }
}

#[gpui::test]
fn a_pen_draws_a_stroke_as_one_edit(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, with_three);
    flow.update(cx, |f, cx| f.set_tool(Tool::Pen(pen()), cx));
    scribble(&flow, cx, xy(500., 400.), xy(700., 420.), false);
    // Nothing's added until the button comes up.
    assert!(inks(&flow, cx).is_empty());
    let at = screen(&flow, cx, xy(700., 420.));
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    draw(cx);
    let ink = inks(&flow, cx);
    assert_eq!(ink.len(), 1);
    let stroke = InkStroke::of(&ink[0]).unwrap();
    assert_eq!(
        (stroke.tool, stroke.color, stroke.width),
        (InkTool::Ball, 0xd030_30ff, 3.)
    );
    // Smoothed and thinned, from where it began to where it ended, in the flow.
    let (first, last) = (stroke.points[0], *stroke.points.last().unwrap());
    assert!(stroke.points.len() >= 3 && stroke.points.len() < 41);
    assert!((first.x - 500.).abs() < 0.5 && (last.x - 700.).abs() < 0.5);
    assert!(last.t >= first.t);
    // Its box is where it was drawn.
    let b = stroke.bounds().unwrap();
    assert_eq!(ink[0].position, xy(b.x, b.y));
    assert_eq!(
        edits(&events),
        [vec![Op::AddNode {
            node: Box::new(ink[0].clone())
        }]]
    );
    // And it's one undo.
    flow.update(cx, |f, cx| f.undo(cx));
    draw(cx);
    assert!(inks(&flow, cx).is_empty());
}

#[gpui::test]
fn a_pen_draws_over_nodes_without_moving_them(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    flow.update(cx, |f, cx| f.set_tool(Tool::Pen(pen()), cx));
    let b = flow.read_with(cx, |f, _| f.node_rect("b").unwrap());
    scribble(
        &flow,
        cx,
        xy(b.x + 5., b.y + 10.),
        xy(b.right() + 60., b.y + 10.),
        true,
    );
    assert_eq!(position(&flow, cx, "b"), xy(100., 250.));
    assert_eq!(inks(&flow, cx).len(), 1);
    assert!(flow.read_with(cx, |f, _| f.selected_nodes()).is_empty());
}

#[gpui::test]
fn the_pen_is_put_down_with_escape(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, with_three);
    flow.update(cx, |f, cx| f.set_tool(Tool::Pen(pen()), cx));
    scribble(&flow, cx, xy(500., 400.), xy(700., 420.), false);
    cx.simulate_keystrokes("escape");
    let at = screen(&flow, cx, xy(700., 420.));
    cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
    draw(cx);
    assert!(inks(&flow, cx).is_empty());
    assert!(edits(&events).is_empty());
}

fn strokes(flow: Flow) -> Flow {
    let line = |y: f32| {
        InkStroke::new(
            InkTool::Ball,
            0xff,
            3.,
            (0..=20)
                .map(|i| InkPoint::at(100. + i as f32 * 10., y, i as f32 / 50., 3.))
                .collect(),
        )
    };
    let mut kept = Node::ink("kept", &line(300.));
    kept.deletable = Some(false);
    flow.with_nodes(vec![
        Node::ink("top", &line(100.)),
        Node::ink("middle", &line(160.)),
        kept,
        Node::ink("far", &line(500.)),
        Node::new("box", xy(180., 200.)).label("Box").size(60., 30.),
    ])
}

#[gpui::test]
fn the_eraser_rubs_out_what_it_touches_in_one_edit(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, strokes);
    flow.update(cx, |f, cx| f.set_tool(Tool::Eraser, cx));
    // Down across top, middle, the box, and the stroke that may not be deleted.
    let (a, b) = (
        screen(&flow, cx, xy(200., 80.)),
        screen(&flow, cx, xy(200., 320.)),
    );
    drag(cx, a, b, Modifiers::none());
    let left: Vec<String> = flow.read_with(cx, |f, _| {
        f.nodes().iter().map(|n| n.id.to_string()).collect()
    });
    assert_eq!(left, ["kept", "far", "box"]);
    let batches = edits(&events);
    assert_eq!(
        batches,
        [vec![
            Op::RemoveNode {
                id: "middle".into()
            },
            Op::RemoveNode { id: "top".into() },
        ]]
    );
    flow.update(cx, |f, cx| f.undo(cx));
    draw(cx);
    assert_eq!(inks(&flow, cx).len(), 4);
}

#[gpui::test]
fn escape_puts_back_what_the_eraser_took(cx: &mut TestAppContext) {
    let (flow, events, cx) = open(cx, strokes);
    flow.update(cx, |f, cx| f.set_tool(Tool::Eraser, cx));
    let (a, b) = (
        screen(&flow, cx, xy(200., 80.)),
        screen(&flow, cx, xy(200., 180.)),
    );
    cx.simulate_mouse_down(a, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(b, MouseButton::Left, Modifiers::none());
    draw(cx);
    assert_eq!(inks(&flow, cx).len(), 2, "rubbed out as it goes");
    cx.simulate_keystrokes("escape");
    cx.simulate_mouse_up(b, MouseButton::Left, Modifiers::none());
    draw(cx);
    let ids: Vec<String> = inks(&flow, cx).iter().map(|n| n.id.to_string()).collect();
    assert_eq!(ids, ["top", "middle", "kept", "far"]);
    assert!(edits(&events).is_empty());
}

#[gpui::test]
fn back_to_select_presses_select_again(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    flow.update(cx, |f, cx| {
        f.set_tool(Tool::Pen(pen()), cx);
        f.set_tool(Tool::Select, cx);
    });
    let at = center(&flow, cx, "b");
    drag(cx, at, offset(at, 30., 0.), Modifiers::none());
    assert_eq!(position(&flow, cx, "b"), xy(130., 250.));
    assert!(inks(&flow, cx).is_empty());
}

#[gpui::test]
fn a_palette_pen_stores_its_colour(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, |f| f);
    let colors = flow.read_with(cx, |_, cx| FlowColors::from_theme(cx));
    assert_eq!(Pen::palette(None, 2., &colors).color, 0x0000_00ff);
    let red = Pen::palette(Some("red"), 2., &colors).color;
    assert_eq!(red & 0xff, 0xff);
    assert!(red >> 24 > (red >> 16) & 0xff, "{red:08x} is red");
}
