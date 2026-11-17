//! gpui-core's navigation components in a window.
//!
//! ```sh
//! cargo run --example navigation [-- --light]
//! ```
//!
//! Tab moves through everything that takes the focus; ⌘+ and ⌘− zoom.

use gpui::*;
use gpui_core::*;

actions!(navigation, [ZoomIn, ZoomOut, Quit]);

struct Navigation {
    /// The settings tabs' selection, owned here.
    tab: SharedString,
}

#[allow(dead_code)]
fn heading(text: &'static str, theme: &Theme) -> Div {
    div()
        .text_size(theme.text.sm)
        .text_color(theme.muted_foreground)
        .child(text)
}

#[allow(dead_code)]
fn note(text: String, theme: &Theme) -> Div {
    div()
        .text_size(theme.text.sm)
        .text_color(theme.muted_foreground)
        .child(text)
}

#[allow(dead_code)]
fn panel(title: &'static str, body: &'static str, theme: &Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(rems(0.25))
        .p(rems(1.))
        .rounded(theme.radius_lg())
        .border_1()
        .border_color(theme.border)
        .bg(theme.card)
        .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
        .child(
            div()
                .text_size(theme.text.sm)
                .text_color(theme.muted_foreground)
                .child(body),
        )
}

impl Render for Navigation {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        div()
            .id("navigation")
            .size_full()
            .overflow_y_scroll()
            .p(rems(1.5))
            .flex()
            .flex_col()
            .gap(rems(1.))
            .child(heading("Tabs", &theme))
            .child(
                Tabs::new("settings")
                    .accessibility_label("Settings")
                    .tab(Tab::new("account", "Account").child(panel(
                        "Account",
                        "Make changes to your account here.",
                        &theme,
                    )))
                    .tab(Tab::new("password", "Password").child(panel(
                        "Password",
                        "Change your password here.",
                        &theme,
                    )))
                    .tab(Tab::new("billing", "Billing").disabled(true))
                    .selected(self.tab.clone())
                    .on_change(cx.listener(|this, id: &SharedString, _, cx| {
                        this.tab = id.clone();
                        cx.notify();
                    })),
            )
            .child(heading("Tabs, underlined, left to themselves", &theme))
            .child(
                Tabs::new("views")
                    .line()
                    .accessibility_label("Views")
                    .tab(Tab::new("overview", "Overview").icon(IconName::LayoutDashboard))
                    .tab(Tab::new("analytics", "Analytics").icon(IconName::ChartLine))
                    .tab(Tab::new("reports", "Reports").icon(IconName::FileText)),
            )
            .child(heading("Tabs that scroll in a narrow box", &theme))
            .child(
                div().w(rems(16.)).child(
                    Tabs::new("many")
                        .overflow(TabsOverflow::Scroll)
                        .accessibility_label("Months")
                        .tabs(
                            [
                                "January", "February", "March", "April", "May", "June", "July",
                                "August",
                            ]
                            .map(|m| Tab::new(m, m)),
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
            let bounds = Bounds::centered(None, size(px(1000.), px(700.)), cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            };
            gpui_core::open_window(options, cx, |_, cx| {
                cx.new(|cx| {
                    let _ = &cx;
                    Navigation {
                        tab: "account".into(),
                    }
                })
            })
            .expect("the window");
            cx.activate(true);
        });
}
