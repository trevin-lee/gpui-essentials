//! A progress bar, as shadcn draws it: an eight-pixel rounded track in the primary colour at
//! 20%, filled with the primary colour as far as its value (0 to 100). Without a value it's
//! indeterminate: a stretch of the fill sweeping across it.
//!
//! The sweep runs on GPUI's frame clock only while the bar is drawn; a bar with a value never
//! asks for frames, and GPUI's reduce-motion setting (`cx.set_reduce_motion(true)`) holds the
//! sweep still.
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::Progress;
//! # fn upload(sent: f32) -> impl IntoElement {
//! div()
//!     .w(px(240.))
//!     .flex()
//!     .flex_col()
//!     .gap_2()
//!     .child(Progress::new("upload").value(sent).accessibility_label("Uploading"))
//!     .child(Progress::new("sync"))
//! # }
//! ```

use std::time::Duration;

use gpui::{
    Animation, AnimationExt as _, App, ElementId, InteractiveElement as _, IntoElement,
    ParentElement as _, Refineable as _, Rems, RenderOnce, Role, SharedString,
    StatefulInteractiveElement as _, StyleRefinement, Styled, Window, div, ease_in_out,
    prelude::FluentBuilder as _, relative, rems,
};

use crate::theme::ActiveTheme as _;

/// One sweep of an indeterminate bar.
const SWEEP: Duration = Duration::from_millis(1500);

/// How much of the track an indeterminate bar's moving stretch covers.
const STRETCH: f32 = 0.4;

fn css(v: f32) -> Rems {
    rems(v / 15.)
}

/// A value as a bar shows it: 0 to 100, and 0 for something that isn't a number.
fn clamp(value: f32) -> f32 {
    if value.is_nan() {
        0.
    } else {
        value.clamp(0., 100.)
    }
}

/// A progress bar across its parent's width. Give it a [`value`](Progress::value) to show how
/// far something has come; without one it's indeterminate.
#[derive(IntoElement)]
pub struct Progress {
    id: ElementId,
    value: Option<f32>,
    label: Option<SharedString>,
    style: StyleRefinement,
}

impl Progress {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            value: None,
            label: None,
            style: StyleRefinement::default(),
        }
    }

    /// How far along it is, from 0 to 100 (a value outside that is drawn, and told to
    /// assistive technology, at the nearer end).
    pub fn value(mut self, value: f32) -> Self {
        self.value = Some(value);
        self
    }

    /// What's progressing, to assistive technology ("Uploading").
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl Styled for Progress {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Progress {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let fill = div().absolute().top_0().bottom_0().bg(theme.primary);
        let fill = match self.value.map(clamp) {
            Some(v) => fill.left_0().w(relative(v / 100.)).into_any_element(),
            // A stretch of the fill from just off the left end to just off the right.
            None => fill
                .w(relative(STRETCH))
                .with_animation(
                    ElementId::NamedChild(std::sync::Arc::new(self.id.clone()), "sweep".into()),
                    Animation::new(SWEEP).repeat().with_easing(ease_in_out),
                    |f, t| f.left(relative(-STRETCH + (1. + STRETCH) * t)),
                )
                .into_any_element(),
        };
        let mut bar = div()
            .id(self.id)
            .role(Role::ProgressIndicator)
            .when_some(self.value.map(clamp), |d, v| {
                d.aria_numeric_value(v as f64)
                    .aria_min_numeric_value(0.)
                    .aria_max_numeric_value(100.)
            })
            .when_some(self.label, |d, l| d.aria_label(l))
            .relative()
            .w_full()
            .h(css(8.))
            .flex_none()
            .rounded_full()
            .overflow_hidden()
            .bg(theme.primary.opacity(0.2));
        bar.style().refine(&self.style);
        bar.child(fill)
    }
}
