// An app's side navigation: groups of items, the current one marked, beside the content. A
// `SidebarState` keeps whether it's collapsed to icons; below `breakpoint` it opens over the
// content instead.
pub struct Example {
    sidebar: Entity<SidebarState>,
    page: SharedString,
}

impl Example {
    pub fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            sidebar: cx.new(|_| SidebarState::new()),
            page: "inbox".into(),
        }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        div()
            .w(rems(36.))
            .h(rems(22.))
            .rounded(theme.radius_lg())
            .border_1()
            .border_color(theme.border)
            .overflow_hidden()
            .child(
                Sidebar::new("nav", &self.sidebar)
                    .breakpoint(px(480.))
                    .group(
                        SidebarGroup::new("mail")
                            .label("Mail")
                            .item(
                                SidebarItem::new("inbox", "Inbox")
                                    .icon(IconName::Inbox)
                                    .badge("4"),
                            )
                            .item(SidebarItem::new("sent", "Sent").icon(IconName::Send))
                            .item(SidebarItem::new("archive", "Archive").icon(IconName::Archive)),
                    )
                    .group(
                        SidebarGroup::new("workspace")
                            .label("Workspace")
                            .item(SidebarItem::new("documents", "Documents").icon(IconName::FileText))
                            .item(SidebarItem::new("team", "Team").icon(IconName::Users))
                            .item(
                                SidebarItem::new("settings", "Settings").icon(IconName::Settings),
                            ),
                    )
                    .active(self.page.clone())
                    .on_select(cx.listener(|this, id: &SharedString, _, cx| {
                        this.page = id.clone();
                        cx.notify();
                    }))
                    .content(
                        div()
                            .p(rems(1.))
                            .flex()
                            .items_center()
                            .gap(rems(0.5))
                            .child(SidebarTrigger::new("toggle", &self.sidebar))
                            .child(match self.page.as_ref() {
                                "inbox" => "Inbox",
                                "sent" => "Sent",
                                "archive" => "Archive",
                                "documents" => "Documents",
                                "team" => "Team",
                                _ => "Settings",
                            }),
                    ),
            )
    }
}
