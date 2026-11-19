// A list whose rows share one Tab stop: Up and Down move between them, Home and End go to the
// ends, and Enter or a click opens one. As a list box, it says which row is chosen.
pub struct Example {
    open: SharedString,
}

impl Example {
    pub fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self { open: "sam".into() }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let mut list = RovingFocus::new("people")
            .accessibility_label("People")
            .role(Role::ListBox)
            .selected(self.open.clone())
            .w(rems(18.))
            .gap(rems(0.125));
        for (id, name, mail) in [
            ("alex", "Alex Kim", "alex@example.com"),
            ("sam", "Sam Ortiz", "sam@example.com"),
            ("jo", "Jo Park", "jo@example.com"),
        ] {
            let chosen = self.open.as_ref() == id;
            list = list.row(
                id,
                name,
                base::Press::new(id)
                    .flex()
                    .flex_col()
                    .px(rems(0.75))
                    .py(rems(0.5))
                    .rounded(theme.radius_md())
                    .bg(if chosen { theme.accent } else { transparent_black() })
                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(name))
                    .child(div().text_xs().text_color(theme.muted_foreground).child(mail))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open = id.into();
                        cx.notify();
                    })),
            );
        }
        list
    }
}
