// A box that keeps a width-to-height ratio as its width changes: here 16 by 9.
div().w(rems(24.)).child(
    AspectRatio::new(16. / 9.)
        .rounded(cx.theme().radius_lg())
        .bg(cx.theme().muted)
        .child(
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(cx.theme().muted_foreground)
                .child("16 : 9"),
        ),
)
