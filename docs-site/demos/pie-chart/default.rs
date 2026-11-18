// Slices of a whole, with a legend.
Card::new()
    .w(rems(22.))
    .title("Pie chart")
    .description("Visitors by browser")
    .child(
        PieChart::new("browsers")
            .slice("chrome", "Chrome", 275.)
            .slice("safari", "Safari", 200.)
            .slice("firefox", "Firefox", 187.)
            .slice("edge", "Edge", 173.)
            .slice("other", "Other", 90.)
            .legend(true)
            .h(rems(16.)),
    )
