// A modal panel in the middle of the window, over a scrim. The view owns whether it's open,
// and closes it from `on_dismiss` (Escape, a press on the scrim, the close button).
pub struct Example {
    open: bool,
    name: Entity<InputState>,
}

impl Example {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            open: false,
            name: cx.new(|cx| InputState::new(window, cx).default_value("Sam Rivera")),
        }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .child(
                Button::new("edit-profile")
                    .label("Edit profile")
                    .outline()
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.open = true;
                        cx.notify();
                    })),
            )
            .child(
                Dialog::new("profile")
                    .open(self.open)
                    .title("Edit profile")
                    .description("Make changes to your profile here. Save when you're done.")
                    .on_dismiss(cx.listener(|this, _, _, cx| {
                        this.open = false;
                        cx.notify();
                    }))
                    .child(
                        Field::new("name")
                            .label("Name")
                            .for_focus(&self.name.focus_handle(cx))
                            .child(Input::new(&self.name).accessibility_label("Name")),
                    )
                    .footer(
                        Button::new("save")
                            .label("Save changes")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.open = false;
                                cx.notify();
                            })),
                    ),
            )
    }
}
