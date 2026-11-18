// Panels shown one at a time, chosen from a row of tabs. The view keeps the tab chosen; the
// arrows, Home and End move between tabs.
pub struct Example {
    tab: SharedString,
}

impl Example {
    pub fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self {
            tab: "account".into(),
        }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().w(rems(24.)).child(
            Tabs::new("settings")
                .accessibility_label("Settings")
                .tab(
                    Tab::new("account", "Account").child(
                        Card::new()
                            .title("Account")
                            .description("Make changes to your account here.")
                            .child("Your name and email."),
                    ),
                )
                .tab(
                    Tab::new("password", "Password").child(
                        Card::new()
                            .title("Password")
                            .description("Change your password here.")
                            .child("Your current and new password."),
                    ),
                )
                .selected(self.tab.clone())
                .on_change(cx.listener(|this, id: &SharedString, _, cx| {
                    this.tab = id.clone();
                    cx.notify();
                })),
        )
    }
}
