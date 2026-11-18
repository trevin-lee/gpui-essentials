// A one-time code: six digits in two groups. Typing moves along, pasting fills every box,
// and `on_complete` is told the code once the last box is filled.
pub struct Example {
    code: Entity<InputOtpState>,
}

impl Example {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            code: cx.new(|cx| InputOtpState::new(window, cx)),
        }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        InputOtp::new(&self.code)
            .accessibility_label("Verification code")
            .groups([3, 3])
    }
}
