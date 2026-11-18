// Smooth lines, one per series, with a legend.
Card::new()
    .w(rems(30.))
    .title("Line chart")
    .description("January to June")
    .child(
        LineChart::new("visitors")
            .categories(["Jan", "Feb", "Mar", "Apr", "May", "Jun"])
            .series("desktop", "Desktop", [186., 305., 237., 73., 209., 214.])
            .series("mobile", "Mobile", [80., 200., 120., 190., 130., 140.])
            .legend(true)
            .h(rems(14.)),
    )
