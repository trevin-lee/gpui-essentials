//! gpui-core's overlays in a window: HoverCard.
//!
//! ```sh
//! cargo run --example overlays [-- --light]
//! ```
//!
//! Tab moves through everything that takes the focus; ⌘+ and
//! ⌘− zoom.

use gpui::*;
use gpui_core::*;

actions!(overlays, [ZoomIn, ZoomOut, Quit]);

struct Overlays;

impl Render for Overlays {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let section = |title: &'static str| {
            div()
                .text_size(theme.text.sm)
                .text_color(theme.muted_foreground)
                .child(title)
        };
        div()
            .size_full()
            .p(rems(2.))
            .flex()
            .flex_col()
            .gap(rems(1.))
            .child(div().text_size(theme.text.lg).child("Overlays"))
            .child(section(
                "HoverCard: rest the pointer on the link, or Tab to it",
            ))
            .child(
                div().flex().child(
                    HoverCard::new("nextjs")
                        .trigger(Button::new("nextjs-link").link().label("@nextjs"))
                        .flex()
                        .flex_col()
                        .gap(rems(0.25))
                        .child(div().font_weight(FontWeight::SEMIBOLD).child("@nextjs"))
                        .child("The React Framework, created and maintained by @vercel.")
                        .child(
                            div()
                                .text_size(theme.text.xs)
                                .text_color(theme.muted_foreground)
                                .child("Joined December 2021"),
                        ),
                ),
            )
    }
}

fn main() {
    let light = std::env::args().any(|a| a == "--light");
    gpui_platform::application()
        .with_assets(Icons)
        .run(move |cx| {
            gpui_core::init(cx);
            Theme::set(if light { Theme::light() } else { Theme::dark() }, cx);
            cx.bind_keys([
                KeyBinding::new("cmd-=", ZoomIn, None),
                KeyBinding::new("cmd--", ZoomOut, None),
                KeyBinding::new("cmd-q", Quit, None),
            ]);
            cx.on_action(|_: &ZoomIn, cx| {
                let t = Theme::global_mut(cx);
                t.font_size = (t.font_size * 1.1).min(px(60.));
            });
            cx.on_action(|_: &ZoomOut, cx| {
                let t = Theme::global_mut(cx);
                t.font_size = (t.font_size / 1.1).max(px(6.));
            });
            cx.on_action(|_: &Quit, cx| cx.quit());
            let bounds = Bounds::centered(None, size(px(800.), px(600.)), cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            };
            gpui_core::open_window(options, cx, |_, cx| cx.new(|_| Overlays)).unwrap();
            cx.activate(true);
        });
}
