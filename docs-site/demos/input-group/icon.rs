// An icon before the text and a note after it, inside the field's border.
pub struct Example {
    search: Entity<InputState>,
}

impl Example {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            search: cx.new(|cx| InputState::new(window, cx).placeholder("Search")),
        }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(rems(20.)).child(
            InputGroup::new(&self.search)
                .accessibility_label("Search")
                .leading_icon(IconName::Search)
                .trailing_text("12 results"),
        )
    }
}
