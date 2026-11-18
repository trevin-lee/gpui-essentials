// An error with an action, and a description under its title.
Button::new("publish")
    .label("Publish")
    .outline()
    .on_click(|_, window, cx| {
        Toast::error("The page didn't publish.")
            .description("Line 3 isn't valid. Fix it, then publish again.")
            .action("Show it", |_, _| {})
            .show(window, cx)
    })
