// One of a few choices, as joined segments; the arrows, Home and End move between them.
pub struct Example {
    range: SharedString,
}

impl Example {
    pub fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self {
            range: "week".into(),
        }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().w(rems(20.)).child(
            SegmentedControl::new("range")
                // The box's width, split between the options.
                .w_full()
                .accessibility_label("Range")
                .segment("day", "Day")
                .segment("week", "Week")
                .segment("month", "Month")
                .selected(self.range.clone())
                .on_change(cx.listener(|this, id: &SharedString, _, cx| {
                    this.range = id.clone();
                    cx.notify();
                })),
        )
    }
}
