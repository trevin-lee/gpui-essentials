// Dimmed, and a press does nothing.
div()
    .flex()
    .items_center()
    .gap(rems(0.5))
    .child(Toggle::new("off").label("Disabled").disabled(true))
    .child(
        Toggle::new("on")
            .label("Disabled and on")
            .outline()
            .pressed(true)
            .disabled(true),
    )
