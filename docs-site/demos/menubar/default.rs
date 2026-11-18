// A row of menus, each opening below its title. While one is open, the pointer moving onto
// another title opens that one; Left and Right do the same from the keyboard.
Menubar::new("menubar")
    .menu(MenubarMenu::new(
        "file",
        "File",
        PopupMenu::new("file-menu")
            .item(PopupMenuItem::new("new-tab", "New Tab").shortcut("⌘T"))
            .item(PopupMenuItem::new("new-window", "New Window").shortcut("⌘N"))
            .separator()
            .item(PopupMenuItem::new("print", "Print…").shortcut("⌘P")),
    ))
    .menu(MenubarMenu::new(
        "edit",
        "Edit",
        PopupMenu::new("edit-menu")
            .item(PopupMenuItem::new("undo", "Undo").shortcut("⌘Z"))
            .item(PopupMenuItem::new("redo", "Redo").shortcut("⇧⌘Z"))
            .separator()
            .item(PopupMenuItem::new("cut", "Cut").shortcut("⌘X"))
            .item(PopupMenuItem::new("copy", "Copy").shortcut("⌘C"))
            .item(PopupMenuItem::new("paste", "Paste").shortcut("⌘V")),
    ))
    .menu(MenubarMenu::new(
        "view",
        "View",
        PopupMenu::new("view-menu")
            .item(PopupMenuItem::new("sidebar", "Show Sidebar").checked(true))
            .item(PopupMenuItem::new("full-screen", "Enter Full Screen")),
    ))
