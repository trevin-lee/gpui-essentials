// A masked field, for a password: each character drawn as a dot.
pub struct Example {
    password: Entity<InputState>,
}

impl Example {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            password: cx.new(|cx| {
                InputState::new(window, cx)
                    .masked(true)
                    .default_value("correct horse")
            }),
        }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(rems(18.))
            .child(Input::new(&self.password).accessibility_label("Password"))
    }
}
