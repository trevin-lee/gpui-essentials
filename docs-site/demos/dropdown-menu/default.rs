// A button that opens a menu hung from it. The menu keeps whether it's open; `on_select` is
// told the entry picked, and the menu closes.
DropdownMenu::new("account")
    .trigger(Button::new("account-button").label("My account").outline())
    .heading("My Account")
    .items([
        PopupMenuItem::new("profile", "Profile")
            .icon(IconName::User)
            .shortcut("⇧⌘P"),
        PopupMenuItem::new("billing", "Billing")
            .icon(IconName::CreditCard)
            .shortcut("⌘B"),
        PopupMenuItem::new("settings", "Settings")
            .icon(IconName::Settings)
            .shortcut("⌘S"),
    ])
    .separator()
    .item(PopupMenuItem::new("log-out", "Log out").icon(IconName::LogOut))
