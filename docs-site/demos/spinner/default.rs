// Something is happening: a turning ring, in three sizes and any colour.
div()
    .flex()
    .items_center()
    .gap(rems(1.5))
    .child(Spinner::new("small").small())
    .child(Spinner::new("medium"))
    .child(Spinner::new("large").large())
    .child(
        div()
            .flex()
            .items_center()
            .gap(rems(0.5))
            .child(Spinner::new("saving").small())
            .child("Saving…"),
    )
