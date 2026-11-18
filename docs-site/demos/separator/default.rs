// A thin line between things, across or down.
div()
    .w(rems(20.))
    .flex()
    .flex_col()
    .gap(rems(0.75))
    .child(
        div()
            .flex()
            .flex_col()
            .gap(rems(0.25))
            .child(div().font_weight(FontWeight::MEDIUM).child("GPUI Essentials"))
            .child(
                div()
                    .text_size(cx.theme().text.sm)
                    .text_color(cx.theme().muted_foreground)
                    .child("Components for GPUI apps."),
            ),
    )
    .child(Separator::horizontal())
    .child(
        div()
            .flex()
            .items_center()
            .gap(rems(0.75))
            .h(rems(1.25))
            .text_size(cx.theme().text.sm)
            .child("Blog")
            .child(Separator::vertical())
            .child("Docs")
            .child(Separator::vertical())
            .child("Source"),
    )
