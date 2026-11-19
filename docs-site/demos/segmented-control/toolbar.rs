// Without a width, it's as wide as its labels, whole, even beside a spacer that takes the rest
// of a toolbar. Given one (`w_full()`, `w(…)`), the options split it equally instead.
div()
    .w(rems(28.))
    .flex()
    .items_center()
    .gap(rems(0.75))
    .child(div().flex_1().text_sm().child("Notebooks"))
    .child(
        SegmentedControl::new("shelf")
            .accessibility_label("Shelf")
            .segment("favorites", "Favorites")
            .segment("recents", "Recents")
            .segment("all", "Everything")
            .selected("recents"),
    )
