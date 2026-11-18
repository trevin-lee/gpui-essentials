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

fn with_edit_toolbar(f: Flow) -> Flow {
    with_three(f).with_options(|o| o.edit_toolbar = true)
}

fn press(cx: &mut gpui::VisualTestContext, id: &'static str) {
    let at = cx
        .debug_bounds(id)
        .unwrap_or_else(|| panic!("{id} isn't drawn"))
        .center();
    click(cx, at, Modifiers::none());
}

#[gpui::test]
fn the_edit_toolbar_copies_pastes_duplicates_and_deletes_with_a_finger(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_edit_toolbar);
    assert!(
        cx.debug_bounds("flow-edit-toolbar").is_none(),
        "nothing selected"
    );
    click(cx, center(&flow, cx, "b"), Modifiers::none());
    // Above the node, each button finger-sized.
    let bar = cx
        .debug_bounds("flow-edit-toolbar")
        .expect("the edit toolbar");
    let node = flow.read_with(cx, |f, _| f.node_rect("b").unwrap());
    let top = flow.read_with(cx, |f, _| f.flow_to_screen(xy(node.x, node.y)));
    assert!(bar.bottom() <= top.y, "{bar:?} {top:?}");
    for id in ["flow-copy", "flow-paste", "flow-duplicate", "flow-delete"] {
        let b = cx.debug_bounds(id).unwrap_or_else(|| panic!("{id}"));
        assert!(
            f32::from(b.size.width) >= 44. && f32::from(b.size.height) >= 44.,
            "{id} {b:?}"
        );
    }

    // Duplicate: a copy a step along, selected, the toolbar on it now.
    press(cx, "flow-duplicate");
    let copy = flow.read_with(cx, |f, _| {
        assert_eq!(f.nodes().len(), 4);
        assert_eq!(f.announcement(), "Duplicated 1 node");
        f.selected_nodes()[0].clone()
    });
    assert_ne!(copy.as_ref(), "b");
    assert_eq!(position(&flow, cx, &copy), xy(120., 270.));
    assert!(cx.debug_bounds("flow-edit-toolbar").is_some());

    // Copy, then paste: another, a step on from b.
    click(cx, center(&flow, cx, "b"), Modifiers::none());
    press(cx, "flow-copy");
    press(cx, "flow-paste");
    flow.read_with(cx, |f, _| {
        assert_eq!(f.nodes().len(), 5);
        assert_eq!(f.announcement(), "Pasted 1 node");
    });

    // Delete: the selected one goes, said as the key says it.
    press(cx, "flow-delete");
    flow.read_with(cx, |f, _| {
        assert_eq!(f.nodes().len(), 4);
        assert_eq!(f.announcement(), "Deleted 1 node");
    });
}

#[gpui::test]
fn the_edit_toolbar_follows_a_hosts_toolbar_and_leaves_out_delete_where_it_cant(
    cx: &mut TestAppContext,
) {
    let (flow, _, cx) = open(cx, |f| {
        with_toolbar(ToolbarPlace::default())(f)
            .with_options(|o| o.edit_toolbar = true)
            .with_nodes(vec![
                Node::new("a", xy(100., 100.)).label("A"),
                Node::new("fixed", xy(100., 250.))
                    .label("Fixed")
                    .deletable(false),
            ])
    });
    click(cx, center(&flow, cx, "a"), Modifiers::none());
    // The host's content and the edit buttons in one bar, the host's first.
    let bar = cx.debug_bounds("flow-node-toolbar").expect("one bar");
    let edit = cx
        .debug_bounds("flow-edit-toolbar")
        .expect("the edit buttons");
    assert!(
        bar.left() < edit.left() && edit.right() <= bar.right(),
        "{bar:?} {edit:?}"
    );
    assert!(cx.debug_bounds("flow-delete").is_some());
    click(cx, center(&flow, cx, "fixed"), Modifiers::none());
    assert!(cx.debug_bounds("flow-copy").is_some());
    assert!(
        cx.debug_bounds("flow-delete").is_none(),
        "it may not be deleted"
    );
}

#[gpui::test]
fn the_edit_toolbars_buttons_are_named_buttons(cx: &mut TestAppContext) {
    gpui_core::audit_start();
    let (flow, _, cx) = open(cx, with_edit_toolbar);
    click(cx, center(&flow, cx, "b"), Modifiers::none());
    let seen = gpui_core::audit_take();
    let mut buttons: Vec<String> = seen
        .iter()
        .filter(|a| a.what == "Press" && a.role == gpui::Role::Button)
        .filter_map(|a| a.label.as_ref().map(|l| l.to_string()))
        .collect();
    buttons.sort();
    buttons.dedup();
    assert_eq!(buttons, ["Copy", "Delete", "Duplicate", "Paste"]);
}
