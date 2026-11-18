// Rows under column headings, drawn only as far as they're seen, so a long table stays quick.
// Press a row (or move with the arrows) to select it; the view keeps which.
pub struct Example {
    selected: Option<usize>,
}

const INVOICES: [(&str, &str, &str, &str); 5] = [
    ("INV001", "Paid", "Credit card", "$250.00"),
    ("INV002", "Pending", "PayPal", "$150.00"),
    ("INV003", "Unpaid", "Bank transfer", "$350.00"),
    ("INV004", "Paid", "Credit card", "$450.00"),
    ("INV005", "Paid", "PayPal", "$550.00"),
];

impl Example {
    pub fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self { selected: Some(1) }
    }
}

impl Render for Example {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        Table::new("invoices")
            .w(rems(30.))
            .h(rems(20.))
            .column(TableColumn::new("Invoice").fixed(px(90.)))
            .column(TableColumn::new("Status"))
            .column(TableColumn::new("Method"))
            .column(TableColumn::new("Amount").align(ColumnAlign::End))
            .rows(INVOICES.len(), |range, _, _| {
                range
                    .map(|i| {
                        let (invoice, status, method, amount) = INVOICES[i];
                        TableRow::new()
                            .cell(invoice)
                            .cell(status)
                            .cell(method)
                            .cell(amount)
                    })
                    .collect()
            })
            .selected(self.selected)
            .on_select(cx.listener(|this, row: &usize, _, cx| {
                this.selected = Some(*row);
                cx.notify();
            }))
            .caption("A list of your recent invoices.")
    }
}
