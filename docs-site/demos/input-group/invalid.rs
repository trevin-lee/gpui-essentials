// Marked invalid: the border and the focus ring take the destructive colour.
pub struct Example {
    price: Entity<InputState>,
}

impl Example {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            price: cx.new(|cx| InputState::new(window, cx).default_value("12.5x")),
        }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(rems(14.)).child(
            InputGroup::new(&self.price)
                .accessibility_label("Price")
                .leading_text("$")
                .trailing_text("USD")
                .invalid(true),
        )
    }
}
