// One choice of several. The view keeps the choice; `on_change` is told the option pressed.
pub struct Example {
    plan: SharedString,
}

impl Example {
    pub fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self { plan: "pro".into() }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        RadioGroup::new("plan")
            .accessibility_label("Plan")
            .option("free", "Free")
            .option("pro", "Pro")
            .option("team", "Team")
            .selected(self.plan.clone())
            .on_change(cx.listener(|this, id: &SharedString, _, cx| {
                this.plan = id.clone();
                cx.notify();
            }))
    }
}
