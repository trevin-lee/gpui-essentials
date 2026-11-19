// One segment disabled.
div().w(rems(20.)).child(
    SegmentedControl::new("quality")
        // The box's width, split between the options.
        .w_full()
        .accessibility_label("Quality")
        .segment("low", "Low")
        .segment("high", "High")
        .segment("lossless", "Lossless")
        .disable("lossless")
        .selected("high"),
)
