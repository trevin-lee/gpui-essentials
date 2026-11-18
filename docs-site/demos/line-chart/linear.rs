// Straight segments with a dot at each value; a missing value (NaN) leaves a gap.
Card::new()
    .w(rems(30.))
    .title("Line chart, linear")
    .description("With dots, and no data for April")
    .child(
        LineChart::new("linear")
            .categories(["Jan", "Feb", "Mar", "Apr", "May", "Jun"])
            .series("desktop", "Desktop", [186., 305., 237., f64::NAN, 209., 214.])
            .curve(Curve::Linear)
            .dots(true)
            .h(rems(14.)),
    )
