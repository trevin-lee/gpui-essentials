// In a horizontal Field: a label and a description beside the switch.
Field::new("newsletter")
    .w(rems(24.))
    .horizontal(true)
    .label("Newsletter")
    .description("A short email once a month, and nothing else.")
    .child(Switch::new("newsletter-switch").accessibility_label("Newsletter").checked(true))
