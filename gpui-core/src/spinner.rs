//! A spinner, as shadcn draws it: Lucide's loader-circle turning once a second, in the text's
//! colour, 16 pixels unless sized otherwise. To assistive technology it's a busy indicator
//! labelled "Loading".
//!
//! It turns on GPUI's frame clock only while it's drawn: once the host stops drawing it (the
//! wait is over), nothing asks for frames. GPUI's reduce-motion setting
//! (`cx.set_reduce_motion(true)`) keeps it still.
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::{Sizable as _, Spinner};
//! # fn saving() -> impl IntoElement {
//! div()
//!     .flex()
//!     .items_center()
//!     .gap_2()
//!     .child(Spinner::new("saving").small())
//!     .child("Saving…")
//! # }
//! ```

use std::time::Duration;

use gpui::{
    Animation, AnimationExt as _, App, ElementId, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Refineable as _, Rems, RenderOnce, Role, SharedString,
    StatefulInteractiveElement as _, StyleRefinement, Styled, Transformation, Window, div,
    percentage, rems, svg,
};

use crate::base::{ControlSize, IconName, Sizable};

/// One turn.
const TURN: Duration = Duration::from_secs(1);

fn css(v: f32) -> Rems {
    rems(v / 15.)
}

/// A turning loader. Sized with [`Sizable`] (12, 14, 16 or 20 pixels) or like a `div`
/// (`.size(px(32.))`).
#[derive(IntoElement)]
pub struct Spinner {
    id: ElementId,
    size: ControlSize,
    color: Option<Hsla>,
    label: SharedString,
    style: StyleRefinement,
}

impl Spinner {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            size: ControlSize::Medium,
            color: None,
            label: "Loading".into(),
            style: StyleRefinement::default(),
        }
    }

    /// Its colour (otherwise the text's).
    pub fn color(mut self, color: impl Into<Hsla>) -> Self {
        self.color = Some(color.into());
        self
    }

    /// What assistive technology calls it, if not "Loading" ("Sending", "Signing in").
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        self
    }
}

impl Sizable for Spinner {
    fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }
}

impl Styled for Spinner {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Spinner {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let side = css(match self.size {
            ControlSize::XSmall => 12.,
            ControlSize::Small => 14.,
            ControlSize::Medium | ControlSize::Touch => 16.,
            ControlSize::Large => 20.,
        });
        // An SVG draws in its own text colour, not the one around it: take that one here.
        let color = self.color.unwrap_or_else(|| window.text_style().color);
        let loader = svg()
            .path(IconName::LoaderCircle.path())
            .size_full()
            .text_color(color)
            .with_animation(
                ElementId::NamedChild(std::sync::Arc::new(self.id.clone()), "turn".into()),
                Animation::new(TURN).repeat(),
                |s, t| s.with_transformation(Transformation::rotate(percentage(t))),
            );
        // An indeterminate progress indicator (one with no value) is what platforms call a busy
        // indicator; shadcn's is a `status`, which AccessKit has too, but that's a live region
        // for messages, not a thing that's busy.
        let mut el = div()
            .id(self.id)
            .role(Role::ProgressIndicator)
            .aria_label(self.label)
            .flex_none()
            .size(side);
        el.style().refine(&self.style);
        el.child(loader)
    }
}
