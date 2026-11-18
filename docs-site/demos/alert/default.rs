// A note in the page that calls for attention, with an icon, a title and a description.
div().w(rems(28.)).child(
    Alert::new("update")
        .icon(IconName::CircleCheck)
        .title("Your changes were saved")
        .description("Everyone with the link sees the new version now."),
)
