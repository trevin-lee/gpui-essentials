//! gpui-core's menus in a window: DropdownMenu.
//!
//! ```sh
//! cargo run --example menus [-- --light]
//! ```
//!
//! Tab moves through everything that takes the focus; ⌘+ and ⌘− zoom.

use gpui::*;
use gpui_core::*;

actions!(menus, [ZoomIn, ZoomOut, Quit]);

struct Menus {
    picked: Option<SharedString>,
    status_bar: bool,
    activity_bar: bool,
}

fn heading(text: &'static str, theme: &Theme) -> Div {
    div()
        .text_size(theme.text.sm)
        .text_color(theme.muted_foreground)
        .child(text)
}

impl Menus {
    /// Remembers a pick, and flips a checked entry.
    fn picked(cx: &mut Context<Self>) -> impl Fn(&MenuSelect, &mut Window, &mut App) + 'static {
        cx.listener(|this, pick: &MenuSelect, _, cx| {
            match pick.id.as_ref() {
                "status-bar" => this.status_bar = !this.status_bar,
                "activity-bar" => this.activity_bar = !this.activity_bar,
                _ => {}
            }
            this.picked = Some(pick.id.clone());
            cx.notify();
        })
    }

    fn dropdowns(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        div()
            .flex()
            .flex_col()
            .gap(rems(0.75))
            .child(heading("DropdownMenu", &theme))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(rems(0.75))
                    .child(
                        DropdownMenu::new("account")
                            .trigger(Button::new("account-button").outline().label("Open"))
                            .heading("My Account")
                            .items([
                                PopupMenuItem::new("profile", "Profile").shortcut("⇧⌘P"),
                                PopupMenuItem::new("billing", "Billing").shortcut("⌘B"),
                                PopupMenuItem::new("settings", "Settings").shortcut("⌘S"),
                            ])
                            .separator()
                            .item(PopupMenuItem::new("team", "Team"))
                            .item(
                                PopupMenuItem::new("invite", "Invite users").submenu(
                                    PopupMenu::new("invite-menu")
                                        .item(PopupMenuItem::new("email", "Email"))
                                        .item(PopupMenuItem::new("message", "Message")),
                                ),
                            )
                            .item(PopupMenuItem::new("api", "API").disabled(true))
                            .separator()
                            .item(PopupMenuItem::new("log-out", "Log out").shortcut("⇧⌘Q"))
                            .on_select(Self::picked(cx)),
                    )
                    .child(
                        DropdownMenu::new("view")
                            .trigger(Button::new("view-button").outline().label("View"))
                            .heading("Appearance")
                            .item(
                                PopupMenuItem::new("status-bar", "Status bar")
                                    .checked(self.status_bar),
                            )
                            .item(
                                PopupMenuItem::new("activity-bar", "Activity bar")
                                    .checked(self.activity_bar),
                            )
                            .align(PopoverAlign::Center)
                            .on_select(Self::picked(cx)),
                    )
                    .child(
                        DropdownMenu::new("more")
                            .trigger(
                                Button::new("more-button")
                                    .ghost()
                                    .icon(IconName::Ellipsis)
                                    .accessibility_label("More"),
                            )
                            .items([
                                PopupMenuItem::new("edit", "Edit").icon(IconName::Pencil),
                                PopupMenuItem::new("delete", "Delete").icon(IconName::Trash),
                            ])
                            .align(PopoverAlign::End)
                            .on_select(Self::picked(cx)),
                    )
                    .child(
                        DropdownMenu::new("above")
                            .trigger(Button::new("above-button").secondary().label("Above"))
                            .items([
                                PopupMenuItem::new("one", "Opens above"),
                                PopupMenuItem::new("two", "Where there's room"),
                            ])
                            .side(PopoverSide::Top)
                            .on_select(Self::picked(cx)),
                    )
                    .child(
                        DropdownMenu::new("disabled")
                            .trigger(Button::new("disabled-button").outline().label("Disabled"))
                            .item(PopupMenuItem::new("none", "Never shown"))
                            .disabled(true),
                    ),
            )
            .child(
                div()
                    .text_size(theme.text.sm)
                    .text_color(theme.muted_foreground)
                    .child(match &self.picked {
                        Some(id) => format!("Picked: {id}"),
                        None => "Pick something".into(),
                    }),
            )
    }
}

impl Render for Menus {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        div()
            .id("menus")
            .size_full()
            .overflow_y_scroll()
            .p(rems(2.))
            .flex()
            .flex_col()
            .gap(rems(2.))
            .child(div().text_size(theme.text.lg).child("Menus"))
            .child(self.dropdowns(cx))
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
            gpui_core::open_window(options, cx, |_, cx| {
                cx.new(|_| Menus {
                    picked: None,
                    status_bar: true,
                    activity_bar: false,
                })
            })
            .unwrap();
            cx.activate(true);
        });
}
