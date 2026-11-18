// A long path with `max_items`: the middle folds into an ellipsis.
div().child(
    Breadcrumb::new("deep-path")
        .item(BreadcrumbItem::new("home", "Home").icon(IconName::House))
        .item(BreadcrumbItem::new("projects", "Projects"))
        .item(BreadcrumbItem::new("website", "Website"))
        .item(BreadcrumbItem::new("pages", "Pages"))
        .item(BreadcrumbItem::new("about", "About"))
        .max_items(3),
)
