// A one-line text field. Its text lives in an `InputState`, made once and kept by the view.
pub struct Example {
    name: Entity<InputState>,
}

impl Example {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            name: cx.new(|cx| InputState::new(window, cx).placeholder("Your name")),
        }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(rems(18.))
            .child(Input::new(&self.name).accessibility_label("Name"))
    }
}
