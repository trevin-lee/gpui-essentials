// With an error in place of the help, in the destructive colour.
pub struct Example {
    email: Entity<InputState>,
}

impl Example {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            email: cx.new(|cx| InputState::new(window, cx).default_value("sam.example.com")),
        }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let email = self.email.read(cx).value();
        let error = (!email.contains('@')).then_some("Enter an address with an @ in it.");
        Field::new("email")
            .w(rems(20.))
            .label("Email")
            .for_focus(&self.email.focus_handle(cx))
            .error(error)
            .child(Input::new(&self.email).accessibility_label("Email"))
    }
}
