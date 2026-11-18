// A check box with its label. The host keeps whether it's checked and passes it back, so
// `on_click` is told the state to take; here the picture shows one of each.
div()
    .flex()
    .flex_col()
    .gap(rems(0.75))
    .child(Checkbox::new("terms").label("Accept terms and conditions").checked(true))
    .child(Checkbox::new("news").label("Email me about new features"))
