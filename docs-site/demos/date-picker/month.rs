// The month on its own, in place, showing this month: the arrows move the day, and `dots`
// can mark days (a calendar's days with events).
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
        MonthGrid::new(&self.date)
    }
}
