// Values below zero hang under the zero line.
Card::new()
    .w(rems(30.))
    .title("Bar chart, negative")
    .description("Net visitors: losses hang below the line")
    .child(
        BarChart::new("negative")
            .categories(["Jan", "Feb", "Mar", "Apr", "May", "Jun"])
            .series("net", "Net", [186., 205., -207., 173., -209., 214.]),
    )
