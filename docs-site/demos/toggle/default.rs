// A button that stays pressed until pressed again. The view keeps whether it's on.
pub struct Example {
    italic: bool,
}

impl Example {
    pub fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self { italic: true }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        Toggle::new("italic")
            .icon(IconName::Italic)
            .accessibility_label("Italic")
            .pressed(self.italic)
            .on_click(cx.listener(|this, on: &bool, _, cx| {
                this.italic = *on;
                cx.notify();
            }))
    }
}
