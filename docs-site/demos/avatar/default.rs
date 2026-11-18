// A picture of someone, or their initials when there's none, in three sizes.
div()
    .flex()
    .items_center()
    .gap(rems(1.))
    .child(Avatar::new("Sam Rivera").small())
    .child(Avatar::new("Alex Kim"))
    .child(Avatar::new("Jordan Lee").large())
