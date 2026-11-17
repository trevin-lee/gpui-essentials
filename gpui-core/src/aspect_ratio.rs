//! A box that keeps a ratio, as shadcn's `AspectRatio` (Radix's): as wide as it's given, and
//! as tall as the ratio makes that, whatever is in it.
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::AspectRatio;
//! # fn video(poster: impl IntoElement) -> impl IntoElement {
//! div().w(px(480.)).child(AspectRatio::new(16. / 9.).rounded_lg().child(poster))
//! # }
//! ```

use gpui::{
    AnyElement, App, IntoElement, ParentElement, Refineable as _, RenderOnce, StyleRefinement,
    Styled, Window, div,
};

/// A box `ratio` times as wide as it is tall (16 / 9, 4 / 3, 1). It's its parent's width
/// unless given another (`.w(px(300.))`), and its height follows. Its children fill it (an
/// image, a video's poster) and are clipped at its edges; they never change its height.
#[derive(IntoElement)]
pub struct AspectRatio {
    ratio: f32,
    children: Vec<AnyElement>,
    style: StyleRefinement,
}

impl AspectRatio {
    /// A box of width over height `ratio`. One that isn't a positive number is taken as 1, a
    /// square.
    pub fn new(ratio: f32) -> Self {
        Self {
            ratio: if ratio.is_finite() && ratio > 0. {
                ratio
            } else {
                1.
            },
            children: Vec::new(),
            style: StyleRefinement::default(),
        }
    }
}

impl Styled for AspectRatio {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for AspectRatio {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

impl RenderOnce for AspectRatio {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let mut el = div()
            .relative()
            .w_full()
            .flex_none()
            .aspect_ratio(self.ratio)
            .overflow_hidden();
        el.style().refine(&self.style);
        // The children are laid over the box rather than in it, so what they'd take never
        // stretches it past its ratio.
        el.child(
            div()
                .absolute()
                .inset_0()
                .size_full()
                .children(self.children),
        )
    }
}
