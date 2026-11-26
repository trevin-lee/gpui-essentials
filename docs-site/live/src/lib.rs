//! The docs site's live code: gpui-core's components running in the browser on GPUI's web
//! platform (WebGPU, or WebGL2 where there's none). Everything is drawn by GPUI into one
//! canvas, as on the Mac.
//!
//! - The GitHub Pages site's live page calls [`start`] with whether to draw in the dark theme,
//!   for a small app of a few components.
//! - The Next.js site (docs-site/web) calls [`stage::stage`], then [`stage::mount`] for each
//!   demo in the registry it shows live: one at a time, since GPUI's web platform draws one
//!   window, in one canvas, per page.

mod stage;

use std::borrow::Cow;

use gpui::*;
use gpui_core::*;
use wasm_bindgen::prelude::*;

/// Inter (SIL Open Font License, fonts/OFL.txt), cut down to Latin: a browser gives GPUI no
/// system fonts, so the page brings its own, in the weights the components use.
pub(crate) const FONTS: [&[u8]; 4] = [
    include_bytes!("../fonts/Inter-Regular.ttf"),
    include_bytes!("../fonts/Inter-Medium.ttf"),
    include_bytes!("../fonts/Inter-SemiBold.ttf"),
    include_bytes!("../fonts/Inter-Bold.ttf"),
];

const MONTHS: [&str; 6] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun"];
const DESKTOP: [f64; 6] = [186., 305., 237., 73., 209., 214.];
const MOBILE: [f64; 6] = [80., 200., 120., 190., 130., 140.];

struct Playground {
    name: Entity<InputState>,
    email: Entity<InputState>,
    plan: Option<SharedString>,
    seats: Entity<SliderState>,
    terms: bool,
    news: bool,
    chart: SharedString,
    date: Entity<DatePickerState>,
}

impl Playground {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let seats = cx.new(|_| {
            SliderState::new()
                .min(1.)
                .max(50.)
                .step(1.)
                .default_value(5.)
        });
        cx.subscribe(&seats, |_, _, _: &SliderEvent, cx| cx.notify())
            .detach();
        Self {
            name: cx.new(|cx| InputState::new(window, cx).placeholder("Sam Rivera")),
            email: cx.new(|cx| InputState::new(window, cx).placeholder("sam@example.com")),
            plan: Some("team".into()),
            seats,
            terms: false,
            news: true,
            chart: "bar".into(),
            date: cx.new(|cx| DatePickerState::new(window, cx)),
        }
    }

    fn account(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let seats = self.seats.read(cx).value();
        Card::new()
            .w_full()
            .title("Create an account")
            .description("Type, pick, slide and press: every control is gpui-core.")
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
                            .child(Input::new(&self.email).accessibility_label("Email")),
                    )
                    .child(
                        Field::new("plan").label("Plan").child(
                            Select::new("plan-select")
                                .accessibility_label("Plan")
                                .w_full()
                                .options([
                                    ("free", "Free"),
                                    ("team", "Team"),
                                    ("business", "Business"),
                                ])
                                .selected(self.plan.clone())
                                .on_change(cx.listener(|this, id: &SharedString, _, cx| {
                                    this.plan = Some(id.clone());
                                    cx.notify();
                                })),
                        ),
                    )
                    .child(
                        Field::new("seats")
                            .label(format!("Seats: {seats}"))
                            .child(Slider::new(&self.seats).w_full()),
                    )
                    .child(
                        Field::new("news")
                            .horizontal(true)
                            .label("Newsletter")
                            .description("A short email once a month.")
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
                        Checkbox::new("terms")
                            .label("I accept the terms")
                            .checked(self.terms)
                            .on_click(cx.listener(|this, on: &bool, _, cx| {
                                this.terms = *on;
                                cx.notify();
                            })),
                    ),
            )
            .footer(
                Button::new("create")
                    .label("Create account")
                    .disabled(!self.terms)
                    .on_click(cx.listener(|this, _, window, cx| {
                        let name = this.name.read(cx).value();
                        let name = if name.is_empty() { "Sam".into() } else { name };
                        Toast::success(format!("Welcome, {name}.")).show(window, cx);
                    })),
            )
    }

    fn visitors(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let chart: AnyElement = if self.chart == "bar" {
            BarChart::new("bars")
                .categories(MONTHS)
                .series("desktop", "Desktop", DESKTOP)
                .series("mobile", "Mobile", MOBILE)
                .legend(true)
                .h(rems(14.))
                .into_any_element()
        } else {
            AreaChart::new("areas")
                .categories(MONTHS)
                .series("mobile", "Mobile", MOBILE)
                .series("desktop", "Desktop", DESKTOP)
                .stacked(true)
                .fill(AreaFill::Gradient)
                .legend(true)
                .h(rems(14.))
                .into_any_element()
        };
        Card::new()
            .w_full()
            .title("Visitors")
            .description("Point at the chart for the values, or Tab to it and use the arrows.")
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(rems(1.))
                    .child(
                        div().w(rems(10.)).child(
                            SegmentedControl::new("chart-kind")
                                // The box's width, split between the options.
                                .w_full()
                                .accessibility_label("Chart")
                                .segment("bar", "Bars")
                                .segment("area", "Areas")
                                .selected(self.chart.clone())
                                .on_change(cx.listener(|this, id: &SharedString, _, cx| {
                                    this.chart = id.clone();
                                    cx.notify();
                                })),
                        ),
                    )
                    .child(chart),
            )
    }

    fn day(&self) -> impl IntoElement {
        Card::new()
            .title("Pick a day")
            .description("The arrows move the day; Enter picks it.")
            .child(MonthGrid::new(&self.date))
    }
}

impl Render for Playground {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        ScrollArea::new("page")
            .accessibility_label("Live components")
            .tab_stop(false)
            .size_full()
            .bg(theme.background)
            .text_color(theme.foreground)
            .child(
                div()
                    .p(rems(1.5))
                    .flex()
                    .flex_wrap()
                    .justify_center()
                    .items_start()
                    .gap(rems(1.5))
                    // Side by side where there's room, one above the other where there isn't.
                    .child(
                        div()
                            .flex_1()
                            .min_w(rems(17.))
                            .max_w(rems(24.))
                            .child(self.account(cx)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(rems(17.))
                            .max_w(rems(28.))
                            .flex()
                            .flex_col()
                            .gap(rems(1.5))
                            .child(self.visitors(cx))
                            .child(self.day()),
                    ),
            )
    }
}

/// Sets the app up for a page: its fonts, gpui-core, and the light or the dark theme.
pub(crate) fn setup(cx: &mut App, dark: bool) {
    cx.text_system()
        .add_fonts(FONTS.iter().map(|font| Cow::Borrowed(*font)).collect())
        .expect("the bundled fonts load");
    gpui_core::init(cx);
    Theme::set(theme(dark), cx);
}

/// The light or the dark theme, in the page's font.
pub(crate) fn theme(dark: bool) -> Theme {
    let mut theme = if dark { Theme::dark() } else { Theme::light() };
    theme.font_family = "Inter".into();
    // Menlo, the default, isn't in a browser, and the page brings no monospaced font.
    theme.mono_font_family = "Inter".into();
    theme
}

/// Starts the app in the page's canvas, in the dark theme or the light one.
#[wasm_bindgen]
pub fn start(dark: bool) {
    gpui_platform::web_init();
    // Every Lucide icon, so any demo's icons draw: about 85 KB more, gzipped.
    gpui_platform::single_threaded_web()
        .with_assets(Icons)
        .run(move |cx| {
            setup(cx, dark);
            open_window(WindowOptions::default(), cx, |window, cx| {
                cx.new(|cx| Playground::new(window, cx))
            })
            .expect("the page's window opens");
        });
}
