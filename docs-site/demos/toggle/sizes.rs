// Small, the default and large; `.touch()` makes it 44 points for fingers.
div()
    .flex()
    .items_center()
    .gap(rems(0.5))
    .child(Toggle::new("sm").label("Small").outline().small())
    .child(Toggle::new("md").label("Default").outline())
    .child(Toggle::new("lg").label("Large").outline().large())
