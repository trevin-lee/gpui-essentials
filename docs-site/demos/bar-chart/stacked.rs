// The series stacked into one bar per category.
Card::new()
    .w(rems(30.))
    .title("Bar chart, stacked")
    .description("Desktop and mobile, one bar a month")
    .child(
        BarChart::new("stacked")
            .categories(["Jan", "Feb", "Mar", "Apr", "May", "Jun"])
            .series("desktop", "Desktop", [186., 305., 237., 73., 209., 214.])
            .series("mobile", "Mobile", [80., 200., 120., 190., 130., 140.])
            .stacked(true)
            .legend(true),
    )
