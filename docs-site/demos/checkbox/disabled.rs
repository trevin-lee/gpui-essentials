// Dimmed, and a press on it or its label does nothing.
div()
    .flex()
    .flex_col()
    .gap(rems(0.75))
    .child(Checkbox::new("locked").label("Required by your team").checked(true).disabled(true))
    .child(Checkbox::new("unavailable").label("Not on this plan").disabled(true))
