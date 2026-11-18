// A short note at the bottom of the window that goes by itself; toasts stack, newest last.
Button::new("save")
    .label("Save")
    .outline()
    .on_click(|_, window, cx| Toast::success("Saved.").show(window, cx))
