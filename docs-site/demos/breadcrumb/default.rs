// Where a page sits, from the top down; each place but the last is a link, and
// `on_navigate` is told which was pressed.
div().child(
    Breadcrumb::new("path")
        .item(BreadcrumbItem::new("home", "Home"))
        .item(BreadcrumbItem::new("components", "Components"))
        .item(BreadcrumbItem::new("breadcrumb", "Breadcrumb")),
)
