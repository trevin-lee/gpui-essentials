// Several lines that wrap. `auto_grow(2, 6)` grows it with its text, from two rows to six.
pub struct Example {
    note: Entity<InputState>,
}

impl Example {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            note: cx.new(|cx| {
                InputState::textarea(window, cx)
                    .auto_grow(2, 6)
                    .default_value("A textarea wraps a long line rather than running out of its box, and grows as it fills.")
            }),
        }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(rems(22.))
            .child(Input::new(&self.note).accessibility_label("Note"))
    }
}
