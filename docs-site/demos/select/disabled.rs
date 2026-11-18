// Dimmed, and it doesn't open.
Select::new("language")
    .accessibility_label("Language")
    .w(rems(14.))
    .options([("en", "English"), ("fr", "French")])
    .selected(Some("en"))
    .disabled(true)
