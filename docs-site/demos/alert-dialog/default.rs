// A dialog asking for a decision: a press on the scrim doesn't close it, and the cancel
// button has the keyboard when it opens.
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
        let close = cx.listener(|this: &mut Self, _: &ClickEvent, _, cx| {
            this.open = false;
            cx.notify();
        });
        div()
            .child(
                Button::new("delete-account")
                    .label("Delete account")
                    .destructive()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.open = true;
                        cx.notify();
                    })),
            )
            .child(
                AlertDialog::new("confirm")
                    .open(self.open)
                    .title("Delete your account?")
                    .description(
                        "This deletes your account and its data for good. It can't be undone.",
                    )
                    .on_dismiss(cx.listener(|this, _, _, cx| {
                        this.open = false;
                        cx.notify();
                    }))
                    .cancel(
                        Button::new("cancel")
                            .label("Cancel")
                            .outline()
                            .on_click(close),
                    )
                    .action(
                        Button::new("delete")
                            .label("Delete")
                            .destructive()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.open = false;
                                cx.notify();
                            })),
                    ),
            )
    }
}
