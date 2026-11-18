// Entries with checks, a submenu and a disabled one. The view keeps what's checked.
pub struct Example {
    sort: SharedString,
}

impl Example {
    pub fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self {
            sort: "date".into(),
        }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        DropdownMenu::new("sort")
            .trigger(
                Button::new("sort-button")
                    .icon(IconName::ArrowUpDown)
                    .label("Sort")
                    .outline(),
            )
            .items(
                [
                    ("name", "By name"),
                    ("date", "By date modified"),
                    ("size", "By size"),
                ]
                .map(|(id, label)| PopupMenuItem::new(id, label).checked(self.sort.as_ref() == id)),
            )
            .separator()
            .item(
                PopupMenuItem::new("group", "Group by").submenu(
                    PopupMenu::new("group-menu")
                        .item(PopupMenuItem::new("kind", "Kind"))
                        .item(PopupMenuItem::new("owner", "Owner")),
                ),
            )
            .item(
                PopupMenuItem::new("delete", "Delete all")
                    .icon(IconName::Trash)
                    .disabled(true),
            )
            .on_select(cx.listener(|this, pick: &MenuSelect, _, cx| {
                if ["name", "date", "size"].contains(&pick.id.as_ref()) {
                    this.sort = pick.id.clone();
                    cx.notify();
                }
            }))
    }
}
