// A site's top links; an item with a panel opens it, a grid of links with descriptions.
NavigationMenu::new("site")
    .item(
        NavigationMenuItem::panel("getting-started", "Getting started")
            .with_link(
                NavigationMenuLink::new("introduction", "Introduction")
                    .description("What's in it, and how to add it to an app."),
            )
            .with_link(
                NavigationMenuLink::new("theming", "Theming")
                    .description("Colours, radii and type, light and dark."),
            ),
    )
    .item(
        NavigationMenuItem::panel("components", "Components")
            .columns(2)
            .with_link(
                NavigationMenuLink::new("dialog", "Dialog")
                    .description("A window over the page that takes the keyboard."),
            )
            .with_link(
                NavigationMenuLink::new("tabs", "Tabs").description("Panels shown one at a time."),
            )
            .with_link(
                NavigationMenuLink::new("tooltip", "Tooltip")
                    .description("A few words under the pointer."),
            )
            .with_link(
                NavigationMenuLink::new("toast", "Toast")
                    .description("A short note that goes by itself."),
            ),
    )
    .item(NavigationMenuItem::link("docs", "Docs"))
