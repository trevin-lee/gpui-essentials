//! gpui-core's navigation components in a window: a sidebar around tabs, breadcrumbs,
//! pagination, a navigation menu and a menubar.
//!
//! ```sh
//! cargo run --example navigation [-- --light]
//! ```
//!
//! Tab moves through everything that takes the focus; ⌘+ and ⌘− zoom.

use gpui::*;
use gpui_core::*;

actions!(navigation, [ZoomIn, ZoomOut, Quit]);

struct Navigation {
    sidebar: Entity<SidebarState>,
    /// The sidebar's active item.
    item: SharedString,
    /// The settings tabs' selection, owned here.
    tab: SharedString,
    /// Where the breadcrumbs last went.
    went: SharedString,
    /// The current page.
    page: usize,
    /// Where the navigation menu last went.
    nav: SharedString,
    /// The menubar's last pick.
    picked: SharedString,
}

#[allow(dead_code)]
fn heading(text: &'static str, theme: &Theme) -> Div {
    div()
        .text_size(theme.text.sm)
        .text_color(theme.muted_foreground)
        .child(text)
}

#[allow(dead_code)]
fn note(text: String, theme: &Theme) -> Div {
    div()
        .text_size(theme.text.sm)
        .text_color(theme.muted_foreground)
        .child(text)
}

#[allow(dead_code)]
fn panel(title: &'static str, body: &'static str, theme: &Theme) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(rems(0.25))
        .p(rems(1.))
        .rounded(theme.radius_lg())
        .border_1()
        .border_color(theme.border)
        .bg(theme.card)
        .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
        .child(
            div()
                .text_size(theme.text.sm)
                .text_color(theme.muted_foreground)
                .child(body),
        )
}

impl Render for Navigation {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let page_body = div()
            .id("navigation")
            .size_full()
            .overflow_y_scroll()
            .p(rems(1.5))
            .flex()
            .flex_col()
            .gap(rems(1.))
            .child(heading("Tabs", &theme))
            .child(
                Tabs::new("settings")
                    .accessibility_label("Settings")
                    .tab(Tab::new("account", "Account").child(panel(
                        "Account",
                        "Make changes to your account here.",
                        &theme,
                    )))
                    .tab(Tab::new("password", "Password").child(panel(
                        "Password",
                        "Change your password here.",
                        &theme,
                    )))
                    .tab(Tab::new("billing", "Billing").disabled(true))
                    .selected(self.tab.clone())
                    .on_change(cx.listener(|this, id: &SharedString, _, cx| {
                        this.tab = id.clone();
                        cx.notify();
                    })),
            )
            .child(heading("Tabs, underlined, left to themselves", &theme))
            .child(
                Tabs::new("views")
                    .line()
                    .accessibility_label("Views")
                    .tab(Tab::new("overview", "Overview").icon(IconName::LayoutDashboard))
                    .tab(Tab::new("analytics", "Analytics").icon(IconName::ChartLine))
                    .tab(Tab::new("reports", "Reports").icon(IconName::FileText)),
            )
            .child(heading("Tabs that scroll in a narrow box", &theme))
            .child(
                div().w(rems(16.)).child(
                    Tabs::new("many")
                        .overflow(TabsOverflow::Scroll)
                        .accessibility_label("Months")
                        .tabs(
                            [
                                "January", "February", "March", "April", "May", "June", "July",
                                "August",
                            ]
                            .map(|m| Tab::new(m, m)),
                        ),
                ),
            )
            .child(heading("Breadcrumb", &theme))
            .child(
                Breadcrumb::new("path")
                    .item(BreadcrumbItem::new("home", "Home").icon(IconName::House))
                    .item(BreadcrumbItem::new("docs", "Docs"))
                    .item(BreadcrumbItem::new("components", "Components"))
                    .item(BreadcrumbItem::new("navigation", "Navigation"))
                    .item(BreadcrumbItem::new("breadcrumb", "Breadcrumb"))
                    .max_items(4)
                    .on_navigate(cx.listener(|this, id: &SharedString, _, cx| {
                        this.went = id.clone();
                        cx.notify();
                    })),
            )
            .child(heading("Breadcrumb in a narrow box", &theme))
            .child(
                div().w(rems(14.)).child(
                    Breadcrumb::new("narrow-path")
                        .item(BreadcrumbItem::new("home", "Home"))
                        .item(BreadcrumbItem::new("projects", "Projects"))
                        .item(BreadcrumbItem::new("reports", "Reports"))
                        .item(BreadcrumbItem::new("models", "Models and their keys"))
                        .on_navigate(cx.listener(|this, id: &SharedString, _, cx| {
                            this.went = id.clone();
                            cx.notify();
                        })),
                ),
            )
            .child(note(format!("Went to: {}", self.went), &theme))
            .child(heading("Pagination", &theme))
            .child(
                Pagination::new("pages")
                    .total(20)
                    .page(self.page)
                    .on_page(cx.listener(|this, page: &usize, _, cx| {
                        this.page = *page;
                        cx.notify();
                    })),
            )
            .child(heading("Pagination in a narrow box", &theme))
            .child(
                div().w(rems(14.)).child(
                    Pagination::new("narrow-pages")
                        .total(20)
                        .page(self.page)
                        .on_page(cx.listener(|this, page: &usize, _, cx| {
                            this.page = *page;
                            cx.notify();
                        })),
                ),
            )
            .child(heading("Navigation menu", &theme))
            .child(
                NavigationMenu::new("site")
                    .item(
                        NavigationMenuItem::panel("start", "Getting started")
                            .with_link(NavigationMenuLink::new("intro", "Introduction").description(
                                "Re-usable components drawn in your theme.",
                            ))
                            .with_link(NavigationMenuLink::new("install", "Installation").description(
                                "How to add gpui-core to an app.",
                            )),
                    )
                    .item(
                        NavigationMenuItem::panel("components", "Components")
                            .columns(2)
                            .with_link(NavigationMenuLink::new("dialog", "Alert Dialog").description(
                                "A modal dialog that interrupts with important content and expects a response.",
                            ))
                            .with_link(NavigationMenuLink::new("hover", "Hover Card").description(
                                "For sighted users to preview content behind a link.",
                            ))
                            .with_link(NavigationMenuLink::new("progress", "Progress").description(
                                "Shows how far along a task is.",
                            ))
                            .with_link(NavigationMenuLink::new("tabs", "Tabs").description(
                                "Layered panels of content, shown one at a time.",
                            )),
                    )
                    .item(NavigationMenuItem::link("docs", "Docs"))
                    .on_navigate(cx.listener(|this, id: &SharedString, _, cx| {
                        this.nav = id.clone();
                        cx.notify();
                    })),
            )
            .child(note(format!("Went to: {}", self.nav), &theme))
            .child(heading("Menubar", &theme))
            .child(
                Menubar::new("menubar")
                    .menu(MenubarMenu::new(
                        "file",
                        "File",
                        PopupMenu::new("file-menu")
                            .item(PopupMenuItem::new("new-tab", "New Tab").shortcut("⌘T"))
                            .item(PopupMenuItem::new("new-window", "New Window").shortcut("⌘N"))
                            .separator()
                            .item(
                                PopupMenuItem::new("share", "Share").submenu(
                                    PopupMenu::new("share-menu")
                                        .item(PopupMenuItem::new("email", "Email link"))
                                        .item(PopupMenuItem::new("messages", "Messages")),
                                ),
                            )
                            .separator()
                            .item(PopupMenuItem::new("print", "Print…").shortcut("⌘P")),
                    ))
                    .menu(MenubarMenu::new(
                        "edit",
                        "Edit",
                        PopupMenu::new("edit-menu")
                            .item(PopupMenuItem::new("undo", "Undo").shortcut("⌘Z"))
                            .item(PopupMenuItem::new("redo", "Redo").shortcut("⇧⌘Z")),
                    ))
                    .menu(MenubarMenu::new(
                        "view",
                        "View",
                        PopupMenu::new("view-menu")
                            .item(
                                PopupMenuItem::new("bookmarks", "Always Show Bookmarks Bar")
                                    .checked(true),
                            )
                            .item(
                                PopupMenuItem::new("urls", "Always Show Full URLs").checked(false),
                            ),
                    ))
                    .on_select(cx.listener(|this, (menu, pick): &MenubarSelect, _, cx| {
                        this.picked = format!("{menu} › {}", pick.id).into();
                        cx.notify();
                    })),
            )
            .child(note(format!("Picked: {}", self.picked), &theme));
        let page = page_body;
        Sidebar::new("app-sidebar", &self.sidebar)
            .header(
                div()
                    .flex()
                    .items_center()
                    .gap(rems(0.5))
                    .p(rems(0.25))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(Icon::new(IconName::Command))
                    .child("Acme"),
            )
            .group(
                SidebarGroup::new("platform")
                    .label("Platform")
                    .item(
                        SidebarItem::new("inbox", "Inbox")
                            .icon(IconName::Inbox)
                            .badge("12"),
                    )
                    .item(
                        SidebarItem::new("documents", "Documents")
                            .icon(IconName::FileText)
                            .action(IconName::Plus, "New document", |_, _, _| {}),
                    )
                    .item(SidebarItem::new("calendar", "Calendar").icon(IconName::Calendar))
                    .item(SidebarItem::new("search", "Search").icon(IconName::Search)),
            )
            .group(
                SidebarGroup::new("projects")
                    .label("Projects")
                    .collapsible(true)
                    .item(SidebarItem::new("design", "Design engineering").icon(IconName::Frame))
                    .item(SidebarItem::new("sales", "Sales and marketing").icon(IconName::ChartPie))
                    .item(SidebarItem::new("travel", "Travel").icon(IconName::Map)),
            )
            .footer(
                div()
                    .text_size(theme.text.sm)
                    .text_color(theme.sidebar_foreground)
                    .child("ada@example.com"),
            )
            .active(self.item.clone())
            .on_select(cx.listener(|this, id: &SharedString, _, cx| {
                this.item = id.clone();
                cx.notify();
            }))
            .content(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(rems(0.5))
                            .p(rems(0.5))
                            .border_b_1()
                            .border_color(theme.border)
                            .child(SidebarTrigger::new("sidebar-trigger", &self.sidebar))
                            .child(
                                div()
                                    .text_size(theme.text.sm)
                                    .child(format!("Sidebar: {} (⌘B)", self.item)),
                            ),
                    )
                    .child(page),
            )
    }
}

fn main() {
    let light = std::env::args().any(|a| a == "--light");
    gpui_platform::application()
        .with_assets(Icons)
        .run(move |cx| {
            gpui_core::init(cx);
            Theme::set(if light { Theme::light() } else { Theme::dark() }, cx);
            cx.bind_keys([
                KeyBinding::new("cmd-=", ZoomIn, None),
                KeyBinding::new("cmd--", ZoomOut, None),
                KeyBinding::new("cmd-q", Quit, None),
            ]);
            cx.on_action(|_: &ZoomIn, cx| {
                let t = Theme::global_mut(cx);
                t.font_size = (t.font_size * 1.1).min(px(60.));
            });
            cx.on_action(|_: &ZoomOut, cx| {
                let t = Theme::global_mut(cx);
                t.font_size = (t.font_size / 1.1).max(px(6.));
            });
            cx.on_action(|_: &Quit, cx| cx.quit());
            let bounds = Bounds::centered(None, size(px(1000.), px(700.)), cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            };
            gpui_core::open_window(options, cx, |_, cx| {
                cx.new(|cx| {
                    let _ = &cx;
                    Navigation {
                        sidebar: cx.new(|_| SidebarState::new()),
                        item: "inbox".into(),
                        tab: "account".into(),
                        went: "nowhere yet".into(),
                        page: 4,
                        nav: "nowhere yet".into(),
                        picked: "nothing yet".into(),
                    }
                })
            })
            .expect("the window");
            cx.activate(true);
        });
}
