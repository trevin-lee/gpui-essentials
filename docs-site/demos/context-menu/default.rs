// A menu that opens where an element is right-clicked, pressed with two fingers, or held,
// or from the keyboard (Shift-F10) under the focused element.
ContextMenu::new(
    "file-row",
    Press::new("file")
        .w(rems(20.))
        .h(rems(8.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(cx.theme().radius_lg())
        .border_1()
        .border_dashed()
        .border_color(cx.theme().border)
        .text_size(cx.theme().text.sm)
        .child("Right-click here"),
)
.menu(|_, _| {
    PopupMenu::new("file-menu")
        .item(PopupMenuItem::new("back", "Back").shortcut("⌘["))
        .item(PopupMenuItem::new("forward", "Forward").shortcut("⌘]").disabled(true))
        .item(PopupMenuItem::new("reload", "Reload").shortcut("⌘R"))
        .separator()
        .item(PopupMenuItem::new("bookmarks", "Show bookmarks").checked(true))
        .item(PopupMenuItem::new("urls", "Show full URLs"))
})
