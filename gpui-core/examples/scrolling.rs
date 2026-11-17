//! gpui-core's scrolling components in a window.
//!
//! ```sh
//! cargo run --example scrolling [-- --light]
//! ```
//!
//! Hover or scroll an area to see its bars; Tab to one and the arrows, Page Up and Page Down,
//! Space, Home and End scroll it. ⌘+ and ⌘− zoom.

use gpui::*;
use gpui_core::*;

actions!(scrolling, [ZoomIn, ZoomOut, Quit]);

struct Scrolling {
    /// The long list's scroll, for the buttons that move it.
    tags: ScrollAreaHandle,
}

fn heading(text: &'static str, theme: &Theme) -> Div {
    div()
        .text_size(theme.text.sm)
        .text_color(theme.muted_foreground)
        .child(text)
}

impl Render for Scrolling {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let tags = ScrollArea::new("tags")
            .accessibility_label("Tags")
            .track_scroll(&self.tags)
            .h(rems(18.))
            .w(rems(12.))
            .rounded(theme.radius_md())
            .border_1()
            .border_color(theme.border)
            // The tags are the area's own children, so the handle can scroll to one.
            .child(
                div()
                    .px(rems(1.))
                    .pt(rems(1.))
                    .pb(rems(0.5))
                    .text_size(theme.text.sm)
                    .font_weight(FontWeight::MEDIUM)
                    .child("Tags"),
            )
            .children((1..=50).rev().map(|i| {
                div()
                    .mx(rems(1.))
                    .text_size(theme.text.sm)
                    .py(rems(0.5))
                    .border_b_1()
                    .border_color(theme.border)
                    .child(format!("v1.2.0-beta.{i}"))
            }));
        let artworks = ScrollArea::new("artworks")
            .accessibility_label("Artworks")
            .horizontal()
            .w(rems(24.))
            .rounded(theme.radius_md())
            .border_1()
            .border_color(theme.border)
            .child(
                div().flex().gap(rems(1.)).p(rems(1.)).children(
                    [
                        "Ornella Binni",
                        "Tom Byrom",
                        "Vladimir Malyavko",
                        "Ana",
                        "Bo",
                    ]
                    .map(|name| {
                        div()
                            .flex_none()
                            .flex()
                            .flex_col()
                            .gap(rems(0.5))
                            .child(
                                div()
                                    .w(rems(9.))
                                    .h(rems(12.))
                                    .rounded(theme.radius_md())
                                    .bg(theme.muted),
                            )
                            .child(
                                div()
                                    .text_size(theme.text.xs)
                                    .text_color(theme.muted_foreground)
                                    .child(format!("Photo by {name}")),
                            )
                    }),
                ),
            );
        let grid = ScrollArea::new("grid")
            .accessibility_label("A grid")
            .both()
            .w(rems(20.))
            .h(rems(12.))
            .rounded(theme.radius_md())
            .border_1()
            .border_color(theme.border)
            .child(
                div()
                    .p(rems(1.))
                    .flex()
                    .flex_col()
                    .gap(rems(0.25))
                    .children((1..=30).map(|row| {
                        div()
                            .flex()
                            .gap(rems(0.25))
                            .children((1..=12).map(move |col| {
                                div()
                                    .flex_none()
                                    .w(rems(4.))
                                    .text_size(theme.text.xs)
                                    .child(format!("R{row} C{col}"))
                            }))
                    })),
            );
        let handle = self.tags.clone();
        let handle2 = self.tags.clone();
        div()
            .size_full()
            .p(rems(2.))
            .flex()
            .flex_col()
            .gap(rems(1.5))
            .text_color(theme.foreground)
            .child(heading("ScrollArea", &theme))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_start()
                    .gap(rems(2.))
                    .child(tags)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(rems(0.5))
                            .child(Button::new("top").label("To the top").outline().on_click(
                                cx.listener(move |_, _, _, cx| {
                                    handle.scroll_to_top();
                                    cx.notify();
                                }),
                            ))
                            .child(
                                Button::new("beta-25")
                                    .label("To beta.25")
                                    .outline()
                                    .on_click(cx.listener(move |_, _, _, cx| {
                                        // After the heading, beta.50 down to beta.1.
                                        handle2.scroll_to_child_top(1 + 50 - 25);
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .child(heading("Sideways", &theme))
            .child(artworks)
            .child(heading("Both ways", &theme))
            .child(grid)
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
            let bounds = Bounds::centered(None, size(px(760.), px(720.)), cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            };
            gpui_core::open_window(options, cx, |_, cx| {
                cx.new(|_| Scrolling {
                    tags: ScrollAreaHandle::new(),
                })
            })
            .expect("the window");
            cx.activate(true);
        });
}
