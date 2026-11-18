// A button that opens a list to pick one from. The view keeps the choice.
pub struct Example {
    fruit: Option<SharedString>,
}

impl Example {
    pub fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self { fruit: None }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        Select::new("fruit")
            .accessibility_label("Fruit")
            .placeholder("Pick a fruit")
            .w(rems(14.))
            .options([
                ("apple", "Apple"),
                ("banana", "Banana"),
                ("blueberry", "Blueberry"),
                ("pineapple", "Pineapple"),
            ])
            .selected(self.fruit.clone())
            .on_change(cx.listener(|this, id: &SharedString, _, cx| {
                this.fruit = Some(id.clone());
                cx.notify();
            }))
    }
}
