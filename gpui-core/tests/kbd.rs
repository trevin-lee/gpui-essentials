//! Kbd and KbdGroup: a key is a muted cap 20 pixels tall and at least as wide, its name in the
//! muted text colour; a group lines its keys up four pixels apart; a narrow space cuts a long
//! key name short and keeps the group inside it; and nothing redraws while they sit there.
//! Laid out with Core Text.

#![cfg(target_os = "macos")]

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    AppContext as _, Bounds, Context, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, Styled as _, TestAppContext, TestDispatcher,
    VisualTestContext, Window, div, point, px,
};
use gpui_core::{IconName, Kbd, KbdGroup, Root, Theme};

struct Host {
    width: f32,
    draws: Rc<Cell<usize>>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        div()
            .p(px(40.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                div()
                    .flex()
                    .debug_selector(|| "one".into())
                    .child(Kbd::new("K")),
            )
            .child(
                div()
                    .flex()
                    .w(px(self.width))
                    .debug_selector(|| "group-box".into())
                    .child(
                        KbdGroup::new()
                            .child(Kbd::new("⌘"))
                            .child(Kbd::new("Shift").icon(IconName::ArrowBigUp))
                            .child(Kbd::new("K")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .w(px(self.width))
                    .debug_selector(|| "long-box".into())
                    .child(Kbd::new("A key name far too long for the room it has")),
            )
    }
}

fn open(cx: &mut TestAppContext, width: f32) -> (Rc<Cell<usize>>, &mut VisualTestContext) {
    cx.update(gpui_core::init);
    let draws = Rc::new(Cell::new(0));
    let d = draws.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let host = cx.new(|_| Host { width, draws: d });
        Root::new(host, window, cx)
    });
    vcx.run_until_parked();
    (draws, vcx)
}

fn app() -> TestAppContext {
    TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        Arc::new(gpui_apple::AppleTextSystem::new()),
    )
}

fn close(a: Hsla, b: Hsla) -> bool {
    let (a, b) = (gpui::Rgba::from(a), gpui::Rgba::from(b));
    [(a.r, b.r), (a.g, b.g), (a.b, b.b), (a.a, b.a)]
        .iter()
        .all(|(x, y)| (x - y).abs() < 0.01)
}

/// The quads (fill) and the glyphs (colour) drawn inside `within`, each as much of it as
/// shows inside its clip.
type Drawn = (Vec<(Bounds<f32>, Hsla)>, Vec<(Bounds<f32>, Hsla)>);

fn drawn(vcx: &mut VisualTestContext, within: Bounds<Pixels>) -> Drawn {
    vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let k = window.scale_factor();
        let scene = window.rendered_scene();
        let b = |b: Bounds<gpui::ScaledPixels>| Bounds {
            origin: point(b.origin.x.0 / k, b.origin.y.0 / k),
            size: gpui::size(b.size.width.0 / k, b.size.height.0 / k),
        };
        let inside = |b: &Bounds<f32>| within.contains(&point(px(b.center().x), px(b.center().y)));
        let quads = scene
            .quads
            .iter()
            .filter_map(|q| {
                let shown = b(q.bounds).intersect(&b(q.content_mask.bounds));
                Some((shown, q.background.as_solid()?))
            })
            .filter(|(q, _)| q.size.width > 0.)
            .filter(|(q, c)| c.a > 0. && inside(q))
            .collect();
        let glyphs = scene
            .monochrome_sprites
            .iter()
            .map(|g| (b(g.bounds).intersect(&b(g.content_mask.bounds)), g.color))
            .chain(
                scene
                    .subpixel_sprites
                    .iter()
                    .map(|g| (b(g.bounds).intersect(&b(g.content_mask.bounds)), g.color)),
            )
            .filter(|(g, _)| g.size.width > 0. && inside(g))
            .collect();
        (quads, glyphs)
    })
}

#[gpui::test]
fn a_key_is_a_muted_cap_20_pixels_tall() {
    let mut cx = app();
    let (_, vcx) = open(&mut cx, 400.);
    let one = vcx.debug_bounds("one").unwrap();
    assert_eq!(one.size.height, px(20.));
    assert!(one.size.width >= px(20.), "{one:?}");
    let theme = Theme::dark();
    let (quads, glyphs) = drawn(vcx, one);
    assert!(
        quads.iter().any(|(_, c)| close(*c, theme.muted)),
        "no muted cap: {quads:?}"
    );
    assert!(!glyphs.is_empty());
    assert!(
        glyphs
            .iter()
            .all(|(_, c)| close(*c, theme.muted_foreground))
    );
}

#[gpui::test]
fn a_group_lines_its_keys_up_four_pixels_apart() {
    let mut cx = app();
    let (_, vcx) = open(&mut cx, 400.);
    let group = vcx.debug_bounds("group-box").unwrap();
    let theme = Theme::dark();
    let mut caps: Vec<Bounds<f32>> = drawn(vcx, group)
        .0
        .into_iter()
        .filter(|(_, c)| close(*c, theme.muted))
        .map(|(b, _)| b)
        .collect();
    caps.sort_by(|a, b| a.origin.x.total_cmp(&b.origin.x));
    assert_eq!(caps.len(), 3, "{caps:?}");
    for w in caps.windows(2) {
        let gap = w[1].origin.x - (w[0].origin.x + w[0].size.width);
        assert!((gap - 4.).abs() < 0.5, "a gap of {gap}");
        assert_eq!(w[0].origin.y, w[1].origin.y);
    }
}

#[gpui::test]
fn a_narrow_space_cuts_a_key_short_and_keeps_the_group_inside() {
    let mut cx = app();
    let (_, vcx) = open(&mut cx, 60.);
    for which in ["group-box", "long-box"] {
        let bx = vcx.debug_bounds(which).unwrap();
        let (quads, glyphs) = drawn(vcx, bx);
        assert!(!glyphs.is_empty(), "{which}: nothing drawn");
        for (b, _) in quads.iter().chain(&glyphs) {
            assert!(
                b.origin.x + b.size.width <= f32::from(bx.right()) + 1.,
                "{which}: {b:?} past {bx:?}"
            );
        }
    }
}

fn frame(vcx: &mut VisualTestContext) -> usize {
    vcx.executor().advance_clock(Duration::from_millis(16));
    let asked = vcx.update(|window, cx| window.simulate_next_frame(cx));
    vcx.run_until_parked();
    asked
}

#[gpui::test]
fn nothing_redraws_while_they_sit_there(cx: &mut TestAppContext) {
    let (draws, vcx) = open(cx, 400.);
    let settled = draws.get();
    for _ in 0..30 {
        assert_eq!(frame(vcx), 0, "asked for a frame");
    }
    assert_eq!(draws.get(), settled);
}
