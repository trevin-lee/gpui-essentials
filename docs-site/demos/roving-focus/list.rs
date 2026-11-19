// A list whose rows share one Tab stop: Up and Down move between them, Home and End go to the
// ends, and Enter or a click opens one. As a list box, it says which row is chosen. Each row's
// mail button is a row action: not a Tab stop, Right reaches it and Left comes back.
pub struct Example {
    open: SharedString,
    mail: Vec<FocusHandle>,
}

impl Example {
    pub fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            open: "sam".into(),
            mail: (0..3).map(|_| cx.focus_handle()).collect(),
        }
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
        for (i, (id, name, mail)) in [
            ("alex", "Alex Kim", "alex@example.com"),
            ("sam", "Sam Ortiz", "sam@example.com"),
            ("jo", "Jo Park", "jo@example.com"),
        ]
        .into_iter()
        .enumerate()
        {
            let chosen = self.open.as_ref() == id;
            let mail_button = base::Press::new(format!("mail-{id}"))
                .track_focus(&self.mail[i])
                .tab_stop(false)
                .accessibility_label(format!("Write to {name}"))
                .p(rems(0.375))
                .rounded(theme.radius_md())
                .child(Icon::new(IconName::Mail).color(theme.muted_foreground));
            list = list
                .row(
                    id,
                    name,
                    base::Press::new(id)
                        .flex()
                        .items_center()
                        .gap(rems(0.5))
                        .px(rems(0.75))
                        .py(rems(0.5))
                        .rounded(theme.radius_md())
                        .bg(if chosen {
                            theme.accent
                        } else {
                            transparent_black()
                        })
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(name))
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(theme.muted_foreground)
                                        .child(mail),
                                ),
                        )
                        .child(mail_button)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open = id.into();
                            cx.notify();
                        })),
                )
                .row_actions(id, [RowAction::new(&self.mail[i])]);
        }
        list
    }
}
