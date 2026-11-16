//! A reply in Markdown drawn as it streams in ([`StreamingMarkdown`]): blocks laid out once and
//! kept, the open block closed for now, the caret, its height while it's written, and what
//! happens when it ends or its message replaces it. Laid out with Core Text and the bundled
//! Inter, as on screen.

#![cfg(all(target_os = "macos", feature = "gpui"))]

mod common;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use ::gpui::TestDispatcher;
use common::{draw, setup};
use gpui::*;
use gpui_chatwidgets::StreamingMarkdown;
use gpui_core::text::TextViewStyle;
// The built-in #[test], not GPUI's, which gpui::* brings in.
use std::prelude::v1::test;

const REPLY: &str = include_str!("../examples/replies/long.md");

/// A style like a chat app's: fractional gaps and code lines, so rounding would show.
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
        heading_base_font_size: Some(px(14.)),
        heading_font_size: Some(Arc::new(|level, base| match level {
            1 => base * 1.3,
            _ => base,
        })),
        code_block: code,
        ..Default::default()
    }
}

/// A chat's foot: the draft at a width, in the text style around it, or the card its message
/// brought in its place.
struct Chat {
    draft: Entity<StreamingMarkdown>,
    card: Option<Entity<gpui_chatwidgets::CardView>>,
    width: Pixels,
}

impl Render for Chat {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let shown = match &self.card {
            Some(card) => card.clone().into_any_element(),
            None => self.draft.clone().into_any_element(),
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_start()
            .font_family("Inter")
            .text_size(px(14.))
            .line_height(px(14. * 1.625))
            .text_color(rgb(0xeeeeee))
            .child(div().flex_none().w(self.width).child(shown))
    }
}

/// A test app that lays text out as the screen does.
fn app() -> TestAppContext {
    TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        Arc::new(gpui_apple::AppleTextSystem::new()),
    )
}

struct Opened<'a> {
    draft: Entity<StreamingMarkdown>,
    chat: Entity<Chat>,
    cx: &'a mut VisualTestContext,
    links: Rc<RefCell<Vec<String>>>,
}

/// The draft and the chat, once the window has made them.
type Made = Rc<RefCell<Option<(Entity<StreamingMarkdown>, Entity<Chat>)>>>;

fn open(app: &mut TestAppContext, width: f32) -> Opened<'_> {
    open_in(app, width, 8000.)
}

fn open_in(app: &mut TestAppContext, width: f32, height: f32) -> Opened<'_> {
    setup(app);
    let links: Rc<RefCell<Vec<String>>> = Rc::default();
    let made: Made = Rc::default();
    let (m, l) = (made.clone(), links.clone());
    let (_, cx) = app.add_window_view(move |window, cx| {
        let draft = cx.new(|_| {
            StreamingMarkdown::new("d")
                .style(style())
                .selectable(true)
                .on_link_click(move |url, _, _, _| l.borrow_mut().push(url.to_string()))
        });
        let chat = cx.new(|_| Chat {
            draft: draft.clone(),
            card: None,
            width: px(width),
        });
        *m.borrow_mut() = Some((draft, chat.clone()));
        gpui_core::Root::new(chat, window, cx)
    });
    cx.simulate_resize(size(px(1000.), px(height)));
    let (draft, chat) = made.borrow().clone().unwrap();
    draw(cx);
    Opened {
        draft,
        chat,
        cx,
        links,
    }
}

/// The text cut into deltas of a few characters, on character boundaries, as a server's.
fn deltas(text: &str, n: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    chars.chunks(n).map(|c| c.iter().collect()).collect()
}

fn sel(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// The bounds of the draft and of every block drawn, by element.
fn bounds(
    cx: &mut VisualTestContext,
    keys: usize,
) -> (Bounds<Pixels>, Vec<Option<Bounds<Pixels>>>) {
    let draft = cx.debug_bounds("d:draft").expect("the draft is drawn");
    let blocks = (0..keys)
        .map(|k| cx.debug_bounds(sel(format!("d:md{k}"))))
        .collect();
    (draft, blocks)
}

fn append(o: &mut Opened, text: &str) {
    let d = o.draft.clone();
    o.cx.update(|_, cx| d.update(cx, |d, cx| d.append(text, cx)));
    draw(o.cx);
}

fn shown(o: &mut Opened) -> String {
    o.draft
        .read_with(o.cx, |d, _| d.shown_tail().map(|s| s.to_string()))
        .unwrap_or_default()
}

#[test]
fn a_reply_streams_in_and_settled_blocks_stay_laid_out() {
    let mut app = app();
    let mut o = open(&mut app, 560.);
    let mut tallest = px(0.);
    let mut last_settled = 0;
    let mut kept: Vec<(usize, Bounds<Pixels>)> = Vec::new();
    for (i, delta) in deltas(REPLY, 7).iter().enumerate() {
        append(&mut o, delta);
        let stats = o.draft.read_with(o.cx, |d, _| d.stats());
        // Each block is made once, when it settles, and never again.
        assert_eq!(stats.made, stats.settled, "at delta {i}");
        assert!(stats.settled >= last_settled, "blocks only settle");
        last_settled = stats.settled;
        let (draft, blocks) = bounds(o.cx, stats.settled + 1);
        // The draft only grows while it's written.
        assert!(
            draft.size.height >= tallest,
            "shrank at delta {i}: {:?} < {tallest:?}",
            draft.size.height
        );
        tallest = draft.size.height;
        // A settled block doesn't move or change size as the text after it grows.
        for (k, b) in &kept {
            assert_eq!(blocks[*k], Some(*b), "block {k} moved at delta {i}");
        }
        if i % 40 == 0 {
            kept = blocks
                .iter()
                .enumerate()
                .take(stats.settled)
                .filter_map(|(k, b)| Some((k, (*b)?)))
                .collect();
        }
    }
    let stats = o.draft.read_with(o.cx, |d, _| d.stats());
    assert!(
        stats.settled > 30,
        "a long reply settles into many blocks: {stats:?}"
    );
    assert_eq!(o.draft.read_with(o.cx, |d, _| d.text().to_string()), REPLY);
}

#[test]
fn the_open_block_shows_as_it_will_end_up() {
    let mut app = app();
    let mut o = open(&mut app, 560.);
    let c = StreamingMarkdown::CARET;
    // Nothing yet: the caret alone.
    assert_eq!(shown(&mut o), c);
    for (text, want) in [
        ("Some **bold", format!("Some **bold{c}**")),
        (
            " words** and `co",
            format!("Some **bold words** and `co{c}`"),
        ),
        (
            "de` and [a li",
            format!("Some **bold words** and `code` and a li{c}"),
        ),
        (
            "nk](https://exa",
            format!("Some **bold words** and `code` and a link{c}"),
        ),
        (
            "mple.com).\n\n1.",
            format!("Some **bold words** and `code` and [a link](https://example.com).{c}"),
        ),
    ] {
        append(&mut o, text);
        assert_eq!(shown(&mut o), want);
    }
    // The paragraph settled once the list's first line came whole.
    append(&mut o, " one\n```rust\nfn main() {\n");
    let tail = shown(&mut o);
    assert!(
        tail.ends_with(&format!("```rust\nfn main() {{{c}")),
        "{tail:?}"
    );
    // An open fence is code, Markdown and all.
    append(&mut o, "  let x = **y;\n");
    assert!(shown(&mut o).ends_with(&format!("let x = **y;{c}")));
}

#[test]
fn the_same_message_moves_nothing() {
    let mut app = app();
    let mut o = open(&mut app, 520.);
    for delta in deltas(REPLY, 29) {
        append(&mut o, &delta);
    }
    let stats = o.draft.read_with(o.cx, |d, _| d.stats());
    let keys = stats.settled + 1;
    let before = bounds(o.cx, keys);
    let d = o.draft.clone();
    o.cx.update(|_, cx| d.update(cx, |d, cx| d.finish(REPLY, cx)));
    draw(o.cx);
    let after = bounds(o.cx, keys);
    assert_eq!(before, after, "every block where it was");
    let now = o.draft.read_with(o.cx, |d, _| d.stats());
    assert_eq!(
        now.made, stats.made,
        "nothing laid out again but the caret's block"
    );
    assert!(!o.draft.read_with(o.cx, |d, _| d.is_writing()));
    assert!(!shown(&mut o).contains(StreamingMarkdown::CARET));
    // `end` alone does the same.
    let mut app = self::app();
    let mut o = open(&mut app, 520.);
    for delta in deltas(REPLY, 29) {
        append(&mut o, &delta);
    }
    let before = bounds(o.cx, keys);
    let d = o.draft.clone();
    o.cx.update(|_, cx| d.update(cx, |d, cx| d.end(cx)));
    draw(o.cx);
    assert_eq!(before, bounds(o.cx, keys));
    // Deltas after the end are ignored.
    append(&mut o, "more");
    assert_eq!(o.draft.read_with(o.cx, |d, _| d.text().len()), REPLY.len());
}

#[test]
fn blocks_far_off_screen_are_kept_as_their_space_while_written() {
    let mut app = app();
    let mut o = open_in(&mut app, 520., 700.);
    for delta in deltas(REPLY, 37) {
        append(&mut o, &delta);
    }
    let stats = o.draft.read_with(o.cx, |d, _| d.stats());
    assert!(
        stats.kept_as_space > 5,
        "the blocks far below the window: {stats:?}"
    );
    let keys = stats.settled + 1;
    let before = bounds(o.cx, keys);
    // Ended, every block is text again, each where its space was.
    let d = o.draft.clone();
    o.cx.update(|_, cx| d.update(cx, |d, cx| d.end(cx)));
    draw(o.cx);
    assert_eq!(o.draft.read_with(o.cx, |d, _| d.stats().kept_as_space), 0);
    assert_eq!(before, bounds(o.cx, keys));
}

#[test]
fn a_shorter_or_tidied_message_keeps_what_it_shares() {
    // The first of a reply's messages ends at a block's end, and the rest follow as messages
    // of their own; or the message is cut mid-paragraph.
    let cut_at_block = REPLY.find("\n## Step 1").unwrap();
    let mid = REPLY.find("Make a config for rclone").unwrap() + 20;
    for cut in [cut_at_block, mid] {
        let mut app = app();
        let mut o = open(&mut app, 520.);
        for delta in deltas(REPLY, 31) {
            append(&mut o, &delta);
        }
        let stats = o.draft.read_with(o.cx, |d, _| d.stats());
        let keys = stats.settled + 1;
        let (draft_before, before) = bounds(o.cx, keys);
        let first = REPLY[..cut].trim_end();
        let d = o.draft.clone();
        o.cx.update(|_, cx| d.update(cx, |d, cx| d.finish(first, cx)));
        draw(o.cx);
        let (draft_after, after) = bounds(o.cx, keys);
        assert_eq!(o.draft.read_with(o.cx, |d, _| d.text().to_string()), first);
        // Every block wholly before the cut is where it was, laid out as it was.
        let kept = before
            .iter()
            .zip(&after)
            .take_while(|(b, a)| b.is_some() && b == a)
            .count();
        let now = o.draft.read_with(o.cx, |d, _| d.stats());
        assert!(
            now.settled + 1 >= kept && kept + 2 >= now.settled,
            "kept {kept} of {now:?}"
        );
        assert!(
            now.made <= stats.made + 1,
            "at most the cut block made again: {now:?}"
        );
        assert!(draft_after.size.height < draft_before.size.height);
        assert_eq!(draft_after.origin, draft_before.origin);
    }
    // A message tidied in the middle: the blocks before the change stay.
    let mut app = app();
    let mut o = open(&mut app, 520.);
    for delta in deltas(REPLY, 31) {
        append(&mut o, &delta);
    }
    let stats = o.draft.read_with(o.cx, |d, _| d.stats());
    let tidied = REPLY.replace("Step 3: the job itself", "Step 3: the job");
    let changed_block = {
        let at = REPLY.find("Step 3: the job itself").unwrap();
        REPLY[..at].matches("\n\n").count()
    };
    let (_, before) = bounds(o.cx, stats.settled + 1);
    let d = o.draft.clone();
    o.cx.update(|_, cx| d.update(cx, |d, cx| d.finish(&tidied, cx)));
    draw(o.cx);
    let (_, after) = bounds(o.cx, stats.settled + 1);
    let same = before
        .iter()
        .zip(&after)
        .take_while(|(b, a)| b.is_some() && b == a)
        .count();
    assert!(
        same + 3 >= changed_block.min(stats.settled),
        "{same} kept, change near {changed_block}"
    );
    assert!(same > 10);
}

#[test]
fn a_card_takes_the_draft_s_place_drawn_whole() {
    let mut app = app();
    let mut o = open(&mut app, 520.);
    for delta in deltas(
        "Here's the week at a glance. Pick a day and I'll book it.",
        5,
    ) {
        append(&mut o, &delta);
    }
    let top = o.cx.debug_bounds("d:draft").unwrap().origin;
    // The message comes with a card: in one update the host draws it where the draft was.
    let host = common::TestHost::new(common::Images::Refuse);
    let chat = o.chat.clone();
    o.cx.update(|window, cx| {
        chat.update(cx, |c, cx| {
            let h: Rc<dyn gpui_chatwidgets::CardHost> = host.clone();
            let src = r#"root = Stack([CardHeader("Your week"), Buttons([Button("Book Tuesday"), Button("Book Thursday")])])"#;
            c.card = Some(cx.new(|cx| gpui_chatwidgets::CardView::new(src, "c", h, window, cx)));
            cx.notify();
        })
    });
    // One frame: the card whole, where the draft was, and no draft.
    o.cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    let card =
        o.cx.debug_bounds("c:card")
            .expect("the card is drawn in the first frame");
    assert_eq!(card.origin, top);
    assert!(card.size.height > px(40.));
    assert!(o.cx.debug_bounds("d:draft").is_none());
    let c = o.chat.read_with(o.cx, |c, _| c.card.clone().unwrap());
    assert!(c.read_with(o.cx, |c, _| c.error().is_none()));
}

#[test]
fn a_reconnect_replaces_the_draft_in_place() {
    let mut app = app();
    let mut o = open(&mut app, 520.);
    let half = REPLY.len() / 2;
    let half = (0..=half)
        .rev()
        .find(|i| REPLY.is_char_boundary(*i))
        .unwrap();
    for delta in deltas(&REPLY[..half], 13) {
        append(&mut o, &delta);
    }
    let stats = o.draft.read_with(o.cx, |d, _| d.stats());
    // The whole draft so far, a little longer: nothing settled is made again.
    let more = &REPLY[..half + 200];
    let d = o.draft.clone();
    o.cx.update(|_, cx| d.update(cx, |d, cx| d.replace(more, cx)));
    draw(o.cx);
    let now = o.draft.read_with(o.cx, |d, _| d.stats());
    assert_eq!(o.draft.read_with(o.cx, |d, _| d.text().to_string()), more);
    assert!(now.settled >= stats.settled);
    assert_eq!(now.made - stats.made, now.settled - stats.settled);
}

#[test]
fn nothing_draws_while_no_text_comes() {
    let mut app = app();
    let mut o = open(&mut app, 520.);
    for delta in deltas(&REPLY[..2000], 5) {
        append(&mut o, &delta);
    }
    for writing in [true, false] {
        if !writing {
            let d = o.draft.clone();
            o.cx.update(|_, cx| d.update(cx, |d, cx| d.end(cx)));
            draw(o.cx);
        }
        let reads = o.draft.read_with(o.cx, |d, _| d.stats().tail_reads);
        o.cx.executor().advance_clock(Duration::from_secs(10));
        o.cx.run_until_parked();
        assert_eq!(
            o.cx.update(|window, cx| window.simulate_next_frame(cx)),
            0,
            "no frames asked for (writing: {writing})"
        );
        assert_eq!(
            o.draft.read_with(o.cx, |d, _| d.stats().tail_reads),
            reads,
            "nothing read again"
        );
    }
}

#[test]
fn a_link_goes_to_the_host() {
    let mut app = app();
    let mut o = open(&mut app, 520.);
    append(
        &mut o,
        "See [the docs](https://x.test/docs) first.\n\nThen the rest",
    );
    let at = o.cx.debug_bounds("d:md0").expect("the first block settled");
    // The link's words, a little way in.
    let x = at.origin.x + px(40.);
    o.cx.simulate_click(point(x, at.origin.y + px(8.)), Modifiers::none());
    draw(o.cx);
    assert_eq!(o.links.borrow().as_slice(), ["https://x.test/docs"]);
}

/// Markdown an agent (or whoever got into its context) could write, cut anywhere.
fn hostile() -> String {
    let mut s = String::new();
    s.push_str(&">".repeat(400));
    s.push_str(" deep\n\n");
    s.push_str(&"- ".repeat(200));
    s.push_str("nested\n\n");
    s.push_str(&"*".repeat(300));
    s.push_str("stars\n\n**open bold [open link](https://x.test/");
    s.push_str(&"a".repeat(3000));
    s.push_str("\n\n| a | b |\n|---|---|\n");
    for i in 0..120 {
        s.push_str(&format!("| {i} | {} |\n", "x".repeat(i % 40)));
    }
    s.push_str("\n```\n");
    s.push_str(&"unclosed code ".repeat(150));
    s.push_str("\n\n<script>alert(1)</script> ![img](javascript:alert(1)) `` ` ``` ~~~\n");
    s.push_str("👩‍👩‍👧‍👦🇫🇷 é\u{301} 職務経歴書 يرجى\n");
    s
}

#[test]
fn hostile_text_streamed_a_character_at_a_time_never_panics() {
    let text = hostile();
    let mut app = app();
    let o = open(&mut app, 280.);
    let d = o.draft.clone();
    // Every character its own delta, which cuts graphemes too (a family emoji's joined
    // people, a flag's two letters, an accent after its letter): a frame every 41 of them, and
    // a frame after each that isn't ASCII.
    for (i, c) in text.chars().enumerate() {
        let delta = c.to_string();
        o.cx.update(|_, cx| d.update(cx, |d, cx| d.append(&delta, cx)));
        if i % 41 == 0 || !c.is_ascii() {
            draw(o.cx);
        }
    }
    draw(o.cx);
    o.cx.update(|_, cx| d.update(cx, |d, cx| d.finish(&text, cx)));
    draw(o.cx);
    assert_eq!(o.draft.read_with(o.cx, |d, _| d.text().to_string()), text);
}

/// Every glyph drawn past the draft's sides or the window, mid-stream.
fn overflows(cx: &mut VisualTestContext) -> Vec<String> {
    let draft = cx.debug_bounds("d:draft").expect("the draft is drawn");
    cx.update(|window, _| {
        let scale = window.scale_factor();
        let (l, r) = (
            f32::from(draft.origin.x) * scale,
            f32::from(draft.right()) * scale,
        );
        let slack = 2. * scale;
        let scene = window.rendered_scene();
        scene
            .monochrome_sprites
            .iter()
            .map(|s| &s.bounds)
            .chain(scene.subpixel_sprites.iter().map(|s| &s.bounds))
            .filter(|b| {
                let (x0, x1) = (
                    b.origin.x.as_f32(),
                    b.origin.x.as_f32() + b.size.width.as_f32(),
                );
                x0 < l - slack || x1 > r + slack
            })
            .map(|b| format!("{:?}", b.origin))
            .collect()
    })
}

#[test]
fn nothing_runs_out_of_a_narrow_draft_mid_stream() {
    let word = "Pneumonoultramicroscopicsilicovolcanoconiosis-Antidisestablishmentarianism";
    let worst = format!(
        "# {word}\n\nA **{word}** and `{word}` and [{word}](https://x.test/{word}) 職務経歴書と志望動機を来週の月曜日までに提出してください。🎓📚\n\n- {word}\n- https://example.com/{}\n\n| {word} | b |\n|---|---|\n| {word} | 1 |\n\n```\n{word}{word}\n",
        "x".repeat(200)
    );
    for width in [280., 400.] {
        let mut app = app();
        let mut o = open(&mut app, width);
        let mut found = Vec::new();
        for delta in deltas(&worst, 23) {
            append(&mut o, &delta);
            found.extend(overflows(o.cx));
        }
        assert!(
            found.is_empty(),
            "{width}: {} glyphs out: {:?}",
            found.len(),
            &found[..found.len().min(10)]
        );
    }
}
