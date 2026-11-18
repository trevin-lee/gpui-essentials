// In steps of 5, with its value beside it: the view redraws when the slider changes.
pub struct Example {
    level: Entity<SliderState>,
}

impl Example {
    pub fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        let level = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(100.)
                .step(5.)
                .default_value(65.)
        });
        cx.subscribe(&level, |_, _, _: &SliderEvent, cx| cx.notify())
            .detach();
        Self { level }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap(rems(1.))
            .child(div().w(rems(16.)).child(Slider::new(&self.level).w_full()))
            .child(format!("{}%", self.level.read(cx).value()))
    }
}
