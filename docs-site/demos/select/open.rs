// Open: the list hangs from the button, the picked option checked.
Select::new("fruit-open")
    .accessibility_label("Fruit")
    .w(rems(14.))
    .heading("Fruit")
    .options([
        ("apple", "Apple"),
        ("banana", "Banana"),
        ("blueberry", "Blueberry"),
        ("pineapple", "Pineapple"),
    ])
    .selected(Some("banana"))
