// Underlined instead of in a pill, with icons, and one tab disabled.
div().w(rems(28.)).child(
    Tabs::new("project")
        .accessibility_label("Project")
        .line()
        .tab(Tab::new("overview", "Overview").icon(IconName::LayoutDashboard))
        .tab(Tab::new("analytics", "Analytics").icon(IconName::ChartLine))
        .tab(Tab::new("reports", "Reports").icon(IconName::FileText))
        .tab(Tab::new("settings", "Settings").icon(IconName::Settings).disabled(true))
        .default_selected("analytics"),
)
