// How far along something is, from 0 to 100.
div()
    .w(rems(18.))
    .flex()
    .flex_col()
    .gap(rems(1.))
    .child(Progress::new("upload").value(66.).accessibility_label("Uploading"))
    .child(Progress::new("export").value(25.).accessibility_label("Exporting"))
