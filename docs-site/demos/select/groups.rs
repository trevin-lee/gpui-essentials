// Options under headings, with a line between the groups, and one picked.
Select::new("zone")
    .accessibility_label("Time zone")
    .w(rems(18.))
    .heading("North America")
    .options([
        ("est", "Eastern Standard Time (EST)"),
        ("pst", "Pacific Standard Time (PST)"),
    ])
    .separator()
    .heading("Europe")
    .options([
        ("gmt", "Greenwich Mean Time (GMT)"),
        ("cet", "Central European Time (CET)"),
    ])
    .selected(Some("cet"))
