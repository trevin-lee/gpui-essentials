// Open: the month hangs from the button. (The box is tall only to leave the month room in
// the picture.)
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
            .h(rems(24.))
            .child(DatePicker::new(&self.date).placeholder("Pick a date"))
    }
}
