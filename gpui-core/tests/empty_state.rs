//! EmptyState: its icon (in a 40-pixel muted circle), title, description and actions are centred one
//! above another; its text wraps in a narrow space and never runs out of it; its actions work
//! from the keyboard; and nothing redraws while it sits there. Laid out with Core Text.

#![cfg(target_os = "macos")]

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    AppContext as _, Bounds, Context, InteractiveElement as _, IntoElement, KeyDownEvent,
    KeyUpEvent, Keystroke, ParentElement as _, Render, Styled as _, TestAppContext, TestDispatcher,
    VisualTestContext, Window, div, point, px,
};
use gpui_core::{EmptyState, IconName, Press, Root, Theme};

struct Host {
    width: f32,
    pressed: Rc<Cell<usize>>,
    draws: Rc<Cell<usize>>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let pressed = self.pressed.clone();
        div().p(px(20.)).child(
            div()
                .flex()
                .w(px(self.width))
                .h(px(400.))
                .debug_selector(|| "box".into())
                .child(
                    EmptyState::new()
                        .icon(IconName::Inbox)
                        .title("No messages yet")
                        .description(
                            "When an agent writes to you, its message shows up here, newest \
                             first. Pneumonoultramicroscopicsilicovolcanoconiosis.",
                        )
                        .child(
                            Press::new("start")
                                .debug_selector(|| "start".into())
                                .px(px(12.))
                                .h(px(32.))
                                .child("Start a chat")
                                .on_click(move |_, _, _| pressed.set(pressed.get() + 1)),
                        ),
                ),
        )
    }
}

fn app() -> TestAppContext {
    TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        Arc::new(gpui_apple::AppleTextSystem::new()),
    )
}

struct Opened<'a> {
    pressed: Rc<Cell<usize>>,
    draws: Rc<Cell<usize>>,
    vcx: &'a mut VisualTestContext,
}

fn open(cx: &mut TestAppContext, width: f32) -> Opened<'_> {
    cx.update(gpui_core::init);
    let pressed = Rc::new(Cell::new(0));
    let draws = Rc::new(Cell::new(0));
    let (p, d) = (pressed.clone(), draws.clone());
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let host = cx.new(|_| Host {
            width,
            pressed: p,
            draws: d,
        });
        Root::new(host, window, cx)
    });
    vcx.run_until_parked();
    Opened {
        pressed,
        draws,
        vcx,
    }
}

/// The muted circle and the glyphs drawn, in window pixels as shown inside their clips.
fn drawn(vcx: &mut VisualTestContext) -> (Vec<Bounds<f32>>, Vec<Bounds<f32>>) {
    let muted = Theme::dark().muted;
    vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let k = window.scale_factor();
        let scene = window.rendered_scene();
        let f = |b: gpui::Bounds<gpui::ScaledPixels>| Bounds {
            origin: point(b.origin.x.0 / k, b.origin.y.0 / k),
            size: gpui::size(b.size.width.0 / k, b.size.height.0 / k),
        };
        let circles = scene
            .quads
            .iter()
            .filter(|q| q.background.as_solid() == Some(muted))
            .map(|q| f(q.bounds))
            .collect();
        let glyphs = scene
            .monochrome_sprites
            .iter()
            .map(|g| f(g.bounds).intersect(&f(g.content_mask.bounds)))
            .chain(
                scene
                    .subpixel_sprites
                    .iter()
                    .map(|g| f(g.bounds).intersect(&f(g.content_mask.bounds))),
            )
            .filter(|b| b.size.width > 0.)
            .collect();
        (circles, glyphs)
    })
}

#[gpui::test]
fn it_is_centred_one_above_another() {
    let mut cx = app();
    let o = open(&mut cx, 500.);
    let bx = o.vcx.debug_bounds("box").unwrap();
    let mid = f32::from(bx.center().x);
    let (circles, glyphs) = drawn(o.vcx);
    assert_eq!(circles.len(), 1, "{circles:?}");
    let circle = circles[0];
    assert_eq!((circle.size.width, circle.size.height), (40., 40.));
    assert!(
        (circle.center().x - mid).abs() < 0.5,
        "{circle:?} off {mid}"
    );
    // Each line of text is centred: the first and last glyph on it are as far from the middle.
    // A line is the glyphs from top to bottom until one starts below every glyph so far
    // (its tallest letter and its full stop alike).
    let mut sorted = glyphs.clone();
    sorted.sort_by(|a, b| a.center().y.total_cmp(&b.center().y));
    let mut lines: Vec<(f32, f32, f32, f32)> = Vec::new(); // (top, bottom, left, right)
    for g in &sorted {
        match lines.last_mut() {
            Some(l) if g.center().y < l.1 => {
                l.1 = l.1.max(g.bottom());
                l.2 = l.2.min(g.origin.x);
                l.3 = l.3.max(g.right());
            }
            _ => lines.push((g.origin.y, g.bottom(), g.origin.x, g.right())),
        }
    }
    let lines: Vec<(f32, f32, f32)> = lines.into_iter().map(|(t, _, l, r)| (t, l, r)).collect();
    assert!(lines.len() >= 3, "title, description, action: {lines:?}");
    for (y, l, r) in &lines {
        assert!(
            ((l + r) / 2. - mid).abs() < 4.,
            "line at {y} runs {l}–{r}, middle {mid}"
        );
        assert!(*y > circle.bottom(), "text above the icon");
    }
    let start = o.vcx.debug_bounds("start").unwrap();
    assert!((f32::from(start.center().x) - mid).abs() < 0.5, "{start:?}");
}

#[gpui::test]
fn its_text_wraps_in_a_narrow_space() {
    let mut cx = app();
    let o = open(&mut cx, 140.);
    let bx = o.vcx.debug_bounds("box").unwrap();
    let (_, glyphs) = drawn(o.vcx);
    assert!(!glyphs.is_empty());
    let mut rows: Vec<f32> = glyphs.iter().map(|g| g.center().y.round()).collect();
    rows.sort_by(f32::total_cmp);
    rows.dedup_by(|a, b| (*a - *b).abs() < 4.);
    assert!(rows.len() >= 6, "it didn't wrap: {} rows", rows.len());
    for g in &glyphs {
        assert!(
            g.origin.x >= f32::from(bx.left()) - 1. && g.right() <= f32::from(bx.right()) + 1.,
            "{g:?} out of {bx:?}"
        );
    }
}

#[gpui::test]
fn its_actions_work_from_the_keyboard() {
    let mut cx = app();
    let o = open(&mut cx, 500.);
    for k in ["tab", "enter"] {
        let keystroke = Keystroke::parse(k).unwrap();
        o.vcx.simulate_event(KeyDownEvent {
            keystroke: keystroke.clone(),
            is_held: false,
            prefer_character_input: false,
        });
        o.vcx.simulate_event(KeyUpEvent { keystroke });
    }
    o.vcx.run_until_parked();
    assert_eq!(o.pressed.get(), 1);
}

#[gpui::test]
fn nothing_redraws_while_it_sits_there() {
    let mut cx = app();
    let o = open(&mut cx, 500.);
    let settled = o.draws.get();
    for _ in 0..30 {
        o.vcx.executor().advance_clock(Duration::from_millis(16));
        let asked = o.vcx.update(|window, cx| window.simulate_next_frame(cx));
        o.vcx.run_until_parked();
        assert_eq!(asked, 0, "asked for a frame");
    }
    assert_eq!(o.draws.get(), settled);
}
