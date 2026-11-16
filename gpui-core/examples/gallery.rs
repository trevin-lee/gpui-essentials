//! Every gpui-core component in a window.
//!
//! ```sh
//! cargo run --example gallery [-- --light]
//! cargo run --features shots --example gallery -- --shot out.png [--light] [--zoom 200]
//! ```
//!
//! Tab moves through everything that takes the focus; ⌘+ and ⌘− zoom. `--shot` takes a
//! picture of the window without showing it.

use std::path::PathBuf;

use gpui::*;
use gpui_core::*;

actions!(gallery, [ZoomIn, ZoomOut, Quit]);

struct Args {
    shot: Option<PathBuf>,
    light: bool,
    zoom: f32,
}

fn args() -> Args {
    let mut out = Args {
        shot: None,
        light: false,
        zoom: 1.,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--shot" => out.shot = it.next().map(PathBuf::from),
            "--light" => out.light = true,
            "--zoom" => {
                out.zoom = it
                    .next()
                    .and_then(|z| z.parse::<f32>().ok())
                    .unwrap_or(100.)
                    / 100.
            }
            _ => {}
        }
    }
    out
}

struct Gallery {
    pressed: SharedString,
    view: usize,
    on: bool,
}

const VARIANTS: [(ButtonVariant, &str); 6] = [
    (ButtonVariant::Primary, "Primary"),
    (ButtonVariant::Outline, "Outline"),
    (ButtonVariant::Secondary, "Secondary"),
    (ButtonVariant::Ghost, "Ghost"),
    (ButtonVariant::Destructive, "Delete"),
    (ButtonVariant::Link, "Link"),
];

const SIZES: [(ControlSize, &str); 4] = [
    (ControlSize::XSmall, "XSmall"),
    (ControlSize::Small, "Small"),
    (ControlSize::Medium, "Medium"),
    (ControlSize::Large, "Large"),
];

fn heading(text: &'static str, theme: &Theme) -> Div {
    div()
        .text_size(theme.text.sm)
        .text_color(theme.muted_foreground)
        .child(text)
}

fn pressed(
    this: &WeakEntity<Gallery>,
    what: &str,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let what: SharedString = what.to_string().into();
    let this = this.clone();
    move |_, _, cx| {
        let what = what.clone();
        this.update(cx, |g, cx| {
            g.pressed = what;
            cx.notify();
        })
        .ok();
    }
}

impl Render for Gallery {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let me = cx.entity().downgrade();
        let theme = cx.theme().clone();
        let rows = SIZES.iter().map(|(size, size_name)| {
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(rems(0.5))
                .child(
                    div()
                        .w(rems(4.))
                        .text_size(theme.text.xs)
                        .text_color(theme.muted_foreground)
                        .child(*size_name),
                )
                .children(VARIANTS.iter().enumerate().map(|(i, (variant, name))| {
                    Button::new(SharedString::from(format!("{size_name}-{i}")))
                        .label(*name)
                        .with_variant(*variant)
                        .with_size(*size)
                        .on_click(pressed(&me, &format!("{name} {size_name}")))
                }))
                .child(
                    Button::new(SharedString::from(format!("{size_name}-icon")))
                        .icon(IconName::Plus)
                        .accessibility_label("Add")
                        .outline()
                        .with_size(*size)
                        .on_click(pressed(&me, "Add")),
                )
        });
        let views = ["Day", "Week", "Month"];
        let this = me.clone();
        div()
            .size_full()
            .p(rems(1.5))
            .flex()
            .flex_col()
            .gap(rems(1.))
            .child(heading("Buttons", &theme))
            .children(rows)
            .child(heading("With icons, selected, disabled, touch", &theme))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(rems(0.5))
                    .child(
                        Button::new("today")
                            .label("Today")
                            .outline()
                            .small()
                            .on_click(pressed(&me, "Today")),
                    )
                    .child(
                        Button::new("prev")
                            .icon(IconName::ChevronLeft)
                            .accessibility_label("Previous")
                            .ghost()
                            .small(),
                    )
                    .child(
                        Button::new("next")
                            .icon(IconName::ChevronRight)
                            .accessibility_label("Next")
                            .ghost()
                            .small(),
                    )
                    .child(
                        ButtonGroup::new("views")
                            .outline()
                            .small()
                            .children(views.iter().enumerate().map(|(i, v)| {
                                Button::new(("view", i)).label(*v).selected(i == self.view)
                            }))
                            .on_click(move |picked: &Vec<usize>, _, cx| {
                                let i = picked[0];
                                this.update(cx, |g, cx| {
                                    g.view = i;
                                    cx.notify();
                                })
                                .ok();
                            }),
                    )
                    .child(
                        Button::new("star")
                            .icon(IconName::Star)
                            .label("Star")
                            .secondary(),
                    )
                    .child(Button::new("off").label("Disabled").disabled(true))
                    .child(
                        Button::new("touch")
                            .icon(IconName::Plus)
                            .accessibility_label("Add")
                            .ghost()
                            .touch(),
                    ),
            )
            .child(heading("A narrow box", &theme))
            .child(
                div().w(rems(10.)).flex().gap(rems(0.5)).child(
                    Button::new("long")
                        .icon(IconName::Mail)
                        .label("Send the whole quarterly report to everyone")
                        .outline(),
                ),
            )
            .child(heading("Switches, and a tooltip on the first", &theme))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(rems(1.))
                    .child(
                        div()
                            .id("switch-tip")
                            .tooltip(|window, cx| {
                                Tooltip::new("Turns the agent's tools on and off").build(window, cx)
                            })
                            .child(
                                Switch::new("tools")
                                    .checked(self.on)
                                    .accessibility_label("Tools")
                                    .on_click({
                                        let me = me.clone();
                                        move |on, _, cx| {
                                            let on = *on;
                                            me.update(cx, |g, cx| {
                                                g.on = on;
                                                cx.notify();
                                            })
                                            .ok();
                                        }
                                    }),
                            ),
                    )
                    .child(Switch::new("off").checked(false))
                    .child(Switch::new("small").checked(true).small())
                    .child(Switch::new("disabled").checked(true).disabled(true)),
            )
            .child(heading("Press, with a host's own look", &theme))
            .child(
                Press::new("press")
                    .flex()
                    .items_center()
                    .gap(rems(0.5))
                    .px(rems(0.75))
                    .py(rems(0.5))
                    .rounded(theme.radius_lg())
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.card)
                    .hover(|s| s.bg(theme.accent))
                    .child(Icon::new(IconName::Bell))
                    .child("Notifications")
                    .on_click(pressed(&me, "Notifications")),
            )
            .child(
                div()
                    .text_size(theme.text.sm)
                    .text_color(theme.muted_foreground)
                    .child(format!("Last pressed: {}", self.pressed)),
            )
    }
}

fn main() {
    let args = args();
    gpui_platform::application()
        .with_assets(Icons)
        .run(move |cx| {
            gpui_core::init(cx);
            let mut theme = if args.light {
                Theme::light()
            } else {
                Theme::dark()
            };
            theme.font_size = px(15. * args.zoom);
            Theme::set(theme, cx);
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
            let bounds = Bounds::centered(
                None,
                size(px(900. * args.zoom.max(1.)), px(640. * args.zoom.max(1.))),
                cx,
            );
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                focus: args.shot.is_none(),
                show: args.shot.is_none(),
                ..Default::default()
            };
            let (window, _) = gpui_core::open_window(options, cx, |_, cx| {
                cx.new(|_| Gallery {
                    pressed: "nothing yet".into(),
                    on: true,
                    view: 1,
                })
            })
            .expect("the window");
            if args.shot.is_none() {
                cx.activate(true);
            }
            #[cfg(feature = "shots")]
            if let Some(path) = args.shot.clone() {
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
