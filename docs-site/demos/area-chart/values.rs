// A flat fill that goes below zero, its values in the host's own format.
Card::new()
    .w(rems(30.))
    .title("Area chart, profit")
    .description("In dollars, with a loss in March")
    .child(
        AreaChart::new("profit")
            .categories(["Jan", "Feb", "Mar", "Apr", "May", "Jun"])
            .series("profit", "Profit", [1200., 800., -400., 300., 1500., 900.])
            .series_color("profit", cx.theme().chart_2)
            .format_value(|v| {
                let sign = if v < 0. { "-" } else { "" };
                format!("{sign}${}", chart::compact_number(v.abs())).into()
            })
            .h(rems(14.)),
    )
