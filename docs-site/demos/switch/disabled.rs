// Dimmed, and a press does nothing.
div()
    .flex()
    .items_center()
    .gap(rems(1.))
    .child(Switch::new("sync").accessibility_label("Sync").disabled(true))
    .child(Switch::new("backup").accessibility_label("Backup").checked(true).disabled(true))
