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

use chrono::Datelike as _;
use gpui::prelude::FluentBuilder as _;
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
    date: Entity<DatePickerState>,
    month: Entity<DatePickerState>,
    level: Entity<SliderState>,
    name: Entity<InputState>,
    note: Entity<InputState>,
    /// A code editor: the approval policy as text.
    code: Entity<InputState>,
    more: bool,
    sort_open: bool,
    tag_open: bool,
    tag: SharedString,
    sort: SharedString,
    /// The segmented control's choice: a theme.
    look: SharedString,
    /// What the list is grouped by: the sort menu's submenu.
    group: SharedString,
    /// Whether the split view's sidebar shows.
    sidebar: bool,
    /// The vault list's open entry.
    vault: Option<SharedString>,
}

/// A vault's entries for the searchable list: an id, a title and a user name, five hundred of
/// them.
fn vault_entries() -> Vec<(SharedString, SharedString, SharedString)> {
    const SITES: [&str; 10] = [
        "GitHub",
        "Google",
        "Chase card",
        "Fastmail",
        "Tailscale",
        "Apple",
        "Amazon",
        "Stripe",
        "Cloudflare",
        "Figma",
    ];
    (0..500)
        .map(|i| {
            let site = SITES[i % SITES.len()];
            (
                format!("v{i}").into(),
                format!("{site} {}", i / SITES.len() + 1).into(),
                format!("trevin+{i}@example.com").into(),
            )
        })
        .collect()
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

fn split_pane(title: &'static str, text: &'static str, theme: &Theme) -> Div {
    div()
        .size_full()
        .p(rems(0.75))
        .flex()
        .flex_col()
        .gap(rems(0.25))
        .child(
            div()
                .text_size(theme.text.sm)
                .font_weight(FontWeight::MEDIUM)
                .child(title),
        )
        .child(
            div()
                .text_size(theme.text.xs)
                .text_color(theme.muted_foreground)
                .child(text),
        )
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
            .child(heading("A popover (Escape or a click outside closes it)", &theme))
            .child(
                div().flex().child(
                    Popover::new("more")
                        .open(self.more)
                        .on_dismiss(cx.listener(|g, _, _, cx| {
                            g.more = false;
                            cx.notify();
                        }))
                        .trigger(
                            Button::new("more-button")
                                .icon(IconName::Ellipsis)
                                .label("More")
                                .outline()
                                .on_click(cx.listener(|g, _, _, cx| {
                                    g.more = !g.more;
                                    cx.notify();
                                })),
                        )
                        .p(rems(0.75))
                        .gap(rems(0.25))
                        .text_size(theme.text.sm)
                        .child("Popovers hang from what opens them,")
                        .child("flip above where there's no room below,")
                        .child("and hand the focus back when they close."),
                ).child(div().w(rems(1.)))
                .child(
                    Popover::new("sort")
                        .open(self.sort_open)
                        .on_dismiss(cx.listener(|g, _, _, cx| {
                            g.sort_open = false;
                            cx.notify();
                        }))
                        .trigger(
                            Button::new("sort-button")
                                .icon(IconName::ArrowUpDown)
                                .label("Sort")
                                .outline()
                                .on_click(cx.listener(|g, _, _, cx| {
                                    g.sort_open = !g.sort_open;
                                    cx.notify();
                                })),
                        )
                        .p(rems(0.25))
                        .child(
                            PopupMenu::new("sort-menu")
                                .items(["name", "date", "size"].map(|k| {
                                    PopupMenuItem::new(k, match k {
                                        "name" => "By name",
                                        "date" => "By date modified",
                                        _ => "By size",
                                    })
                                    .checked(self.sort.as_ref() == k)
                                }))
                                .separator()
                                .item(PopupMenuItem::new("group", "Group by").submenu(
                                    PopupMenu::new("group-menu").items(
                                        [("none", "Nothing"), ("kind", "Kind"), ("owner", "Owner")].map(
                                            |(k, label)| {
                                                PopupMenuItem::new(k, label)
                                                    .checked(self.group.as_ref() == k)
                                            },
                                        ),
                                    ),
                                ))
                                .item(PopupMenuItem::new("copy", "Copy the list").icon(IconName::Copy).shortcut("⌘C"))
                                .item(PopupMenuItem::new("delete", "Delete all").icon(IconName::Trash).disabled(true))
                                .on_select(cx.listener(|g, pick: &MenuSelect, _, cx| {
                                    match pick.id.as_ref() {
                                        "name" | "date" | "size" => g.sort = pick.id.clone(),
                                        "none" | "kind" | "owner" => g.group = pick.id.clone(),
                                        _ => {}
                                    }
                                    g.sort_open = false;
                                    cx.notify();
                                })),
                        ),
                )
                .child(div().w(rems(1.)))
                .child(
                    Popover::new("tag")
                        .open(self.tag_open)
                        .on_dismiss(cx.listener(|g, _, _, cx| {
                            g.tag_open = false;
                            cx.notify();
                        }))
                        .trigger(
                            Button::new("tag-button")
                                .icon(IconName::Tag)
                                .label(self.tag.clone())
                                .outline()
                                .on_click(cx.listener(|g, _, _, cx| {
                                    g.tag_open = !g.tag_open;
                                    cx.notify();
                                })),
                        )
                        .w(rems(14.))
                        .p(rems(0.25))
                        .child(
                            Picker::new("tag-picker")
                                .placeholder("Find a tag")
                                .items(["Work", "Home", "Errands", "Reading", "Travel"].map(|t| {
                                    PopupMenuItem::new(t, t).checked(self.tag.as_ref() == t)
                                }))
                                .on_select(cx.listener(|g, pick: &MenuSelect, _, cx| {
                                    g.tag = pick.id.clone();
                                    g.tag_open = false;
                                    cx.notify();
                                })),
                        ),
                ),
            )
            .child(heading("A slider (arrows and drag)", &theme))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(rems(1.))
                    .child(div().w(rems(16.)).child(Slider::new(&self.level).w_full()))
                    .child(
                        div()
                            .text_size(theme.text.sm)
                            .child(format!("{}", self.level.read(cx).value())),
                    ),
            )
            .child(heading("A segmented control (arrows, Home and End)", &theme))
            .child(
                div().w(rems(20.)).child(
                    SegmentedControl::new("look")
                        .accessibility_label("Theme")
                        .segment_with_icon("system", "System", IconName::Monitor)
                        .segment_with_icon("light", "Light", IconName::Sun)
                        .segment_with_icon("dark", "Dark", IconName::Moon)
                        .selected(self.look.clone())
                        .on_change(cx.listener(|g, id: &SharedString, _, cx| {
                            g.look = id.clone();
                            cx.notify();
                        })),
                ),
            )
            .child(heading("A split view (drag or focus a handle; the button collapses the sidebar)", &theme))
            .child(
                Button::new("sidebar-toggle")
                    .icon(IconName::PanelLeft)
                    .label(if self.sidebar { "Hide sidebar" } else { "Show sidebar" })
                    .outline()
                    .small()
                    .on_click(cx.listener(|g, _, _, cx| {
                        g.sidebar = !g.sidebar;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .flex_none()
                    .w(rems(37.))
                    .h(rems(8.))
                    .rounded(theme.radius_lg())
                    .border_1()
                    .border_color(theme.border)
                    .overflow_hidden()
                    .child(
                        SplitView::new("split")
                            .pane(
                                Pane::new(split_pane("Mailboxes", "Inbox, Sent, Archive", &theme))
                                    .width(px(130.))
                                    .min(px(100.))
                                    .max(px(200.))
                                    .collapsed(!self.sidebar)
                                    .accessibility_label("Mailboxes"),
                            )
                            .pane(
                                Pane::new(split_pane("Messages", "Three unread", &theme))
                                    .width(px(170.))
                                    .min(px(120.))
                                    .max(px(260.))
                                    .accessibility_label("Messages"),
                            )
                            .pane(
                                Pane::new(split_pane("Message", "The one selected, filling the rest", &theme))
                                    .min(px(140.))
                                    .accessibility_label("Message"),
                            )
                            .narrow_below(px(320.))
                            .narrow_shows(2),
                    ),
            )
            .child(heading("A searchable list (type to filter, Up and Down, Enter opens)", &theme))
            .child({
                let entries = std::rc::Rc::new(vault_entries());
                let rows = entries.clone();
                let muted = theme.muted_foreground;
                let open = self
                    .vault
                    .as_ref()
                    .and_then(|id| entries.iter().find(|e| &e.0 == id))
                    .map(|e| format!("Open: {} ({})", e.1, e.2))
                    .unwrap_or_else(|| "Nothing open".into());
                div()
                    .flex()
                    .items_start()
                    .gap(rems(1.))
                    .child(
                        div().w(rems(18.)).h(rems(12.)).child(
                            SearchList::new("vault")
                                .placeholder("Search the vault")
                                .accessibility_label("Vault")
                                .empty_text("Nothing in the vault matches")
                                .row_height(rems(36. / 15.))
                                .items(entries.iter().map(|(id, title, user)| {
                                    (id.clone(), SharedString::from(format!("{title} {user}")))
                                }))
                                .render_row(move |id, _, _, _, _| {
                                    // The id is "v" and the entry's index.
                                    let (title, user) = id
                                        .strip_prefix('v')
                                        .and_then(|i| i.parse::<usize>().ok())
                                        .and_then(|i| rows.get(i))
                                        .map(|e| (e.1.clone(), e.2.clone()))
                                        .unwrap_or_default();
                                    div()
                                        .flex()
                                        .gap(rems(0.5))
                                        .min_w_0()
                                        .child(div().flex_none().child(title))
                                        .child(
                                            div()
                                                .min_w_0()
                                                .overflow_hidden()
                                                .text_ellipsis()
                                                .text_color(muted)
                                                .child(user),
                                        )
                                })
                                .when_some(self.vault.clone(), |l, id| l.selected(id))
                                .on_open(cx.listener(|g, id: &SharedString, _, cx| {
                                    g.vault = Some(id.clone());
                                    cx.notify();
                                })),
                        ),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .text_size(theme.text.sm)
                            .text_color(theme.muted_foreground)
                            .child(open),
                    )
            })
            .child(heading("Fields", &theme))
            .child(
                div()
                    .flex()
                    .gap(rems(1.))
                    .child(div().w(rems(14.)).child(Input::new(&self.name)))
                    .child(div().w(rems(22.)).child(Input::new(&self.note))),
            )
            .child(
                div()
                    .flex_none()
                    .w(rems(37.))
                    .h(rems(9.))
                    .child(Input::new(&self.code).size_full()),
            )
            .child(heading("Markdown", &theme))
            .child(
                div().w(rems(36.)).child(
                    TextView::markdown(
                        "md",
                        "### A heading\n\nA paragraph with **bold**, *italic*, `code` and [a link](https://example.test), long enough to wrap in this column. يرجى إرسال خطة الدراسة\n\n- One\n- [x] Done\n\n1. First\n2. Second\n\n> A quote\n\n```\nlet code = \"block\";\n```\n\n| Left | Centre | Right |\n|---|:-:|--:|\n| a | b | c |",
                    )
                    .style(TextViewStyle {
                        inline_code: gpui_core::text::InlineCodeStyle {
                            size: Some(0.875),
                            padding: Some(px(4.)),
                            radius: Some(px(4.)),
                            ..Default::default()
                        },
                        heading_margin_bottom: Some(px(4.)),
                        colors: gpui_core::text::TextViewColors {
                            table_header: Some(theme.muted),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
                    .selectable(true),
                ),
            )
            .child(heading("A date, and a month in place (Monday first, dots on some days)", &theme))
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(rems(1.))
                    .child(div().w(rems(14.)).child(DatePicker::new(&self.date).placeholder("Pick a date")))
                    .child(MonthGrid::new(&self.month).dots(|d| d.day() % 5 == 0)),
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
            let shot = args.shot.is_some();
            let (window, _) = gpui_core::open_window(options, cx, |window, cx| {
                let name = cx.new(|cx| InputState::new(window, cx).placeholder("Your name"));
                let note = cx.new(|cx| {
                    InputState::textarea(window, cx)
                        .auto_grow(2, 6)
                        .default_value("A textarea grows with its text, from two rows to six, and wraps a long line rather than running out of its box.")
                });
                let code = cx.new(|cx| {
                    InputState::code(window, cx).default_value(
                        "rules:\n  - name: Read anything in the shared folder\n    match: { tool: read, path: \"/shared/**\" }\n    action: allow\n  - name: Ask before sending mail, whoever it's to and whatever it says\n    match: { tool: send_mail }\n    action: ask\n",
                    )
                });
                let date = cx.new(|cx| DatePickerState::new(window, cx));
                let month = cx.new(|cx| DatePickerState::new(window, cx).week_start(chrono::Weekday::Mon));
                cx.new(|cx| Gallery {
                    pressed: "nothing yet".into(),
                    on: true,
                    date,
                    month,
                    level: cx.new(|_| {
                        SliderState::new()
                            .min(0.)
                            .max(100.)
                            .step(5.)
                            .default_value(40.)
                    }),
                    view: 1,
                    name,
                    note,
                    code,
                    more: false,
                    sort_open: shot,
                    tag_open: false,
                    tag: "Work".into(),
                    sort: "name".into(),
                    look: "system".into(),
                    group: "none".into(),
                    sidebar: true,
                    vault: Some("v2".into()),
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
