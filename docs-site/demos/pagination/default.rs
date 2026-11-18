// Pages by number, with Previous and Next; far-off pages fold into an ellipsis. The view
// keeps the page.
pub struct Example {
    page: usize,
}

impl Example {
    pub fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self { page: 5 }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        Pagination::new("results")
            .total(20)
            .page(self.page)
            .on_page(cx.listener(|this, page: &usize, _, cx| {
                this.page = *page;
                cx.notify();
            }))
    }
}
