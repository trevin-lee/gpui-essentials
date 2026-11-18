// Off and on. Name a switch for assistive tech when no label says what it is.
div()
    .flex()
    .items_center()
    .gap(rems(1.))
    .child(Switch::new("wifi").accessibility_label("Wi-Fi"))
    .child(Switch::new("bluetooth").accessibility_label("Bluetooth").checked(true))
