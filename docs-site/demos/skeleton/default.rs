// Grey shapes standing in for what's loading, shaped like it.
div()
    .flex()
    .items_center()
    .gap(rems(1.))
    .child(Skeleton::new().size(rems(3.)).rounded_full())
    .child(
        div()
            .flex()
            .flex_col()
            .gap(rems(0.5))
            .child(Skeleton::new().h(rems(1.)).w(rems(15.)))
            .child(Skeleton::new().h(rems(1.)).w(rems(12.))),
    )
