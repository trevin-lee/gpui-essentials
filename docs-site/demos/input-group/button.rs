// Any element at either end: here a small button that copies the address.
pub struct Example {
    link: Entity<InputState>,
}

impl Example {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            link: cx.new(|cx| {
                InputState::new(window, cx).default_value("https://example.com/invite/7f3a")
            }),
        }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let link = self.link.read(cx).value();
        div().w(rems(22.)).child(
            InputGroup::new(&self.link)
                .accessibility_label("Invite link")
                .leading_icon(IconName::Globe)
                .trailing(
                    Button::new("copy")
                        .ghost()
                        .xsmall()
                        .icon(IconName::Copy)
                        .accessibility_label("Copy")
                        .on_click(move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(link.to_string()))
                        }),
                ),
        )
    }
}
