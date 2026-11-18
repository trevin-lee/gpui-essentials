// Outlined, with a label after the icon: off, then on.
div()
    .flex()
    .items_center()
    .gap(rems(0.5))
    .child(
        Toggle::new("underline")
            .icon(IconName::Underline)
            .label("Underline")
            .outline(),
    )
    .child(
        Toggle::new("italic")
            .icon(IconName::Italic)
            .label("Italic")
            .outline()
            .pressed(true),
    )
