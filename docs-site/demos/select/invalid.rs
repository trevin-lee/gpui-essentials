// Marked invalid, when a choice is required and none was made.
Select::new("country")
    .accessibility_label("Country")
    .placeholder("Country")
    .w(rems(14.))
    .options([("ca", "Canada"), ("mx", "Mexico"), ("us", "United States")])
    .selected(None::<SharedString>)
    .invalid(true)
