//! A sortable list, and a row of chips sorted along it with a line where a chip would land.
//! Drag with the mouse, trackpad or touch; or Tab to a list, use the arrows to reach an item,
//! Space to pick it up, the arrows to move it and Space to drop it.
//!
//! ```sh
//! cargo run -p gpui-dnd --example sortable
//! cargo run -p gpui-dnd --features shots --example sortable -- --shot out.png --scene drag
//! ```

#[path = "shared/mod.rs"]
mod shared;

use gpui::*;
use gpui_core::ActiveTheme as _;
use gpui_dnd::*;
use shared::{Step, hold};

const PAD: f32 = 32.;
const ROW: f32 = 44.;
const GAP: f32 = 7.5;
const TOP: f32 = PAD + 36.;

struct Lists {
    tasks: Vec<&'static str>,
    chips: Vec<&'static str>,
}

fn reorder<T>(list: &mut Vec<T>, m: &SortableMove<impl Clone>) {
    let item = list.remove(m.from.1);
    list.insert(m.to.1, item);
}

impl Render for Lists {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let (a, b) = (cx.entity().downgrade(), cx.entity().downgrade());
        DndProvider::new(
            div()
                .size_full()
                .bg(theme.background)
                .text_color(theme.foreground)
                .p(px(PAD))
                .flex()
                .gap(px(48.))
                .child(
                    div()
                        .w(px(320.))
                        .flex()
                        .flex_col()
                        .gap(px(12.))
                        .child(div().h(px(24.)).text_lg().child("This week"))
                        .child(
                            Sortable::new("tasks", self.tasks.clone())
                                .label("This week")
                                .item_label(|t: &&str| (*t).into())
                                .item(|t, item, _, cx| {
                                    let theme = cx.theme();
                                    div()
                                        .h(px(ROW))
                                        .px(px(14.))
                                        .flex()
                                        .items_center()
                                        .gap(px(10.))
                                        .rounded(theme.radius_md())
                                        .border_1()
                                        .border_color(if item.preview {
                                            theme.ring
                                        } else {
                                            theme.border
                                        })
                                        .bg(theme.card)
                                        .text_sm()
                                        .child(
                                            div()
                                                .text_color(theme.muted_foreground)
                                                .child(format!("{}", item.index + 1)),
                                        )
                                        .child(*t)
                                        .into_any_element()
                                })
                                .on_move(move |m, _, cx| {
                                    a.update(cx, |l, cx| {
                                        reorder(&mut l.tasks, m);
                                        cx.notify();
                                    })
                                    .ok();
                                }),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(12.))
                        .child(div().h(px(24.)).text_lg().child("Tags"))
                        .child(
                            Sortable::new("chips", self.chips.clone())
                                .axis(Axis::Horizontal)
                                .placeholder(Placeholder::Line)
                                .label("Tags")
                                .item_label(|t: &&str| (*t).into())
                                .item(|t, _, _, cx| {
                                    let theme = cx.theme();
                                    div()
                                        .h(px(32.))
                                        .px(px(12.))
                                        .flex()
                                        .items_center()
                                        .rounded_full()
                                        .bg(theme.secondary)
                                        .text_color(theme.secondary_foreground)
                                        .text_sm()
                                        .child(*t)
                                        .into_any_element()
                                })
                                .on_move(move |m, _, cx| {
                                    b.update(cx, |l, cx| {
                                        reorder(&mut l.chips, m);
                                        cx.notify();
                                    })
                                    .ok();
                                }),
                        ),
                ),
        )
    }
}

fn row(i: usize) -> Point<Pixels> {
    point(px(PAD + 160.), px(TOP + i as f32 * (ROW + GAP) + ROW / 2.))
}

fn script(scene: &str) -> Vec<Step> {
    match scene {
        // "Plan the offsite" held between the fourth and fifth tasks.
        "drag" => hold(row(1), row(3) + point(px(14.), px(14.))),
        _ => Vec::new(),
    }
}

fn main() {
    shared::run(
        "Sortable",
        size(px(800.), px(480.)),
        |_, cx| {
            cx.new(|_| Lists {
                tasks: vec![
                    "Answer the reviews",
                    "Plan the offsite",
                    "Update the roadmap",
                    "Write the changelog",
                    "Call the printer",
                    "Clean up the backlog",
                ],
                chips: vec!["Design", "Writing", "Research", "Legal"],
            })
        },
        script,
    )
}
