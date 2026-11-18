// "Select all" over a list: checked when every item is, a dash when only some are. A view
// keeps the state; each box's `on_click` is told the state to take, and sets it.
pub struct Example {
    mailboxes: [bool; 3],
}

impl Example {
    pub fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self {
            mailboxes: [true, false, true],
        }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let all = self.mailboxes.iter().all(|on| *on);
        let some = self.mailboxes.iter().any(|on| *on);
        div()
            .flex()
            .flex_col()
            .gap(rems(0.75))
            .child(
                Checkbox::new("all")
                    .label("All mailboxes")
                    .checked(all)
                    .indeterminate(some && !all)
                    .on_click(cx.listener(|this, on: &bool, _, cx| {
                        this.mailboxes = [*on; 3];
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .pl(rems(1.5))
                    .flex()
                    .flex_col()
                    .gap(rems(0.5))
                    .children(["Inbox", "Drafts", "Archive"].into_iter().enumerate().map(
                        |(i, name)| {
                            Checkbox::new(("mailbox", i))
                                .label(name)
                                .checked(self.mailboxes[i])
                                .on_click(cx.listener(move |this, on: &bool, _, cx| {
                                    this.mailboxes[i] = *on;
                                    cx.notify();
                                }))
                        },
                    )),
            )
    }
}
