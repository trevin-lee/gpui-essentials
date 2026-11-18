// Four boxes that take letters as well as digits, partly filled.
pub struct Example {
    pin: Entity<InputOtpState>,
}

impl Example {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let pin = cx.new(|cx| {
            let mut pin = InputOtpState::with_length(4, window, cx).digits_only(false);
            pin.set_value("K7", window, cx);
            pin
        });
        Self { pin }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        InputOtp::new(&self.pin).accessibility_label("Pairing code")
    }
}
