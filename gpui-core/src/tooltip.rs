//! A tooltip, as shadcn/ui draws it (its `tooltip.tsx`): small text on the foreground colour,
//! fading in. Give it to GPUI's `.tooltip()`, which shows it after the pointer rests:
//!
//! ```ignore
//! div().id("stop").tooltip(|window, cx| Tooltip::new("Stop the build").build(window, cx))
//! ```
//!
//! A tooltip only shows under a pointer, so it must never be the only place something is said:
//! give what it explains an accessibility label with the same words.

use std::time::Duration;

use gpui::{
    Animation, AnimationExt as _, AnyView, App, AppContext as _, Context, Hsla, IntoElement,
    ParentElement as _, Render, SharedString, Styled as _, Window, div, ease_out_quint,
    prelude::FluentBuilder as _, rems,
};

use crate::base::ActiveTheme as _;

pub struct Tooltip {
    text: SharedString,
    background: Option<Hsla>,
    color: Option<Hsla>,
    border: Option<Hsla>,
    font: Option<SharedString>,
}

impl Tooltip {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            background: None,
            color: None,
            border: None,
            font: None,
        }
    }

    /// Its fill (otherwise the theme's foreground), for a host with a palette of its own.
    pub fn background(mut self, color: impl Into<Hsla>) -> Self {
        self.background = Some(color.into());
        self
    }

    /// Its text's colour (otherwise the theme's background).
    pub fn text_color(mut self, color: impl Into<Hsla>) -> Self {
        self.color = Some(color.into());
        self
    }

    /// A one-pixel border in this colour (otherwise none).
    /// Its font's family (otherwise the theme's). A tooltip is drawn over the window, apart from
    /// the element it's for, so it can't take that element's text style by itself.
    pub fn font_family(mut self, family: impl Into<SharedString>) -> Self {
        self.font = Some(family.into());
        self
    }

    pub fn border_color(mut self, color: impl Into<Hsla>) -> Self {
        self.border = Some(color.into());
        self
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
                    .bg(self.background.unwrap_or(theme.foreground))
                    .text_color(self.color.unwrap_or(theme.background))
                    .when_some(self.border, |d, c| d.border_1().border_color(c))
                    .text_size(theme.text.xs)
                    .font_family(
                        self.font
                            .clone()
                            .unwrap_or_else(|| theme.font_family.clone()),
                    )
                    .child(self.text.clone())
                    .with_animation(
                        "tooltip-in",
                        Animation::new(Duration::from_millis(150)).with_easing(ease_out_quint()),
                        |el, t| el.opacity(t),
                    ),
            )
    }
}
