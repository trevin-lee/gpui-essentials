// What a place shows with nothing in it yet: an icon, a title, a line of help and an action.
EmptyState::new()
    .icon(IconName::Inbox)
    .title("No messages yet")
    .description("When someone writes to you, it shows up here.")
    .child(Button::new("new").label("Start a chat"))
