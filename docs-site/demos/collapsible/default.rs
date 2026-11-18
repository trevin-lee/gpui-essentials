// A label that shows or hides what's under it. It keeps whether it's open itself, unless the
// host passes `open` and `on_toggle`.
div().w(rems(22.)).child(
    Collapsible::new("details")
        .label("Order details")
        .default_open(true)
        .child(
            div()
                .flex()
                .flex_col()
                .gap(rems(0.5))
                .child("Three items, shipped on Monday.")
                .child("Arriving Thursday by noon."),
        ),
)
