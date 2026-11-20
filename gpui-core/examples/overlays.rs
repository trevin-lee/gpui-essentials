//! gpui-core's overlays in a window: Dialog, AlertDialog, HoverCard, Drawer, Toast, Sheet and
//! CommandDialog (⌘K).
//!
//! ```sh
//! cargo run --example overlays [-- --light]
//! ```
//!
//! Tab moves through everything that takes the focus (inside an open dialog, round it); ⌘+ and
//! ⌘− zoom.

use gpui::*;
use gpui_core::*;

actions!(overlays, [ZoomIn, ZoomOut, Quit, OpenCommands]);

struct Overlays {
    dialog: bool,
    alert: bool,
    drawer: Option<DrawerSide>,
    sheet: bool,
    commands: bool,
    /// The page's own focus, so ⌘K reaches it before anything else has the keyboard.
    focus: FocusHandle,
    name: Entity<InputState>,
    said: SharedString,
}

impl Overlays {
    fn set(&mut self, said: &str, cx: &mut Context<Self>) {
        self.said = said.to_string().into();
        cx.notify();
    }
}

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
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &OpenCommands, _, cx| {
                this.commands = true;
                cx.notify();
            }))
            .p(rems(2.))
            .flex()
            .flex_col()
            .gap(rems(1.))
            .child(div().text_size(theme.text.lg).child("Overlays"))
            .child(
                div()
                    .text_size(theme.text.sm)
                    .text_color(theme.muted_foreground)
                    .child(format!("Last: {}", self.said)),
            )
            .child(section("Dialog"))
            .child(
                div()
                    .flex()
                    .gap(rems(0.5))
                    .child(
                        Button::new("open-dialog")
                            .outline()
                            .label("Edit profile")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.dialog = true;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("open-alert")
                            .destructive()
                            .label("Delete the project")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.alert = true;
                                cx.notify();
                            })),
                    ),
            )
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
            .child(section("Drawer"))
            .child(
                div().flex().gap(rems(0.5)).children(
                    [
                        (DrawerSide::Bottom, "Bottom"),
                        (DrawerSide::Top, "Top"),
                        (DrawerSide::Left, "Left"),
                        (DrawerSide::Right, "Right"),
                    ]
                    .map(|(side, label)| {
                        Button::new(label)
                            .outline()
                            .label(label)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.drawer = Some(side);
                                cx.notify();
                            }))
                    }),
                ),
            )
            .child(
                Drawer::new("goal")
                    .open(self.drawer.is_some())
                    .side(self.drawer.unwrap_or_default())
                    .title("Move goal")
                    .description("Set your daily activity goal.")
                    .on_dismiss(cx.listener(|this, reason: &DismissReason, _, cx| {
                        this.drawer = None;
                        this.set(&format!("drawer dismissed ({reason:?})"), cx);
                    }))
                    .child(
                        div()
                            .py(rems(2.))
                            .flex()
                            .justify_center()
                            .text_size(rems(3.))
                            .child("350"),
                    )
                    .footer(
                        Button::new("goal-submit")
                            .label("Submit")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.drawer = None;
                                this.set("goal submitted", cx);
                            })),
                    )
                    .footer(
                        Button::new("goal-cancel")
                            .outline()
                            .label("Cancel")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.drawer = None;
                                this.set("drawer cancelled", cx);
                            })),
                    ),
            )
            .child(section("Toast"))
            .child(
                div()
                    .flex()
                    .gap(rems(0.5))
                    .child(
                        Button::new("toast-plain")
                            .outline()
                            .label("Save")
                            .on_click(|_, window, cx| Toast::new("Saved.").show(window, cx)),
                    )
                    .child(
                        Button::new("toast-success")
                            .outline()
                            .label("Copy the password")
                            .on_click(|_, window, cx| {
                                Toast::success("Copied the password.")
                                    .description("It leaves the clipboard in a minute.")
                                    .show(window, cx)
                            }),
                    )
                    .child(
                        Button::new("toast-error")
                            .outline()
                            .label("Fail, with Retry")
                            .on_click(|_, window, cx| {
                                Toast::error("Couldn't reach the server.")
                                    .action("Retry", |_, _| {})
                                    .show(window, cx)
                            }),
                    ),
            )
            .child(section("Sheet (touch)"))
            .child(
                div().flex().child(
                    Button::new("open-sheet")
                        .outline()
                        .label("Open a sheet")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.sheet = true;
                            cx.notify();
                        })),
                ),
            )
            .child(
                Sheet::new("sheet")
                    .open(self.sheet)
                    .on_dismiss(cx.listener(|this, reason: &DismissReason, _, cx| {
                        this.sheet = false;
                        this.set(&format!("sheet dismissed ({reason:?})"), cx);
                    }))
                    .child(
                        div()
                            .p(rems(1.5))
                            .child("A panel across the bottom of the window, for touch."),
                    ),
            )
            .child(section("Command"))
            .child(
                div().flex().child(
                    Button::new("open-commands")
                        .outline()
                        .label("Open the command palette (⌘K)")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.commands = true;
                            cx.notify();
                        })),
                ),
            )
            .child(
                CommandDialog::new("commands")
                    .open(self.commands)
                    .group(
                        "Suggestions",
                        [
                            PopupMenuItem::new("calendar", "Calendar").icon(IconName::Calendar),
                            PopupMenuItem::new("search", "Search emoji").icon(IconName::Search),
                            PopupMenuItem::new("calculator", "Calculator").disabled(true),
                        ],
                    )
                    .group(
                        "Settings",
                        [
                            PopupMenuItem::new("profile", "Profile").shortcut("⌘P"),
                            PopupMenuItem::new("billing", "Billing").shortcut("⌘B"),
                            PopupMenuItem::new("settings", "Settings").shortcut("⌘S"),
                        ],
                    )
                    .on_select(cx.listener(|this, pick: &MenuSelect, _, cx| {
                        this.commands = false;
                        this.set(&format!("ran {}", pick.id), cx);
                    }))
                    .on_dismiss(cx.listener(|this, _, _, cx| {
                        this.commands = false;
                        this.set("palette dismissed", cx);
                    })),
            )
            .child(
                Dialog::new("profile")
                    .open(self.dialog)
                    .title("Edit profile")
                    .description("Make changes to your profile here. Save when you're done.")
                    .on_dismiss(cx.listener(|this, reason: &DismissReason, _, cx| {
                        this.dialog = false;
                        this.set(&format!("dialog dismissed ({reason:?})"), cx);
                    }))
                    .gap(rems(0.5))
                    .child(
                        div()
                            .text_size(theme.text.sm)
                            .font_weight(FontWeight::MEDIUM)
                            .child("Name"),
                    )
                    .child(Input::new(&self.name))
                    .footer(
                        Button::new("profile-cancel")
                            .outline()
                            .label("Cancel")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.dialog = false;
                                this.set("dialog cancelled", cx);
                            })),
                    )
                    .footer(Button::new("profile-save").label("Save changes").on_click(
                        cx.listener(|this, _, _, cx| {
                            this.dialog = false;
                            let name = this.name.read(cx).value();
                            this.set(&format!("saved “{name}”"), cx);
                        }),
                    )),
            )
            .child(
                AlertDialog::new("delete")
                    .open(self.alert)
                    .title("Delete the project?")
                    .description(
                        "This can't be undone. Its files and history are deleted with it.",
                    )
                    .on_dismiss(cx.listener(|this, _, _, cx| {
                        this.alert = false;
                        this.set("alert dismissed", cx);
                    }))
                    .cancel(
                        Button::new("delete-cancel")
                            .outline()
                            .label("Cancel")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.alert = false;
                                this.set("alert cancelled", cx);
                            })),
                    )
                    .action(
                        Button::new("delete-confirm")
                            .destructive()
                            .label("Delete")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.alert = false;
                                this.set("deleted", cx);
                            })),
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
                KeyBinding::new("cmd-k", OpenCommands, None),
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
            gpui_core::open_window(options, cx, |window, cx| {
                let name = cx.new(|cx| InputState::new(window, cx).default_value("Pedro Duarte"));
                let focus = cx.focus_handle();
                window.focus(&focus, cx);
                cx.new(|_| Overlays {
                    focus,
                    dialog: false,
                    alert: false,
                    drawer: None,
                    sheet: false,
                    commands: false,
                    name,
                    said: "nothing yet".into(),
                })
            })
            .unwrap();
            cx.activate(true);
        });
}
