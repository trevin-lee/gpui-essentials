//! gpui-core's containers in a window: cards, alerts, collapsibles, accordions, items and a
//! table.
//!
//! ```sh
//! cargo run --example containers [-- --light]
//! ```
//!
//! Tab moves through everything that takes the focus; ⌘+ and ⌘− zoom.

use gpui::*;
use gpui_core::*;

actions!(containers, [ZoomIn, ZoomOut, Quit]);

struct Containers {
    /// What was pressed last.
    pressed: SharedString,
    /// Whether the collapsible the host keeps is open.
    collapsible_open: bool,
    /// The open items of the accordion the host keeps.
    accordion_open: Vec<SharedString>,
    /// The table's selected row.
    selected: Option<usize>,
}

fn heading(text: &'static str, theme: &Theme) -> Div {
    div()
        .text_size(theme.text.sm)
        .text_color(theme.muted_foreground)
        .child(text)
}

fn pressed(
    this: &WeakEntity<Containers>,
    what: &'static str,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let this = this.clone();
    move |_, _, cx| {
        this.update(cx, |c, cx| {
            c.pressed = what.into();
            cx.notify();
        })
        .ok();
    }
}

impl Containers {
    fn cards(&self, me: &WeakEntity<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_wrap()
            .items_start()
            .gap(rems(1.))
            .child(
                Card::new()
                    .w(rems(22.))
                    .title("Team plan")
                    .description("Five seats, billed monthly. Change the plan at any time.")
                    .action(
                        Button::new("card-edit")
                            .label("Edit")
                            .outline()
                            .small()
                            .on_click(pressed(me, "Edit")),
                    )
                    .child("Your next charge is on the 1st of the month.")
                    .footer(
                        Button::new("card-upgrade")
                            .label("Upgrade")
                            .small()
                            .on_click(pressed(me, "Upgrade")),
                    )
                    .footer(
                        Button::new("card-cancel")
                            .label("Cancel")
                            .ghost()
                            .small()
                            .on_click(pressed(me, "Cancel")),
                    ),
            )
            .child(
                Card::new()
                    .small()
                    .w(rems(16.))
                    .title("A small card with a title long enough to wrap")
                    .description("No footer, and tighter spacing.")
                    .child("Content."),
            )
            .child(Card::new().w(rems(12.)).child("Content alone."))
    }

    fn alerts(&self) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(rems(0.75))
            .max_w(rems(36.))
            .child(
                Alert::new("alert-default")
                    .icon(IconName::Info)
                    .title("Heads up")
                    .description("You can add components to your app with the command line."),
            )
            .child(
                Alert::new("alert-destructive")
                    .destructive()
                    .icon(IconName::CircleExclamationPoint)
                    .title("Unable to process your payment")
                    .description("Please verify your billing information and try again.")
                    .child("Check your card details.")
                    .child("Ensure you have enough funds."),
            )
            .child(
                Alert::new("alert-plain")
                    .role(Role::Note)
                    .title("A note with no icon and no description: its title wraps when the window is narrow."),
            )
    }
    fn collapsibles(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        div()
            .flex()
            .flex_col()
            .gap(rems(0.75))
            .max_w(rems(24.))
            .child(
                Collapsible::new("starred")
                    .label("@peduarte starred 3 repositories")
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(rems(0.5))
                            .pt(rems(0.5))
                            .children(
                                [
                                    "@radix-ui/primitives",
                                    "@radix-ui/colors",
                                    "@stitches/react",
                                ]
                                .map(|repo| {
                                    div()
                                        .px(rems(1.))
                                        .py(rems(0.5))
                                        .rounded(theme.radius_md())
                                        .border_1()
                                        .border_color(theme.border)
                                        .text_size(theme.text.sm)
                                        .child(repo)
                                }),
                            ),
                    ),
            )
            .child(
                Collapsible::new("kept-by-host")
                    .open(self.collapsible_open)
                    .on_toggle(cx.listener(|this, open: &bool, _, cx| {
                        this.collapsible_open = *open;
                        cx.notify();
                    }))
                    .trigger(
                        Press::new("kept-by-host-trigger")
                            .px(rems(0.5))
                            .py(rems(0.25))
                            .rounded(theme.radius_md())
                            .text_size(theme.text.sm)
                            .text_color(theme.primary)
                            .child(if self.collapsible_open {
                                "Show less"
                            } else {
                                "Show more"
                            }),
                    )
                    .child(
                        div()
                            .p(rems(0.5))
                            .text_size(theme.text.sm)
                            .text_color(theme.muted_foreground)
                            .child("The host keeps this one's state and draws its own trigger."),
                    ),
            )
            .child(
                Collapsible::new("disabled")
                    .label("Disabled")
                    .disabled(true)
                    .child("Never shown."),
            )
    }
    fn accordions(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let body = |text: &'static str| div().text_color(theme.muted_foreground).child(text);
        div()
            .flex()
            .flex_wrap()
            .items_start()
            .gap(rems(2.))
            .child(
                Accordion::new("faq")
                    .w(rems(22.))
                    .default_open(["shipping"])
                    .item(
                        AccordionItem::new("product")
                            .title("Product information")
                            .child(body("Our flagship product combines cutting-edge technology with sleek design.")),
                    )
                    .item(
                        AccordionItem::new("shipping")
                            .title("Shipping details")
                            .child(body("We offer worldwide shipping through trusted courier partners.")),
                    )
                    .item(
                        AccordionItem::new("archived")
                            .title("Archived (disabled)")
                            .disabled(true)
                            .child(body("Never shown.")),
                    )
                    .item(
                        AccordionItem::new("returns")
                            .title("Return policy, with a title long enough to wrap onto a second line")
                            .child(body("Returns are accepted within thirty days.")),
                    ),
            )
            .child(
                Accordion::new("settings")
                    .w(rems(18.))
                    .multiple(true)
                    .open(self.accordion_open.clone())
                    .on_change(cx.listener(|this, open: &Vec<SharedString>, _, cx| {
                        this.accordion_open = open.clone();
                        cx.notify();
                    }))
                    .item(AccordionItem::new("general").title("General").child(body("Several can be open.")))
                    .item(AccordionItem::new("privacy").title("Privacy").child(body("The host keeps these.")))
                    .item(AccordionItem::new("advanced").title("Advanced").child(body("Up, Down, Home, End."))),
            )
    }
    fn items(&self, me: &WeakEntity<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(rems(0.75))
            .max_w(rems(30.))
            .child(
                Item::new("item-basic")
                    .outline()
                    .title("Basic item")
                    .description("A simple item with a title and a description.")
                    .action(Button::new("item-action").label("Action").outline().small().on_click(pressed(me, "Action"))),
            )
            .child(
                Item::new("item-verified")
                    .outline()
                    .small()
                    .icon(IconName::BadgeCheck)
                    .title("Your profile has been verified.")
                    .on_click(pressed(me, "Verified")),
            )
            .child(
                ItemGroup::new("people")
                    .accessibility_label("People")
                    .child(
                        Item::new("ana")
                            .muted()
                            .icon(IconName::User)
                            .title("Ana Ruiz")
                            .description("Asked to move Thursday's review to Friday, and wondered whether the notes from last week could go out before then.")
                            .on_click(pressed(me, "Ana")),
                    )
                    .child(ItemSeparator::new())
                    .child(
                        Item::new("bo")
                            .icon(IconName::User)
                            .title("Bo Chen, with a name long enough to be cut short at the end of the row")
                            .description("Disabled.")
                            .disabled(true)
                            .on_click(pressed(me, "Bo")),
                    ),
            )
    }
    fn table(&self, cx: &mut Context<Self>) -> impl IntoElement {
        const STATUS: [&str; 3] = ["Paid", "Pending", "Unpaid"];
        const METHOD: [&str; 3] = ["Credit Card", "PayPal", "Bank Transfer"];
        Table::new("invoices")
            .max_w(rems(40.))
            .h(rems(20.))
            .column(TableColumn::new("Invoice").fixed(rems(6.)))
            .column(TableColumn::new("Status"))
            .column(TableColumn::new("Method").fraction(1.5))
            .column(TableColumn::new("Amount").align(ColumnAlign::End))
            .rows(1000, |range, _, _| {
                range
                    .map(|i| {
                        TableRow::new()
                            .cell(format!("INV{:04}", i + 1))
                            .cell(STATUS[i % 3])
                            .cell(METHOD[i % 3])
                            .cell(format!("${}.00", 50 + (i * 37) % 450))
                    })
                    .collect()
            })
            .footer(
                TableRow::new()
                    .cell("Total")
                    .cell("")
                    .cell("")
                    .cell("$250,000.00"),
            )
            .caption("A thousand invoices: only the rows in view are drawn.")
            .selected(self.selected)
            .on_select(cx.listener(|this, row: &usize, _, cx| {
                this.selected = Some(*row);
                cx.notify();
            }))
    }
}

impl Render for Containers {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let me = cx.entity().downgrade();
        let theme = cx.theme().clone();
        div()
            .id("page")
            .size_full()
            .overflow_y_scroll()
            .p(rems(1.5))
            .flex()
            .flex_col()
            .gap(rems(1.))
            .child(heading("Card", &theme))
            .child(self.cards(&me))
            .child(heading("Alert", &theme))
            .child(self.alerts())
            .child(heading("Collapsible", &theme))
            .child(self.collapsibles(cx))
            .child(heading("Accordion", &theme))
            .child(self.accordions(cx))
            .child(heading("Item", &theme))
            .child(self.items(&me))
            .child(heading("Table", &theme))
            .child(self.table(cx))
            .child(
                div()
                    .text_size(theme.text.sm)
                    .text_color(theme.muted_foreground)
                    .child(format!("Pressed: {}", self.pressed)),
            )
    }
}

fn main() {
    let light = std::env::args().any(|a| a == "--light");
    gpui_platform::application()
        .with_assets(Icons)
        .run(move |cx| {
            gpui_core::init(cx);
            Theme::set(if light { Theme::light() } else { Theme::dark() }, cx);
            cx.bind_keys([
                KeyBinding::new("cmd-=", ZoomIn, None),
                KeyBinding::new("cmd--", ZoomOut, None),
                KeyBinding::new("cmd-q", Quit, None),
            ]);
            cx.on_action(|_: &ZoomIn, cx| {
                let t = Theme::global_mut(cx);
                t.font_size = (t.font_size * 1.1).min(px(60.));
                cx.refresh_windows();
            });
            cx.on_action(|_: &ZoomOut, cx| {
                let t = Theme::global_mut(cx);
                t.font_size = (t.font_size / 1.1).max(px(6.));
                cx.refresh_windows();
            });
            cx.on_action(|_: &Quit, cx| cx.quit());
            let bounds = Bounds::centered(None, size(px(900.), px(700.)), cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            };
            gpui_core::open_window(options, cx, |_, cx| {
                cx.new(|_| Containers {
                    pressed: "nothing yet".into(),
                    collapsible_open: false,
                    accordion_open: vec!["general".into()],
                    selected: None,
                })
            })
            .unwrap();
            cx.activate(true);
        });
}
