//! A host's toolbar beside the selected node, as React Flow's `NodeToolbar`.

mod common;

use common::*;
use gpui::{
    IntoElement as _, Modifiers, MouseButton, ParentElement as _, Styled as _, TestAppContext, div,
    px,
};
use gpui_flow::*;

fn with_toolbar(place: ToolbarPlace) -> impl FnOnce(Flow) -> Flow {
    move |f| {
        with_three(f).with_node_toolbar(place, |node, _, _, _| {
            // None for c: a node can have no toolbar.
            (node.id != "c").then(|| div().w(px(60.)).h(px(20.)).child("Edit").into_any_element())
        })
    }
}

fn click(cx: &mut gpui::VisualTestContext, at: gpui::Point<gpui::Pixels>, modifiers: Modifiers) {
    cx.simulate_mouse_down(at, MouseButton::Left, modifiers);
    cx.simulate_mouse_up(at, MouseButton::Left, modifiers);
    draw(cx);
}

#[gpui::test]
fn the_toolbar_shows_over_the_one_selected_node(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_toolbar(ToolbarPlace::default()));
    assert!(
        cx.debug_bounds("flow-node-toolbar").is_none(),
        "nothing selected"
    );
    click(cx, center(&flow, cx, "b"), Modifiers::none());
    let bar = cx.debug_bounds("flow-node-toolbar").expect("a toolbar");
    let node = flow.read_with(cx, |f, _| f.node_rect("b").unwrap());
    let top = flow.read_with(cx, |f, _| {
        f.flow_to_screen(xy(node.x + node.w / 2., node.y))
    });
    // Above it, 10 off, centred on it.
    assert!(
        (f32::from(top.y - bar.bottom()) - 10.).abs() < 1.,
        "{bar:?} {top:?}"
    );
    assert!(
        (f32::from(bar.center().x - top.x)).abs() < 1.,
        "{bar:?} {top:?}"
    );
    // A press on it is its own: the node stays selected.
    click(cx, bar.center(), Modifiers::none());
    assert!(flow.read_with(cx, |f, _| f.node("b").unwrap().selected));
    // Two selected: none; a node whose toolbar is none: none.
    click(cx, center(&flow, cx, "a"), Modifiers::command());
    assert!(
        cx.debug_bounds("flow-node-toolbar").is_none(),
        "two selected"
    );
    click(cx, center(&flow, cx, "c"), Modifiers::none());
    assert!(cx.debug_bounds("flow-node-toolbar").is_none(), "c has none");
}

#[gpui::test]
fn the_toolbar_can_sit_on_any_side(cx: &mut TestAppContext) {
    let place = ToolbarPlace {
        side: Side::Right,
        align: ToolbarAlign::Start,
        offset: 6.,
    };
    let (flow, _, cx) = open(cx, with_toolbar(place));
    click(cx, center(&flow, cx, "b"), Modifiers::none());
    let bar = cx.debug_bounds("flow-node-toolbar").expect("a toolbar");
    let node = flow.read_with(cx, |f, _| f.node_rect("b").unwrap());
    let corner = flow.read_with(cx, |f, _| f.flow_to_screen(xy(node.x + node.w, node.y)));
    assert!(
        (f32::from(bar.left() - corner.x) - 6.).abs() < 1.,
        "{bar:?} {corner:?}"
    );
    assert!(
        (f32::from(bar.top() - corner.y)).abs() < 1.,
        "{bar:?} {corner:?}"
    );
}
