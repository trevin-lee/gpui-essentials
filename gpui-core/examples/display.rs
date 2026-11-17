//! gpui-core's display components in a window: badges, keys, separators, skeletons, spinners,
//! progress bars, avatars, empty states and boxes that keep a ratio.
//!
//! ```sh
//! cargo run --example display [-- --light]
//! ```

use gpui::*;
use gpui_core::*;

actions!(display, [Quit]);

struct Display {
    pressed: SharedString,
}

fn heading(text: &'static str, theme: &Theme) -> impl IntoElement {
    div()
        .mt(rems(1.))
        .text_size(theme.text.sm)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(theme.muted_foreground)
        .child(text)
}

fn row() -> Div {
    div().flex().flex_wrap().items_center().gap(rems(0.5))
}

impl Render for Display {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let me = cx.entity().downgrade();
        let pressed = move |what: &'static str| {
            let me = me.clone();
            move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                let _ = me.update(cx, |this, cx| {
                    this.pressed = what.into();
                    cx.notify();
                });
            }
        };
        div()
            .id("display")
            .size_full()
            .overflow_y_scroll()
            .p(rems(1.5))
            .flex()
            .flex_col()
            .gap(rems(0.5))
            .text_color(theme.foreground)
            .text_size(theme.text.base)
            // Badge
            .child(heading("Badge", &theme))
            .child(
                row()
                    .child(Badge::new("Badge"))
                    .child(Badge::new("Secondary").secondary())
                    .child(Badge::new("Destructive").destructive())
                    .child(Badge::new("Outline").outline())
                    .child(
                        Badge::new("Verified")
                            .icon(IconName::BadgeCheck)
                            .secondary(),
                    )
                    .child(
                        Badge::new("Press me")
                            .outline()
                            .on_click(pressed("the badge")),
                    ),
            )
            .child(
                div()
                    .w(rems(8.))
                    .flex()
                    .child(Badge::new("A badge with a label far too long for it").outline()),
            )
            .child(
                div()
                    .text_size(theme.text.sm)
                    .text_color(theme.muted_foreground)
                    .child(format!("Last pressed: {}", self.pressed)),
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
            cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
            cx.on_action(|_: &Quit, cx| cx.quit());
            let bounds = Bounds::centered(None, size(px(720.), px(640.)), cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            };
            gpui_core::open_window(options, cx, |_, cx| {
                cx.new(|_| Display {
                    pressed: "nothing yet".into(),
                })
            })
            .expect("the window");
            cx.activate(true);
        });
}
