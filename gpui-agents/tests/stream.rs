//! A reply in Markdown drawn as it streams in ([`StreamingMarkdown`]): blocks laid out once and
//! kept, the open block closed for now, the caret, its height while it's written, a selection
//! across blocks, and what happens when it ends or its message replaces it. Laid out with Core
//! Text and the bundled Inter, as on screen.

#![cfg(all(target_os = "macos", feature = "gpui"))]

mod common;

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use ::gpui::TestDispatcher;
use common::{draw, setup};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_agents::StreamingMarkdown;
use gpui_core::text::{TextView, TextViewStyle};
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

/// The text colour around the draft: the caret's.
const TEXT: u32 = 0xeeeeee;

/// A chat's foot: the draft at a width, in the text style around it, or the card its message
/// brought in its place.
struct Chat {
    draft: Entity<StreamingMarkdown>,
    card: Option<Entity<gpui_agents::CardView>>,
    width: Pixels,
    /// Some text laid out below the draft as one TextView, in the same style, and not painted:
    /// to measure how tall it is.
    probe: Option<SharedString>,
    /// Some text drawn in the draft's place as one TextView, as a message of that text is.
    whole: Option<SharedString>,
    /// The links pressed, in the draft or in `whole`.
    links: Rc<RefCell<Vec<String>>>,
}

impl Render for Chat {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let links = self.links.clone();
        let shown = match (&self.card, &self.whole) {
            (Some(card), _) => card.clone().into_any_element(),
            (None, Some(text)) => div()
                .debug_selector(|| "whole".into())
                .child(
                    TextView::markdown("whole", text.clone())
                        .style(style())
                        .selectable(true)
                        .on_link_click(move |url, _, _, _| {
                            links.borrow_mut().push(url.to_string())
                        }),
                )
                .into_any_element(),
            (None, None) => self.draft.clone().into_any_element(),
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_start()
            .font_family("Inter")
            .text_size(px(14.))
            .line_height(px(14. * 1.625))
            .text_color(rgb(TEXT))
            .child(div().flex_none().w(self.width).child(shown))
            .when_some(self.probe.clone(), |d, text| {
                d.child(
                    div()
                        .debug_selector(|| "probe".into())
                        .invisible()
                        .flex_none()
                        .w(self.width)
                        .child(TextView::markdown("probe", text).style(style())),
                )
            })
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
    let (m, l, l2) = (made.clone(), links.clone(), links.clone());
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
            probe: None,
            whole: None,
            links: l2,
        });
        *m.borrow_mut() = Some((draft, chat.clone()));
        gpui_core::Root::new(chat, window, cx)
    });
    cx.simulate_resize(size(px(1000.), px(height)));
    cx.update(|window, _| window.activate_window());
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

/// A glyph drawn: its bounds in pixels, a hundredth of a pixel at a time.
type Glyph = [i64; 4];

/// Every glyph drawn last frame, in order.
fn glyphs(cx: &mut VisualTestContext) -> Vec<Glyph> {
    cx.update(|window, _| {
        let k = window.scale_factor();
        let at = |v: ScaledPixels| (v.as_f32() / k * 100.).round() as i64;
        let scene = window.rendered_scene();
        let mut all: Vec<Glyph> = scene
            .monochrome_sprites
            .iter()
            .map(|s| s.bounds)
            .chain(scene.subpixel_sprites.iter().map(|s| s.bounds))
            .map(|b| {
                [
                    at(b.origin.x),
                    at(b.origin.y),
                    at(b.size.width),
                    at(b.size.height),
                ]
            })
            .collect();
        all.sort_by_key(|g| (g[1], g[0]));
        all
    })
}

/// How tall `text` is, drawn as one TextView at the draft's width.
fn height_of(o: &mut Opened, text: &str) -> Pixels {
    let chat = o.chat.clone();
    let text = SharedString::from(text.to_string());
    o.cx.update(|_, cx| {
        chat.update(cx, |c, cx| {
            c.probe = Some(text);
            cx.notify();
        })
    });
    draw(o.cx);
    let h = o.cx.debug_bounds("probe").expect("the probe").size.height;
    o.cx.update(|_, cx| {
        chat.update(cx, |c, cx| {
            c.probe = None;
            cx.notify();
        })
    });
    draw(o.cx);
    h
}

/// The glyphs of the settled blocks: those above where the settled text ends.
fn settled_glyphs(o: &mut Opened) -> Vec<Glyph> {
    let settled = o.draft.read_with(o.cx, |d, _| d.settled_text().to_string());
    let h = height_of(o, &settled);
    let top = o.cx.debug_bounds("d:draft").expect("the draft").top();
    let below = ((f32::from(top) + f32::from(h)) * 100.).round() as i64;
    glyphs(o.cx).into_iter().filter(|g| g[1] < below).collect()
}

/// The glyphs of `kept` that aren't in `now`.
fn missing(kept: &[Glyph], now: &[Glyph]) -> Vec<Glyph> {
    let now: HashSet<&Glyph> = now.iter().collect();
    kept.iter().filter(|g| !now.contains(g)).copied().collect()
}

/// The caret: the quads drawn in the text's colour.
fn carets(cx: &mut VisualTestContext) -> Vec<Bounds<Pixels>> {
    let colour: Hsla = rgb(TEXT).into();
    cx.update(|window, _| {
        let k = window.scale_factor();
        window
            .rendered_scene()
            .quads
            .iter()
            .filter(|q| q.background.as_solid() == Some(colour))
            .map(|q| {
                Bounds::new(
                    point(
                        px(q.bounds.origin.x.as_f32() / k),
                        px(q.bounds.origin.y.as_f32() / k),
                    ),
                    size(
                        px(q.bounds.size.width.as_f32() / k),
                        px(q.bounds.size.height.as_f32() / k),
                    ),
                )
            })
            .collect()
    })
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
    let mut kept: Vec<Glyph> = Vec::new();
    for (i, delta) in deltas(REPLY, 7).iter().enumerate() {
        append(&mut o, delta);
        let stats = o.draft.read_with(o.cx, |d, _| d.stats());
        // Each block is made once, when it settles, and never again.
        assert_eq!(stats.made, stats.settled, "at delta {i}");
        assert!(stats.settled >= last_settled, "blocks only settle");
        last_settled = stats.settled;
        let draft = o.cx.debug_bounds("d:draft").expect("the draft is drawn");
        // The draft only grows while it's written.
        assert!(
            draft.size.height >= tallest,
            "shrank at delta {i}: {:?} < {tallest:?}",
            draft.size.height
        );
        tallest = draft.size.height;
        // A settled block's text doesn't move as the text after it grows.
        let gone = missing(&kept, &glyphs(o.cx));
        assert!(gone.is_empty(), "{} glyphs moved at delta {i}", gone.len());
        if i % 40 == 0 {
            kept = settled_glyphs(&mut o);
        }
    }
    let stats = o.draft.read_with(o.cx, |d, _| d.stats());
    assert!(
        stats.settled > 30,
        "a long reply settles into many blocks: {stats:?}"
    );
    assert!(kept.len() > 2000, "{} glyphs kept", kept.len());
    assert_eq!(o.draft.read_with(o.cx, |d, _| d.text().to_string()), REPLY);
}

#[test]
fn the_open_block_shows_as_it_will_end_up() {
    let mut app = app();
    let mut o = open(&mut app, 560.);
    // Nothing yet: the caret alone.
    assert_eq!(shown(&mut o), "");
    assert_eq!(carets(o.cx).len(), 1);
    for (text, want) in [
        ("Some **bold", "Some **bold**"),
        (" words** and `co", "Some **bold words** and `co`"),
        ("de` and [a li", "Some **bold words** and `code` and a li"),
        (
            "nk](https://exa",
            "Some **bold words** and `code` and a link",
        ),
        (
            "mple.com).\n\n1.",
            "Some **bold words** and `code` and [a link](https://example.com).",
        ),
    ] {
        append(&mut o, text);
        assert_eq!(shown(&mut o), want);
    }
    // The paragraph settled once the list's first line came whole.
    append(&mut o, " one\n```rust\nfn main() {\n");
    let tail = shown(&mut o);
    assert!(tail.ends_with("```rust\nfn main() {"), "{tail:?}");
    // An open fence is code, Markdown and all.
    append(&mut o, "  let x = **y;\n");
    assert!(shown(&mut o).ends_with("let x = **y;"));
    assert_eq!(carets(o.cx).len(), 1);
}

#[test]
fn the_same_message_moves_nothing() {
    let mut app = app();
    let mut o = open(&mut app, 520.);
    for delta in deltas(REPLY, 29) {
        append(&mut o, &delta);
    }
    let stats = o.draft.read_with(o.cx, |d, _| d.stats());
    let before = (o.cx.debug_bounds("d:draft"), glyphs(o.cx));
    let d = o.draft.clone();
    o.cx.update(|_, cx| d.update(cx, |d, cx| d.finish(REPLY, cx)));
    draw(o.cx);
    let after = (o.cx.debug_bounds("d:draft"), glyphs(o.cx));
    assert_eq!(before, after, "every glyph where it was");
    let now = o.draft.read_with(o.cx, |d, _| d.stats());
    assert_eq!(now.made, stats.made, "no block made again");
    assert!(!o.draft.read_with(o.cx, |d, _| d.is_writing()));
    assert!(carets(o.cx).is_empty());
    // `end` alone does the same.
    let mut app = self::app();
    let mut o = open(&mut app, 520.);
    for delta in deltas(REPLY, 29) {
        append(&mut o, &delta);
    }
    let before = (o.cx.debug_bounds("d:draft"), glyphs(o.cx));
    let d = o.draft.clone();
    o.cx.update(|_, cx| d.update(cx, |d, cx| d.end(cx)));
    draw(o.cx);
    assert_eq!(before, (o.cx.debug_bounds("d:draft"), glyphs(o.cx)));
    // Deltas after the end are ignored.
    append(&mut o, "more");
    assert_eq!(o.draft.read_with(o.cx, |d, _| d.text().len()), REPLY.len());
}

#[test]
fn blocks_off_screen_are_laid_out_but_not_painted() {
    let mut app = app();
    let mut o = open_in(&mut app, 520., 700.);
    for delta in deltas(REPLY, 37) {
        append(&mut o, &delta);
    }
    let draft = o.cx.debug_bounds("d:draft").unwrap();
    assert!(
        draft.size.height > px(2000.),
        "most of the reply is below the window: {draft:?}"
    );
    // Only what's in the window is painted.
    let drawn = glyphs(o.cx);
    assert!(!drawn.is_empty());
    assert!(
        drawn.iter().all(|g| g[1] < 700 * 100),
        "a glyph painted below the window"
    );
    // Ended, every block is where it was.
    let d = o.draft.clone();
    o.cx.update(|_, cx| d.update(cx, |d, cx| d.end(cx)));
    draw(o.cx);
    assert_eq!(o.cx.debug_bounds("d:draft"), Some(draft));
    assert_eq!(glyphs(o.cx), drawn);
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
        let draft_before = o.cx.debug_bounds("d:draft").unwrap();
        let before = glyphs(o.cx);
        let first = REPLY[..cut].trim_end();
        let d = o.draft.clone();
        o.cx.update(|_, cx| d.update(cx, |d, cx| d.finish(first, cx)));
        draw(o.cx);
        let draft_after = o.cx.debug_bounds("d:draft").unwrap();
        assert_eq!(o.draft.read_with(o.cx, |d, _| d.text().to_string()), first);
        // Every block before the cut is where it was, laid out as it was; only the cut block
        // is open.
        let kept = settled_glyphs(&mut o);
        assert!(missing(&kept, &before).is_empty(), "a kept block moved");
        let open = shown(&mut o);
        assert!(open.len() < 800, "only the cut block is open: {open:?}");
        let now = o.draft.read_with(o.cx, |d, _| d.stats());
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
    let tidied = REPLY.replace("Step 3: the job itself", "Step 3: the job");
    let at = REPLY.find("Step 3: the job itself").unwrap();
    let before = glyphs(o.cx);
    let d = o.draft.clone();
    o.cx.update(|_, cx| d.update(cx, |d, cx| d.finish(&tidied, cx)));
    draw(o.cx);
    let kept = settled_glyphs(&mut o);
    assert!(missing(&kept, &before).is_empty(), "a kept block moved");
    let settled = o.draft.read_with(o.cx, |d, _| d.settled_text().len());
    assert!(
        settled + 800 >= at && settled <= at,
        "kept up to {settled}, the change at {at}"
    );
    assert!(o.draft.read_with(o.cx, |d, _| d.stats().settled) > 10);
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
            let h: Rc<dyn gpui_agents::CardHost> = host.clone();
            let src = r#"root = Stack([CardHeader("Your week"), Buttons([Button("Book Tuesday"), Button("Book Thursday")])])"#;
            c.card = Some(cx.new(|cx| gpui_agents::CardView::new(src, "c", h, window, cx)));
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
    let at = o.cx.debug_bounds("d:draft").expect("the draft is drawn");
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

fn drag(o: &mut Opened, from: Point<Pixels>, to: Point<Pixels>) {
    o.cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: from,
        modifiers: Modifiers::none(),
        click_count: 1,
        first_mouse: false,
    });
    o.cx.simulate_event(MouseMoveEvent {
        position: to,
        pressed_button: Some(MouseButton::Left),
        modifiers: Modifiers::none(),
    });
    o.cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: to,
        modifiers: Modifiers::none(),
        click_count: 1,
    });
    draw(o.cx);
}

/// What Cmd-C copies, with the clipboard emptied first.
fn copied(o: &mut Opened) -> String {
    o.cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string(String::new())));
    o.cx.simulate_keystrokes("cmd-c");
    o.cx.update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()))
        .unwrap_or_default()
}

#[test]
fn a_selection_across_settled_blocks_stays_while_the_reply_streams() {
    let mut app = app();
    let mut o = open(&mut app, 520.);
    for delta in deltas(
        "First block.\n\nSecond block.\n\nThird, being written.\n\nFour",
        5,
    ) {
        append(&mut o, &delta);
    }
    assert_eq!(o.draft.read_with(o.cx, |d, _| d.stats().settled), 2);
    let top = o.cx.debug_bounds("d:draft").unwrap();
    let first = height_of(&mut o, "First block.");
    let both = height_of(&mut o, "First block.\n\nSecond block.");
    // From the first block's start into the second block's words.
    drag(
        &mut o,
        point(top.left() + px(1.), top.top() + first / 2.),
        point(top.left() + px(30.), top.top() + both - px(4.)),
    );
    let picked = copied(&mut o);
    assert!(picked.starts_with("First block.\n\nS"), "{picked:?}");
    assert!(
        "First block.\n\nSecond block.".starts_with(&picked),
        "{picked:?}"
    );
    // The reply goes on: the third block settles, a fourth streams in, and the selection
    // is still there to copy.
    for delta in deltas("th, a list:\n\n- a\n- b\n\nAnd more text", 3) {
        append(&mut o, &delta);
    }
    assert_eq!(o.draft.read_with(o.cx, |d, _| d.stats().settled), 4);
    assert_eq!(copied(&mut o), picked);
    // Select all reaches every block, the open one too.
    o.cx.simulate_keystrokes("cmd-a");
    assert_eq!(
        copied(&mut o),
        "First block.\n\nSecond block.\n\nThird, being written.\n\nFourth, a list:\n\na\nb\n\nAnd more text"
    );
}

#[test]
fn the_caret_follows_the_tail_and_never_moves_the_text() {
    let mut app = app();
    let mut o = open(&mut app, 520.);
    for delta in deltas("A settled paragraph.\n\nThe tail, being written", 4) {
        append(&mut o, &delta);
    }
    let draft = o.cx.debug_bounds("d:draft").unwrap();
    let drawn = glyphs(o.cx);
    let c = carets(o.cx);
    assert_eq!(c.len(), 1, "{c:?}");
    let c = c[0];
    // After the tail's last glyph, on its line.
    let (top, bottom) = (
        (f32::from(c.top()) * 100.) as i64,
        (f32::from(c.bottom()) * 100.) as i64,
    );
    let right = drawn
        .iter()
        .filter(|g| g[1] + g[3] / 2 >= top && g[1] + g[3] / 2 <= bottom)
        .map(|g| g[0] + g[2])
        .max()
        .expect("glyphs on the caret's line");
    let x = (f32::from(c.left()) * 100.) as i64;
    assert!(x >= right - 100 && x <= right + 400, "{c:?} after {right}");
    assert!(c.top() > draft.top() + px(20.), "on the tail's line");
    // Ended: the caret goes, and nothing moves.
    let d = o.draft.clone();
    o.cx.update(|_, cx| d.update(cx, |d, cx| d.end(cx)));
    draw(o.cx);
    assert!(carets(o.cx).is_empty());
    assert_eq!(o.cx.debug_bounds("d:draft"), Some(draft));
    assert_eq!(glyphs(o.cx), drawn);
}

#[test]
fn a_link_defined_further_down_resolves_while_the_reply_streams() {
    let mut app = app();
    let mut o = open(&mut app, 520.);
    let press = |o: &mut Opened| {
        let at = o.cx.debug_bounds("d:draft").unwrap();
        let mut x = at.left() + px(2.);
        while o.links.borrow().is_empty() && x < at.left() + px(200.) {
            o.cx.simulate_click(point(x, at.top() + px(8.)), Modifiers::none());
            x += px(6.);
        }
        draw(o.cx);
    };
    for delta in deltas("See [the docs][d] first.\n\nThen more text.\n\n", 5) {
        append(&mut o, &delta);
    }
    assert!(o.draft.read_with(o.cx, |d, _| d.stats().settled) >= 1);
    // No definition yet: the words as written, and no link.
    press(&mut o);
    assert!(o.links.borrow().is_empty());
    for delta in deltas("[d]: https://x.test/docs\n\nThe end", 5) {
        append(&mut o, &delta);
    }
    // The settled first block's link goes to the definition, while the reply is written.
    assert!(o.draft.read_with(o.cx, |d, _| d.is_writing()));
    press(&mut o);
    assert_eq!(o.links.borrow().as_slice(), ["https://x.test/docs"]);
}

/// Draws `text` whole as one TextView in the draft's place, as a message of it is.
fn show_whole(o: &mut Opened, text: &str) {
    let chat = o.chat.clone();
    let text = SharedString::from(text.to_string());
    o.cx.update(|_, cx| {
        chat.update(cx, |c, cx| {
            c.whole = Some(text);
            cx.notify();
        })
    });
    draw(o.cx);
}

/// All the text what's marked `what` draws, selected whole and copied.
fn all_text(o: &mut Opened, what: &'static str) -> String {
    let at = o.cx.debug_bounds(what).expect("drawn");
    o.cx.simulate_click(
        point(at.left() + px(2.), at.top() + px(4.)),
        Modifiers::none(),
    );
    o.cx.simulate_keystrokes("cmd-a");
    copied(o)
}

/// The first link pressed along the line `y` below the top of what's marked `what`, from its
/// left.
fn press_along(o: &mut Opened, what: &'static str, y: Pixels) -> Option<String> {
    o.links.borrow_mut().clear();
    let at = o.cx.debug_bounds(what).expect("drawn");
    let mut x = at.left() + px(2.);
    while o.links.borrow().is_empty() && x < at.left() + px(300.) {
        o.cx.simulate_click(point(x, at.top() + y), Modifiers::none());
        x += px(6.);
    }
    draw(o.cx);
    o.links.borrow().first().cloned()
}

#[test]
fn a_footnote_defined_in_another_block_resolves_and_the_blocks_stay() {
    // Each note defined in a block of its own, away from its reference: one before it, and one
    // at the end, still open when the draft ends.
    const TEXT: &str = "A claim.[^1]\n\n[^2]: An earlier note.\n\nSome text between.\n\n\
                        A second claim.[^2]\n\nThe end.\n\n[^1]: The source.\n";
    let mut app = app();
    let mut o = open(&mut app, 520.);
    for delta in deltas(TEXT, 5) {
        append(&mut o, &delta);
    }
    let settled = o.draft.read_with(o.cx, |d, _| d.settled_text().to_string());
    assert_eq!(
        settled,
        "A claim.[^1]\n\n[^2]: An earlier note.\n\nSome text between.\n\nA second claim.[^2]\n\n\
         The end.\n\n"
    );
    let stats = o.draft.read_with(o.cx, |d, _| d.stats());
    assert_eq!(stats.settled, 5, "{stats:?}");
    let kept = settled_glyphs(&mut o);
    assert!(!kept.is_empty());
    let d = o.draft.clone();
    o.cx.update(|_, cx| d.update(cx, |d, cx| d.end(cx)));
    draw(o.cx);
    // Ended, the settled blocks stay as they were: none read or made again (only the open tail
    // is read, a last time), and each laid out where it was.
    let now = o.draft.read_with(o.cx, |d, _| d.stats());
    assert_eq!(
        (now.settled, now.made, now.reread),
        (stats.settled, stats.made, stats.reread),
        "{now:?}"
    );
    assert_eq!(
        o.draft.read_with(o.cx, |d, _| d.settled_text().to_string()),
        settled
    );
    let draft = o.cx.debug_bounds("d:draft").unwrap();
    let drawn = glyphs(o.cx);
    assert!(missing(&kept, &drawn).is_empty(), "a settled block moved");
    // Each reference is its note's label, and each note is drawn once, where it's written.
    let text = all_text(&mut o, "d:draft");
    assert_eq!(text.matches("The source.").count(), 1, "{text:?}");
    assert_eq!(text.matches("An earlier note.").count(), 1, "{text:?}");
    assert!(
        text.contains("A claim.[1]") && text.contains("A second claim.[2]"),
        "{text:?}"
    );
    assert!(!text.contains("[^"), "{text:?}");
    // The same as a message of the text: drawn whole as one TextView.
    show_whole(&mut o, TEXT);
    assert_eq!(o.cx.debug_bounds("whole"), Some(draft));
    assert_eq!(glyphs(o.cx), drawn);
    assert_eq!(all_text(&mut o, "whole"), text);
}

#[test]
fn of_a_link_defined_twice_in_different_blocks_the_first_wins() {
    // The first definition with the first reference, and the second with a reference of its
    // own, in a later block: in one document the first wins, for both.
    const BEFORE: &str =
        "See [the docs][d] first.\n\n[d]: https://first.test\n\nSome text between.";
    const TEXT: &str = "See [the docs][d] first.\n\n[d]: https://first.test\n\nSome text between.\n\n\
                        Again, [the docs][d].\n\n[d]: https://second.test\n\nThe end.\n";
    let mut app = app();
    let mut o = open(&mut app, 520.);
    for delta in deltas(TEXT, 5) {
        append(&mut o, &delta);
    }
    let settled = o.draft.read_with(o.cx, |d, _| d.settled_text().to_string());
    // The two definitions are in different blocks.
    assert!(
        settled.starts_with(&format!("{BEFORE}\n\n"))
            && settled.ends_with("[d]: https://second.test\n\n"),
        "{settled:?}"
    );
    assert_eq!(o.draft.read_with(o.cx, |d, _| d.stats().settled), 3);
    // The second reference's line: the last of the text up to it.
    let again = height_of(&mut o, &format!("{BEFORE}\n\nAgain, [the docs][d].")) - px(10.);
    let first = Some("https://first.test".to_string());
    // While it's written, and once it has ended.
    assert_eq!(press_along(&mut o, "d:draft", px(8.)), first);
    assert_eq!(press_along(&mut o, "d:draft", again), first);
    let d = o.draft.clone();
    o.cx.update(|_, cx| d.update(cx, |d, cx| d.end(cx)));
    draw(o.cx);
    assert_eq!(press_along(&mut o, "d:draft", px(8.)), first);
    assert_eq!(press_along(&mut o, "d:draft", again), first);
    let drawn = glyphs(o.cx);
    // As a message of the text, drawn whole as one TextView, reads it.
    show_whole(&mut o, TEXT);
    assert_eq!(glyphs(o.cx), drawn);
    assert_eq!(press_along(&mut o, "whole", px(8.)), first);
    assert_eq!(press_along(&mut o, "whole", again), first);
}

#[test]
fn an_empty_tail_keeps_the_caret_after_the_last_character() {
    let mut app = app();
    let mut o = open(&mut app, 520.);
    let caret = |o: &mut Opened| {
        let c = carets(o.cx);
        assert_eq!(c.len(), 1, "{c:?}");
        c[0]
    };
    // Nothing yet: the caret at the start, inside a draft a line tall.
    let draft = o.cx.debug_bounds("d:draft").unwrap();
    let c = caret(&mut o);
    assert_eq!(shown(&mut o), "");
    assert!(draft.size.height > px(20.), "{draft:?}");
    assert!(c.left() <= draft.left() + px(1.), "{c:?} in {draft:?}");
    assert!(
        c.top() >= draft.top() && c.bottom() <= draft.bottom(),
        "{c:?} in {draft:?}"
    );
    for delta in deltas("A settled paragraph.\n", 4) {
        append(&mut o, &delta);
    }
    assert_eq!(o.draft.read_with(o.cx, |d, _| d.stats().settled), 0);
    let before = o.cx.debug_bounds("d:draft").unwrap();
    let drawn = glyphs(o.cx);
    // A table's first row settles the paragraph and waits for its delimiter row: the open
    // tail is empty, and the caret stays after the paragraph's last character, on its line.
    append(&mut o, "\n| a | b |\n");
    assert_eq!(o.draft.read_with(o.cx, |d, _| d.stats().settled), 1);
    assert_eq!(shown(&mut o), "");
    assert_eq!(glyphs(o.cx), drawn, "nothing moved");
    let after = o.cx.debug_bounds("d:draft").unwrap();
    assert_eq!(after, before, "the height didn't change");
    let c = caret(&mut o);
    let right = drawn
        .iter()
        .map(|g| g[0] + g[2])
        .max()
        .expect("the paragraph's glyphs");
    let x = (f32::from(c.left()) * 100.) as i64;
    assert!(x >= right - 100 && x <= right + 400, "{c:?} after {right}");
    assert!(
        c.top() >= after.top() && c.bottom() <= after.bottom(),
        "{c:?} in {after:?}"
    );
    // Select All copies the text, and nothing that isn't in it.
    o.cx.simulate_click(after.origin + point(px(2.), px(8.)), Modifiers::none());
    o.cx.simulate_keystrokes("cmd-a");
    let all = copied(&mut o);
    assert_eq!(all, "A settled paragraph.");
    assert!(!all.contains('\u{200B}'), "{all:?}");
    // Nothing draws while the empty tail waits.
    o.cx.executor().advance_clock(Duration::from_secs(10));
    o.cx.run_until_parked();
    assert_eq!(o.cx.update(|window, cx| window.simulate_next_frame(cx)), 0);
}

/// The Copy buttons drawn, top to bottom: their bounds and the code block each is for (its
/// source's key and its place in it).
fn copy_buttons(o: &mut Opened, id: &str) -> Vec<(Bounds<Pixels>, (u64, usize))> {
    let mut found = Vec::new();
    for key in 0..64 {
        for index in 0..4 {
            let sel: &'static str =
                Box::leak(format!("{id}/code/{key}/{index}/copy").into_boxed_str());
            if let Some(b) = o.cx.debug_bounds(sel) {
                found.push((b, (key, index)));
            }
        }
    }
    found.sort_by(|a, b| a.0.top().partial_cmp(&b.0.top()).unwrap());
    found
}

/// What a click at `at` puts on the clipboard, with the clipboard emptied first.
fn clipped(o: &mut Opened, at: Point<Pixels>) -> String {
    o.cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string(String::new())));
    o.cx.simulate_click(at, Modifiers::none());
    draw(o.cx);
    o.cx.update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()))
        .unwrap_or_default()
}

const CODE: &str = "Run this first:\n\n```rust\nfn main() {\n    println!(\"hi\");\n}\n```\n\nThen the script:\n\n```sh\necho one\necho tw";

#[test]
fn each_code_block_has_a_copy_button_settled_or_still_being_written() {
    let mut app = app();
    let mut o = open(&mut app, 520.);
    for delta in deltas(CODE, 5) {
        append(&mut o, &delta);
    }
    // The first code block has settled; the second's fence is still open, in the tail.
    assert!(
        o.draft
            .read_with(o.cx, |d, _| d.settled_text().contains("```rust"))
    );
    assert!(shown(&mut o).contains("echo tw"));
    let buttons = copy_buttons(&mut o, "d");
    assert_eq!(buttons.len(), 2, "{buttons:?}");
    let draft = o.cx.debug_bounds("d:draft").unwrap();
    for (b, _) in &buttons {
        // In the top-right corner of its block, inside the draft.
        assert!(b.right() <= draft.right() && b.right() > draft.right() - px(24.));
    }
    // Each copies its code without the fences; the open one what's there so far.
    assert_eq!(
        clipped(&mut o, buttons[0].0.center()),
        "fn main() {\n    println!(\"hi\");\n}"
    );
    assert_eq!(clipped(&mut o, buttons[1].0.center()), "echo one\necho tw");
    // A press on the button selects nothing.
    assert_eq!(copied(&mut o), "");
    // It goes on growing; the button copies what's there now.
    append(&mut o, "o\n");
    let buttons = copy_buttons(&mut o, "d");
    assert_eq!(clipped(&mut o, buttons[1].0.center()), "echo one\necho two");
    // Once the draft has ended, both blocks keep theirs.
    let d = o.draft.clone();
    o.cx.update(|_, cx| d.update(cx, |d, cx| d.end(cx)));
    draw(o.cx);
    let buttons = copy_buttons(&mut o, "d");
    assert_eq!(buttons.len(), 2);
    assert_eq!(clipped(&mut o, buttons[1].0.center()), "echo one\necho two");
}

#[test]
fn copy_says_copied_for_a_moment_then_nothing_draws() {
    let mut app = app();
    let mut o = open(&mut app, 520.);
    append(&mut o, CODE);
    let d = o.draft.clone();
    o.cx.update(|_, cx| d.update(cx, |d, cx| d.end(cx)));
    draw(o.cx);
    let (before, _) = copy_buttons(&mut o, "d")[0];
    clipped(&mut o, before.center());
    // "Copied" is wider than "Copy", and the button keeps its corner.
    let (now, _) = copy_buttons(&mut o, "d")[0];
    assert!(now.size.width > before.size.width, "{now:?} {before:?}");
    assert_eq!(now.right(), before.right());
    o.cx.executor().advance_clock(Duration::from_millis(1000));
    draw(o.cx);
    assert_eq!(copy_buttons(&mut o, "d")[0].0, now, "still Copied");
    o.cx.executor().advance_clock(Duration::from_millis(600));
    draw(o.cx);
    assert_eq!(copy_buttons(&mut o, "d")[0].0, before, "Copy again");
    o.cx.executor().advance_clock(Duration::from_secs(10));
    o.cx.run_until_parked();
    assert_eq!(
        o.cx.update(|window, cx| window.simulate_next_frame(cx)),
        0,
        "no frames asked for"
    );
}

#[test]
fn a_hosts_copy_button_is_drawn_in_its_place_and_still_copies() {
    let mut app = app();
    let mut o = open(&mut app, 520.);
    let chat = o.chat.clone();
    let draft = o.cx.update(|_, cx| {
        let d = cx.new(|_| {
            StreamingMarkdown::new("h")
                .style(style())
                .copy_button(|copied, _, _| {
                    div()
                        .debug_selector(move || format!("host-copy:{copied}"))
                        .w(px(48.))
                        .h(px(24.))
                        .child(if copied { "Done" } else { "Take" })
                        .into_any_element()
                })
        });
        chat.update(cx, |c, cx| {
            c.draft = d.clone();
            cx.notify();
        });
        d
    });
    o.draft = draft;
    append(&mut o, CODE);
    let d = o.draft.clone();
    o.cx.update(|_, cx| d.update(cx, |d, cx| d.end(cx)));
    draw(o.cx);
    // The host's look, as the crate's button: one a block, in its corner.
    let buttons = copy_buttons(&mut o, "h");
    assert_eq!(buttons.len(), 2, "{buttons:?}");
    assert_eq!(buttons[0].0.size, size(px(48.), px(24.)));
    assert!(o.cx.debug_bounds("host-copy:false").is_some());
    // A press copies the code and tells the look it was copied.
    assert_eq!(
        clipped(&mut o, buttons[0].0.center()),
        "fn main() {\n    println!(\"hi\");\n}"
    );
    assert!(o.cx.debug_bounds("host-copy:true").is_some());
}

#[test]
fn for_a_finger_copy_takes_presses_across_44_points() {
    let mut app = app();
    let mut o = open(&mut app, 520.);
    let chat = o.chat.clone();
    let draft = o.cx.update(|_, cx| {
        let d = cx.new(|_| StreamingMarkdown::new("t").style(style()).touch(true));
        chat.update(cx, |c, cx| {
            c.draft = d.clone();
            cx.notify();
        });
        d
    });
    o.draft = draft;
    append(&mut o, CODE);
    let buttons = copy_buttons(&mut o, "t");
    assert_eq!(buttons.len(), 2);
    let b = buttons[1].0;
    assert!(b.size.height < px(44.));
    // Just above what's drawn, inside the 44 points around it.
    let above = point(b.center().x, b.center().y - px(20.));
    assert_eq!(clipped(&mut o, above), "echo one\necho tw");
}
