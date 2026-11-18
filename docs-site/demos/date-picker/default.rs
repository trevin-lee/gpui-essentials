// A button that opens a month to pick a day from. A `DatePickerState` keeps the date.
pub struct Example {
    date: Entity<DatePickerState>,
}

impl Example {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            date: cx.new(|cx| DatePickerState::new(window, cx)),
        }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(rems(14.))
            .child(DatePicker::new(&self.date).placeholder("Pick a date"))
    }
}
