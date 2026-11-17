// A switch that turns: a view keeps whether it's on. The switch's `on_click` is told the state
// to take, and a press on the label turns it too.
pub struct Example {
    airplane_mode: bool,
}

impl Example {
    pub fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self {
            airplane_mode: false,
        }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        Field::new("airplane")
            .w(rems(12.))
            .horizontal(true)
            .control_first(true)
            .label("Airplane mode")
            .on_label_click(cx.listener(|this, _, _, cx| {
                this.airplane_mode = !this.airplane_mode;
                cx.notify();
            }))
            .child(
                Switch::new("airplane-switch")
                    .accessibility_label("Airplane mode")
                    .checked(self.airplane_mode)
                    .on_click(cx.listener(|this, on: &bool, _, cx| {
                        this.airplane_mode = *on;
                        cx.notify();
                    })),
            )
    }
}
