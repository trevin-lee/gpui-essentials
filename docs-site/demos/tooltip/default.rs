// Shown after the pointer rests on what it explains: GPUI's `.tooltip()` takes it, on any
// element with an id. Give what it explains the same words as an accessibility label.
div()
    .id("archive")
    .child(
        Button::new("archive-button")
            .icon(IconName::Archive)
            .accessibility_label("Archive")
            .outline(),
    )
    .tooltip(|window, cx| Tooltip::new("Archive").build(window, cx))
