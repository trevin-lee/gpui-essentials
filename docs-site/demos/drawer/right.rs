// From the right edge, as a side panel.
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
                Button::new("open-filters")
                    .label("Filters")
                    .icon(IconName::Settings)
                    .outline()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.open = true;
                        cx.notify();
                    })),
            )
            .child(
                Drawer::new("filters")
                    .side(DrawerSide::Right)
                    .open(self.open)
                    .title("Filters")
                    .description("Show only the runs you want.")
                    .on_dismiss(cx.listener(|this, _, _, cx| {
                        this.open = false;
                        cx.notify();
                    }))
                    .child(Checkbox::new("failed").label("Failed runs").checked(true))
                    .footer(Button::new("apply").label("Apply")),
            )
    }
}
