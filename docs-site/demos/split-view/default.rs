// Panes side by side with handles to drag between them (or focus and move with the arrows).
// The view keeps the widths, from `on_resize`; below `narrow_below` it shows one pane at a time.
pub struct Example {
    widths: Vec<Pixels>,
}

impl Example {
    pub fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self { widths: Vec::new() }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let pane = |title: &'static str| {
            div()
                .size_full()
                .p(rems(1.))
                .text_size(theme.text.sm)
                .child(title)
        };
        div()
            .w(rems(36.))
            .h(rems(16.))
            .rounded(theme.radius_lg())
            .border_1()
            .border_color(theme.border)
            .overflow_hidden()
            .child(
                SplitView::new("mail")
                    .pane(
                        Pane::new(pane("Mailboxes"))
                            .width(px(140.))
                            .min(px(100.))
                            .accessibility_label("Mailboxes"),
                    )
                    .pane(
                        Pane::new(pane("Messages"))
                            .width(px(180.))
                            .min(px(140.))
                            .accessibility_label("Messages"),
                    )
                    .pane(
                        Pane::new(pane("Message"))
                            .min(px(160.))
                            .accessibility_label("Message"),
                    )
                    .widths(self.widths.clone())
                    .on_resize(cx.listener(|this, widths: &[Pixels], _, cx| {
                        this.widths = widths.to_vec();
                        cx.notify();
                    })),
            )
    }
}
