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

/// The time each delta took (the text added and a frame drawn), by the length after it: the
/// first `from` characters come in one delta, untimed, and the rest a few at a time.
fn stream(text: &str, from: usize, whole: bool, seed: u64) -> Vec<(usize, Duration)> {
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
    let chars: Vec<char> = text.chars().collect();
    let (first, rest) = chars.split_at(from);
    let mut text = String::new();
    let mut out = Vec::new();
    let pieces = std::iter::once((first, false)).filter(|(p, _)| !p.is_empty());
    for (piece, timed) in pieces.chain(rest.chunks(DELTA).map(|p| (p, true))) {
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
        if timed {
            out.push((text.len(), start.elapsed()));
        }
    }
    out
}

/// The median time of the deltas just before each mark.
fn at_marks(times: &[(usize, Duration)], marks: &[usize]) -> Vec<Duration> {
    marks
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
        streaming.extend(stream(REPLY, 0, false, run));
        whole.extend(stream(REPLY, 0, true, run));
    }
    let s = at_marks(&streaming, &MARKS);
    let w = at_marks(&whole, &MARKS);
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

/// A list that grows an item at a time: it's one block until something after it ends it, so
/// it's the open block all along, and gpui-core lays a block out whole when it changes. A delta
/// at its end costs what laying the list out does, growing with the list; printed at 100, 500
/// and 1,000 items (the list given in one go up to 600 characters before, 100 in a debug build,
/// then streamed), and held to a bound at 1,000 items (33,000 characters).
#[test]
fn a_long_list_growing_by_items_is_laid_out_whole() {
    let mut text = String::new();
    let mut marks = Vec::new();
    for i in 1..=1000 {
        text.push_str(&format!("- item {i}: a few words about it\n"));
        if [100, 500, 1000].contains(&i) {
            marks.push(text.len());
        }
    }
    let (runs, lead) = if cfg!(debug_assertions) {
        (1, 100)
    } else {
        (3, 600)
    };
    let mut times = Vec::new();
    for run in 0..runs {
        for &m in &marks {
            times.extend(stream(&text[..m], m - lead, false, run));
        }
    }
    let t = at_marks(&times, &marks);
    for (i, n) in [100, 500, 1000].iter().enumerate() {
        eprintln!(
            "a delta at the end of a {n}-item list ({} characters): {:.2} ms",
            marks[i],
            ms(t[i])
        );
    }
    assert!(t[2] < bound(60.), "a delta at 1,000 items took {:?}", t[2]);
}

/// A code fence 50,000 lines long, then a paragraph, given in one append, a line at a time and
/// through `replace`: settling reads each new line once and checks only a line that could end
/// the open block, so the fence costs what reading it once does. (Reading the open block again
/// at each line, as settling did, took 17 s for 4,000 lines and grew with their square.)
#[test]
fn a_long_code_fence_settles_in_linear_time() {
    let lines = 50_000;
    let mut text = String::from("```rust\n");
    for i in 0..lines {
        text.push_str(&format!("f({});\n", i % 1000));
    }
    text.push_str("```\n\nAfter the code.\n\nThe end.\n");
    let mut app = TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        Arc::new(gpui_apple::AppleTextSystem::new()),
    );
    setup(&mut app);
    let new = |app: &mut TestAppContext| {
        app.update(|cx| cx.new(|_| StreamingMarkdown::new("d").style(style())))
    };
    // Settles until there's no more, a frame at a time: the frames' times.
    let settle = |app: &mut TestAppContext, d: &Entity<StreamingMarkdown>| {
        let mut frames = Vec::new();
        loop {
            let start = Instant::now();
            let more = d.update(app, |d, _| d.settle_now());
            frames.push(start.elapsed());
            if !more {
                return frames;
            }
        }
    };
    let settled = |app: &mut TestAppContext, d: &Entity<StreamingMarkdown>| {
        let stats = d.read_with(app, |d, _| d.stats());
        assert_eq!(stats.settled, 2, "the fence and the paragraph: {stats:?}");
        stats
    };

    // In one append.
    let d = new(&mut app);
    d.update(&mut app, |d, cx| d.append(&text, cx));
    let frames = settle(&mut app, &d);
    let stats = settled(&mut app, &d);
    let one = frames.iter().sum::<Duration>();
    // Only the paragraph after the fence was checked.
    assert!(stats.reread < 100, "{stats:?}");

    // A line at a time, a frame each.
    let d = new(&mut app);
    let mut most = Duration::ZERO;
    let start = Instant::now();
    for line in text.split_inclusive('\n') {
        d.update(&mut app, |d, cx| d.append(line, cx));
        most = most.max(settle(&mut app, &d).into_iter().max().unwrap());
    }
    let by_line = start.elapsed();
    settled(&mut app, &d);

    // Half the text, then the whole of it in its place (a reconnect), then with a line inside
    // the fence changed, which opens the fence again.
    let d = new(&mut app);
    d.update(&mut app, |d, cx| d.append(&text[..text.len() / 2], cx));
    settle(&mut app, &d);
    d.update(&mut app, |d, cx| d.replace(&text, cx));
    let mut replaced = settle(&mut app, &d);
    let changed = text.replacen("f(500);", "g(500);", 1);
    d.update(&mut app, |d, cx| d.replace(&changed, cx));
    replaced.extend(settle(&mut app, &d));
    settled(&mut app, &d);

    let ms = |d: Duration| d.as_secs_f64() * 1e3;
    eprintln!(
        "a {lines}-line code fence ({} KB): {:.2} ms in one append ({} frames, the longest {:.2} ms); \
         {:.1} ms a line at a time (the longest frame {:.3} ms); through replace, the longest frame {:.2} ms",
        text.len() / 1000,
        ms(one),
        frames.len(),
        ms(frames.iter().copied().max().unwrap()),
        ms(by_line),
        ms(most),
        ms(replaced.iter().copied().max().unwrap()),
    );
    for f in frames.iter().chain(&replaced) {
        assert!(*f < bound(5.), "a frame settling took {f:?}");
    }
    assert!(most < bound(1.), "a line's frame settling took {most:?}");
    // Linear: a line at a time costs about what it does in one go, a line's worth of work each.
    assert!(by_line < bound(60.), "a line at a time took {by_line:?}");
}
