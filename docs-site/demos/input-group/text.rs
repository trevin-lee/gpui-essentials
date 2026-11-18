// Text either side: what the address starts and ends with.
pub struct Example {
    site: Entity<InputState>,
}

impl Example {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            site: cx.new(|cx| InputState::new(window, cx).placeholder("example")),
        }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(rems(20.)).child(
            InputGroup::new(&self.site)
                .accessibility_label("Website")
                .leading_text("https://")
                .trailing_text(".com"),
        )
    }
}
