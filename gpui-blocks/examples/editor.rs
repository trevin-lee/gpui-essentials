//! The editor in a window.
//!
//! ```sh
//! cargo run --release --example editor                # the sample document, not saved
//! cargo run --release --example editor -- notes.md    # a file, saved as you type (rewritten
//!                                                      # in the editor's own Markdown)
//! cargo run --release --example editor -- --bench     # the measurements, then quits
//! cargo run --release --features shots --example editor -- --shot out.png [--scene slash|hover|drag|touch]
//! ```
//!
//! `--bench` types into the sample and into a 5,000-line document, scrolls the long one end to
//! end, and prints how long each keystroke took until the frame showing it was painted (before
//! the display presents it), the frame times while
//! scrolling, the memory in use, and what the window drew while nothing changed. `--shot` takes
//! a picture of the window without showing it: `doc` (the sample), `slash` (the slash menu
//! open), `hover` (the side menu beside a block), `drag` (a block carried mid-drag) or `end`
//! (the sample's end).

use std::cell::{Cell, RefCell};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_blocks::docs::{DocInfo, Docs};
use gpui_blocks::slash::{SlashAction, SlashItem};
use gpui_blocks::widget::{Widget, fenced};
use gpui_blocks::{Editor, EditorEvent, Options};
use gpui_kit::component::{ActiveTheme as _, Theme, ThemeMode};
use gpui_kit::*;

const SAMPLE: &str = include_str!("sample.md");

struct Args {
    file: Option<PathBuf>,
    bench: bool,
    shot: Option<PathBuf>,
    scene: String,
}

fn args() -> Args {
    let mut out = Args {
        file: None,
        bench: false,
        shot: None,
        scene: "doc".into(),
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--bench" => out.bench = true,
            "--shot" => match it.next() {
                Some(path) if !path.starts_with("--") => out.shot = Some(PathBuf::from(path)),
                _ => exit("--shot needs the picture's path"),
            },
            "--scene" => out.scene = it.next().unwrap_or_default(),
            flag if flag.starts_with("--") => exit(&format!("unknown option {flag}")),
            _ => out.file = Some(PathBuf::from(arg)),
        }
    }
    if out.shot.is_some() && !cfg!(feature = "shots") {
        exit("--shot needs the shots feature: cargo run --features shots --example editor");
    }
    // A measurement or a picture changes the document; it never writes to a file.
    if (out.bench || out.shot.is_some()) && out.file.take().is_some() {
        eprintln!("the file isn't opened for --bench or --shot");
    }
    out
}

fn exit(message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(2);
}

/// Writes the document to `path`: to a file beside it first, then moved over it, so a crash or
/// a full disk mid-write leaves the old file whole.
fn save(path: &Path, markdown: &str) {
    // A file ends with one line ending; the editor keeps a trailing empty line.
    let text = format!("{}\n", markdown.trim_end());
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();
    let partial = path.with_file_name(format!(".{name}.saving"));
    let written = std::fs::File::create(&partial)
        .and_then(|mut f| f.write_all(text.as_bytes()).and_then(|_| f.sync_all()))
        .and_then(|_| std::fs::rename(&partial, path));
    if let Err(e) = written {
        let _ = std::fs::remove_file(&partial);
        eprintln!("{} wasn't saved: {e}", path.display());
    }
}

/// A charcoal dark theme.
fn apply_theme(cx: &mut App) {
    Theme::change(ThemeMode::Dark, None, cx);
    let t = Theme::global_mut(cx);
    t.font_size = px(15.);
    t.radius = px(8.);
    t.radius_lg = px(10.);
    let c = &mut t.colors;
    let hover: Hsla = rgba(0xededef0d).into();
    c.background = rgb(0x0f0f11).into();
    c.foreground = rgb(0xededef).into();
    c.border = rgb(0x28282d).into();
    c.input = rgb(0x3a3a41).into();
    c.muted = rgb(0x1f1f23).into();
    c.muted_foreground = rgb(0x8b8b93).into();
    c.secondary = rgb(0x1f1f23).into();
    c.secondary_foreground = rgb(0xededef).into();
    c.popover = rgb(0x18181b).into();
    c.popover_foreground = rgb(0xededef).into();
    c.accent = hover;
    c.accent_foreground = rgb(0xededef).into();
    c.list_hover = hover;
    c.list_active = hover;
    c.danger = rgb(0xe5665b).into();
}

/// When frames start and finish, read by `--bench`. Every frame renders the root again, so
/// its render marks the start and a canvas painted last marks the end.
#[derive(Default)]
struct Probe {
    started: Cell<Option<Instant>>,
    frames: RefCell<Vec<(Instant, Instant)>>,
    /// Hears when the next frame has been painted.
    painted: RefCell<Option<futures::channel::oneshot::Sender<Instant>>>,
}

impl Probe {
    /// A receiver for the end of the next frame's painting.
    fn next_paint(&self) -> futures::channel::oneshot::Receiver<Instant> {
        let (tx, rx) = futures::channel::oneshot::channel();
        *self.painted.borrow_mut() = Some(tx);
        rx
    }
}

struct Root {
    editor: Entity<Editor>,
    file: Option<PathBuf>,
    save: Option<Task<()>>,
    probe: Rc<Probe>,
    /// `--bench` scrolling: this many points each frame, as a smooth scroll does, until the end,
    /// then the sender hears.
    scrolling: Option<(Pixels, futures::channel::oneshot::Sender<()>)>,
    /// Pages made from the slash menu, for their ids.
    pages: usize,
    _subscription: Subscription,
}

impl Root {
    fn new(
        source: &str,
        file: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // The example's own widget, and a slash menu item that inserts one.
        let options = Options::default()
            .widget(Answer)
            .slash_item(
                SlashItem::new(
                    "Answer",
                    SlashAction::Markdown(
                        fenced(
                            "answer",
                            "from: Assistant\nasked: What should I look at first today?\n---\n",
                        )
                        .into(),
                    ),
                )
                .hint("A question and its answer, as a card")
                .aliases(["ask", "ai", "assistant", "question"])
                .icon("✦"),
            )
            .docs(DemoDocs);
        let editor = cx.new(|cx| Editor::with_options(source, options, window, cx));
        let subscription = cx.subscribe(&editor, |this, editor, event: &EditorEvent, cx| {
            // An app opens the document, makes a new one, or shows a picker; the example has one
            // other document to link to, and makes untitled pages that go nowhere.
            match event {
                EditorEvent::DocClicked { id } => eprintln!("open the document {id}"),
                EditorEvent::NewPageRequested => {
                    this.pages += 1;
                    let id = format!("new-{}", this.pages);
                    editor.update(cx, |e, cx| e.insert_page(&id, "Untitled", cx));
                }
                EditorEvent::DocLinkRequested => {
                    editor.update(cx, |e, cx| e.insert_mention("roadmap", "Roadmap", cx));
                }
                _ => {}
            }
            if !matches!(event, EditorEvent::Changed) {
                return;
            }
            // Saved a moment after typing stops, not on every keystroke.
            let Some(path) = this.file.clone() else {
                return;
            };
            let editor = editor.downgrade();
            this.save = Some(cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(400))
                    .await;
                let Ok(markdown) = editor.read_with(cx, |e, cx| e.markdown(cx)) else {
                    return;
                };
                save(&path, &markdown);
                let _ = this.update(cx, |this, _| this.save = None);
            }));
        });
        // Quitting or closing the window saves what's waiting rather than dropping it.
        cx.on_app_quit(|this, cx| {
            this.save_now(cx);
            async {}
        })
        .detach();
        window.on_window_should_close(cx, |_, cx| {
            cx.quit();
            true
        });
        editor.update(cx, |e, cx| e.focus(window, cx));
        Self {
            editor,
            file,
            save: None,
            probe: Rc::default(),
            scrolling: None,
            pages: 0,
            _subscription: subscription,
        }
    }
}

impl Root {
    /// Saves a change still waiting out its 400 ms, now.
    fn save_now(&mut self, cx: &App) {
        if self.save.take().is_some()
            && let Some(path) = &self.file
        {
            save(path, &self.editor.read(cx).markdown(cx));
        }
    }
}

impl Render for Root {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.probe.started.set(Some(Instant::now()));
        if let Some((step, _)) = &self.scrolling {
            let handle = self.editor.read(cx).scroll_handle().clone();
            let max = handle.max_offset().y;
            let y = (-handle.offset().y + *step).min(max);
            handle.set_offset(point(px(0.), -y));
            if y >= max {
                if let Some((_, done)) = self.scrolling.take() {
                    let _ = done.send(());
                }
            } else {
                window.request_animation_frame();
            }
        }
        let probe = self.probe.clone();
        let theme = cx.theme();
        div()
            .size_full()
            .relative()
            .bg(theme.background)
            .text_color(theme.foreground)
            .text_size(px(15.))
            .child(self.editor.clone())
            .child(
                canvas(
                    |_, _, _| {},
                    move |_, _, _, _| {
                        let now = Instant::now();
                        if let Some(start) = probe.started.take() {
                            probe.frames.borrow_mut().push((start, now));
                        }
                        if let Some(tx) = probe.painted.borrow_mut().take() {
                            let _ = tx.send(now);
                        }
                    },
                )
                .absolute()
                .size_0(),
            )
    }
}

fn main() {
    let args = args();
    let source = match &args.file {
        // A new file starts empty. A file that can't be read isn't opened: saving over it would
        // lose it.
        Some(path) => match std::fs::read_to_string(path) {
            Ok(text) => text.trim_end_matches(['\r', '\n']).to_string(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => exit(&format!("{} can't be opened: {e}", path.display())),
        },
        None => SAMPLE.trim_end().to_string(),
    };
    let quiet = args.bench || args.shot.is_some();
    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        gpui_blocks::init(cx);
        apply_theme(cx);
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        cx.on_action(|_: &Quit, cx| cx.quit());
        let bounds = Bounds::centered(None, size(px(1280.), px(820.)), cx);
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("Editor".into()),
                ..Default::default()
            }),
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            // A measurement or a picture doesn't take the keyboard or come forward; a picture
            // isn't shown at all.
            focus: !quiet,
            show: args.shot.is_none(),
            // GPUI draws a window that isn't active at most 30 times a second. The measurement's
            // window stays inactive so it doesn't take the keyboard, so it lifts that cap to
            // measure what the editor does at the display's own rate.
            inactive_frame_interval: if args.bench {
                None
            } else {
                WindowOptions::default().inactive_frame_interval
            },
            ..Default::default()
        };
        let file = args.file.clone();
        let (window, root) = gpui_kit::open_window(options, cx, move |window, cx| {
            cx.new(|cx| Root::new(&source, file, window, cx))
        })
        .expect("the window");
        if !quiet {
            cx.activate(true);
        }
        if args.bench {
            bench(window, root.clone(), cx);
        }
        #[cfg(feature = "shots")]
        if let Some(path) = args.shot.clone() {
            shot(window, root.clone(), path, args.scene.clone(), cx);
        }
        let _ = root;
    });
}

actions!(editor_example, [Quit]);

/// Asks for a frame and returns when it starts.
async fn next_frame(window: AnyWindowHandle, cx: &mut AsyncApp) -> Instant {
    let (tx, rx) = futures::channel::oneshot::channel();
    let _ = cx.update_window(window, |_, window, _| {
        window.refresh();
        window.on_next_frame(move |_, _| {
            let _ = tx.send(Instant::now());
        });
    });
    rx.await.unwrap_or_else(|_| Instant::now())
}

async fn wait(cx: &mut AsyncApp, ms: u64) {
    cx.background_executor()
        .timer(Duration::from_millis(ms))
        .await;
}

/// A document of `lines` lines, built from the sample's kinds of block.
fn long_document(lines: usize) -> String {
    let mut out = String::new();
    let mut n = 0;
    let mut section = 0;
    while out.lines().count() + 1 < lines {
        section += 1;
        out.push_str(&format!("## Section {section}\n\n"));
        for p in 0..6 {
            n += 1;
            out.push_str(&format!(
                "Paragraph {n} of section {section} has **bold**, *italic* and `code` in it, and \
                 runs long enough to wrap once or twice at the column's width, as notes do. ({p})\n\n"
            ));
        }
        out.push_str(
            "- [x] A task that's done\n- [ ] A task to do\n  - [ ] Nested under it\n- A bullet\n\n",
        );
        out.push_str("> [!NOTE]\n> A callout with a [link](https://example.com).\n\n");
        out.push_str("| a | b | c |\n| --- | --- | --- |\n| 1 | 2 | 3 |\n| 4 | 5 | 6 |\n\n");
        out.push_str("```rust\nfn section() -> usize {\n    42\n}\n```\n\n");
        out.push_str(
            "<details>\n<summary>A closed toggle</summary>\n\nHidden text.\n\n</details>\n\n",
        );
    }
    out.trim_end().to_string()
}

fn summary(label: &str, mut v: Vec<f64>) {
    if v.is_empty() {
        println!("{label}: no samples");
        return;
    }
    v.sort_by(f64::total_cmp);
    let at = |q: f64| v[((v.len() - 1) as f64 * q).round() as usize];
    println!(
        "{label}: median {:.1} ms, p95 {:.1} ms, max {:.1} ms ({} samples)",
        at(0.5),
        at(0.95),
        v[v.len() - 1],
        v.len()
    );
}

/// The process's memory as Activity Monitor counts it (its footprint, GPU buffers included),
/// from `top`.
fn footprint_mb() -> f64 {
    let out = std::process::Command::new("top")
        .args([
            "-l",
            "1",
            "-pid",
            &std::process::id().to_string(),
            "-stats",
            "mem",
        ])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    let Some(value) = out.lines().last().map(str::trim) else {
        return 0.;
    };
    let (number, unit) = value.split_at(value.trim_end_matches(['K', 'M', 'G', '+', '-']).len());
    let n = number.parse::<f64>().unwrap_or(0.);
    match unit.trim_end_matches(['+', '-']) {
        "K" => n / 1024.,
        "G" => n * 1024.,
        _ => n,
    }
}

/// The process's resident memory and CPU time so far, from `ps`.
fn usage() -> (f64, f64) {
    let out = std::process::Command::new("ps")
        .args(["-o", "rss=,time=", "-p", &std::process::id().to_string()])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();
    let mut parts = out.split_whitespace();
    let rss_mb = parts
        .next()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.)
        / 1024.;
    // "M:SS.ss"
    let cpu = parts
        .next()
        .and_then(|t| {
            let (m, s) = t.split_once(':')?;
            Some(m.parse::<f64>().ok()? * 60. + s.parse::<f64>().ok()?)
        })
        .unwrap_or(0.);
    (rss_mb, cpu)
}

/// Types `count` characters, one every 50 ms, and returns each one's time from the keystroke to
/// the end of painting the frame that shows it.
async fn type_and_time(
    window: AnyWindowHandle,
    probe: &Probe,
    count: usize,
    cx: &mut AsyncApp,
) -> Vec<f64> {
    let mut out = Vec::new();
    for i in 0..count {
        let key = if i % 6 == 5 { "space" } else { "a" };
        let rx = probe.next_paint();
        let start = Instant::now();
        let _ = cx.update_window(window, |_, window, cx| {
            window.dispatch_keystroke(Keystroke::parse(key).unwrap(), cx);
        });
        if let Ok(end) = rx.await {
            out.push((end - start).as_secs_f64() * 1000.);
        }
        wait(cx, 50).await;
    }
    out
}

fn bench(window: AnyWindowHandle, root: Entity<Root>, cx: &mut App) {
    cx.spawn(async move |cx| {
        wait(cx, 1500).await;
        let probe = root.read_with(cx, |root, _| root.probe.clone());
        let (rss_start, _) = usage();
        let active = cx
            .update_window(window, |_, window, _| window.is_window_active())
            .unwrap_or(false);
        let scale = cx
            .update_window(window, |_, window, _| window.scale_factor())
            .unwrap_or(0.);
        println!(
            "window: 1280×820 points at {scale}x, release build, active: {active} (inactive windows' 30 fps cap lifted)"
        );
        println!(
            "memory with the sample open: {:.0} MB (Activity Monitor), {rss_start:.0} MB resident",
            footprint_mb()
        );

        // Typing at the end of the sample's first paragraph.
        let _ = cx.update_window(window, |_, window, cx| {
            root.update(cx, |root, cx| {
                root.editor.update(cx, |e, cx| {
                    e.focus(window, cx);
                    e.state().update(cx, |s, cx| {
                        let id = s.model().document().leaves().nth(1).unwrap().id();
                        let len = s.model().document().block(id).and_then(|b| b.text()).map_or(0, str::len);
                        let _ = s.set_selection(
                            markdown_editor_selection(id, len),
                            cx,
                        );
                    });
                });
            });
        });
        next_frame(window, cx).await;
        let typed = type_and_time(window, &probe, 120, cx).await;
        summary("keystroke to painted frame, sample", typed);

        // The 5,000-line document.
        let long = long_document(5000);
        let lines = long.lines().count();
        let painted = probe.next_paint();
        let started = Instant::now();
        let _ = cx.update_window(window, |_, _, cx| {
            root.update(cx, |root, cx| {
                root.editor.update(cx, |e, cx| e.set_markdown(&long, cx));
            });
        });
        let shown = painted.await.unwrap_or_else(|_| Instant::now());
        println!(
            "opening a {lines}-line document ({} KB): {:.0} ms to its first painted frame",
            long.len() / 1024,
            (shown - started).as_secs_f64() * 1000.
        );
        wait(cx, 500).await;
        let (rss_long, _) = usage();
        println!(
            "memory with it open: {:.0} MB (Activity Monitor), {rss_long:.0} MB resident",
            footprint_mb()
        );

        // Scrolling it end to end, 60 points a frame, driven from the frame loop as a smooth
        // scroll is: each frame moves it and asks for the next.
        let handle = root.read_with(cx, |root, cx| root.editor.read(cx).scroll_handle().clone());
        handle.set_offset(point(px(0.), px(0.)));
        next_frame(window, cx).await;
        let first = probe.frames.borrow().len();
        let scroll_start = Instant::now();
        let (done_tx, done_rx) = futures::channel::oneshot::channel();
        let _ = cx.update_window(window, |_, _, cx| {
            root.update(cx, |root, cx| {
                root.scrolling = Some((px(60.), done_tx));
                cx.notify();
            })
        });
        let _ = done_rx.await;
        let y = -handle.offset().y;
        let frames: Vec<(Instant, Instant)> = probe.frames.borrow()[first..].to_vec();
        let elapsed = scroll_start.elapsed().as_secs_f64();
        summary(
            "frame time while scrolling (render start to paint end)",
            frames.iter().map(|(a, b)| (*b - *a).as_secs_f64() * 1000.).collect(),
        );
        let intervals: Vec<f64> = frames
            .windows(2)
            .map(|w| (w[1].1 - w[0].1).as_secs_f64() * 1000.)
            .collect();
        summary("time between frames while scrolling", intervals.clone());
        println!(
            "scrolled {:.0} points in {} frames over {elapsed:.1} s ({:.0} frames a second)",
            f32::from(y),
            frames.len(),
            frames.len() as f64 / elapsed
        );
        let (rss_scrolled, _) = usage();
        println!(
            "memory after scrolling it end to end: {:.0} MB (Activity Monitor), {rss_scrolled:.0} MB resident",
            footprint_mb()
        );

        // Typing in the middle of the long document.
        let _ = cx.update_window(window, |_, window, cx| {
            root.update(cx, |root, cx| {
                root.editor.update(cx, |e, cx| {
                    e.focus(window, cx);
                    e.state().update(cx, |s, cx| {
                        let leaves: Vec<_> = s.model().document().leaves().map(|l| l.id()).collect();
                        let id = leaves[leaves.len() / 2];
                        let _ = s.set_selection(markdown_editor_selection(id, 0), cx);
                        s.reveal_caret(cx);
                    });
                });
            });
        });
        wait(cx, 300).await;
        let typed = type_and_time(window, &probe, 120, cx).await;
        summary("keystroke to painted frame, 5,000-line document", typed);

        // Idle: nothing changes for ten seconds, with the caret in the text. The caret blinks
        // twice a second while the editor has focus, so a few frames are drawn for it.
        wait(cx, 1000).await;
        let before = probe.frames.borrow().len();
        let (_, cpu_before) = usage();
        wait(cx, 10_000).await;
        let drawn = probe.frames.borrow().len() - before;
        let (_, cpu_after) = usage();
        println!(
            "idle for 10 s, caret in the text and blinking: {drawn} frames drawn, {:.2} s of CPU ({:.1}%)",
            cpu_after - cpu_before,
            (cpu_after - cpu_before) / 10. * 100.
        );
        cx.update(|cx| cx.quit());
    })
    .detach();
}

fn markdown_editor_selection(
    block: gpui_blocks::markdown_editor::BlockId,
    offset: usize,
) -> gpui_blocks::markdown_editor::BlockSelection {
    gpui_blocks::markdown_editor::BlockSelection::caret(
        gpui_blocks::markdown_editor::BlockPosition::new(block, offset),
    )
}

/// Draws the window now and lets what that drew settle. A window that isn't shown gets no
/// frames from the display, so a picture draws it itself, twice: the menus are placed from the
/// geometry of the frame before.
#[cfg(feature = "shots")]
async fn settle(window: AnyWindowHandle, cx: &mut AsyncApp) {
    for _ in 0..2 {
        let _ = cx.update_window(window, |_, window, cx| {
            window.refresh();
            let _ = window.draw(cx);
        });
        wait(cx, 100).await;
    }
}

/// A picture of the window, rendered offscreen, then quits.
#[cfg(feature = "shots")]
fn shot(window: AnyWindowHandle, root: Entity<Root>, path: PathBuf, scene: String, cx: &mut App) {
    cx.spawn(async move |cx| {
        wait(cx, 800).await;
        match scene.as_str() {
            // The slash menu, opened on a new line under the "This week" heading.
            "slash" => {
                let _ = cx.update_window(window, |_, window, cx| {
                    root.update(cx, |root, cx| {
                        root.editor.update(cx, |e, cx| {
                            e.focus(window, cx);
                            e.state().update(cx, |s, cx| {
                                let id = s.model().document().leaves().nth(2).unwrap().id();
                                let len = s.model().document().block(id).and_then(|b| b.text()).map_or(0, str::len);
                                let _ = s.set_selection(markdown_editor_selection(id, len), cx);
                                let third = s.model().document().root().children().nth(2).unwrap().id();
                                let _ = s.run(
                                    gpui_blocks::markdown_editor::EditorCommand::InsertParagraphAfter(third),
                                    cx,
                                );
                            });
                        });
                    });
                    // Typed, as the menu opens only for a `/` that's typed.
                    window.dispatch_keystroke(Keystroke::parse("/").unwrap(), cx);
                });
                settle(window, cx).await;
            }
            // A word selected by a long press, with the touch selection handles at its ends.
            "touch" => {
                settle(window, cx).await;
                let at = root.read_with(cx, |root, cx| {
                    let state = root.editor.read(cx).state().read(cx);
                    let leaf = state.model().document().leaves().nth(1)?.id();
                    let pos = gpui_blocks::markdown_editor::BlockPosition::new(leaf, 53);
                    state.position_bounds(pos).map(|b| b.center())
                });
                if let Some(at) = at {
                    let touch = |phase| {
                        PlatformInput::Touch(TouchEvent {
                            id: TouchId(1),
                            phase,
                            position: at,
                            predicted_position: None,
                            force: None,
                        })
                    };
                    let _ = cx.update_window(window, |_, window, cx| {
                        root.update(cx, |root, cx| root.editor.update(cx, |e, cx| e.focus(window, cx)));
                        window.dispatch_event(touch(TouchPhase::Started), cx);
                    });
                    wait(cx, 900).await;
                    let _ = cx.update_window(window, |_, window, cx| {
                        window.dispatch_event(touch(TouchPhase::Ended), cx);
                    });
                }
                settle(window, cx).await;
            }
            // The side menu beside the to-do list.
            "hover" => {
                settle(window, cx).await;
                let at = root
                    .read_with(cx, |root, cx| {
                        let state = root.editor.read(cx).state().read(cx);
                        state.top_level_blocks().get(3).map(|(_, b)| b.center())
                    });
                if let Some(at) = at {
                    let _ = cx.update_window(window, |_, window, cx| {
                        window.dispatch_event(
                            PlatformInput::MouseMove(MouseMoveEvent {
                                position: at,
                                pressed_button: None,
                                modifiers: Modifiers::none(),
                            }),
                            cx,
                        );
                    });
                }
                settle(window, cx).await;
            }
            // Mid-drag: "This week" picked up by its handle and carried down past the to-do
            // list, the blocks between sliding up to make room.
            "drag" => {
                settle(window, cx).await;
                let picked = root.read_with(cx, |root, cx| {
                    let state = root.editor.read(cx).state().read(cx);
                    let blocks = state.top_level_blocks();
                    Some((blocks.get(2)?.0, blocks.get(2)?.1.center(), blocks.get(3)?.1.bottom()))
                });
                if let Some((block, from, past)) = picked {
                    let _ = cx.update_window(window, |_, _, cx| {
                        root.update(cx, |root, cx| {
                            root.editor.update(cx, |e, cx| {
                                e.state().update(cx, |s, _| s.begin_block_drag(block, from))
                            })
                        })
                    });
                    let to = past - px(24.);
                    for step in 1..=12 {
                        let y = from.y + (to - from.y) * (step as f32 / 12.);
                        let _ = cx.update_window(window, |_, window, cx| {
                            window.dispatch_event(
                                PlatformInput::MouseMove(MouseMoveEvent {
                                    position: point(from.x, y),
                                    pressed_button: Some(MouseButton::Left),
                                    modifiers: Modifiers::none(),
                                }),
                                cx,
                            );
                            window.refresh();
                            let _ = window.draw(cx);
                        });
                        wait(cx, 16).await;
                    }
                    for _ in 0..20 {
                        let _ = cx.update_window(window, |_, window, cx| {
                            window.refresh();
                            let _ = window.draw(cx);
                        });
                        wait(cx, 16).await;
                    }
                }
            }
            // The end of the document: the answer widget, the divider and code.
            "end" => {
                settle(window, cx).await;
                let handle = root.read_with(cx, |root, cx| root.editor.read(cx).scroll_handle().clone());
                handle.set_offset(point(px(0.), -handle.max_offset().y));
                settle(window, cx).await;
            }
            _ => {}
        }
        settle(window, cx).await;
        let _ = cx.update_window(window, |_, window, cx| {
            window.refresh();
            let _ = window.draw(cx);
            match window.render_to_image() {
                Ok(img) => {
                    if let Err(e) = img.save(&path) {
                        eprintln!("the picture wasn't saved: {e}");
                    }
                }
                Err(e) => eprintln!("the window didn't render to a picture: {e}"),
            }
            cx.quit();
        });
    })
    .detach();
}

/// The example's other documents: a real app answers from its own list.
struct DemoDocs;

impl Docs for DemoDocs {
    fn info(&self, id: &str, _: &App) -> Option<DocInfo> {
        match id {
            "roadmap" => Some(DocInfo::new("Q4 roadmap").icon("🗺")),
            "last-week" => Some(DocInfo::new("Notes from last week").icon("🗒")),
            _ if id.starts_with("new-") => Some(DocInfo::new("Untitled")),
            _ => None,
        }
    }
}

/// The example's widget: a question and its answer, drawn as a card. In Markdown it's a fenced
/// `answer` block: `from:` and `asked:` lines, `---`, then the answer in Markdown.
struct Answer;

impl Widget for Answer {
    fn name(&self) -> &str {
        "answer"
    }

    fn label(&self, body: &str) -> String {
        let (from, asked, _) = answer_parts(body);
        format!("{from} answered: {asked}")
    }

    fn render(&self, body: &str, _: &mut Window, cx: &mut App) -> AnyElement {
        use gpui_kit::component::text::TextView;
        use gpui_kit::prelude::FluentBuilder as _;
        let (from, asked, answer) = answer_parts(body);
        let theme = cx.theme();
        let initial = from
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_default();
        let hue = from
            .bytes()
            .fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(b as u32))
            % 360;
        // The text view's id, from the body: two cards with the same body share it, which is
        // harmless for text that isn't selectable.
        let id = SharedString::from(format!(
            "answer-{:x}",
            body.bytes().fold(0u64, |h, b| h
                .wrapping_mul(1099511628211)
                .wrapping_add(b as u64))
        ));
        div()
            .w_full()
            .my_1()
            .rounded(px(10.))
            .border_1()
            .border_color(theme.border)
            .bg(theme.secondary.opacity(0.4))
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .size(px(20.))
                            .rounded(px(6.))
                            .bg(hsla(hue as f32 / 360., 0.55, 0.55, 1.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(white())
                            .child(initial),
                    )
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(from.to_string()),
                    ),
            )
            .when(!asked.is_empty(), |d| {
                d.child(
                    div()
                        .text_sm()
                        .text_color(theme.muted_foreground)
                        .child(asked.to_string()),
                )
            })
            .when(!answer.trim().is_empty(), |d| {
                d.child(TextView::markdown(id, answer.to_string()).selectable(false))
            })
            .into_any_element()
    }
}

/// An answer block's `from`, `asked` and answer.
fn answer_parts(body: &str) -> (&str, &str, &str) {
    // The header ends at a line that is just `---`.
    let split = body
        .match_indices("---")
        .find(|(at, _)| {
            let line_start = *at == 0 || body[..*at].ends_with('\n');
            let rest = &body[at + 3..];
            line_start && (rest.is_empty() || rest.starts_with('\n'))
        })
        .map(|(at, _)| (&body[..at], &body[at + 3..]));
    let (head, answer) = split.unwrap_or(("", body));
    let field = |key: &str| {
        head.lines()
            .find_map(|l| l.strip_prefix(key).map(str::trim))
            .unwrap_or_default()
    };
    (
        field("from:"),
        field("asked:"),
        answer.trim_start_matches('\n'),
    )
}
