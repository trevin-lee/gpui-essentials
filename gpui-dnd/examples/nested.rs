//! Nested targets: folders inside a drive. The folder under the pointer takes a file (shallow
//! over), the drive shows it's over it too (deep over), and the "Archive" folder passes drops
//! of drafts out to the drive. The log at the bottom says who handled each drop.
//!
//! ```sh
//! cargo run -p gpui-dnd --example nested
//! cargo run -p gpui-dnd --features shots --example nested -- --shot out.png --scene hover
//! ```

#[path = "shared/mod.rs"]
mod shared;

use gpui::*;
use gpui_core::ActiveTheme as _;
use gpui_dnd::*;
use shared::{Step, hold};

#[derive(Clone)]
struct File {
    name: &'static str,
    draft: bool,
}

struct Drive {
    log: Vec<String>,
}

fn file(f: File, cx: &App) -> Dnd {
    let theme = cx.theme();
    dnd(f.name)
        .label(f.name)
        .h(px(36.))
        .px(px(12.))
        .flex()
        .items_center()
        .rounded(theme.radius_md())
        .border_1()
        .border_color(theme.border)
        .bg(theme.card)
        .text_sm()
        .dragging_style(|s| s.opacity(0.4))
        .child(f.name)
        .source(DragSource::item(f))
}

impl Render for Drive {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let this = cx.entity().downgrade();
        let folder = |name: &'static str, passes_drafts: bool| {
            let this = this.clone();
            dnd(name)
                .label(name)
                .w(px(200.))
                .h(px(120.))
                .p(px(12.))
                .rounded(theme.radius_lg())
                .border_1()
                .border_color(theme.border)
                .bg(theme.background)
                .text_sm()
                .over_style(|s| s.bg(gpui::hsla(0.6, 0.6, 0.5, 0.12)))
                .child(name)
                .target(
                    DropTarget::new()
                        .accept::<File>()
                        .on_drop(move |item, _, _, cx| {
                            let f = item.get::<File>().unwrap();
                            if passes_drafts && f.draft {
                                return DropOutcome::Pass;
                            }
                            let line = format!("{} took {}", name, f.name);
                            this.update(cx, |d, cx| {
                                d.log.push(line);
                                cx.notify();
                            })
                            .ok();
                            DropOutcome::Handled(None)
                        }),
                )
        };
        let log_to = this.clone();
        // The drive shows a file is somewhere inside it (deep over), from the monitor.
        let inside = DragMonitor::of(cx).is_over(&"drive".into(), Over::Deep);
        DndProvider::new(
            div()
                .size_full()
                .bg(theme.background)
                .text_color(theme.foreground)
                .p(px(32.))
                .flex()
                .gap(px(32.))
                .child(
                    div()
                        .w(px(200.))
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .child(div().text_lg().child("Files"))
                        .child(file(
                            File {
                                name: "report.pdf",
                                draft: false,
                            },
                            cx,
                        ))
                        .child(file(
                            File {
                                name: "notes-draft.md",
                                draft: true,
                            },
                            cx,
                        ))
                        .child(file(
                            File {
                                name: "photo.png",
                                draft: false,
                            },
                            cx,
                        )),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(12.))
                        .child(
                            dnd("drive")
                                .label("Drive")
                                .w(px(480.))
                                .h(px(260.))
                                .p(px(16.))
                                .rounded(theme.radius_xl())
                                .bg(theme.muted)
                                .border_2()
                                .border_color(if inside {
                                    gpui::hsla(0.6, 0.7, 0.6, 0.6)
                                } else {
                                    gpui::transparent_black()
                                })
                                .over_style(|s| s.bg(gpui::hsla(0.6, 0.6, 0.5, 0.18)))
                                .target(DropTarget::new().accept::<File>().on_drop(
                                    move |item, m, _, cx| {
                                        let f = item.get::<File>().unwrap();
                                        let line = if m.did_drop() {
                                            format!("Drive saw {} taken inside", f.name)
                                        } else {
                                            format!("Drive took {} (passed out)", f.name)
                                        };
                                        log_to
                                            .update(cx, |d, cx| {
                                                d.log.push(line);
                                                cx.notify();
                                            })
                                            .ok();
                                        DropOutcome::Handled(None)
                                    },
                                ))
                                .child(div().text_sm().mb(px(12.)).child("Drive"))
                                .child(
                                    div()
                                        .flex()
                                        .gap(px(16.))
                                        .child(folder("Projects", false))
                                        .child(folder("Archive", true)),
                                ),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .children(self.log.iter().rev().take(4).cloned()),
                        ),
                ),
        )
    }
}

fn script(scene: &str) -> Vec<Step> {
    let report = point(px(132.), px(84.));
    let projects = point(px(380.), px(160.));
    match scene {
        // report.pdf held over Projects, inside the drive.
        "hover" => hold(report, projects),
        _ => Vec::new(),
    }
}

fn main() {
    shared::run(
        "Nested targets",
        size(px(800.), px(420.)),
        |_, cx| cx.new(|_| Drive { log: Vec::new() }),
        script,
    )
}
