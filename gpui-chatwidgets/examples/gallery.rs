//! Every golden program (tests/openui/*.oui) as a host would draw it: a list on the left, the
//! card on the right, light and dark, at several widths. What the card hands the host (a
//! message to send, a link to open) shows in the log at the bottom instead of being sent.
//!
//! ```sh
//! cargo run --release --example gallery
//! cargo run --release --features shots --example gallery -- --shots docs
//! ```
//!
//! With `--shots <dir>` it renders the pictures in the README into `<dir>` from a window that's
//! never shown and never takes the keyboard, then quits.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use gpui_chatwidgets::openui::{ImageSrc, Link};
use gpui_chatwidgets::*;
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// A few programs of the gallery's own, after the golden ones: charts split so each fits a
/// picture, and the worst a card can be asked to fit.
const SHOWCASE: &[(&str, &str)] = &[
    (
        "showcase-bars",
        r#"root = Stack([bar, hbar])
labels = ["Oct", "Nov", "Dec"]
bar = BarChart(labels, [Series("Product A", [120, 150, 180]), Series("Product B", [90, -10, 140.5])], "grouped", "Month", "Revenue", 260)
hbar = HorizontalBarChart(["A long category name", "Another one", "Third"], [Series("Votes", [10, 20, 14])], "grouped")"#,
    ),
    (
        "showcase-lines",
        r#"root = Stack([line, area, radar])
labels = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
line = LineChart(labels, [Series("Visits", [12, 19, 15, 24, 22, 30, 28]), Series("Signups", [3, 5, 4, 8, 7, 11, 9])], "natural", null, "People", 220)
area = AreaChart(labels, [Series("Load", [40, 55, 48, 70, 62, 58, 45])], "step", null, null, 200)
radar = RadarChart(["Speed", "Power", "Range", "Comfort", "Price"], [Series("Car", [3, 4, 5, 4, 2]), Series("Bike", [2, 1, 3, 2, 5])])"#,
    ),
    (
        "showcase-pies",
        r#"root = Stack([Stack([pie, donut], "row", "l"), Stack([half, rings], "row", "l"), bar])
pie = PieChart(["Rent", "Food", "Fun"], [1200, 340, 120])
donut = PieChart(["Low", "Med", "High"], [1, 1, 2], "donut")
half = PieChart(["Done", "Left"], [7, 3], "pie", "semiCircular")
rings = RadialChart(["Mon", "Tue"], [5, 8])
bar = SingleStackedBarChart(["Used", "Free"], [60, 40])"#,
    ),
    (
        "showcase-worst",
        r#"root = Stack([head, row, tags, tbl, ents, btns])
w = "Pneumonoultramicroscopicsilicovolcanoconiosis-Antidisestablishmentarianism-Floccinaucinihilipilification"
head = CardHeader("A title with " + w, "https://example.com/a/very/long/address/that/never/breaks/" + w)
row = Stack([Card([TextContent(w)]), Card([TextContent("日本語のテキストと " + w + " 🎓📚✅")])], "row")
tags = TagBlock([w, "short", "123456789012345678901234567890"])
tbl = Table([Col("Name", [w, "Ann"]), Col("Amount", [123456789012345, 2], "number"), Col("C3", ["x", "y"]), Col("C4", ["x", "y"]), Col("C5", ["x", "y"]), Col("C6", ["x", "y"]), Col("C7", ["x", "y"]), Col("C8", ["x", "y"])])
ents = EntityList([{left: w, right: "$1,000,000,000,000.00"}])
btns = Buttons([Button("A button label as long as " + w), Button("OK", @ToAssistant("ok"), "secondary")])"#,
    ),
];

/// The golden programs, by name, read from the repository, and the gallery's own.
fn programs() -> Vec<(String, String)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/openui");
    let mut out: Vec<(String, String)> = std::fs::read_dir(&dir)
        .map(|d| {
            d.filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "oui"))
                .filter_map(|p| {
                    Some((
                        p.file_stem()?.to_string_lossy().into_owned(),
                        std::fs::read_to_string(&p).ok()?,
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out.extend(SHOWCASE.iter().map(|(n, s)| (n.to_string(), s.to_string())));
    out
}

/// Inter in the weights the cards draw (examples/fonts, under the OFL). GPUI takes only a
/// variable font's default instance, so these are fixed instances.
fn load_fonts(cx: &mut App) {
    let _ = cx.text_system().add_fonts(vec![
        std::borrow::Cow::Borrowed(include_bytes!("fonts/Inter-Regular.ttf").as_slice()),
        std::borrow::Cow::Borrowed(include_bytes!("fonts/Inter-Medium.ttf").as_slice()),
        std::borrow::Cow::Borrowed(include_bytes!("fonts/Inter-SemiBold.ttf").as_slice()),
    ]);
}

/// The gallery's assets: gpui-core's Lucide icons, which `lucide_icon` names for the cards,
/// then GPUI Kit's own for its date picker. Both serve `icons/<name>.svg`.
struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<std::borrow::Cow<'static, [u8]>>> {
        match gpui_core::Icons.load(path)? {
            Some(svg) => Ok(Some(svg)),
            None => gpui_kit::assets::Assets::new("").load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut all = gpui_core::Icons.list(path)?;
        all.extend(gpui_kit::assets::Assets::new("").list(path)?);
        all.sort();
        all.dedup();
        Ok(all)
    }
}

/// GPUI Kit's theme (its date picker and Markdown) in the same mode as the cards. gpui-core's
/// text fields, slider and tooltips take their colours from the card.
fn apply_theme(dark: bool, cx: &mut App) {
    Theme::change(
        if dark {
            ThemeMode::Dark
        } else {
            ThemeMode::Light
        },
        None,
        cx,
    );
    let t = Theme::global_mut(cx);
    t.font_family = "Inter".into();
    t.font_size = px(16.);
    t.radius = px(8.);
    t.radius_lg = px(10.);
}

/// A picture for any image a card asks for: a landscape drawn as SVG, so no file or network
/// is involved. A real host fetches the reference from its own server, if it allows it.
fn sample_image(src: &ImageSrc) -> Option<Arc<Image>> {
    src.same_origin("https://host.example")?;
    let hue = src
        .as_str()
        .bytes()
        .fold(0u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32))
        % 360;
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="640" height="400" viewBox="0 0 640 400">
<defs><linearGradient id="s" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="hsl({hue},60%,72%)"/><stop offset="1" stop-color="hsl({},55%,88%)"/></linearGradient></defs>
<rect width="640" height="400" fill="url(#s)"/><circle cx="470" cy="120" r="48" fill="hsl(45,90%,80%)"/>
<path d="M0 300 L150 170 L260 260 L380 140 L640 320 L640 400 L0 400 Z" fill="hsl({},35%,38%)"/>
<path d="M0 340 L200 260 L420 330 L640 280 L640 400 L0 400 Z" fill="hsl({},30%,28%)"/></svg>"##,
        (hue + 30) % 360,
        (hue + 160) % 360,
        (hue + 170) % 360
    );
    Some(Arc::new(Image::from_bytes(
        ImageFormat::Svg,
        svg.into_bytes(),
    )))
}

/// The gallery's host: its theme, images, icons, and a log of what the card hands over.
struct Host {
    dark: Cell<bool>,
    log: Rc<RefCell<Vec<String>>>,
}

impl CardHost for Host {
    fn theme(&self, _: &App) -> CardTheme {
        let mut t = if self.dark.get() {
            CardTheme::dark()
        } else {
            CardTheme::light()
        };
        // The window's rem is GPUI Kit's font size, 16 here.
        t.rem_base = 16.;
        t
    }

    fn send(&self, text: &str, _: &mut App) {
        self.log
            .borrow_mut()
            .push(format!("send: {}", text.replace('\n', " ⏎ ")));
    }

    fn open_link(&self, link: &Link, _: &mut App) {
        self.log
            .borrow_mut()
            .push(format!("open: {}", link.as_str()));
    }

    fn image(&self, src: &ImageSrc, _: ImageReply, _: &mut App) -> ImageLoad {
        match sample_image(src) {
            Some(img) => ImageLoad::Ready(img),
            None => ImageLoad::Refused,
        }
    }

    fn icon(&self, name: &str, _: &App) -> Option<SharedString> {
        lucide_icon(name)
    }
}

struct Gallery {
    programs: Vec<(String, String)>,
    chosen: usize,
    width: Option<f32>,
    host: Rc<Host>,
    card: Entity<CardView>,
    _events: Subscription,
    /// Only the card (for pictures): no list, no toolbar, no log.
    bare: bool,
    /// Where the card was last drawn (for cutting pictures to it).
    card_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
}

impl Gallery {
    fn new(bare: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let programs = programs();
        let host = Rc::new(Host {
            dark: Cell::new(true),
            log: Rc::default(),
        });
        let (card, events) = Self::open(&programs, 0, &host, window, cx);
        Gallery {
            programs,
            chosen: 0,
            width: Some(400.),
            host,
            card,
            _events: events,
            bare,
            card_bounds: Rc::default(),
        }
    }

    fn open(
        programs: &[(String, String)],
        i: usize,
        host: &Rc<Host>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Entity<CardView>, Subscription) {
        let (name, source) = programs.get(i).cloned().unwrap_or_default();
        let h: Rc<dyn CardHost> = host.clone();
        let card = cx.new(|cx| CardView::new(&source, format!("gallery-{name}"), h, window, cx));
        let events = cx.subscribe(&card, |_, _, _: &CardEvent, cx| cx.notify());
        (card, events)
    }

    fn choose(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.chosen = i;
        let (card, events) = Self::open(&self.programs, i, &self.host, window, cx);
        self.card = card;
        self._events = events;
        cx.notify();
    }

    fn set_dark(&mut self, dark: bool, cx: &mut Context<Self>) {
        self.host.dark.set(dark);
        apply_theme(dark, cx);
        self.card.update(cx, |c, cx| c.refresh(cx));
        cx.notify();
    }
}

impl Render for Gallery {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dark = self.host.dark.get();
        let theme = self.host.theme(cx);
        let (bg, fg, muted, line) = if dark {
            (rgb(0x0f0f11), rgb(0xededef), rgb(0x8b8b93), rgb(0x28282d))
        } else {
            (rgb(0xf4f4f5), rgb(0x18181b), rgb(0x71717a), rgb(0xe4e4e7))
        };
        let card = div()
            .w_full()
            .when_some(self.width, |d, w| d.w(px(w)))
            .flex_none()
            .child(self.card.clone());
        if self.bare {
            let at = self.card_bounds.clone();
            return div()
                .size_full()
                .bg(bg)
                .p(px(24.))
                .font_family("Inter")
                .on_children_prepainted(move |b, _, _| at.set(b.first().copied()))
                .child(card)
                .into_any_element();
        }
        let mut list = div()
            .id("list")
            .w(px(240.))
            .h_full()
            .flex_none()
            .overflow_y_scroll()
            .border_r_1()
            .border_color(line)
            .py(px(8.));
        for (i, (name, _)) in self.programs.iter().enumerate() {
            let on = i == self.chosen;
            list = list.child(
                div()
                    .id(("program", i))
                    .px(px(14.))
                    .py(px(5.))
                    .text_size(px(13.))
                    .text_color(if on { fg } else { muted })
                    .when(on, |d| d.bg(theme.colors.sunk))
                    .hover(|s| s.bg(theme.colors.highlight))
                    .truncate()
                    .child(name.clone())
                    .on_click(cx.listener(move |this, _, window, cx| this.choose(i, window, cx))),
            );
        }
        let toggle = |id: &'static str, label: &'static str, on: bool| {
            div()
                .id(id)
                .px(px(10.))
                .py(px(4.))
                .rounded(px(6.))
                .text_size(px(12.))
                .border_1()
                .border_color(line)
                .text_color(if on { fg } else { muted })
                .when(on, |d| d.bg(theme.colors.raised))
                .child(label)
        };
        let mut bar = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.))
            .px(px(16.))
            .py(px(10.))
            .border_b_1()
            .border_color(line);
        bar = bar
            .child(
                toggle("dark", "Dark", dark)
                    .on_click(cx.listener(|this, _, _, cx| this.set_dark(true, cx))),
            )
            .child(
                toggle("light", "Light", !dark)
                    .on_click(cx.listener(|this, _, _, cx| this.set_dark(false, cx))),
            )
            .child(div().w(px(16.)));
        for (id, label, w) in [
            ("w280", "280", Some(280.)),
            ("w400", "400", Some(400.)),
            ("w736", "736", Some(736.)),
            ("wfill", "Fill", None),
        ] {
            bar = bar.child(toggle(id, label, self.width == w).on_click(cx.listener(
                move |this, _, _, cx| {
                    this.width = w;
                    cx.notify();
                },
            )));
        }
        let log = self.host.log.borrow();
        let mut pane = div()
            .id("log")
            .h(px(140.))
            .flex_none()
            .overflow_y_scroll()
            .border_t_1()
            .border_color(line)
            .px(px(16.))
            .py(px(8.))
            .text_size(px(12.))
            .font_family("Menlo")
            .text_color(muted);
        if log.is_empty() {
            pane = pane.child("What the card hands the host shows here.");
        }
        for l in log.iter().rev().take(50) {
            pane = pane.child(div().text_color(fg).truncate().child(l.clone()));
        }
        div()
            .size_full()
            .flex()
            .flex_row()
            .bg(bg)
            .text_color(fg)
            .font_family("Inter")
            .child(list)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(bar)
                    .child(
                        div()
                            .id("stage")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .p(px(24.))
                            .child(card),
                    )
                    .child(pane),
            )
            .into_any_element()
    }
}

actions!(gallery, [Quit]);

/// GPUI Kit's desktop application. On iOS and Android an app brings GPUI's platform itself
/// (`Application::with_platform`), so there the gallery builds but doesn't run.
#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn application() -> Application {
    gpui_kit::application()
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn application() -> Application {
    panic!("the gallery runs on a desktop; an iOS or Android app hands GPUI its platform")
}

fn main() {
    let shots: Option<PathBuf> = {
        let mut it = std::env::args().skip(1);
        let mut out = None;
        while let Some(a) = it.next() {
            if a == "--shots" {
                out = it.next().map(PathBuf::from);
            }
        }
        out
    };
    application().with_assets(Assets).run(move |cx| {
        load_fonts(cx);
        gpui_kit::init(cx);
        apply_theme(true, cx);
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
        cx.on_action(|_: &Quit, cx| cx.quit());
        let offscreen = shots.is_some();
        let size = if offscreen {
            size(px(784.), px(3600.))
        } else {
            size(px(1280.), px(860.))
        };
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("OpenUI cards".into()),
                ..Default::default()
            }),
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(None, size, cx))),
            // Pictures never show the window or take the keyboard.
            focus: !offscreen,
            show: !offscreen,
            ..Default::default()
        };
        let (window, root) = gpui_kit::open_window(options, cx, |window, cx| {
            cx.new(|cx| Gallery::new(offscreen, window, cx))
        })
        .expect("the window");
        if !offscreen {
            cx.activate(true);
        }
        #[cfg(feature = "shots")]
        if let Some(dir) = shots.clone() {
            shots::run(window, root.clone(), dir, cx);
        }
        let _ = (window, root);
    });
}

#[cfg(feature = "shots")]
mod shots {
    use super::*;

    /// (picture, program, dark, width, what to do first)
    const SHOTS: &[(&str, &str, bool, f32, &str)] = &[
        ("cards-kpi", "cards-kpi", true, 736., ""),
        ("charts", "showcase-bars", true, 736., ""),
        ("charts-lines", "showcase-lines", true, 736., ""),
        ("charts-light", "showcase-bars", false, 400., ""),
        ("pies", "showcase-pies", true, 736., ""),
        ("scatter", "scatter", true, 736., ""),
        ("form", "form-inputs", true, 736., ""),
        ("form-light", "form-contact", false, 400., "submit"),
        ("table", "table", true, 736., ""),
        ("editable-table", "editable-table", true, 736., ""),
        ("content", "content", true, 736., ""),
        ("tabs-accordion", "tabs-accordion", true, 400., "open"),
        ("data-display", "data-display", true, 400., ""),
        ("cards-more", "cards-more", true, 736., ""),
        ("modal", "modal", true, 736., "modal"),
        ("hostile", "hostile-html", true, 400., ""),
        ("worst-case", "showcase-worst", true, 280., ""),
    ];

    fn draw(window: AnyWindowHandle, cx: &mut AsyncApp) {
        let _ = cx.update_window(window, |_, window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
    }

    async fn wait(cx: &mut AsyncApp, ms: u64) {
        cx.background_executor()
            .timer(std::time::Duration::from_millis(ms))
            .await;
    }

    /// A window that isn't shown gets no frames from the display, so this draws it itself.
    async fn settle(window: AnyWindowHandle, cx: &mut AsyncApp) {
        for _ in 0..4 {
            draw(window, cx);
            wait(cx, 80).await;
        }
    }

    pub fn run(window: AnyWindowHandle, root: Entity<Gallery>, dir: PathBuf, cx: &mut App) {
        let _ = std::fs::create_dir_all(&dir);
        cx.spawn(async move |cx| {
            wait(cx, 300).await;
            for (file, program, dark, width, step) in SHOTS {
                let _ = cx.update_window(window, |_, window, cx| {
                    root.update(cx, |g, cx| {
                        let i = g
                            .programs
                            .iter()
                            .position(|(n, _)| n == program)
                            .unwrap_or(0);
                        g.width = Some(*width);
                        g.set_dark(*dark, cx);
                        g.choose(i, window, cx);
                    })
                });
                settle(window, cx).await;
                let card = root.read_with(cx, |g, _| g.card.clone());
                let _ = cx.update_window(window, |_, window, cx| {
                    card.update(cx, |c, cx| match *step {
                        "submit" => {
                            c.press_label("Submit", window, cx);
                        }
                        "modal" => {
                            c.press_label("Edit", window, cx);
                        }
                        "open" => c.expand_all(cx),
                        _ => {}
                    })
                });
                settle(window, cx).await;
                let _ = cx.update_window(window, |_, window, cx| {
                    window.refresh();
                    window.draw(cx).clear(cx);
                    let scale = window.scale_factor();
                    let bounds = root.read(cx).card_bounds.get();
                    match window.render_to_image() {
                        Ok(img) => {
                            let img = crop(img, bounds, scale, *step == "modal");
                            let path = dir.join(format!("{file}.png"));
                            if let Err(e) = img.save(&path) {
                                eprintln!("{file}: the picture wasn't saved: {e}");
                            }
                        }
                        Err(e) => eprintln!("{file}: the window didn't render to a picture: {e}"),
                    }
                });
            }
            cx.update(|cx| cx.quit());
        })
        .detach();
    }

    /// The picture cut to the card and a margin around it (the whole window for a modal).
    fn crop(
        img: image::RgbaImage,
        card: Option<Bounds<Pixels>>,
        scale: f32,
        whole: bool,
    ) -> image::RgbaImage {
        let Some(b) = card.filter(|_| !whole) else {
            // The window's middle, where the dialog is.
            let h = ((640. * scale) as u32).min(img.height());
            return image::imageops::crop_imm(&img, 0, (img.height() - h) / 2, img.width(), h)
                .to_image();
        };
        let m = 24. * scale;
        let x = (f32::from(b.origin.x) * scale - m).max(0.) as u32;
        let y = (f32::from(b.origin.y) * scale - m).max(0.) as u32;
        let w = ((f32::from(b.size.width) * scale + 2. * m) as u32).min(img.width() - x);
        let h = ((f32::from(b.size.height) * scale + 2. * m) as u32).min(img.height() - y);
        image::imageops::crop_imm(&img, x, y, w, h).to_image()
    }
}
