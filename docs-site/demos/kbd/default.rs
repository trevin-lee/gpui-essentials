// A key, or keys pressed together, as they're printed on the keyboard.
div()
    .flex()
    .items_center()
    .gap(rems(1.))
    .child(Kbd::new("Esc"))
    .child(KbdGroup::keys(["⌘", "K"]))
    .child(KbdGroup::keys(["⇧", "⌘", "P"]))
