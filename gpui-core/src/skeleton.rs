//! A skeleton, as shadcn draws it: a muted rounded block standing in for content that's on its
//! way, the size the host gives it, pulsing.
//!
//! The pulse (shadcn's `animate-pulse`: its opacity down to half and back every two seconds)
//! runs on GPUI's frame clock only while the skeleton is drawn: once the host stops drawing it
//! (its content came), nothing asks for frames. `animate(false)` keeps it still, and so does
//! GPUI's reduce-motion setting (`cx.set_reduce_motion(true)`).
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::Skeleton;
//! # fn loading() -> impl IntoElement {
//! div()
//!     .flex()
//!     .items_center()
//!     .gap_4()
//!     .child(Skeleton::new().size_12().rounded_full())
//!     .child(
//!         div()
//!             .flex()
//!             .flex_col()
//!             .gap_2()
//!             .child(Skeleton::new().h_4().w(px(250.)))
//!             .child(Skeleton::new().h_4().w(px(200.))),
//!     )
//! # }
//! ```

use std::time::Duration;

use gpui::{
    Animation, AnimationExt as _, App, ElementId, IntoElement, Refineable as _, RenderOnce,
    StyleRefinement, Styled, Window, div,
};

use crate::base::ActiveTheme as _;

/// How long one pulse takes: down to half and back.
const PULSE: Duration = Duration::from_secs(2);

/// Its opacity `t` of the way through a pulse: shadcn's keyframes (whole at the ends, half in
/// the middle) on a smooth curve.
fn pulse(t: f32) -> f32 {
    0.75 + 0.25 * (t * std::f32::consts::TAU).cos()
}

/// A muted rounded block, as big as the host makes it (`.h_4().w(px(250.))`, `.size_12()`),
/// rounded as shadcn's (`rounded-md`) unless styled otherwise (`.rounded_full()` for an
/// avatar's place).
#[derive(IntoElement)]
pub struct Skeleton {
    id: ElementId,
    animate: bool,
    style: StyleRefinement,
}

impl Skeleton {
    pub fn new() -> Self {
        Self {
            id: ElementId::Name("skeleton".into()),
            animate: true,
            style: StyleRefinement::default(),
        }
    }

    /// Whether it pulses (it does unless told not to).
    pub fn animate(mut self, animate: bool) -> Self {
        self.animate = animate;
        self
    }

    /// Its id among its siblings (it's `"skeleton"` otherwise). Skeletons pulse as one
    /// whatever their ids.
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = id.into();
        self
    }
}

impl Default for Skeleton {
    fn default() -> Self {
        Self::new()
    }
}

impl Styled for Skeleton {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Skeleton {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        // Never wider than its parent: a 250-pixel line in a 200-pixel card takes 200.
        let mut block = div()
            .max_w_full()
            .rounded(theme.radius_md())
            .bg(theme.muted);
        block.style().refine(&self.style);
        if !self.animate {
            return block.into_any_element();
        }
        // Synced: every skeleton on screen pulses as one, as they do on the web.
        block
            .with_animation(
                self.id,
                Animation::new(PULSE).repeat_synced(),
                |block, t| block.opacity(pulse(t)),
            )
            .into_any_element()
    }
}
