// Marked invalid, when a choice is required and none was made.
RadioGroup::new("delivery")
    .accessibility_label("Delivery")
    .option("standard", "Standard")
    .option("express", "Express")
    .invalid(true)
