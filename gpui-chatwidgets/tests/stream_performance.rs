//! What a delta costs while a long reply streams in: the text added, the blocks worked out, and
//! a frame laid out and painted, with Core Text laying the text out as on screen, in a chat 900
//! pixels tall that follows the reply's end as it grows. Timed at 1,000,
//! 5,000 and 10,000 characters into a 10,000-character reply, a few characters a delta, it stays
//! about the same: only the open block is read and laid out again. The same reply drawn as one
//! TextView, read and laid out whole on every delta, is timed alongside for comparison.
//!
//! Each step fails past a bound, in a release build, or forty times that in a debug one. On CI
//! (`CI` set, as GitHub's runners do) the bound is four times looser: the shared runners'
//! timings swing with their load, and the strict bound is for a quiet machine.
//! `cargo test --release --test stream_performance -- --nocapture` prints the numbers the README
//! quotes.

#![cfg(all(target_os = "macos", feature = "gpui"))]

mod common;

use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ::gpui::TestDispatcher;
use common::setup;
use gpui::*;
use gpui_chatwidgets::StreamingMarkdown;
use gpui_core::text::{TextView, TextViewStyle};
// The built-in #[test], not GPUI's, which gpui::* brings in.
use std::prelude::v1::test;

const REPLY: &str = include_str!("../examples/replies/long.md");
/// Characters a delta: a token or two, as a model writes them.
const DELTA: usize = 4;
const MARKS: [usize; 3] = [1_000, 5_000, 10_000];

fn bound(ms: f64) -> Duration {
    let ms = if cfg!(debug_assertions) { ms * 40. } else { ms };
    let ms = if std::env::var_os("CI").is_some() {
        ms * 4.
    } else {
        ms
    };
    Duration::from_secs_f64(ms / 1e3)
}

fn style() -> TextViewStyle {
    let mut code = StyleRefinement::default()
        .bg(rgb(0x202020))
        .border_1()
        .border_color(rgb(0x404040))
        .rounded(px(10.))
        .px(px(12.))
        .py(px(10.));
    code.text = TextStyleRefinement {
        font_family: Some("Menlo".into()),
        font_size: Some(px(13.).into()),
        line_height: Some(px(13. * 1.6).into()),
        ..Default::default()
    };
    TextViewStyle {
        paragraph_gap: rems(0.6),
        code_block: code,
        ..Default::default()
    }
}

/// What's drawn: the streaming view, or the whole text as one TextView, read again each time.
enum Shown {
    Streaming(Entity<StreamingMarkdown>),
    Whole(SharedString, Rc<TextViewStyle>),
}

struct Chat(Shown, ScrollHandle);

impl Render for Chat {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let shown = match &self.0 {
            Shown::Streaming(d) => d.clone().into_any_element(),
            Shown::Whole(text, style) => TextView::markdown("whole", text.clone())
                .style((**style).clone())
                .selectable(true)
                .into_any_element(),
        };
        // A chat scrolled to its foot, following the reply as it grows.
        div()
            .id("chat")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.1)
            .font_family("Inter")
            .text_size(px(14.))
            .line_height(px(14. * 1.625))
            .text_color(rgb(0xeeeeee))
            .child(div().w(px(560.)).child(shown))
    }
}

/// The time each delta took (the text added and a frame drawn), by the length after it.
fn stream(whole: bool, seed: u64) -> Vec<(usize, Duration)> {
    let mut app = TestAppContext::build_with_text_system(
        TestDispatcher::new(seed),
        None,
        Arc::new(gpui_apple::AppleTextSystem::new()),
    );
    setup(&mut app);
    let draft = app.update(|cx| cx.new(|_| StreamingMarkdown::new("d").style(style())));
    let shown = if whole {
        Shown::Whole(SharedString::default(), Rc::new(style()))
    } else {
        Shown::Streaming(draft.clone())
    };
    let scroll = ScrollHandle::new();
    let s = scroll.clone();
    let (chat, cx) = app.add_window_view(move |_, _| Chat(shown, s));
    cx.simulate_resize(size(px(800.), px(900.)));
    let chars: Vec<char> = REPLY.chars().collect();
    let mut text = String::new();
    let mut out = Vec::new();
    for piece in chars.chunks(DELTA) {
        let delta: String = piece.iter().collect();
        text.push_str(&delta);
        let start = Instant::now();
        if whole {
            let t = SharedString::from(text.clone());
            chat.update(cx, |c, cx| {
                if let Shown::Whole(s, _) = &mut c.0 {
                    *s = t;
                }
                cx.notify();
            });
        } else {
            draft.update(cx, |d, cx| d.append(&delta, cx));
        }
        scroll.scroll_to_bottom();
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        out.push((text.len(), start.elapsed()));
    }
    out
}

/// The median time of the deltas just before each mark.
fn at_marks(times: &[(usize, Duration)]) -> Vec<Duration> {
    MARKS
        .iter()
        .map(|&m| {
            let mut near: Vec<Duration> = times
                .iter()
                .filter(|(len, _)| *len <= m && *len + 600 > m)
                .map(|(_, t)| *t)
                .collect();
            near.sort();
            near[near.len() / 2]
        })
        .collect()
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

#[test]
fn a_delta_costs_about_the_same_at_the_end_of_a_long_reply() {
    assert!(REPLY.len() >= 10_000);
    let mut streaming = Vec::new();
    let mut whole = Vec::new();
    // Three runs in a release build; one in a debug one, where the bounds are loose anyway.
    let runs = if cfg!(debug_assertions) { 1 } else { 3 };
    for run in 0..runs {
        streaming.extend(stream(false, run));
        whole.extend(stream(true, run));
    }
    let s = at_marks(&streaming);
    let w = at_marks(&whole);
    for (i, m) in MARKS.iter().enumerate() {
        eprintln!(
            "a delta at {m} characters: {:.2} ms streaming, {:.2} ms as one TextView read whole",
            ms(s[i]),
            ms(w[i])
        );
    }
    // Flat: the end of the reply costs little more than its start (the frame still paints all
    // the text there is, so a little more).
    assert!(
        s[2] < s[0] * 3 + bound(0.5),
        "a delta at 10,000 characters took {:?}, at 1,000 {:?}",
        s[2],
        s[0]
    );
    for (i, m) in MARKS.iter().enumerate() {
        assert!(
            s[i] < bound(4.),
            "a delta at {m} characters took {:?}",
            s[i]
        );
    }
    // The whole text read again each delta is printed for comparison, not held to a ratio:
    // since gpui-core's TextView keeps the layout of every unchanged block, re-reading it whole
    // costs about what streaming does, and either is fine as long as streaming stays flat and
    // under its bound.
}
