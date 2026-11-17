//! gpui-core's containers in a window: cards.
//!
//! ```sh
//! cargo run --example containers [-- --light]
//! ```
//!
//! Tab moves through everything that takes the focus; ⌘+ and ⌘− zoom.

use gpui::*;
use gpui_core::*;

actions!(containers, [ZoomIn, ZoomOut, Quit]);

struct Containers {
    /// What was pressed last.
    pressed: SharedString,
}

fn heading(text: &'static str, theme: &Theme) -> Div {
    div()
        .text_size(theme.text.sm)
        .text_color(theme.muted_foreground)
        .child(text)
}

fn pressed(
    this: &WeakEntity<Containers>,
    what: &'static str,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let this = this.clone();
    move |_, _, cx| {
        this.update(cx, |c, cx| {
            c.pressed = what.into();
            cx.notify();
        })
        .ok();
    }
}

impl Containers {
    fn cards(&self, me: &WeakEntity<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_wrap()
            .items_start()
            .gap(rems(1.))
            .child(
                Card::new()
                    .w(rems(22.))
                    .title("Team plan")
                    .description("Five seats, billed monthly. Change the plan at any time.")
                    .action(
                        Button::new("card-edit")
                            .label("Edit")
                            .outline()
                            .small()
                            .on_click(pressed(me, "Edit")),
                    )
                    .child("Your next charge is on the 1st of the month.")
                    .footer(
                        Button::new("card-upgrade")
                            .label("Upgrade")
                            .small()
                            .on_click(pressed(me, "Upgrade")),
                    )
                    .footer(
                        Button::new("card-cancel")
                            .label("Cancel")
                            .ghost()
                            .small()
                            .on_click(pressed(me, "Cancel")),
                    ),
            )
            .child(
                Card::new()
                    .small()
                    .w(rems(16.))
                    .title("A small card with a title long enough to wrap")
                    .description("No footer, and tighter spacing.")
                    .child("Content."),
            )
            .child(Card::new().w(rems(12.)).child("Content alone."))
    }
}

impl Render for Containers {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let me = cx.entity().downgrade();
        let theme = cx.theme().clone();
        div()
            .id("page")
            .size_full()
            .overflow_y_scroll()
            .p(rems(1.5))
            .flex()
            .flex_col()
            .gap(rems(1.))
            .child(heading("Card", &theme))
            .child(self.cards(&me))
            .child(
                div()
                    .text_size(theme.text.sm)
                    .text_color(theme.muted_foreground)
                    .child(format!("Pressed: {}", self.pressed)),
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
                cx.refresh_windows();
            });
            cx.on_action(|_: &ZoomOut, cx| {
                let t = Theme::global_mut(cx);
                t.font_size = (t.font_size / 1.1).max(px(6.));
                cx.refresh_windows();
            });
            cx.on_action(|_: &Quit, cx| cx.quit());
            let bounds = Bounds::centered(None, size(px(900.), px(700.)), cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            };
            gpui_core::open_window(options, cx, |_, cx| {
                cx.new(|_| Containers {
                    pressed: "nothing yet".into(),
                })
            })
            .unwrap();
            cx.activate(true);
        });
}
