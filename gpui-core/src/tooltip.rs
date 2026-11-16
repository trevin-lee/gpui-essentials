//! A tooltip, as the web app draws it (its `tooltip.tsx`): small text on the foreground colour,
//! fading in. Give it to GPUI's `.tooltip()`, which shows it after the pointer rests:
//!
//! ```ignore
//! div().id("stop").tooltip(|window, cx| Tooltip::new("Stop the agent").build(window, cx))
//! ```
//!
//! A tooltip only shows under a pointer, so it must never be the only place something is said:
//! give what it explains an accessibility label with the same words.

use std::time::Duration;

use gpui::{
    Animation, AnimationExt as _, AnyView, App, AppContext as _, Context, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Window, div, ease_out_quint, rems,
};

use crate::theme::ActiveTheme as _;

pub struct Tooltip {
    text: SharedString,
}

impl Tooltip {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self { text: text.into() }
    }

    /// The view `.tooltip()` asks for.
    pub fn build(self, _: &mut Window, cx: &mut App) -> AnyView {
        cx.new(|_| self).into()
    }
}

impl Render for Tooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let css = |v: f32| rems(v / 15.);
        div()
            // Off the pointer, as GPUI places it; no wider than the web's `max-w-xs`, nor than
            // the window, so its text wraps inside it (a word too long for it breaks).
            .flex()
            .max_w_full()
            .pl(css(10.))
            .pt(css(18.))
            .child(
                div()
                    .min_w_0()
                    .flex_shrink_1()
                    .max_w(css(320.))
                    .px(css(12.))
                    .py(css(6.))
                    .rounded(theme.radius_md())
                    .bg(theme.foreground)
                    .text_color(theme.background)
                    .text_size(theme.text.xs)
                    .font_family(theme.font_family.clone())
                    .child(self.text.clone())
                    .with_animation(
                        "tooltip-in",
                        Animation::new(Duration::from_millis(150)).with_easing(ease_out_quint()),
                        |el, t| el.opacity(t),
                    ),
            )
    }
}
