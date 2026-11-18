//! A button's children draw after its icon and label, in order, inside it; a host restyles it
//! like any element (its children's order reversed, full width); its id is its test selector;
//! and its children never become its name.

use gpui::{
    AppContext as _, Context, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    Styled as _, TestAppContext, VisualTestContext, Window, div, px, size,
};
use gpui_core::{Button, IconName, Root, audit_start, audit_take};

struct Host;

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(400.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                Button::new("count")
                    .label("Inbox")
                    .child(div().debug_selector(|| "count-badge".into()).child("12")),
            )
            .child(
                Button::new("reversed")
                    .label("Next")
                    .icon(IconName::ChevronRight)
                    .flex_row_reverse(),
            )
            .child(Button::new("wide").label("Save").w_full())
    }
}

fn open(cx: &mut TestAppContext) -> &mut VisualTestContext {
    cx.update(gpui_core::init);
    audit_start();
    let (_, vcx) = cx.add_window_view(|window, cx| Root::new(cx.new(|_| Host), window, cx));
    vcx.simulate_resize(size(px(600.), px(400.)));
    vcx.run_until_parked();
    vcx
}

#[gpui::test]
fn children_draw_inside_after_the_label(cx: &mut TestAppContext) {
    let vcx = open(cx);
    let button = vcx.debug_bounds("count").expect("found by its id");
    let badge = vcx.debug_bounds("count-badge").expect("its child is drawn");
    assert!(
        button.contains(&badge.origin) && button.contains(&badge.bottom_right()),
        "the child is inside the button: {badge:?} in {button:?}"
    );
}

#[gpui::test]
fn a_host_restyles_it_like_any_element(cx: &mut TestAppContext) {
    let vcx = open(cx);
    let wide = vcx.debug_bounds("wide").unwrap();
    assert_eq!(
        wide.size.width,
        px(400.),
        "w_full fills its 400-point column"
    );
    let reversed = vcx.debug_bounds("reversed").unwrap();
    assert!(reversed.size.width > px(0.));
}

#[gpui::test]
fn children_are_drawn_not_announced(cx: &mut TestAppContext) {
    let _ = open(cx);
    let said = audit_take();
    let count = said
        .iter()
        .find(|a| a.label.as_ref().is_some_and(|l| l.as_ref() == "Inbox"))
        .expect("the button with a child is called by its label");
    assert_eq!(
        count.label.as_ref().unwrap().as_ref(),
        "Inbox",
        "not \"Inbox 12\""
    );
}

struct Tip;

impl Render for Tip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().debug_selector(|| "tip".into()).child("Copy")
    }
}

struct Tipped;

impl Render for Tipped {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().p(px(20.)).child(
            Button::new("copy")
                .icon(IconName::Copy)
                .accessibility_label("Copy")
                .debug_selector(|| "copy-button".into())
                .tooltip(|_, cx| cx.new(|_| Tip).into()),
        )
    }
}

#[gpui::test]
fn a_host_names_its_selector_and_gives_a_tooltip(cx: &mut TestAppContext) {
    cx.update(gpui_core::init);
    let (_, vcx) = cx.add_window_view(|window, cx| Root::new(cx.new(|_| Tipped), window, cx));
    vcx.simulate_resize(size(px(300.), px(200.)));
    vcx.run_until_parked();
    let at = vcx
        .debug_bounds("copy-button")
        .expect("found by the host's selector");
    assert!(
        vcx.debug_bounds("copy").is_none(),
        "the host's selector replaces its id's"
    );
    vcx.simulate_mouse_move(at.center(), None, gpui::Modifiers::none());
    vcx.executor()
        .advance_clock(std::time::Duration::from_secs(2));
    vcx.run_until_parked();
    assert!(
        vcx.debug_bounds("tip").is_some(),
        "the tooltip shows once the pointer rests"
    );
}
