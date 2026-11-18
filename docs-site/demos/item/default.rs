// A row with an icon, a title, a description and an action; pressable when given
// `on_click`. An ItemGroup stacks them.
div().w(rems(28.)).child(
    ItemGroup::new("people")
        .child(
            Item::new("alex")
                .outline()
                .icon(IconName::User)
                .title("Alex Kim")
                .description("Asked to move Thursday's review to Friday.")
                .action(Button::new("reply").label("Reply").outline().small()),
        )
        .child(
            Item::new("security")
                .muted()
                .icon(IconName::Bell)
                .title("Security alert")
                .description("A new sign-in from a browser on a Mac."),
        ),
)
