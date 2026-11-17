//! gpui-core's charts in a window: a pie and a donut (with its total in the hole), line charts smooth and straight (with dots and a gap), area
//! charts stacked with a gradient and flat with negative values and dollars, and bar charts
//! grouped and stacked, with negative values, huge values, crowded labels, a single point and
//! no data at all.
//!
//! ```sh
//! cargo run --example charts [-- --light]
//! cargo run --features shots --example charts -- --shot out.png [--light]
//! ```
//!
//! Point at a chart (or tap it) for its tooltip; Tab to one and use Left, Right and Escape.

use gpui::*;
use gpui_core::chart::compact_number;
use gpui_core::*;

actions!(charts, [Quit]);

struct Charts;

const MONTHS: [&str; 6] = ["January", "February", "March", "April", "May", "June"];
const DESKTOP: [f64; 6] = [186., 305., 237., 73., 209., 214.];
const MOBILE: [f64; 6] = [80., 200., 120., 190., 130., 140.];

fn card(title: &'static str, description: &'static str, chart: impl IntoElement) -> Div {
    // Kept at its height in the scrolling column.
    div().flex_none().child(
        Card::new()
            .title(title)
            .description(description)
            .child(chart),
    )
}

impl Render for Charts {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let short = MONTHS.map(|m| &m[..3]);
        div()
            .id("charts")
            .size_full()
            .overflow_y_scroll()
            .p(rems(1.5))
            .flex()
            .flex_col()
            .gap(rems(1.))
            .text_color(theme.foreground)
            .text_size(theme.text.base)
            .child(
                div()
                    .flex()
                    .gap(rems(1.))
                    .child(
                        div().flex_1().min_w_0().child(card(
                            "Pie chart",
                            "Visitors by browser",
                            PieChart::new("pie")
                                .slice("chrome", "Chrome", 275.)
                                .slice("safari", "Safari", 200.)
                                .slice("firefox", "Firefox", 187.)
                                .slice("edge", "Edge", 173.)
                                .slice("other", "Other", 90.)
                                .legend(true)
                                .h(rems(16.)),
                        )),
                    )
                    .child(
                        div().flex_1().min_w_0().child(card(
                            "Pie chart - donut with text",
                            "A zero and a negative slice are left out",
                            PieChart::new("donut")
                                .slice("chrome", "Chrome", 275.)
                                .slice("safari", "Safari", 200.)
                                .slice("none", "None", 0.)
                                .slice("firefox", "Firefox", 187.)
                                .slice("refunds", "Refunds", -20.)
                                .donut(true)
                                .center_label("662", "Visitors")
                                .format_value(|v| format!("{v} visits").into())
                                .h(rems(16.)),
                        )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(rems(1.))
                    .child(
                        div().flex_1().min_w_0().child(card(
                            "Line chart",
                            "Smooth (monotone) lines",
                            LineChart::new("line")
                                .categories(short)
                                .series("desktop", "Desktop", DESKTOP)
                                .series("mobile", "Mobile", MOBILE)
                                .legend(true)
                                .h(rems(14.)),
                        )),
                    )
                    .child(
                        div().flex_1().min_w_0().child(card(
                            "Line chart - linear",
                            "Straight, with dots and a gap in April",
                            LineChart::new("linear")
                                .categories(short)
                                .series(
                                    "desktop",
                                    "Desktop",
                                    [186., 305., 237., f64::NAN, 209., 214.],
                                )
                                .curve(Curve::Linear)
                                .dots(true)
                                .h(rems(14.)),
                        )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap(rems(1.))
                    .child(
                        div().flex_1().min_w_0().child(card(
                            "Area chart - stacked",
                            "With a gradient",
                            AreaChart::new("area-stacked")
                                .categories(short)
                                .series("mobile", "Mobile", MOBILE)
                                .series("desktop", "Desktop", DESKTOP)
                                .stacked(true)
                                .fill(AreaFill::Gradient)
                                .legend(true)
                                .h(rems(14.)),
                        )),
                    )
                    .child(
                        div().flex_1().min_w_0().child(card(
                            "Area chart - profit",
                            "Flat fill, below zero, in dollars",
                            AreaChart::new("area-profit")
                                .categories(short)
                                .series("profit", "Profit", [1200., 800., -400., 300., 1500., 900.])
                                .series_color("profit", theme.chart_2)
                                .format_value(|v| {
                                    let sign = if v < 0. { "-" } else { "" };
                                    format!("{sign}${}", compact_number(v.abs())).into()
                                })
                                .h(rems(14.)),
                        )),
                    ),
            )
            .child(card(
                "Bar chart - multiple",
                "January to June",
                BarChart::new("multiple")
                    .categories(short)
                    .series("desktop", "Desktop", DESKTOP)
                    .series("mobile", "Mobile", MOBILE)
                    .legend(true),
            ))
            .child(
                div()
                    .flex()
                    .gap(rems(1.))
                    .child(
                        div().w(rems(18.)).child(card(
                            "Crowded labels",
                            "Cut short or thinned out",
                            BarChart::new("crowded")
                                .categories((1..=30).map(|d| format!("November {d}")))
                                .series("tasks", "Tasks", (1..=30).map(|d| ((d * 37) % 23) as f64))
                                .y_axis(false)
                                .h(rems(10.)),
                        )),
                    )
                    .child(
                        div().flex_1().min_w_0().child(card(
                            "One point",
                            "A single category",
                            BarChart::new("single")
                                .categories(["Today"])
                                .series("done", "Done", [42.])
                                .h(rems(10.)),
                        )),
                    )
                    .child(div().flex_1().min_w_0().child(card(
                        "Nothing yet",
                        "No data at all",
                        BarChart::new("empty").h(rems(10.)),
                    ))),
            )
            .child(card(
                "Bar chart - stacked",
                "Desktop and mobile, one bar a month",
                BarChart::new("stacked")
                    .categories(short)
                    .series("desktop", "Desktop", DESKTOP)
                    .series("mobile", "Mobile", MOBILE)
                    .stacked(true)
                    .legend(true),
            ))
            .child(card(
                "Bar chart - negative",
                "Net visitors: losses hang below the zero line",
                BarChart::new("negative")
                    .categories(short)
                    .series("net", "Net", [186., 205., -207., 173., -209., 214.])
                    .h(rems(12.)),
            ))
            .child(card(
                "Bar chart - huge numbers",
                "Short labels on the axis, the full value in the tooltip",
                BarChart::new("huge")
                    .categories(["2021", "2022", "2023", "2024"])
                    .series(
                        "views",
                        "Views",
                        [1_234_567., 3_400_000., 12_500_000., 8_900_000.],
                    )
                    .series_color("views", theme.chart_2)
                    .h(rems(12.)),
            ))
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let light = args.iter().any(|a| a == "--light");
    let shot = args
        .iter()
        .position(|a| a == "--shot")
        .and_then(|i| args.get(i + 1))
        .map(std::path::PathBuf::from);
    gpui_platform::application()
        .with_assets(Icons)
        .run(move |cx| {
            gpui_core::init(cx);
            Theme::set(if light { Theme::light() } else { Theme::dark() }, cx);
            cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
            cx.on_action(|_: &Quit, cx| cx.quit());
            let bounds = Bounds::centered(None, size(px(760.), px(1240.)), cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                focus: shot.is_none(),
                show: shot.is_none(),
                ..Default::default()
            };
            let (window, _) = gpui_core::open_window(options, cx, |_, cx| cx.new(|_| Charts))
                .expect("the window");
            if shot.is_none() {
                cx.activate(true);
            }
            #[cfg(feature = "shots")]
            if let Some(path) = shot.clone() {
                cx.spawn(async move |cx| {
                    for _ in 0..3 {
                        let _ = cx.update_window(window, |_, window, cx| {
                            window.refresh();
                            let _ = window.draw(cx);
                        });
                        cx.background_executor()
                            .timer(std::time::Duration::from_millis(80))
                            .await;
                    }
                    let _ = cx.update_window(window, |_, window, cx| {
                        window.refresh();
                        let _ = window.draw(cx);
                        match window.render_to_image() {
                            Ok(img) => {
                                if let Err(e) = img.save(&path) {
                                    eprintln!("the picture wasn't saved: {e}");
                                }
                            }
                            Err(e) => eprintln!("the window didn't render to a picture: {e}"),
                        }
                        cx.quit();
                    });
                })
                .detach();
            }
            let _ = window;
        });
}
