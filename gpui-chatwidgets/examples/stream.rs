//! A reply streaming into a chat, as a host draws it with `StreamingMarkdown`: a long Markdown
//! reply (examples/replies/long.md) written a few characters at a time, at a model's pace, then
//! ended and replaced by its message; then a short reply whose message comes with an OpenUI
//! card, which takes the draft's place.
//!
//! ```sh
//! cargo run --release --example stream
//! cargo run --release --example stream -- --cps 600 --delta 3
//! cargo run --release --features shots --example stream -- --shots /tmp/stream
//! ```
//!
//! `--cps` is characters a second (240, about 60 tokens a second, unless given) and `--delta`
//! characters a delta (6). With `--shots <dir>` it draws the reply from a window that's never
//! shown, saves a picture mid-stream and one after the message replaced the draft, then quits.

use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gpui::*;
use gpui_chatwidgets::openui::{ImageSrc, Link};
use gpui_chatwidgets::*;
use gpui_core::Theme;
use gpui_core::text::TextViewStyle;

const LONG: &str = include_str!("replies/long.md");
const SHORT: &str = "Here's the week at a glance, with the two days that still need a **decision** marked. Pick one and I'll book it.";
const CARD: &str = r#"root = Stack([CardHeader("Your week"), TextContent("Two days still need a decision."), Buttons([Button("Book Tuesday"), Button("Book Thursday", @ToAssistant("Book Thursday"), "secondary")])])"#;

struct Options {
    cps: f32,
    delta: usize,
    shots: Option<PathBuf>,
}

fn options() -> Options {
    let mut o = Options {
        cps: 240.,
        delta: 6,
        shots: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--cps" => o.cps = it.next().and_then(|v| v.parse().ok()).unwrap_or(o.cps),
            "--delta" => o.delta = it.next().and_then(|v| v.parse().ok()).unwrap_or(o.delta),
            "--shots" => o.shots = it.next().map(PathBuf::from),
            _ => {}
        }
    }
    o.cps = o.cps.max(1.);
    o.delta = o.delta.max(1);
    o
}

/// A chat app's Markdown style for replies.
fn style() -> TextViewStyle {
    let mut code = StyleRefinement::default()
        .bg(rgb(0x1c1c1f))
        .border_1()
        .border_color(rgb(0x2e2e33))
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
        heading_font_size: Some(std::sync::Arc::new(|level, base| match level {
            1 => base * 1.3,
            _ => base,
        })),
        code_block: code,
        ..Default::default()
    }
}

/// The card's host: what it hands over goes to the log.
struct Host;

impl CardHost for Host {
    fn theme(&self, _: &App) -> CardTheme {
        CardTheme {
            rem_base: 16.,
            frame: true,
            ..CardTheme::dark()
        }
    }
    fn send(&self, text: &str, _: &mut App) {
        println!("to the agent: {text}");
    }
    fn open_link(&self, link: &Link, _: &mut App) {
        println!("open: {}", link.as_str());
    }
    fn image(&self, _: &ImageSrc, _: ImageReply, _: &mut App) -> ImageLoad {
        ImageLoad::Refused
    }
    fn icon(&self, name: &str, _: &App) -> Option<SharedString> {
        lucide_icon(name)
    }
}

/// A message in the chat: the owner's, or the agent's reply (a draft, its finished Markdown,
/// or a card).
enum Message {
    Owner(&'static str),
    Reply(Entity<StreamingMarkdown>),
    Card(Entity<CardView>),
}

struct Chat {
    messages: Vec<Message>,
    scroll: ScrollHandle,
}

impl Render for Chat {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let rows = self.messages.iter().map(|m| match m {
            Message::Owner(text) => div()
                .flex()
                .justify_end()
                .child(
                    div()
                        .max_w(px(420.))
                        .px(px(14.))
                        .py(px(8.))
                        .rounded(px(16.))
                        .bg(rgb(0x2b2b31))
                        .child(*text),
                )
                .into_any_element(),
            Message::Reply(d) => div().w_full().child(d.clone()).into_any_element(),
            Message::Card(c) => div().w_full().child(c.clone()).into_any_element(),
        });
        div()
            .id("chat")
            .size_full()
            .bg(rgb(0x111114))
            .font_family("Inter")
            .text_size(px(14.))
            .line_height(px(14. * 1.625))
            .text_color(rgb(0xe8e8ea))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .child(
                div()
                    .max_w(px(680.))
                    .mx_auto()
                    .p(px(24.))
                    .flex()
                    .flex_col()
                    .gap(px(20.))
                    .children(rows),
            )
    }
}

impl Chat {
    fn draft(&self) -> Option<Entity<StreamingMarkdown>> {
        self.messages.iter().rev().find_map(|m| match m {
            Message::Reply(d) => Some(d.clone()),
            _ => None,
        })
    }
}

/// Writes `text` into a new draft at the chat's foot at the given pace, as a server's deltas.
async fn stream(
    chat: &Entity<Chat>,
    text: &str,
    o: &Options,
    on_progress: &mut dyn FnMut(f32, &mut AsyncApp),
    cx: &mut AsyncApp,
) -> Entity<StreamingMarkdown> {
    let n = chat.read_with(cx, |c, _| c.messages.len());
    let draft = cx.new(|_| {
        StreamingMarkdown::new(format!("draft-{n}"))
            .style(style())
            .selectable(true)
            .on_link_click(|url, _, _, _| println!("open: {url}"))
    });
    chat.update(cx, |c, cx| {
        c.messages.push(Message::Reply(draft.clone()));
        cx.notify();
    });
    let chars: Vec<char> = text.chars().collect();
    let pause = Duration::from_secs_f32(o.delta as f32 / o.cps);
    let mut sent = 0;
    for piece in chars.chunks(o.delta) {
        let delta: String = piece.iter().collect();
        sent += piece.len();
        draft.update(cx, |d, cx| d.append(&delta, cx));
        // A chat follows a reply as it grows.
        chat.update(cx, |c, _| c.scroll.scroll_to_bottom());
        on_progress(sent as f32 / chars.len() as f32, cx);
        if o.shots.is_none() {
            cx.background_executor().timer(pause).await;
        }
    }
    draft
}

/// GPUI's platform for the Mac. On iOS and Android an app brings GPUI's platform itself
/// (`Application::with_platform`), so there the example builds but doesn't run.
#[cfg(target_os = "macos")]
fn application() -> Application {
    gpui_platform::application()
}

#[cfg(not(target_os = "macos"))]
fn application() -> Application {
    panic!("the example runs on a Mac; an iOS or Android app hands GPUI its platform")
}

fn main() {
    let o = options();
    application().with_assets(gpui_core::Icons).run(move |cx| {
        let _ = cx.text_system().add_fonts(vec![
            std::borrow::Cow::Borrowed(include_bytes!("fonts/Inter-Regular.ttf").as_slice()),
            std::borrow::Cow::Borrowed(include_bytes!("fonts/Inter-Medium.ttf").as_slice()),
            std::borrow::Cow::Borrowed(include_bytes!("fonts/Inter-SemiBold.ttf").as_slice()),
        ]);
        gpui_core::init(cx);
        Theme::set(
            Theme {
                font_family: "Inter".into(),
                font_size: px(16.),
                ..Theme::dark()
            },
            cx,
        );
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        cx.on_action(|_: &Quit, cx| cx.quit());
        let offscreen = o.shots.is_some();
        let size = if offscreen {
            size(px(760.), px(1100.))
        } else {
            size(px(820.), px(900.))
        };
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("A reply streaming in".into()),
                ..Default::default()
            }),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size, cx))),
            focus: !offscreen,
            show: !offscreen,
            ..Default::default()
        };
        let (window, chat) = gpui_core::open_window(options, cx, |_, cx| {
            cx.new(|_| Chat {
                messages: vec![Message::Owner(
                    "Can you write up how we move the nightly backups to the new server?",
                )],
                scroll: ScrollHandle::new(),
            })
        })
        .expect("the window");
        if !offscreen {
            cx.activate(true);
        }
        cx.spawn(async move |cx| {
            let shots = o.shots.clone();
            let mut taken = 0;
            let mut progress = |done: f32, cx: &mut AsyncApp| {
                if let Some(dir) = &shots
                    && taken == 0
                    && done >= 0.07
                {
                    taken += 1;
                    shot(window, dir, "stream-mid", cx);
                }
            };
            cx.background_executor()
                .timer(Duration::from_millis(300))
                .await;
            // The long reply: written, ended, then its message (the same text) replaces it.
            let draft = stream(&chat, LONG, &o, &mut progress, cx).await;
            draft.update(cx, |d, cx| d.end(cx));
            if o.shots.is_none() {
                cx.background_executor()
                    .timer(Duration::from_millis(400))
                    .await;
            }
            draft.update(cx, |d, cx| d.finish(LONG, cx));
            if let Some(dir) = &o.shots {
                shot(window, dir, "stream-final", cx);
                cx.update(|cx| cx.quit());
                return;
            }
            cx.background_executor().timer(Duration::from_secs(1)).await;
            // A short reply whose message comes with a card: the card takes the draft's
            // place, drawn whole in its first frame.
            chat.update(cx, |c, cx| {
                c.messages
                    .push(Message::Owner("And what's left to decide this week?"));
                cx.notify();
            });
            let _ = stream(&chat, SHORT, &o, &mut |_, _| {}, cx).await;
            let _ = cx.update_window(window, |_, window, cx| {
                chat.update(cx, |c, cx| {
                    let host: Rc<dyn CardHost> = Rc::new(Host);
                    let card = cx.new(|cx| CardView::new(CARD, "card-1", host, window, cx));
                    if c.draft().is_some() {
                        c.messages.pop();
                    }
                    c.messages.push(Message::Card(card));
                    cx.notify();
                })
            });
        })
        .detach();
        let _ = window;
    });
}

/// Saves the window as a picture (with the `shots` feature).
#[allow(unused_variables)]
fn shot(window: AnyWindowHandle, dir: &std::path::Path, name: &str, cx: &mut AsyncApp) {
    #[cfg(feature = "shots")]
    {
        let _ = std::fs::create_dir_all(dir);
        let _ = cx.update_window(window, |_, window, cx| {
            // A window that isn't shown gets no frames from the display: draw it here.
            for _ in 0..2 {
                window.refresh();
                window.draw(cx).clear(cx);
            }
            match window.render_to_image() {
                Ok(img) => {
                    let path = dir.join(format!("{name}.png"));
                    if let Err(e) = img.save(&path) {
                        eprintln!("{name}: the picture wasn't saved: {e}");
                    }
                }
                Err(e) => eprintln!("{name}: the window didn't render to a picture: {e}"),
            }
        });
    }
    #[cfg(not(feature = "shots"))]
    eprintln!("{name}: pictures need the shots feature");
}

actions!(stream_example, [Quit]);
