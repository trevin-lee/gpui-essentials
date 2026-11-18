// A label, the control and a line of help under it. A press on the label focuses the field.
pub struct Example {
    email: Entity<InputState>,
}

impl Example {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            email: cx.new(|cx| InputState::new(window, cx).placeholder("sam@example.com")),
        }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        Field::new("email")
            .w(rems(20.))
            .label("Email")
            .for_focus(&self.email.focus_handle(cx))
            .description("We only use it to sign you in.")
            .child(Input::new(&self.email).accessibility_label("Email"))
    }
}
