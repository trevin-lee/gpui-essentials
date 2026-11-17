//! gpui-core's form controls in a window.
//!
//! ```sh
//! cargo run --example forms [-- --light]
//! ```
//!
//! Tab moves through everything that takes the focus; ⌘+ and ⌘− zoom.

use gpui::*;
use gpui_core::*;

actions!(forms, [ZoomIn, ZoomOut, Quit]);

struct Forms {
    terms: bool,
    marketing: bool,
    /// The three items under the "select all" check box.
    items: [bool; 3],
}

fn heading(text: &'static str, theme: &Theme) -> Div {
    div()
        .text_size(theme.text.sm)
        .text_color(theme.muted_foreground)
        .child(text)
}

impl Forms {
    fn checkboxes(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let all = self.items.iter().all(|i| *i);
        let some = self.items.iter().any(|i| *i);
        div()
            .flex()
            .flex_col()
            .gap(rems(0.75))
            .child(heading("Checkbox", &theme))
            .child(
                Checkbox::new("terms")
                    .label("Accept terms and conditions")
                    .checked(self.terms)
                    .on_click(cx.listener(|this, on: &bool, _, cx| {
                        this.terms = *on;
                        cx.notify();
                    })),
            )
            .child(
                Checkbox::new("marketing")
                    .label("Email me about new features, now and then, and about nothing else at all — a long label wraps onto a second line rather than running out of its box")
                    .checked(self.marketing)
                    .on_click(cx.listener(|this, on: &bool, _, cx| {
                        this.marketing = *on;
                        cx.notify();
                    })),
            )
            .child(
                Checkbox::new("disabled")
                    .label("Disabled")
                    .checked(true)
                    .disabled(true),
            )
            .child(
                Checkbox::new("invalid")
                    .label("Invalid: required")
                    .invalid(true),
            )
            .child(
                Checkbox::new("all")
                    .label("Select all")
                    .checked(all)
                    .indeterminate(some && !all)
                    .on_click(cx.listener(|this, on: &bool, _, cx| {
                        this.items = [*on; 3];
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .pl(rems(1.5))
                    .flex()
                    .flex_col()
                    .gap(rems(0.5))
                    .children(["Inbox", "Drafts", "Archive"].into_iter().enumerate().map(
                        |(i, name)| {
                            Checkbox::new(("item", i))
                                .label(name)
                                .checked(self.items[i])
                                .on_click(cx.listener(move |this, on: &bool, _, cx| {
                                    this.items[i] = *on;
                                    cx.notify();
                                }))
                        },
                    )),
            )
            .child(
                Checkbox::new("touch")
                    .label("Touch size: 44 points tall")
                    .checked(self.terms)
                    .touch()
                    .on_click(cx.listener(|this, on: &bool, _, cx| {
                        this.terms = *on;
                        cx.notify();
                    })),
            )
    }
}

impl Render for Forms {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("forms")
            .size_full()
            .overflow_y_scroll()
            .p(rems(1.5))
            .flex()
            .flex_col()
            .gap(rems(2.))
            .max_w(rems(36.))
            .child(self.checkboxes(cx))
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
            let bounds = Bounds::centered(None, size(px(720.), px(760.)), cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            };
            gpui_core::open_window(options, cx, |_, cx| {
                cx.new(|_| Forms {
                    terms: true,
                    marketing: false,
                    items: [true, false, false],
                })
            })
            .expect("the window");
            cx.activate(true);
        });
}
