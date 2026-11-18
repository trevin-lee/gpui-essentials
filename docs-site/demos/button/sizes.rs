// Four sizes in rems, so they follow the theme's text size; `.touch()` is a fifth, 44 points.
div()
    .flex()
    .items_center()
    .gap(rems(0.5))
    .child(Button::new("xs").label("XSmall").outline().xsmall())
    .child(Button::new("sm").label("Small").outline().small())
    .child(Button::new("md").label("Medium").outline())
    .child(Button::new("lg").label("Large").outline().large())
