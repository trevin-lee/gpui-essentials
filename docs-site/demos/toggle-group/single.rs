// Toggles in a row where at most one is on: icons only, each named for assistive tech.
pub struct Example {
    align: Vec<SharedString>,
}

impl Example {
    pub fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self {
            align: vec!["start".into()],
        }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        ToggleGroup::new("align")
            .accessibility_label("Alignment")
            .icon_item("start", IconName::TextAlignStart, "Left")
            .icon_item("center", IconName::TextAlignCenter, "Centre")
            .icon_item("end", IconName::TextAlignEnd, "Right")
            .selected(self.align.clone())
            .on_change(cx.listener(|this, ids: &Vec<SharedString>, _, cx| {
                this.align = ids.clone();
                cx.notify();
            }))
    }
}
