//! gpui-core's form controls in a window.
//!
//! ```sh
//! cargo run --example forms [-- --light]
//! ```
//!
//! Tab moves through everything that takes the focus; ⌘+ and ⌘− zoom.

use gpui::*;
use gpui_core::*;

actions!(forms, [ZoomIn, ZoomOut, Quit]);

struct Forms {
    terms: bool,
    marketing: bool,
    /// The three items under the "select all" check box.
    items: [bool; 3],
    plan: SharedString,
    size: SharedString,
    italic: bool,
    underline: bool,
    align: Vec<SharedString>,
    marks: Vec<SharedString>,
    name: Entity<InputState>,
    email: Entity<InputState>,
    news: bool,
    sync: bool,
    fruit: Option<SharedString>,
    zone: Option<SharedString>,
    site: Entity<InputState>,
    search: Entity<InputState>,
    price: Entity<InputState>,
    copied: usize,
    code: Entity<InputOtpState>,
    pin: Entity<InputOtpState>,
    last: Option<SharedString>,
}

fn heading(text: &'static str, theme: &Theme) -> Div {
    div()
        .text_size(theme.text.sm)
        .text_color(theme.muted_foreground)
        .child(text)
}

fn row() -> Div {
    div().flex().flex_wrap().items_center().gap(rems(0.5))
}

impl Forms {
    fn checkboxes(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let all = self.items.iter().all(|i| *i);
        let some = self.items.iter().any(|i| *i);
        div()
            .flex()
            .flex_col()
            .gap(rems(0.75))
            .child(heading("Checkbox", &theme))
            .child(
                Checkbox::new("terms")
                    .label("Accept terms and conditions")
                    .checked(self.terms)
                    .on_click(cx.listener(|this, on: &bool, _, cx| {
                        this.terms = *on;
                        cx.notify();
                    })),
            )
            .child(
                Checkbox::new("marketing")
                    .label("Email me about new features, now and then, and about nothing else at all — a long label wraps onto a second line rather than running out of its box")
                    .checked(self.marketing)
                    .on_click(cx.listener(|this, on: &bool, _, cx| {
                        this.marketing = *on;
                        cx.notify();
                    })),
            )
            .child(
                Checkbox::new("disabled")
                    .label("Disabled")
                    .checked(true)
                    .disabled(true),
            )
            .child(
                Checkbox::new("invalid")
                    .label("Invalid: required")
                    .invalid(true),
            )
            .child(
                Checkbox::new("all")
                    .label("Select all")
                    .checked(all)
                    .indeterminate(some && !all)
                    .on_click(cx.listener(|this, on: &bool, _, cx| {
                        this.items = [*on; 3];
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .pl(rems(1.5))
                    .flex()
                    .flex_col()
                    .gap(rems(0.5))
                    .children(["Inbox", "Drafts", "Archive"].into_iter().enumerate().map(
                        |(i, name)| {
                            Checkbox::new(("item", i))
                                .label(name)
                                .checked(self.items[i])
                                .on_click(cx.listener(move |this, on: &bool, _, cx| {
                                    this.items[i] = *on;
                                    cx.notify();
                                }))
                        },
                    )),
            )
            .child(
                Checkbox::new("touch")
                    .label("Touch size: 44 points tall")
                    .checked(self.terms)
                    .touch()
                    .on_click(cx.listener(|this, on: &bool, _, cx| {
                        this.terms = *on;
                        cx.notify();
                    })),
            )
    }

    fn radios(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        div()
            .flex()
            .flex_col()
            .gap(rems(0.75))
            .child(heading("RadioGroup", &theme))
            .child(
                RadioGroup::new("plan")
                    .accessibility_label("Plan")
                    .option("free", "Free")
                    .option("pro", "Pro")
                    .option("enterprise", "Enterprise (talk to us first)")
                    .option("team", "Team: a long label wraps onto a second line rather than running out of its box, as shadcn's does")
                    .disable("enterprise")
                    .selected(self.plan.clone())
                    .on_change(cx.listener(|this, id: &SharedString, _, cx| {
                        this.plan = id.clone();
                        cx.notify();
                    })),
            )
            .child(
                RadioGroup::new("size")
                    .accessibility_label("Size")
                    .horizontal(true)
                    .option("s", "Small")
                    .option("m", "Medium")
                    .option("l", "Large")
                    .selected(self.size.clone())
                    .on_change(cx.listener(|this, id: &SharedString, _, cx| {
                        this.size = id.clone();
                        cx.notify();
                    })),
            )
            .child(
                RadioGroup::new("touch")
                    .accessibility_label("Size, for touch")
                    .horizontal(true)
                    .touch()
                    .option("s", "Small")
                    .option("m", "Medium")
                    .option("l", "Large")
                    .selected(self.size.clone())
                    .on_change(cx.listener(|this, id: &SharedString, _, cx| {
                        this.size = id.clone();
                        cx.notify();
                    })),
            )
            .child(
                RadioGroup::new("off")
                    .accessibility_label("Disabled")
                    .disabled(true)
                    .option("a", "Disabled")
                    .option("b", "And invalid")
                    .invalid(true)
                    .selected("a"),
            )
    }

    fn toggles(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        div()
            .flex()
            .flex_col()
            .gap(rems(0.75))
            .child(heading("Toggle", &theme))
            .child(
                row()
                    .child(
                        Toggle::new("italic")
                            .icon(IconName::Italic)
                            .accessibility_label("Italic")
                            .pressed(self.italic)
                            .on_click(cx.listener(|this, on: &bool, _, cx| {
                                this.italic = *on;
                                cx.notify();
                            })),
                    )
                    .child(
                        Toggle::new("underline")
                            .icon(IconName::Underline)
                            .label("Underline")
                            .outline()
                            .pressed(self.underline)
                            .on_click(cx.listener(|this, on: &bool, _, cx| {
                                this.underline = *on;
                                cx.notify();
                            })),
                    )
                    .child(Toggle::new("small").label("Small").small())
                    .child(Toggle::new("large").label("Large").large().outline())
                    .child(Toggle::new("off").label("Disabled").disabled(true))
                    .child(
                        Toggle::new("touch")
                            .icon(IconName::Italic)
                            .accessibility_label("Italic, for touch")
                            .touch()
                            .pressed(self.italic)
                            .on_click(cx.listener(|this, on: &bool, _, cx| {
                                this.italic = *on;
                                cx.notify();
                            })),
                    ),
            )
            .child(heading("ToggleGroup: one at most", &theme))
            .child(
                ToggleGroup::new("align")
                    .accessibility_label("Alignment")
                    .icon_item("start", IconName::TextAlignStart, "Left")
                    .icon_item("center", IconName::TextAlignCenter, "Centre")
                    .icon_item("end", IconName::TextAlignEnd, "Right")
                    .icon_item("justify", IconName::TextAlignJustify, "Justified")
                    .disable("justify")
                    .selected(self.align.clone())
                    .on_change(cx.listener(|this, ids: &Vec<SharedString>, _, cx| {
                        this.align = ids.clone();
                        cx.notify();
                    })),
            )
            .child(heading("ToggleGroup: several, outlined", &theme))
            .child(
                ToggleGroup::new("marks")
                    .accessibility_label("Text style")
                    .outline()
                    .multiple(true)
                    .item("bold", "Bold")
                    .item_with_icon("italic", "Italic", IconName::Italic)
                    .item_with_icon("underline", "Underline", IconName::Underline)
                    .selected(self.marks.clone())
                    .on_change(cx.listener(|this, ids: &Vec<SharedString>, _, cx| {
                        this.marks = ids.clone();
                        cx.notify();
                    })),
            )
            .child(heading("In 160 points, the labels are cut short", &theme))
            .child(
                div().w(px(160.)).child(
                    ToggleGroup::new("narrow")
                        .accessibility_label("Text style, narrow")
                        .outline()
                        .multiple(true)
                        .item("bold", "Bold text")
                        .item("italic", "Italic text")
                        .item("underline", "Underlined text")
                        .selected(self.marks.clone())
                        .on_change(cx.listener(|this, ids: &Vec<SharedString>, _, cx| {
                            this.marks = ids.clone();
                            cx.notify();
                        })),
                ),
            )
    }

    fn fields(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let email = self.email.read(cx).value();
        let error = (!email.is_empty() && !email.contains('@'))
            .then_some("Enter an address with an @ in it");
        div()
            .flex()
            .flex_col()
            .gap(rems(0.75))
            .child(heading("Label and Field", &theme))
            .child(
                FieldSet::new("account")
                    .legend("Account")
                    .description("Who you are, and where we reach you.")
                    .child(
                        FieldGroup::new()
                            .child(
                                Field::new("name")
                                    .label("Name")
                                    .for_focus(&self.name.focus_handle(cx))
                                    .child(Input::new(&self.name).accessibility_label("Name")),
                            )
                            .child(
                                Field::new("email")
                                    .label("Email")
                                    .for_focus(&self.email.focus_handle(cx))
                                    .description("We only use it to sign you in. Type something without an @ to see the error.")
                                    .error(error)
                                    .child(Input::new(&self.email).accessibility_label("Email")),
                            )
                            .child(
                                Field::new("news")
                                    .horizontal(true)
                                    .label("Newsletter")
                                    .description("A short email once a month, and nothing else.")
                                    .on_label_click(cx.listener(|this, _, _, cx| {
                                        this.news = !this.news;
                                        cx.notify();
                                    }))
                                    .child(
                                        Switch::new("news-switch")
                                            .accessibility_label("Newsletter")
                                            .checked(self.news)
                                            .on_click(cx.listener(|this, on: &bool, _, cx| {
                                                this.news = *on;
                                                cx.notify();
                                            })),
                                    ),
                            )
                            .child(
                                Field::new("sync")
                                    .horizontal(true)
                                    .control_first(true)
                                    .label("Sync over mobile data")
                                    .on_label_click(cx.listener(|this, _, _, cx| {
                                        this.sync = !this.sync;
                                        cx.notify();
                                    }))
                                    .child(
                                        Switch::new("sync-switch")
                                            .accessibility_label("Sync over mobile data")
                                            .checked(self.sync)
                                            .on_click(cx.listener(|this, on: &bool, _, cx| {
                                                this.sync = *on;
                                                cx.notify();
                                            })),
                                    ),
                            )
                            .child(
                                Field::new("off")
                                    .label("Disabled")
                                    .description("Its label and description are dimmed.")
                                    .disabled(true)
                                    .child(Switch::new("off-switch").disabled(true)),
                            ),
                    ),
            )
            .child(
                div().flex().items_center().gap(rems(0.5)).child(
                    Label::new("plain", "A Label on its own focuses the name field")
                        .for_focus(&self.name.focus_handle(cx)),
                ),
            )
    }

    fn selects(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let zone = Select::new("zone")
            .accessibility_label("Time zone")
            .placeholder("Time zone")
            .w(rems(16.))
            .heading("North America")
            .options([
                ("est", "Eastern Standard Time (EST)"),
                ("pst", "Pacific Standard Time (PST), a long one cut short"),
            ])
            .separator()
            .heading("Europe")
            .options([
                ("gmt", "Greenwich Mean Time (GMT)"),
                ("cet", "Central European Time (CET)"),
            ])
            .selected(self.zone.clone())
            .on_change(cx.listener(|this, id: &SharedString, _, cx| {
                this.zone = Some(id.clone());
                cx.notify();
            }));
        let fruit = |id: &'static str| {
            Select::new(id)
                .accessibility_label("Fruit")
                .placeholder("Pick a fruit")
                .heading("Fruit")
                .options([
                    ("apple", "Apple"),
                    ("banana", "Banana"),
                    ("blueberry", "Blueberry"),
                    ("grapes", "Grapes"),
                    ("pineapple", "Pineapple"),
                ])
                .disable("grapes")
                .separator()
                .heading("Vegetables")
                .options([("leek", "Leek"), ("aubergine", "Aubergine")])
                .selected(self.fruit.clone())
                .on_change(cx.listener(|this, id: &SharedString, _, cx| {
                    this.fruit = Some(id.clone());
                    cx.notify();
                }))
        };
        div()
            .flex()
            .flex_col()
            .gap(rems(0.75))
            .child(heading("Select", &theme))
            .child(
                div()
                    .flex()
                    .gap(rems(0.5))
                    .child(fruit("fruit"))
                    .child(fruit("fruit-small").small()),
            )
            .child(zone)
            .child(fruit("fruit-touch").touch())
            .child(fruit("fruit-invalid").invalid(true))
            .child(fruit("fruit-off").disabled(true))
    }

    fn input_groups(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        div()
            .flex()
            .flex_col()
            .gap(rems(0.75))
            .child(heading("InputGroup", &theme))
            .child(
                InputGroup::new(&self.search)
                    .accessibility_label("Search")
                    .leading_icon(IconName::Search)
                    .trailing_text("12 results"),
            )
            .child(
                InputGroup::new(&self.site)
                    .accessibility_label("Website")
                    .leading_text("https://")
                    .trailing_text(".com"),
            )
            .child(
                InputGroup::new(&self.price)
                    .accessibility_label("Price")
                    .leading_text("$")
                    .trailing_text("USD")
                    .invalid(true),
            )
            .child(
                InputGroup::new(&self.site)
                    .accessibility_label("Website, with a copy button")
                    .leading_icon(IconName::Globe)
                    .trailing(
                        Button::new("copy")
                            .ghost()
                            .xsmall()
                            .icon(IconName::Copy)
                            .accessibility_label("Copy")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.copied += 1;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                InputGroup::new(&self.search)
                    .accessibility_label("Search, for touch")
                    .leading_icon(IconName::Search)
                    .touch(),
            )
            .child(
                div().w(px(140.)).child(
                    InputGroup::new(&self.site)
                        .accessibility_label("Website, narrow")
                        .leading_text("https://")
                        .trailing_text(".com"),
                ),
            )
            .child(
                div()
                    .text_size(theme.text.xs)
                    .text_color(theme.muted_foreground)
                    .child(format!("Copied {} times", self.copied)),
            )
    }

    fn codes(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        div()
            .flex()
            .flex_col()
            .gap(rems(0.75))
            .child(heading("InputOtp: six digits in two groups", &theme))
            .child(
                InputOtp::new(&self.code)
                    .groups([3, 3])
                    .on_complete(cx.listener(|this, code: &SharedString, _, cx| {
                        this.last = Some(code.clone());
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .text_size(theme.text.xs)
                    .text_color(theme.muted_foreground)
                    .child(match &self.last {
                        Some(code) => format!("Completed: {code}"),
                        None => "Type or paste a code".to_string(),
                    }),
            )
            .child(heading(
                "Four letters or digits, for touch, invalid",
                &theme,
            ))
            .child(
                InputOtp::new(&self.pin)
                    .accessibility_label("Pairing code")
                    .touch()
                    .invalid(true),
            )
    }
}

impl Render for Forms {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("forms")
            .size_full()
            .overflow_y_scroll()
            .p(rems(1.5))
            .flex()
            .flex_col()
            .gap(rems(2.))
            .max_w(rems(36.))
            .child(div().id("checkbox").child(self.checkboxes(cx)))
            .child(div().id("radio-group").child(self.radios(cx)))
            .child(div().id("toggle").child(self.toggles(cx)))
            .child(div().id("field").child(self.fields(cx)))
            .child(div().id("select").child(self.selects(cx)))
            .child(div().id("input-group").child(self.input_groups(cx)))
            .child(div().id("input-otp").child(self.codes(cx)))
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
            });
            cx.on_action(|_: &ZoomOut, cx| {
                let t = Theme::global_mut(cx);
                t.font_size = (t.font_size / 1.1).max(px(6.));
            });
            cx.on_action(|_: &Quit, cx| cx.quit());
            let bounds = Bounds::centered(None, size(px(760.), px(900.)), cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            };
            gpui_core::open_window(options, cx, |window, cx| {
                let name = cx.new(|cx| InputState::new(window, cx).placeholder("Ada Lovelace"));
                let email = cx.new(|cx| InputState::new(window, cx).placeholder("ada@example.com"));
                let site = cx.new(|cx| InputState::new(window, cx).placeholder("example"));
                let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
                let price = cx.new(|cx| InputState::new(window, cx).default_value("12.5x"));
                let code = cx.new(|cx| InputOtpState::new(window, cx));
                let pin = cx.new(|cx| InputOtpState::with_length(4, window, cx).digits_only(false));
                cx.new(|cx| {
                    // The field's error follows the email as it's typed.
                    cx.subscribe(&email, |_, _, _: &gpui_core::InputEvent, cx| cx.notify())
                        .detach();
                    Forms {
                        terms: true,
                        marketing: false,
                        items: [true, false, false],
                        plan: "pro".into(),
                        size: "m".into(),
                        italic: true,
                        underline: false,
                        align: vec!["start".into()],
                        marks: vec!["bold".into()],
                        name,
                        email,
                        news: false,
                        sync: true,
                        fruit: None,
                        zone: Some("pst".into()),
                        site,
                        search,
                        price,
                        copied: 0,
                        code,
                        pin,
                        last: None,
                    }
                })
            })
            .expect("the window");
            cx.activate(true);
        });
}
