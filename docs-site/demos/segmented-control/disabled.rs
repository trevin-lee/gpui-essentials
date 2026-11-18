// One segment disabled.
div().w(rems(20.)).child(
    SegmentedControl::new("quality")
        .accessibility_label("Quality")
        .segment("low", "Low")
        .segment("high", "High")
        .segment("lossless", "Lossless")
        .disable("lossless")
        .selected("high"),
)
