// Tiles that wrap, one Tab stop for them all: every arrow moves back or on, and Home and End go
// to the first and the last.
div().w(rems(22.)).child({
    let theme = cx.theme().clone();
    let mut tiles = RovingFocus::new("apps")
        .tiles()
        .accessibility_label("Apps")
        .gap(rems(0.5));
    for (id, name, icon) in [
        ("mail", "Mail", IconName::Mail),
        ("calendar", "Calendar", IconName::Calendar),
        ("files", "Files", IconName::Folder),
        ("people", "People", IconName::Users),
        ("notes", "Notes", IconName::FileText),
        ("settings", "Settings", IconName::Settings),
    ] {
        tiles = tiles.row(
            id,
            name,
            base::Press::new(id)
                .w(rems(6.5))
                .h(rems(4.5))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(rems(0.375))
                .rounded(theme.radius_lg())
                .border_1()
                .border_color(theme.border)
                .child(Icon::new(icon))
                .child(div().text_sm().child(name)),
        );
    }
    tiles
})
