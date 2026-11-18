//! gpui-core's display components in a window: badges, keys, separators, skeletons, spinners,
//! progress bars, avatars, empty states and boxes that keep a ratio.
//!
//! ```sh
//! cargo run --example display [-- --light]
//! ```

use std::sync::Arc;

use gpui::*;
use gpui_core::*;

actions!(display, [Quit]);

struct Display {
    pressed: SharedString,
    /// A picture the app already has (here an SVG in the binary): avatars never fetch one.
    photo: Arc<Image>,
}

const PHOTO: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">
<defs><linearGradient id="g" x1="0" y1="0" x2="1" y2="1">
<stop offset="0" stop-color="#7c3aed"/><stop offset="1" stop-color="#0ea5e9"/></linearGradient></defs>
<rect width="64" height="64" fill="url(#g)"/>
<circle cx="32" cy="26" r="11" fill="#fde68a"/>
<path d="M12 60c2-12 10-18 20-18s18 6 20 18z" fill="#fde68a"/></svg>"##;

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
            ) // Kbd
            .child(heading("Kbd", &theme))
            .child(
                row()
                    .child(Kbd::new("Esc"))
                    .child(Kbd::new("Shift").icon(IconName::ArrowBigUp))
                    .child(Kbd::icon_only(IconName::CornerDownLeft))
                    .child(KbdGroup::keys(["⌘", "K"]))
                    .child(KbdGroup::keys(["Ctrl", "Shift", "P"])),
            ) // Separator
            .child(heading("Separator", &theme))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(rems(0.75))
                    .max_w(rems(24.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(rems(0.25))
                            .child(div().text_size(theme.text.sm).child("gpui-core"))
                            .child(
                                div()
                                    .text_size(theme.text.sm)
                                    .text_color(theme.muted_foreground)
                                    .child("Components for GPUI apps."),
                            ),
                    )
                    .child(Separator::horizontal())
                    .child(
                        div()
                            .flex()
                            .h(rems(1.25))
                            .items_center()
                            .gap(rems(1.))
                            .text_size(theme.text.sm)
                            .child("Blog")
                            .child(Separator::vertical().decorative(false).id("sep-1"))
                            .child("Docs")
                            .child(Separator::vertical().decorative(false).id("sep-2"))
                            .child("Source"),
                    ),
            ) // Progress
            .child(heading("Progress", &theme))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(rems(0.75))
                    .max_w(rems(20.))
                    .child(Progress::new("p-0").value(0.))
                    .child(
                        Progress::new("p-33")
                            .value(33.)
                            .accessibility_label("Uploading"),
                    )
                    .child(Progress::new("p-100").value(100.))
                    .child(Progress::new("p-sync").accessibility_label("Syncing")),
            ) // Spinner
            .child(heading("Spinner", &theme))
            .child(
                row()
                    .child(Spinner::new("xs").xsmall())
                    .child(Spinner::new("sm").small())
                    .child(Spinner::new("md"))
                    .child(Spinner::new("lg").large())
                    .child(
                        Spinner::new("big")
                            .size(rems(2.))
                            .color(theme.muted_foreground),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(rems(0.5))
                            .text_size(theme.text.sm)
                            .child(Spinner::new("saving").small())
                            .child("Saving…"),
                    ),
            ) // Skeleton
            .child(heading("Skeleton", &theme))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(rems(1.))
                    .child(Skeleton::new().size(rems(3.)).rounded_full())
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(rems(0.5))
                            .child(Skeleton::new().h(rems(1.)).w(rems(16.)))
                            .child(Skeleton::new().h(rems(1.)).w(rems(12.))),
                    )
                    .child(Skeleton::new().animate(false).h(rems(3.)).w(rems(6.))),
            ) // Avatar
            .child(heading("Avatar", &theme))
            .child(
                row()
                    .gap(rems(1.))
                    .child(Avatar::new("Picture").image(self.photo.clone()).small())
                    .child(
                        Avatar::new("Picture")
                            .id("photo-2")
                            .image(self.photo.clone()),
                    )
                    .child(Avatar::new("Ada Lovelace"))
                    .child(Avatar::new("Grace Hopper").large())
                    .child(Avatar::new("").small())
                    .child(
                        AvatarGroup::new("team")
                            .accessibility_label("Team")
                            .max(3)
                            .children(
                                [
                                    "Ada Lovelace",
                                    "Grace Hopper",
                                    "Alan Turing",
                                    "Edsger Dijkstra",
                                ]
                                .map(Avatar::new),
                            ),
                    )
                    .child(
                        AvatarGroup::new("small-team")
                            .small()
                            .children(["Ann Lee", "Bo Diaz"].map(Avatar::new))
                            .child(Avatar::new("Picture").image(self.photo.clone())),
                    ),
            ) // AspectRatio
            .child(heading("AspectRatio", &theme))
            .child(div().flex().items_start().gap(rems(1.)).children(
                [(16. / 9., "16 / 9"), (4. / 3., "4 / 3"), (1., "1 / 1")].map(|(ratio, name)| {
                    AspectRatio::new(ratio)
                        .w(rems(10.))
                        .rounded(theme.radius_lg())
                        .bg(theme.muted)
                        .child(
                            div()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(theme.text.sm)
                                .text_color(theme.muted_foreground)
                                .child(name),
                        )
                }),
            )) // EmptyState
            .child(heading("EmptyState", &theme))
            .child(
                div()
                    .flex()
                    .h(rems(18.))
                    .rounded(theme.radius_lg())
                    .border_1()
                    .border_dashed()
                    .border_color(theme.border)
                    .child(
                        EmptyState::new()
                            .icon(IconName::Inbox)
                            .title("No messages yet")
                            .description(
                                "When an agent writes to you, its message shows up here, \
                                 newest first.",
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap(rems(0.5))
                                    .child(Button::new("start").label("Start a chat"))
                                    .child(Button::new("import").outline().label("Import")),
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
                    photo: Arc::new(Image::from_bytes(
                        ImageFormat::Svg,
                        PHOTO.as_bytes().to_vec(),
                    )),
                })
            })
            .expect("the window");
            cx.activate(true);
        });
}
