//! Spinner: a busy indicator labelled "Loading" to assistive technology, sized by
//! `Sizable` or like a `div`, turning while it's drawn, asking for no frames once the host
//! stops drawing it, and none under GPUI's reduce-motion setting.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AppContext as _, Context, Element as _, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, RenderOnce as _, Role, Styled as _, TestAppContext,
    VisualTestContext, Window, accesskit, div, prelude::FluentBuilder as _, px,
};
use gpui_core::{Root, Sizable as _, Spinner};

struct Host {
    shown: bool,
    draws: Rc<Cell<usize>>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        div()
            .p(px(20.))
            .flex()
            .items_start()
            .gap(px(10.))
            .when(self.shown, |d| {
                d.child(
                    div()
                        .debug_selector(|| "small".into())
                        .child(Spinner::new("a").small()),
                )
                .child(
                    div()
                        .debug_selector(|| "medium".into())
                        .child(Spinner::new("b")),
                )
                .child(
                    div()
                        .debug_selector(|| "large".into())
                        .child(Spinner::new("c").large()),
                )
                .child(
                    div()
                        .debug_selector(|| "styled".into())
                        .child(Spinner::new("d").size(px(32.))),
                )
            })
    }
}

struct Opened<'a> {
    host: Entity<Host>,
    draws: Rc<Cell<usize>>,
    vcx: &'a mut VisualTestContext,
}

fn open(cx: &mut TestAppContext, reduce_motion: bool) -> Opened<'_> {
    cx.update(|cx| {
        gpui_core::init(cx);
        cx.set_reduce_motion(reduce_motion);
    });
    let draws = Rc::new(Cell::new(0));
    let d = draws.clone();
    let made = Rc::new(Cell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let host = cx.new(|_| Host {
            shown: true,
            draws: d,
        });
        m.set(Some(host.clone()));
        Root::new(host, window, cx)
    });
    vcx.run_until_parked();
    Opened {
        host: made.take().unwrap(),
        draws,
        vcx,
    }
}

fn frame(vcx: &mut VisualTestContext) -> usize {
    vcx.executor().advance_clock(Duration::from_millis(16));
    let asked = vcx.update(|window, cx| window.simulate_next_frame(cx));
    vcx.run_until_parked();
    asked
}

#[gpui::test]
fn it_is_a_busy_indicator_labelled_loading(cx: &mut TestAppContext) {
    let o = open(cx, false);
    let told = |s: Spinner, vcx: &mut VisualTestContext| {
        vcx.update(|window, cx| {
            let el = s.render(window, cx).into_element();
            let role = el.a11y_role();
            let mut node = accesskit::Node::new(role.unwrap_or(Role::Unknown));
            el.write_a11y_info(&mut node);
            (role, node.label().map(str::to_owned), node.numeric_value())
        })
    };
    assert_eq!(
        told(Spinner::new("s"), o.vcx),
        (Some(Role::ProgressIndicator), Some("Loading".into()), None)
    );
    assert_eq!(
        told(Spinner::new("s").accessibility_label("Sending"), o.vcx).1,
        Some("Sending".into())
    );
}

#[gpui::test]
fn it_comes_in_sizes(cx: &mut TestAppContext) {
    let o = open(cx, false);
    for (which, side) in [
        ("small", 14.),
        ("medium", 16.),
        ("large", 20.),
        ("styled", 32.),
    ] {
        let b = o.vcx.debug_bounds(which).unwrap();
        assert_eq!(
            (b.size.width, b.size.height),
            (px(side), px(side)),
            "{which}"
        );
    }
}

#[gpui::test]
fn it_turns_only_while_drawn(cx: &mut TestAppContext) {
    let o = open(cx, false);
    // Every frame while it's drawn: it turns for as long as it's there.
    for i in 0..30 {
        assert!(frame(o.vcx) > 0, "no frame asked for at frame {i}");
    }
    o.host.update(o.vcx, |h, cx| {
        h.shown = false;
        cx.notify();
    });
    frame(o.vcx);
    let gone = o.draws.get();
    for _ in 0..60 {
        assert_eq!(frame(o.vcx), 0, "asked for a frame once it was gone");
    }
    assert_eq!(o.draws.get(), gone);
}

#[gpui::test]
fn reduce_motion_keeps_it_still(cx: &mut TestAppContext) {
    let o = open(cx, true);
    let settled = o.draws.get();
    for _ in 0..60 {
        assert_eq!(frame(o.vcx), 0, "asked for a frame");
    }
    assert_eq!(o.draws.get(), settled);
}
