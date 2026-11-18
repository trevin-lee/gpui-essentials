// A donut, with its total in the hole.
Card::new()
    .w(rems(22.))
    .title("Donut")
    .description("Visitors by browser")
    .child(
        PieChart::new("donut")
            .slice("chrome", "Chrome", 275.)
            .slice("safari", "Safari", 200.)
            .slice("firefox", "Firefox", 187.)
            .donut(true)
            .center_label("662", "Visitors")
            .h(rems(16.)),
    )
