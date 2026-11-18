// A plain panel of rows across the bottom of the window, for touch (an action sheet).
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
        let row = |id: &'static str, label: &'static str, icon: IconName| {
            Press::new(id)
                .min_h(px(44.))
                .px(rems(1.))
                .flex()
                .items_center()
                .gap(rems(0.75))
                .child(Icon::new(icon))
                .child(label)
        };
        div()
            .child(
                Button::new("share")
                    .label("Share")
                    .icon(IconName::Share)
                    .outline()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.open = true;
                        cx.notify();
                    })),
            )
            .child(
                Sheet::new("share-sheet")
                    .open(self.open)
                    .on_dismiss(cx.listener(|this, _, _, cx| {
                        this.open = false;
                        cx.notify();
                    }))
                    .child(row("copy", "Copy link", IconName::Link))
                    .child(row("mail", "Send by email", IconName::Mail))
                    .child(row("download", "Download", IconName::Download)),
            )
    }
}
