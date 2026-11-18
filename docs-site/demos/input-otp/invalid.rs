// Marked invalid, after a wrong code: every box takes the destructive colour.
pub struct Example {
    code: Entity<InputOtpState>,
}

impl Example {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let code = cx.new(|cx| {
            let mut code = InputOtpState::new(window, cx);
            code.set_value("402981", window, cx);
            code
        });
        Self { code }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        InputOtp::new(&self.code)
            .accessibility_label("Verification code")
            .groups([3, 3])
            .invalid(true)
    }
}
