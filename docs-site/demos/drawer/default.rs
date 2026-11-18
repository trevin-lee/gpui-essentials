// A modal panel from an edge of the window (the bottom unless `side` says otherwise), with a
// title, a description, content and a footer. A bottom one can be dragged away.
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
                Button::new("open-goal")
                    .label("Open drawer")
                    .outline()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.open = true;
                        cx.notify();
                    })),
            )
            .child(
                Drawer::new("goal")
                    .open(self.open)
                    .title("Move goal")
                    .description("Set your daily activity goal.")
                    .on_dismiss(cx.listener(|this, _, _, cx| {
                        this.open = false;
                        cx.notify();
                    }))
                    .child(
                        div()
                            .py(rems(1.))
                            .text_center()
                            .text_size(rems(3.))
                            .font_weight(FontWeight::BOLD)
                            .child("350"),
                    )
                    .footer(Button::new("submit").label("Submit"))
                    .footer(
                        Button::new("cancel")
                            .label("Cancel")
                            .outline()
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.open = false;
                                cx.notify();
                            })),
                    ),
            )
    }
}
