//! A keyboard key, as shadcn draws it (its `kbd`): a small muted cap holding a key's name or
//! symbol ("⌘", "K", "Shift"), and a group of them for a shortcut ("⌘ K").
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::{Kbd, KbdGroup};
//! # fn hint() -> impl IntoElement {
//! div()
//!     .flex()
//!     .gap_2()
//!     .child(Kbd::new("Esc"))
//!     .child(KbdGroup::keys(["⌘", "K"]))
//! # }
//! ```

use gpui::{
    App, FontWeight, IntoElement, ParentElement as _, Refineable as _, Rems, RenderOnce,
    SharedString, StyleRefinement, Styled, Window, div, prelude::FluentBuilder as _, relative,
    rems,
};

use crate::base::{ActiveTheme as _, Icon};

/// A size in the web's CSS pixels, as rems of its 15 pixel text.
fn css(v: f32) -> Rems {
    rems(v / 15.)
}

/// A key: its name or symbol, an icon, or both. It's set in the theme's own font, as shadcn's
/// (not the monospace one), on one line; a long name in a narrow space ends in an ellipsis.
#[derive(IntoElement)]
pub struct Kbd {
    label: Option<SharedString>,
    icon: Option<Icon>,
    style: StyleRefinement,
}

impl Kbd {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: Some(label.into()),
            icon: None,
            style: StyleRefinement::default(),
        }
    }

    /// A key drawn as an icon alone (an arrow, Return).
    pub fn icon_only(icon: impl Into<Icon>) -> Self {
        Self {
            label: None,
            icon: Some(icon.into()),
            style: StyleRefinement::default(),
        }
    }

    /// An icon before the label, drawn at 12 pixels.
    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }
}

impl Styled for Kbd {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Kbd {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let mut key = div()
            .flex()
            // It gives way to its 20 pixels (the label ending in an ellipsis), no further.
            .flex_shrink_1()
            .max_w_full()
            .items_center()
            .justify_center()
            .gap(css(4.))
            .h(css(20.))
            .min_w(css(20.))
            .px(css(4.))
            .rounded(theme.radius_sm())
            .bg(theme.muted)
            .text_color(theme.muted_foreground)
            .text_size(theme.text.xs)
            .font_weight(FontWeight::MEDIUM)
            .line_height(relative(1.))
            .whitespace_nowrap()
            .overflow_hidden();
        key.style().refine(&self.style);
        key.when_some(self.icon, |k, icon| k.child(icon.size(css(12.))))
            .when_some(self.label, |k, label| {
                k.child(
                    // Three pixels' room above and below for descenders and accents, which
                    // reach past a line height of 1.
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .py(css(3.))
                        .my(css(-3.))
                        .text_ellipsis()
                        .child(label),
                )
            })
    }
}

/// Keys side by side, four pixels apart: a shortcut. In a narrow space its keys give way
/// alike and what still can't fit is clipped at its edge.
#[derive(IntoElement, Default)]
pub struct KbdGroup {
    keys: Vec<Kbd>,
    style: StyleRefinement,
}

impl KbdGroup {
    pub fn new() -> Self {
        Self::default()
    }

    /// A group of plain keys, one for each name: `KbdGroup::keys(["⌘", "K"])`.
    pub fn keys<S: Into<SharedString>>(keys: impl IntoIterator<Item = S>) -> Self {
        Self::new().children(keys.into_iter().map(Kbd::new))
    }

    pub fn child(mut self, key: Kbd) -> Self {
        self.keys.push(key);
        self
    }

    pub fn children(mut self, keys: impl IntoIterator<Item = Kbd>) -> Self {
        self.keys.extend(keys);
        self
    }
}

impl Styled for KbdGroup {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for KbdGroup {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let mut group = div()
            .flex()
            .flex_shrink_1()
            .min_w_0()
            .max_w_full()
            .items_center()
            .gap(css(4.))
            .overflow_hidden();
        group.style().refine(&self.style);
        group.children(self.keys)
    }
}
