// Stacked areas filled with a gradient.
Card::new()
    .w(rems(30.))
    .title("Area chart")
    .description("Mobile and desktop, stacked")
    .child(
        AreaChart::new("visitors")
            .categories(["Jan", "Feb", "Mar", "Apr", "May", "Jun"])
            .series("mobile", "Mobile", [80., 200., 120., 190., 130., 140.])
            .series("desktop", "Desktop", [186., 305., 237., 73., 209., 214.])
            .stacked(true)
            .fill(AreaFill::Gradient)
            .legend(true)
            .h(rems(14.)),
    )
