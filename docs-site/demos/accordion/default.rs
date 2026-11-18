// Sections that open one at a time under their titles; `multiple(true)` lets several stay
// open. It keeps which are open itself, starting from `default_open`.
div().w(rems(26.)).child(
    Accordion::new("faq")
        .default_open(["shipping"])
        .item(
            AccordionItem::new("shipping")
                .title("How long does shipping take?")
                .child("Three to five working days, tracked from the day it leaves."),
        )
        .item(
            AccordionItem::new("returns")
                .title("Can I return it?")
                .child("Within thirty days, in its box, for a full refund."),
        )
        .item(
            AccordionItem::new("warranty")
                .title("Is there a warranty?")
                .child("Two years, covering anything that fails in normal use."),
        ),
)
