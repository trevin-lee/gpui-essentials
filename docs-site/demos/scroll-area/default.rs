// A box that scrolls, with thin bars that show while the pointer is on it or it scrolls.
// Tab to it and the arrows, Page Up and Page Down, Home and End scroll it too.
ScrollArea::new("tags")
    .accessibility_label("Tags")
    .h(rems(18.))
    .w(rems(12.))
    .rounded(cx.theme().radius_md())
    .border_1()
    .border_color(cx.theme().border)
    .child(
        div()
            .px(rems(1.))
            .pt(rems(1.))
            .pb(rems(0.5))
            .text_size(cx.theme().text.sm)
            .font_weight(FontWeight::MEDIUM)
            .child("Tags"),
    )
    .children((1..=50).rev().map(|i| {
        div()
            .mx(rems(1.))
            .py(rems(0.5))
            .text_size(cx.theme().text.sm)
            .border_b_1()
            .border_color(cx.theme().border)
            .child(format!("v1.2.0-beta.{i}"))
    }))
