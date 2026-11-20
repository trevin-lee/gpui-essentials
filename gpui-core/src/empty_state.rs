//! An empty state, as shadcn draws it (its `Empty`, a name GPUI already has): an icon in a muted circle, a title, a
//! description and room for actions, centred one above another. The text wraps; nothing runs
//! out of its box at any width.
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::{Button, EmptyState, IconName};
//! # fn no_messages() -> impl IntoElement {
//! EmptyState::new()
//!     .icon(IconName::Inbox)
//!     .title("No messages yet")
//!     .description("When someone writes to you, it shows up here.")
//!     .child(Button::new("new").label("Start a chat"))
//! # }
//! ```

use gpui::{
    AnyElement, App, FontWeight, IntoElement, ParentElement, Refineable as _, Rems, RenderOnce,
    SharedString, StyleRefinement, Styled, Window, div, prelude::FluentBuilder as _, relative,
    rems,
};

use crate::base::{ActiveTheme as _, Icon};

fn css(v: f32) -> Rems {
    rems(v / 15.)
}

/// An empty state. Its children are its action area (a button or two, a link), centred under
/// the description. It fills the space it's given (shadcn's `flex-1`) and centres itself
/// there; its text keeps to shadcn's 384 pixels and wraps.
#[derive(IntoElement, Default)]
pub struct EmptyState {
    icon: Option<Icon>,
    title: Option<SharedString>,
    description: Option<SharedString>,
    actions: Vec<AnyElement>,
    style: StyleRefinement,
}

impl EmptyState {
    pub fn new() -> Self {
        Self::default()
    }

    /// The icon, drawn at 24 pixels in a 40-pixel muted circle.
    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// A sentence or two under the title, in the muted text colour.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }
}

impl Styled for EmptyState {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for EmptyState {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.actions.extend(elements)
    }
}

impl RenderOnce for EmptyState {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        // A line of text that wraps inside its parent, centred.
        let text = || div().w_full().min_w_0().text_center().whitespace_normal();
        let media = self.icon.map(|icon| {
            div()
                .mb(css(8.))
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(css(40.))
                .rounded_full()
                .bg(theme.muted)
                .text_color(theme.foreground)
                .child(icon.size(css(24.)))
        });
        let header = div()
            .flex()
            .flex_col()
            .items_center()
            .gap(css(8.))
            .w_full()
            .max_w(css(384.))
            .min_w_0()
            .children(media)
            .when_some(self.title, |d, t| {
                d.child(
                    text()
                        .text_size(theme.text.lg)
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.foreground)
                        .line_height(relative(1.4))
                        .child(t),
                )
            })
            .when_some(self.description, |d, t| {
                d.child(
                    text()
                        .text_size(theme.text.sm)
                        .text_color(theme.muted_foreground)
                        .line_height(relative(1.625))
                        .child(t),
                )
            });
        let actions = (!self.actions.is_empty()).then(|| {
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(css(16.))
                .w_full()
                .max_w(css(384.))
                .min_w_0()
                .text_size(theme.text.sm)
                .children(self.actions)
        });
        let mut el = div()
            .flex()
            .flex_1()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(css(24.))
            .min_w_0()
            .p(css(24.))
            .rounded(theme.radius_lg())
            .text_center();
        el.style().refine(&self.style);
        el.child(header).children(actions)
    }
}
