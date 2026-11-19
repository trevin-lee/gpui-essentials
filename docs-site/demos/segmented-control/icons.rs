// An icon before each label.
div().w(rems(20.)).child(
    SegmentedControl::new("look")
        // The box's width, split between the options.
        .w_full()
        .accessibility_label("Theme")
        .segment_with_icon("system", "System", IconName::Monitor)
        .segment_with_icon("light", "Light", IconName::Sun)
        .segment_with_icon("dark", "Dark", IconName::Moon)
        .selected("system"),
)
