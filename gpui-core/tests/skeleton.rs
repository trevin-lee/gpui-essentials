//! Skeleton: a muted block the host's size (never wider than its parent) that pulses while it's
//! drawn, asks for no frames once the host stops drawing it, and none with `animate(false)`.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, Styled as _, TestAppContext, VisualTestContext, Window, div,
    prelude::FluentBuilder as _, px,
};
use gpui_core::{Root, Skeleton};

struct Host {
    shown: bool,
    animate: bool,
    draws: Rc<Cell<usize>>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let animate = self.animate;
        div().p(px(20.)).when(self.shown, |d| {
            d.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(16.))
                    .child(
                        div().debug_selector(|| "round".into()).child(
                            Skeleton::new()
                                .animate(animate)
                                .size(px(48.))
                                .rounded_full(),
                        ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.))
                            .child(
                                div()
                                    .debug_selector(|| "line".into())
                                    .child(Skeleton::new().animate(animate).h(px(16.)).w(px(250.))),
                            )
                            .child(Skeleton::new().animate(animate).h(px(16.)).w(px(200.))),
                    ),
            )
            .child(
                div().w(px(100.)).debug_selector(|| "narrow".into()).child(
                    div()
                        .debug_selector(|| "squeezed".into())
                        .child(Skeleton::new().animate(animate).h(px(16.)).w(px(250.))),
                ),
            )
        })
    }
}

struct Opened<'a> {
    host: Entity<Host>,
    draws: Rc<Cell<usize>>,
    vcx: &'a mut VisualTestContext,
}

fn open(cx: &mut TestAppContext, animate: bool) -> Opened<'_> {
    cx.update(gpui_core::init);
    let draws = Rc::new(Cell::new(0));
    let d = draws.clone();
    let made = Rc::new(Cell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let host = cx.new(|_| Host {
            shown: true,
            animate,
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

/// Delivers one display frame and says how many things had asked for it.
fn frame(vcx: &mut VisualTestContext) -> usize {
    vcx.executor().advance_clock(Duration::from_millis(16));
    let asked = vcx.update(|window, cx| window.simulate_next_frame(cx));
    vcx.run_until_parked();
    asked
}

#[gpui::test]
fn it_is_the_size_the_host_gives_it(cx: &mut TestAppContext) {
    let o = open(cx, false);
    let round = o.vcx.debug_bounds("round").unwrap();
    assert_eq!((round.size.width, round.size.height), (px(48.), px(48.)));
    let line = o.vcx.debug_bounds("line").unwrap();
    assert_eq!((line.size.width, line.size.height), (px(250.), px(16.)));
}

#[gpui::test]
fn it_pulses_only_while_drawn(cx: &mut TestAppContext) {
    let o = open(cx, true);
    // Every frame while it's drawn: it pulses for as long as it's there.
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
    assert_eq!(o.draws.get(), gone, "kept drawing once it was gone");
}

#[gpui::test]
fn animate_false_keeps_it_still(cx: &mut TestAppContext) {
    let o = open(cx, false);
    let settled = o.draws.get();
    for _ in 0..60 {
        assert_eq!(frame(o.vcx), 0, "asked for a frame");
    }
    assert_eq!(o.draws.get(), settled);
}

#[cfg(target_os = "macos")]
mod drawn {
    use super::*;

    /// The alpha of the fill drawn over the middle of `which`.
    fn alpha(vcx: &mut VisualTestContext, which: &'static str) -> f32 {
        let c = vcx.debug_bounds(which).unwrap().center();
        vcx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
            let k = window.scale_factor();
            window
                .rendered_scene()
                .quads
                .iter()
                .filter(|q| {
                    let b = q.bounds;
                    let (x, y) = (f32::from(c.x) * k, f32::from(c.y) * k);
                    b.origin.x.0 <= x
                        && x <= b.origin.x.0 + b.size.width.0
                        && b.origin.y.0 <= y
                        && y <= b.origin.y.0 + b.size.height.0
                })
                .filter_map(|q| q.background.as_solid())
                .filter(|f| (f.l - gpui_core::Theme::dark().muted.l).abs() < 0.01)
                .map(|f| f.a)
                .next_back()
                .expect("no fill drawn")
        })
    }

    #[gpui::test]
    fn its_opacity_falls_to_half_and_comes_back(cx: &mut TestAppContext) {
        let o = open(cx, true);
        let mut seen = Vec::new();
        // A pulse is two seconds: sample one every 250 ms for two pulses.
        for _ in 0..16 {
            seen.push(alpha(o.vcx, "line"));
            o.vcx.executor().advance_clock(Duration::from_millis(250));
        }
        let (lo, hi) = seen
            .iter()
            .fold((f32::MAX, f32::MIN), |(lo, hi), a| (lo.min(*a), hi.max(*a)));
        assert!(lo < 0.6 && hi > 0.95, "{seen:?}");
    }

    #[gpui::test]
    fn it_is_never_wider_than_its_parent(cx: &mut TestAppContext) {
        let o = open(cx, false);
        let narrow = o.vcx.debug_bounds("narrow").unwrap();
        let muted = gpui_core::Theme::dark().muted;
        let widths: Vec<f32> = o.vcx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
            let k = window.scale_factor();
            window
                .rendered_scene()
                .quads
                .iter()
                .filter(|q| q.background.as_solid() == Some(muted))
                .filter(|q| {
                    let y = q.bounds.center().y.0 / k;
                    y > f32::from(narrow.top()) && y < f32::from(narrow.bottom())
                })
                .map(|q| q.bounds.size.width.0 / k)
                .collect()
        });
        // 250 pixels asked for, in a 100-pixel parent.
        assert_eq!(widths, [100.]);
    }

    #[gpui::test]
    fn still_it_is_whole(cx: &mut TestAppContext) {
        let o = open(cx, false);
        for _ in 0..4 {
            assert_eq!(alpha(o.vcx, "line"), 1.);
            o.vcx.executor().advance_clock(Duration::from_millis(500));
        }
    }
}
