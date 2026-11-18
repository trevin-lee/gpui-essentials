// A FieldSet: a legend and a description over a group of fields.
pub struct Example {
    name: Entity<InputState>,
    email: Entity<InputState>,
}

impl Example {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            name: cx.new(|cx| InputState::new(window, cx).placeholder("Sam Rivera")),
            email: cx.new(|cx| InputState::new(window, cx).placeholder("sam@example.com")),
        }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().w(rems(22.)).child(
            FieldSet::new("account")
                .legend("Account")
                .description("Who you are, and where we reach you.")
                .child(
                    FieldGroup::new()
                        .child(
                            Field::new("name")
                                .label("Name")
                                .for_focus(&self.name.focus_handle(cx))
                                .child(Input::new(&self.name).accessibility_label("Name")),
                        )
                        .child(
                            Field::new("email")
                                .label("Email")
                                .for_focus(&self.email.focus_handle(cx))
                                .child(Input::new(&self.email).accessibility_label("Email")),
                        ),
                ),
        )
    }
}
