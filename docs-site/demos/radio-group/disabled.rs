// One option disabled, or the whole group.
div()
    .flex()
    .gap(rems(3.))
    .child(
        RadioGroup::new("seats")
            .accessibility_label("Seats")
            .option("five", "Five seats")
            .option("ten", "Ten seats")
            .option("more", "More (talk to us)")
            .disable("more")
            .selected("five"),
    )
    .child(
        RadioGroup::new("region")
            .accessibility_label("Region")
            .disabled(true)
            .option("us", "United States")
            .option("eu", "Europe")
            .selected("eu"),
    )
