//! A custom drag layer (react-dnd's `useDragLayer`): photos picked up as a selection draw as a
//! stack with a count, following the pointer and sliding back to their place on Escape.
//!
//! ```sh
//! cargo run -p gpui-dnd --example custom_preview
//! cargo run -p gpui-dnd --features shots --example custom_preview -- --shot out.png --scene drag
//! ```

#[path = "shared/mod.rs"]
mod shared;

use gpui::*;
use gpui_core::ActiveTheme as _;
use gpui_dnd::*;
use shared::{Step, hold};

#[derive(Clone)]
struct Photo(u32);

struct Gallery {
    album: Vec<u32>,
}

fn swatch(n: u32) -> Hsla {
    hsla((n as f32 * 0.13) % 1., 0.45, 0.55, 1.)
}

impl Render for Gallery {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let this = cx.entity().downgrade();
        let photos = (1..=6u32).map(|n| {
            dnd(("photo", n as usize))
                .label(format!("Photo {n}"))
                .size(px(96.))
                .rounded(theme.radius_lg())
                .bg(swatch(n))
                .dragging_style(|s| s.opacity(0.35))
                // Pressing any photo picks up it and the one after it, as a selection would.
                .source(
                    DragSource::many(move |_| vec![Photo(n), Photo(n % 6 + 1)])
                        .preview(Preview::Layer),
                )
        });
        DndProvider::new(
            div()
                .size_full()
                .bg(theme.background)
                .text_color(theme.foreground)
                .p(px(32.))
                .flex()
                .flex_col()
                .gap(px(24.))
                .child(div().text_lg().child("Pick photos for the album"))
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(12.))
                        .w(px(340.))
                        .children(photos),
                )
                .child(
                    dnd("album")
                        .label("Album")
                        .w(px(520.))
                        .h(px(120.))
                        .p(px(12.))
                        .flex()
                        .gap(px(8.))
                        .rounded(theme.radius_xl())
                        .border_2()
                        .border_dashed()
                        .border_color(theme.border)
                        .can_drop_style(|s| s.border_color(gpui::hsla(0.6, 0.7, 0.55, 1.)))
                        .children(
                            self.album
                                .iter()
                                .map(|n| div().size(px(44.)).rounded(px(6.)).bg(swatch(*n))),
                        )
                        .target(DropTarget::new().accept::<Photo>().on_drop(
                            move |item, _, _, cx| {
                                let picked: Vec<u32> =
                                    item.all::<Photo>().unwrap().iter().map(|p| p.0).collect();
                                this.update(cx, |g, cx| {
                                    g.album.extend(picked);
                                    cx.notify();
                                })
                                .ok();
                                DropOutcome::Handled(None)
                            },
                        )),
                ),
        )
        .layer(|m, _, cx| {
            let photos = m.item()?.all::<Photo>()?.to_vec();
            let origin = m.preview_origin();
            let theme = cx.theme();
            let stack = photos.iter().enumerate().rev().map(|(i, p)| {
                div()
                    .absolute()
                    .left(px(i as f32 * 10.))
                    .top(px(i as f32 * 6.))
                    .size(px(80.))
                    .rounded(theme.radius_lg())
                    .border_2()
                    .border_color(theme.background)
                    .shadow_lg()
                    .bg(swatch(p.0))
            });
            Some(
                div()
                    .absolute()
                    .left(origin.x)
                    .top(origin.y)
                    .size(px(110.))
                    .children(stack)
                    .child(
                        div()
                            .absolute()
                            .top(px(-10.))
                            .left(px(70.))
                            .px(px(8.))
                            .h(px(22.))
                            .rounded_full()
                            .bg(theme.primary)
                            .text_color(theme.primary_foreground)
                            .text_xs()
                            .flex()
                            .items_center()
                            .child(format!("{} photos", photos.len())),
                    )
                    .into_any_element(),
            )
        })
    }
}

fn script(scene: &str) -> Vec<Step> {
    match scene {
        // The first photo and the one after it held over the album.
        "drag" => hold(point(px(80.), px(120.)), point(px(300.), px(330.))),
        _ => Vec::new(),
    }
}

fn main() {
    shared::run(
        "Custom drag layer",
        size(px(800.), px(480.)),
        |_, cx| cx.new(|_| Gallery { album: Vec::new() }),
        script,
    )
}
