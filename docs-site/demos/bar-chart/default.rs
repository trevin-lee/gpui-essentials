// Bars side by side for each category, a series each, with a legend. Point at a bar (or Tab
// to the chart and use Left and Right) for every series' value there.
Card::new()
    .w(rems(30.))
    .title("Bar chart")
    .description("January to June")
    .child(
        BarChart::new("visitors")
            .categories(["Jan", "Feb", "Mar", "Apr", "May", "Jun"])
            .series("desktop", "Desktop", [186., 305., 237., 73., 209., 214.])
            .series("mobile", "Mobile", [80., 200., 120., 190., 130., 140.])
            .legend(true),
    )
