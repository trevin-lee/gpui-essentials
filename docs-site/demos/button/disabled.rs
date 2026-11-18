// Dimmed: it takes no focus and ignores the pointer and the keyboard.
div()
    .flex()
    .items_center()
    .gap(rems(0.5))
    .child(Button::new("send").label("Send").disabled(true))
    .child(Button::new("retry").label("Retry").outline().disabled(true))
