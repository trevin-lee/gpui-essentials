//! Math in a reply, as `StreamingMarkdown` draws it: inline `$…$` as its TeX in the code font,
//! and each `$$…$$` as a block of its own whose TeX stays on its lines and scrolls sideways when
//! it's wider than the reply. The cases the renderer is tested on, two real replies
//! (examples/replies/career-math.md) and one caught mid-equation, at a phone's width (402 points)
//! and a Mac chat's (720).
//!
//! ```sh
//! cargo run --release --example math
//! cargo run --release --features shots --example math -- --shots /tmp/math
//! ```
//!
//! With `--shots <dir>` it draws each from a window that's never shown, saves
//! `<name>-402.png` and `<name>-720.png`, then quits.

use std::path::PathBuf;

use gpui::*;
use gpui_agents::*;
use gpui_core::Theme;
use gpui_core::text::TextViewStyle;

/// The cases the renderer is tested on.
const CASES: &str = r"**Inline**, its TeX as written: the model $x_{k+1} = F x_k + w$, an entry $J_{ij}$, $2n+1$ sigma points, an $m \times n$ matrix ($J_{ij}$), and escapes kept, $a \\ b \, c \{ d \} e \_ f & g * h$.

**Prices stay words:** it costs $5 and $10, about \$230 a month.

**Display**, each a block of its own:
$$\frac{\partial f_i}{\partial x_j}$$
$$a \qquad b$$
$$f(x + \Delta x) \approx f(x) + J\,\Delta x$$
$$J = \begin{pmatrix} \cos\theta & -v\sin\theta \\ \sin\theta & v\cos\theta \end{pmatrix}$$

**A long one**, which scrolls sideways where it doesn't fit:
$$f(x) = a_0 + a_1 x + a_2 x^2 + a_3 x^3 + a_4 x^4 + a_5 x^5 + a_6 x^6 + a_7 x^7 + a_8 x^8 + a_9 x^9 + a_{10} x^{10}$$

The words after it start below it.
";

const CAREER: &str = include_str!("replies/career-math.md");

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
        code_block: code,
        ..Default::default()
    }
}

/// A reply in a chat's column.
struct Page {
    reply: Option<Entity<StreamingMarkdown>>,
}

impl Render for Page {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgb(0x111114))
            .font_family("Inter")
            .text_size(px(14.))
            .line_height(px(14. * 1.625))
            .text_color(rgb(0xe8e8ea))
            .p(px(16.))
            .children(self.reply.clone())
    }
}

/// The texts drawn, by name: the cases, each of Career's replies, and the second caught inside
/// its matrix (written, not finished).
fn texts() -> Vec<(&'static str, String, bool)> {
    let mut replies = CAREER.split("### ").filter(|s| !s.trim().is_empty());
    let first = replies.next().unwrap_or_default();
    let second = replies.next().unwrap_or_default();
    // Without the header line (when it was sent, and by whom).
    let body = |s: &str| s.split_once('\n').map_or("", |(_, b)| b).trim().to_string();
    let second = body(second);
    let cut = second
        .find(r"\sin\theta \\")
        .map_or(second.len(), |i| i + r"\sin\theta \\".len());
    vec![
        ("cases", CASES.to_string(), true),
        ("career-ekf", body(first), true),
        ("career-jacobian", second.clone(), true),
        ("career-mid-matrix", second[..cut].to_string(), false),
    ]
}

#[cfg(target_os = "macos")]
fn application() -> Application {
    gpui_platform::application()
}

#[cfg(not(target_os = "macos"))]
fn application() -> Application {
    panic!("the example runs on a Mac; an iOS or Android app hands GPUI its platform")
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut shots: Option<PathBuf> = None;
    while let Some(a) = args.next() {
        if a == "--shots" {
            shots = args.next().map(PathBuf::from);
        }
    }
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
        let offscreen = shots.is_some();
        let widths: &[f32] = if offscreen { &[402., 720.] } else { &[720.] };
        for &w in widths {
            for (name, text, done) in texts() {
                let height = if w < 500. { 2000. } else { 1300. };
                let options = WindowOptions {
                    titlebar: Some(TitlebarOptions {
                        title: Some(format!("Math: {name}").into()),
                        ..Default::default()
                    }),
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(w), px(height)),
                        cx,
                    ))),
                    focus: !offscreen,
                    show: !offscreen,
                    ..Default::default()
                };
                let (window, _) = gpui_core::open_window(options, cx, move |_, cx| {
                    let reply = cx.new(|cx| {
                        let mut r = StreamingMarkdown::new(format!("math-{name}"))
                            .style(style())
                            .selectable(true)
                            .touch(w < 500.);
                        if done {
                            r.finish(&text, cx);
                        } else {
                            r.append(&text, cx);
                        }
                        r
                    });
                    cx.new(|_| Page { reply: Some(reply) })
                })
                .expect("the window");
                if let Some(dir) = &shots {
                    shot(window, dir, &format!("{name}-{w}"), cx);
                    let _ = cx.update_window(window, |_, window, _| window.remove_window());
                }
                if !offscreen {
                    break;
                }
            }
        }
        if offscreen {
            cx.quit();
        } else {
            cx.activate(true);
        }
    });
}

/// Saves the window as a picture (with the `shots` feature).
#[allow(unused_variables)]
fn shot(window: AnyWindowHandle, dir: &std::path::Path, name: &str, cx: &mut App) {
    #[cfg(feature = "shots")]
    {
        let _ = std::fs::create_dir_all(dir);
        let _ = cx.update_window(window, |_, window, cx| {
            // A window that isn't shown gets no frames from the display: draw it here (twice,
            // for what's laid out from the first frame's sizes).
            for _ in 0..3 {
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
