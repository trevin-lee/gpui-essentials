// A command palette: a search field over grouped commands, filtered as you type. Up, Down
// and Enter pick one; `on_select` is told which.
pub struct Example {
    open: bool,
}

impl Example {
    pub fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self { open: false }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .child(
                Button::new("open-commands")
                    .label("Search commands…")
                    .icon(IconName::Search)
                    .outline()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.open = true;
                        cx.notify();
                    })),
            )
            .child(
                CommandDialog::new("commands")
                    .open(self.open)
                    .placeholder("Type a command or search…")
                    .on_dismiss(cx.listener(|this, _, _, cx| {
                        this.open = false;
                        cx.notify();
                    }))
                    .group(
                        "Suggestions",
                        [
                            PopupMenuItem::new("calendar", "Calendar").icon(IconName::Calendar),
                            PopupMenuItem::new("calculator", "Calculator")
                                .icon(IconName::Calculator),
                        ],
                    )
                    .group(
                        "Settings",
                        [
                            PopupMenuItem::new("profile", "Profile")
                                .icon(IconName::User)
                                .shortcut("⌘P"),
                            PopupMenuItem::new("billing", "Billing")
                                .icon(IconName::CreditCard)
                                .shortcut("⌘B"),
                            PopupMenuItem::new("settings", "Settings")
                                .icon(IconName::Settings)
                                .shortcut("⌘S"),
                        ],
                    )
                    .on_select(cx.listener(|this, _: &MenuSelect, _, cx| {
                        this.open = false;
                        cx.notify();
                    })),
            )
    }
}
