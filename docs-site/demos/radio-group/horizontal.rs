// The options in a row.
RadioGroup::new("size")
    .accessibility_label("Size")
    .horizontal(true)
    .option("s", "Small")
    .option("m", "Medium")
    .option("l", "Large")
    .selected("m")
