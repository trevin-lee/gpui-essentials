//! A flow inside a host app, with things inside its nodes that take presses, scrolling, the
//! focus and keys themselves.

mod common;

use std::cell::Cell;
use std::rc::Rc;

use common::*;
use gpui_flow::*;
use gpui_kit::{
    FocusHandle, InteractiveElement as _, IntoElement as _, KeyBinding, Modifiers, MouseButton,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, TestAppContext, actions, div,
    px,
};

actions!(host, [HostEscape]);

/// A node kind with a focusable field (like a text input: it takes the focus when pressed and
/// doesn't stop the press), a `nodrag` button, and a `nowheel` scroll area.
fn with_inputs(field: FocusHandle, clicks: Rc<Cell<usize>>) -> impl FnOnce(Flow) -> Flow {
    move |flow| {
        with_three(flow).with_node_kind("form", move |node, ctx, _, _, _| {
            let field = field.clone();
            let focus = field.clone();
            let clicks = clicks.clone();
            div()
                .w(units(200.))
                .flex()
                .flex_col()
                .gap(units(10.))
                .p(units(10.))
                .child(
                    div()
                        .id("field")
                        .h(units(20.))
                        .track_focus(&field)
                        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                            focus.focus(window, cx)
                        }),
                )
                .child(
                    div()
                        .id("button")
                        .h(units(20.))
                        .nodrag()
                        .on_click(move |_, _, _| clicks.set(clicks.get() + 1)),
                )
                .child(
                    div()
                        .id("list")
                        .h(units(20.))
                        .nowheel()
                        .overflow_y_scroll()
                        .child(div().h(px(200.))),
                )
                .child(ctx.handle(HandleKind::Target, Side::Top))
                .child(node.label_text())
                .into_any_element()
        })
    }
}

/// Opens the flow with node `b` made a form, and says where its three parts are on screen.
fn form(
    cx: &mut TestAppContext,
) -> (
    gpui_kit::Entity<Flow>,
    Events,
    &mut gpui_kit::VisualTestContext,
    FocusHandle,
    Rc<Cell<usize>>,
) {
    let field = cx.update(|cx| cx.focus_handle());
    let clicks = Rc::new(Cell::new(0));
    let (flow, events, cx) = open(cx, with_inputs(field.clone(), clicks.clone()));
    flow.update(cx, |f, cx| {
        f.update_node("b", |n| n.kind = "form".into(), cx);
    });
    draw(cx);
    draw(cx);
    (flow, events, cx, field, clicks)
}

/// The middle of row `i` (field, button, list) of the form.
fn row(
    flow: &gpui_kit::Entity<Flow>,
    cx: &gpui_kit::VisualTestContext,
    i: usize,
) -> gpui_kit::Point<gpui_kit::Pixels> {
    let b = flow.read_with(cx, |f, _| f.node_rect("b").unwrap());
    screen(flow, cx, xy(b.x + 100., b.y + 20. + 30. * i as f32))
}

#[gpui_kit::test]
fn pressing_a_field_in_a_node_leaves_the_focus_with_it(cx: &mut TestAppContext) {
    let (flow, _, cx, field, _) = form(cx);
    let at = row(&flow, cx, 0);
    // Pressing and dragging in it, as selecting its text would.
    drag(cx, at, offset(at, 40., 0.), Modifiers::none());
    cx.update(|window, _| assert!(field.is_focused(window)));
    flow.read_with(cx, |f, _| {
        assert_eq!(f.node("b").unwrap().position, xy(100., 250.));
        assert!(f.selected_nodes().is_empty());
    });
}

#[gpui_kit::test]
fn the_flows_keys_dont_reach_into_a_focused_field(cx: &mut TestAppContext) {
    let (flow, _, cx, field, _) = form(cx);
    cx.simulate_click(row(&flow, cx, 0), Modifiers::none());
    draw(cx);
    cx.update(|window, _| assert!(field.is_focused(window)));
    flow.update(cx, |f, cx| f.select_all(cx));
    let before = flow.read_with(cx, |f, _| f.viewport());
    // "!" doesn't fit the view; backspace doesn't delete; arrows don't move; cmd-= doesn't zoom.
    cx.simulate_keystrokes("shift-1 backspace up cmd-= escape");
    draw(cx);
    flow.read_with(cx, |f, _| {
        assert_eq!(f.viewport(), before);
        assert_eq!(f.nodes().len(), 3);
        assert_eq!(f.node("a").unwrap().position, xy(100., 100.));
        assert_eq!(f.selected_nodes().len(), 3);
    });
    // Back on the flow, they work again.
    cx.simulate_click(screen(&flow, cx, xy(600., 60.)), Modifiers::none());
    draw(cx);
    flow.update(cx, |f, cx| f.select_all(cx));
    cx.simulate_keystrokes("backspace");
    draw(cx);
    assert!(flow.read_with(cx, |f, _| f.nodes().is_empty()));
}

#[gpui_kit::test]
fn a_nodrag_button_neither_selects_nor_drags_its_node(cx: &mut TestAppContext) {
    let (flow, _, cx, _, clicks) = form(cx);
    let at = row(&flow, cx, 1);
    cx.simulate_click(at, Modifiers::none());
    draw(cx);
    assert_eq!(clicks.get(), 1);
    drag(cx, at, offset(at, 60., 40.), Modifiers::none());
    flow.read_with(cx, |f, _| {
        assert_eq!(f.node("b").unwrap().position, xy(100., 250.));
        assert!(f.selected_nodes().is_empty());
    });
}

#[gpui_kit::test]
fn scrolling_a_nowheel_list_in_a_node_doesnt_zoom(cx: &mut TestAppContext) {
    let (flow, _, cx, _, _) = form(cx);
    scroll(cx, row(&flow, cx, 2), 0., 100., Modifiers::none());
    assert_eq!(flow.read_with(cx, |f, _| f.viewport()), Viewport::default());
    // Elsewhere in the node, scrolling zooms as on the pane.
    let b = flow.read_with(cx, |f, _| f.node_rect("b").unwrap());
    scroll(
        cx,
        screen(&flow, cx, xy(b.x + 5., b.bottom() - 5.)),
        0.,
        100.,
        Modifiers::none(),
    );
    assert!(flow.read_with(cx, |f, _| f.viewport().zoom) > 1.);
}

#[gpui_kit::test]
fn keys_the_flow_has_nothing_to_do_for_go_on_to_the_host(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    let escaped = Rc::new(Cell::new(0));
    let seen = escaped.clone();
    cx.update(|_, cx| {
        // The host's keys first, then the flow's, as `bind_keys` asks.
        cx.bind_keys([KeyBinding::new("escape", HostEscape, None)]);
        gpui_flow::bind_keys(cx);
        cx.on_action(move |_: &HostEscape, _| seen.set(seen.get() + 1));
    });
    // With something selected, escape is the flow's.
    flow.update(cx, |f, cx| f.select_all(cx));
    cx.simulate_keystrokes("escape");
    assert_eq!(escaped.get(), 0);
    assert!(flow.read_with(cx, |f, _| f.selected_nodes()).is_empty());
    // With nothing to drop, it goes on.
    cx.simulate_keystrokes("escape");
    assert_eq!(escaped.get(), 1);
}
