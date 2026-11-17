// An icon before the label, in any variant.
div()
    .flex()
    .items_center()
    .gap(rems(0.5))
    .child(Badge::new("Verified").icon(IconName::BadgeCheck).secondary())
    .child(
        Badge::new("Overdue")
            .icon(IconName::CircleExclamationPoint)
            .destructive(),
    )
