// In the destructive colour, for something that failed.
div().w(rems(28.)).child(
    Alert::new("sync-failed")
        .destructive()
        .icon(IconName::CircleExclamationPoint)
        .title("Sync failed")
        .description("Check your connection and try again."),
)
