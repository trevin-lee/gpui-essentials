//! A card, as the web app draws it (its `card.tsx`): a raised panel with a header (a title, a
//! description, and an action at the top right), content, and a footer along the bottom.
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::{Button, ButtonVariants as _, Card, Sizable as _};
//! # struct Plan;
//! # impl Render for Plan {
//! fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
//!     Card::new()
//!         .title("Team plan")
//!         .description("Five seats, billed monthly.")
//!         .action(Button::new("edit").label("Edit").outline().small())
//!         .child("Next charge on the 1st.")
//!         .footer(Button::new("upgrade").label("Upgrade"))
//! }
//! # }
//! ```
//!
//! It draws only what it's given: no header without a title, a description or an action, and
//! no footer unless one is added. It holds no state and takes no input of its own, so it's the
//! same under a pointer and a finger.

use gpui::{
    AnyElement, App, FontWeight, InteractiveElement as _, IntoElement, ParentElement,
    Refineable as _, Rems, RenderOnce, SharedString, StyleRefinement, Styled, Window, div,
    prelude::FluentBuilder as _, relative, rems,
};

use crate::base::{ActiveTheme as _, ControlSize, Sizable};

/// A size in the web's CSS pixels, as rems of its 15 pixel text.
fn css(v: f32) -> Rems {
    rems(v / 15.)
}

/// A card. Its children are its content; [`title`](Self::title),
/// [`description`](Self::description) and [`action`](Self::action) make its header, and
/// [`footer`](Self::footer) its footer. Style it like a `div` (a width, a margin).
#[derive(IntoElement)]
pub struct Card {
    title: Option<SharedString>,
    description: Option<SharedString>,
    action: Option<AnyElement>,
    content: Vec<AnyElement>,
    footer: Vec<AnyElement>,
    small: bool,
    style: StyleRefinement,
}

impl Card {
    pub fn new() -> Self {
        Self {
            title: None,
            description: None,
            action: None,
            content: Vec::new(),
            footer: Vec::new(),
            small: false,
            style: StyleRefinement::default(),
        }
    }

    /// The title at the top. It wraps.
    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Quieter text under the title. It wraps.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Something at the top right, beside the title and description: a button, a badge, a menu.
    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.action = Some(action.into_any_element());
        self
    }

    /// Adds to the footer: a row along the bottom on a quieter fill, its items centred
    /// vertically and spaced apart.
    pub fn footer(mut self, item: impl IntoElement) -> Self {
        self.footer.push(item.into_any_element());
        self
    }
}

impl Default for Card {
    fn default() -> Self {
        Self::new()
    }
}

impl Sizable for Card {
    /// [`ControlSize::Small`] or smaller tightens the spacing (16 CSS pixels to 12) and sets
    /// the title in the smaller text; the other sizes are the same.
    fn with_size(mut self, size: ControlSize) -> Self {
        self.small = matches!(size, ControlSize::XSmall | ControlSize::Small);
        self
    }
}

impl Styled for Card {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for Card {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.content.extend(elements)
    }
}

impl RenderOnce for Card {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let space = css(if self.small { 12. } else { 16. });
        let has_header =
            self.title.is_some() || self.description.is_some() || self.action.is_some();
        let has_footer = !self.footer.is_empty();
        let header = has_header.then(|| {
            div()
                .debug_selector(|| "card-header".into())
                .flex()
                .items_start()
                .gap(css(8.))
                .px(space)
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_w_0()
                        .gap(css(4.))
                        .when_some(self.title, |d, title| {
                            d.child(
                                div()
                                    .debug_selector(|| "card-title".into())
                                    .min_w_0()
                                    .text_size(if self.small {
                                        theme.text.sm
                                    } else {
                                        theme.text.base
                                    })
                                    .line_height(relative(1.375))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(title),
                            )
                        })
                        .when_some(self.description, |d, description| {
                            d.child(
                                div()
                                    .debug_selector(|| "card-description".into())
                                    .min_w_0()
                                    .text_color(theme.muted_foreground)
                                    .child(description),
                            )
                        }),
                )
                .when_some(self.action, |d, action| {
                    d.child(
                        div()
                            .debug_selector(|| "card-action".into())
                            .flex_none()
                            .max_w(relative(0.5))
                            .child(action),
                    )
                })
        });
        let content = (!self.content.is_empty()).then(|| {
            div()
                .debug_selector(|| "card-content".into())
                .min_w_0()
                .px(space)
                .children(self.content)
        });
        let footer = has_footer.then(|| {
            div()
                .debug_selector(|| "card-footer".into())
                .flex()
                .items_center()
                .gap(css(8.))
                .min_w_0()
                .p(space)
                .border_t_1()
                .border_color(theme.border)
                .bg(theme.muted.opacity(0.5))
                .rounded_b(theme.radius_xl())
                .children(self.footer)
        });
        let mut card = div()
            .debug_selector(|| "card".into())
            .flex()
            .flex_col()
            .min_w_0()
            .gap(space)
            .pt(space)
            .when(!has_footer, |d| d.pb(space))
            .overflow_hidden()
            .rounded(theme.radius_xl())
            .border_1()
            .border_color(theme.border)
            .bg(theme.card)
            .text_color(theme.card_foreground)
            .text_size(theme.text.sm)
            .children(header)
            .children(content)
            .children(footer);
        card.style().refine(&self.style);
        card
    }
}
