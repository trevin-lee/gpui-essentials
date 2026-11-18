// A header (a title, a description and an action at the top right), content and a footer.
// Each part is drawn only when given.
Card::new()
    .w(rems(24.))
    .title("Team plan")
    .description("Five seats, billed monthly. Change the plan at any time.")
    .action(Button::new("edit").label("Edit").outline().small())
    .child("Your next charge is on the 1st of the month.")
    .footer(Button::new("upgrade").label("Upgrade").small())
    .footer(Button::new("cancel").label("Cancel").ghost().small())
