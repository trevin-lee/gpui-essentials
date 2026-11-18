// A value along a track, moved by dragging or the arrow keys. A `SliderState` keeps it.
pub struct Example {
    volume: Entity<SliderState>,
}

impl Example {
    pub fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            volume: cx.new(|_| SliderState::new().min(0.).max(100.).default_value(40.)),
        }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(rems(18.)).child(Slider::new(&self.volume).w_full())
    }
}
