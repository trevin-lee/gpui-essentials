// A panel hanging from what opens it, over everything else. The view owns whether it's open;
// `on_dismiss` runs on a click outside and on Escape.
pub struct Example {
    open: bool,
    width: Entity<InputState>,
    height: Entity<InputState>,
}

impl Example {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            open: false,
            width: cx.new(|cx| InputState::new(window, cx).default_value("100%")),
            height: cx.new(|cx| InputState::new(window, cx).default_value("25px")),
        }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        Popover::new("dimensions")
            .open(self.open)
            .on_dismiss(cx.listener(|this, _, _, cx| {
                this.open = false;
                cx.notify();
            }))
            .trigger(
                Button::new("open-dimensions")
                    .label("Open popover")
                    .outline()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.open = !this.open;
                        cx.notify();
                    })),
            )
            .w(rems(18.))
            .p(rems(1.))
            .flex()
            .flex_col()
            .gap(rems(0.75))
            .child(div().font_weight(FontWeight::MEDIUM).child("Dimensions"))
            .child(
                div()
                    .text_size(theme.text.sm)
                    .text_color(theme.muted_foreground)
                    .child("Set the dimensions for the layer."),
            )
            .child(
                Field::new("width")
                    .label("Width")
                    .child(Input::new(&self.width).accessibility_label("Width")),
            )
            .child(
                Field::new("height")
                    .label("Height")
                    .child(Input::new(&self.height).accessibility_label("Height")),
            )
    }
}
