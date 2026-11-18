// An icon before the label, or alone. Name an icon-only button for assistive tech.
div()
    .flex()
    .items_center()
    .gap(rems(0.5))
    .child(Button::new("new").icon(IconName::Plus).label("New page"))
    .child(Button::new("star").icon(IconName::Star).label("Star").secondary())
    .child(
        Button::new("prev")
            .icon(IconName::ChevronLeft)
            .accessibility_label("Previous")
            .outline(),
    )
    .child(
        Button::new("next")
            .icon(IconName::ChevronRight)
            .accessibility_label("Next")
            .outline(),
    )
