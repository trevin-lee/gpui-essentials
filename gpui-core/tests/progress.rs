//! Progress: a determinate bar fills its track as far as its value, clamped to 0–100, and tells
//! assistive technology that value; an indeterminate one has no value and sweeps only while
//! it's drawn (none once the host stops drawing it, none under reduce motion); a bar with a
//! value never asks for frames.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AppContext as _, Context, Element as _, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, RenderOnce as _, Role, Styled as _, TestAppContext,
    VisualTestContext, Window, accesskit, div, prelude::FluentBuilder as _, px,
};
use gpui_core::{Progress, Root};

struct Host {
    /// The bar's value, or none for an indeterminate one.
    value: Option<f32>,
    shown: bool,
    draws: Rc<Cell<usize>>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let value = self.value;
        div().p(px(20.)).w(px(240.)).when(self.shown, |d| {
            d.child(
                div()
                    .debug_selector(|| "bar".into())
                    .child(Progress::new("p").when_some(value, |p, v| p.value(v))),
            )
        })
    }
}

struct Opened<'a> {
    host: Entity<Host>,
    draws: Rc<Cell<usize>>,
    vcx: &'a mut VisualTestContext,
}

fn open(cx: &mut TestAppContext, value: Option<f32>, reduce_motion: bool) -> Opened<'_> {
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
            value,
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

/// What assistive technology is told of a bar: its role, value, least and most.
fn told(
    vcx: &mut VisualTestContext,
    p: Progress,
) -> (Option<Role>, Option<f64>, Option<f64>, Option<f64>) {
    vcx.update(|window, cx| {
        let el = p.render(window, cx).into_element();
        let role = el.a11y_role();
        let mut node = accesskit::Node::new(role.unwrap_or(Role::Unknown));
        el.write_a11y_info(&mut node);
        (
            role,
            node.numeric_value(),
            node.min_numeric_value(),
            node.max_numeric_value(),
        )
    })
}

#[gpui::test]
fn it_tells_its_value_clamped(cx: &mut TestAppContext) {
    let o = open(cx, Some(40.), false);
    let some = Some(Role::ProgressIndicator);
    for (given, shown) in [(40., 40.), (-10., 0.), (150., 100.), (f32::NAN, 0.)] {
        assert_eq!(
            told(o.vcx, Progress::new("p").value(given)),
            (some, Some(shown), Some(0.), Some(100.)),
            "{given}"
        );
    }
    assert_eq!(told(o.vcx, Progress::new("p")), (some, None, None, None));
}

#[gpui::test]
fn a_bar_with_a_value_never_asks_for_frames(cx: &mut TestAppContext) {
    let o = open(cx, Some(40.), false);
    let settled = o.draws.get();
    for _ in 0..60 {
        assert_eq!(frame(o.vcx), 0, "asked for a frame");
    }
    assert_eq!(o.draws.get(), settled);
}

#[gpui::test]
fn an_indeterminate_bar_sweeps_only_while_drawn(cx: &mut TestAppContext) {
    let o = open(cx, None, false);
    // Every frame while it's drawn: it sweeps for as long as it's there.
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
fn reduce_motion_holds_the_sweep_still(cx: &mut TestAppContext) {
    let o = open(cx, None, true);
    let settled = o.draws.get();
    for _ in 0..60 {
        assert_eq!(frame(o.vcx), 0, "asked for a frame");
    }
    assert_eq!(o.draws.get(), settled);
}

#[gpui::test]
fn the_track_is_eight_pixels_across_its_parent(cx: &mut TestAppContext) {
    let o = open(cx, Some(40.), false);
    let b = o.vcx.debug_bounds("bar").unwrap();
    assert_eq!((b.size.width, b.size.height), (px(200.), px(8.)));
}

#[cfg(target_os = "macos")]
mod drawn {
    use super::*;

    /// The fills drawn inside the bar: (left, width) in pixels from its left, and colour.
    fn fills(vcx: &mut VisualTestContext) -> Vec<(f32, f32, gpui::Hsla)> {
        let b = vcx.debug_bounds("bar").unwrap();
        vcx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
            let k = window.scale_factor();
            window
                .rendered_scene()
                .quads
                .iter()
                .filter_map(|q| {
                    // What shows of it: its bounds inside its clip.
                    let l = q.bounds.origin.x.0.max(q.content_mask.bounds.origin.x.0) / k;
                    let r = (q.bounds.origin.x.0 + q.bounds.size.width.0)
                        .min(q.content_mask.bounds.origin.x.0 + q.content_mask.bounds.size.width.0)
                        / k;
                    let y = (q.bounds.origin.y.0 + q.bounds.size.height.0 / 2.) / k;
                    let inside = y > f32::from(b.top()) && y < f32::from(b.bottom());
                    (inside && r > l).then(|| {
                        let left = f32::from(b.left());
                        (l - left, r - l, q.background.as_solid().unwrap_or_default())
                    })
                })
                .collect()
        })
    }

    fn primary() -> gpui::Hsla {
        gpui_core::Theme::dark().primary
    }

    #[gpui::test]
    fn the_fill_reaches_as_far_as_the_value(cx: &mut TestAppContext) {
        for (value, width) in [(40., 80.), (-10., 0.), (150., 200.)] {
            let o = open(cx, Some(value), false);
            let fill: f32 = fills(o.vcx)
                .iter()
                .filter(|(_, _, c)| *c == primary())
                .map(|(_, w, _)| *w)
                .sum();
            assert!((fill - width).abs() < 0.5, "{value}: a fill {fill} wide");
        }
    }

    #[gpui::test]
    fn the_sweep_moves_and_stays_inside_the_track(cx: &mut TestAppContext) {
        let o = open(cx, None, false);
        let mut lefts = Vec::new();
        for _ in 0..8 {
            for (l, w, c) in fills(o.vcx) {
                if c == primary() {
                    assert!(l >= -0.5 && l + w <= 200.5, "drawn at {l} + {w}");
                    lefts.push(l);
                }
            }
            o.vcx.executor().advance_clock(Duration::from_millis(180));
        }
        lefts.dedup();
        assert!(lefts.len() > 3, "it didn't move: {lefts:?}");
    }
}
