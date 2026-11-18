// More about something, shown while the pointer rests on it or it has the focus. It isn't
// reachable from the keyboard, so it holds nothing that can't be done elsewhere.
HoverCard::new("author")
    .trigger(Button::new("author-link").link().label("@gpui-essentials"))
    .w(rems(18.))
    .child(
        div()
            .flex()
            .gap(rems(0.75))
            .child(Avatar::new("GPUI Essentials"))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(rems(0.25))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("@gpui-essentials"))
                    .child("Components for GPUI apps on macOS and iOS.")
                    .child(
                        div()
                            .text_color(cx.theme().muted_foreground)
                            .child("Joined November 2026"),
                    ),
            ),
    )
