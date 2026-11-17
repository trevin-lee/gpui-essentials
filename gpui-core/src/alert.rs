//! An alert, as shadcn/ui draws one: a bordered box calling attention to something, with an
//! optional icon, a title and a description, in a default and a destructive variant.
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::{Alert, IconName};
//! # struct Page;
//! # impl Render for Page {
//! fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
//!     Alert::new("sync-failed")
//!         .destructive()
//!         .icon(IconName::CircleExclamationPoint)
//!         .title("Sync failed")
//!         .description("Check your connection and try again.")
//! }
//! # }
//! ```
//!
//! Its text wraps (a long word breaks rather than running out of the box). To assistive
//! technology a destructive alert is an alert, announced at once; the default one is a status,
//! announced when the reader is next idle. [`role`](Alert::role) says otherwise (a `Note` for
//! a callout that's always on the page).

use gpui::{
    AnyElement, App, ElementId, FontWeight, InteractiveElement as _, IntoElement, ParentElement,
    Refineable as _, Rems, RenderOnce, Role, SharedString, StatefulInteractiveElement as _,
    StyleRefinement, Styled, Window, div, prelude::FluentBuilder as _, relative, rems,
};

use crate::icon::Icon;
use crate::theme::ActiveTheme as _;

/// A size in the web's CSS pixels, as rems of its 15 pixel text.
fn css(v: f32) -> Rems {
    rems(v / 15.)
}

/// How an alert looks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AlertVariant {
    /// The card's colours.
    #[default]
    Default,
    /// Something went wrong: the title, icon and description in the destructive colour.
    Destructive,
}

/// An alert. [`title`](Self::title) and [`description`](Self::description) are its text; its
/// children go under the description, in the description's colour.
#[derive(IntoElement)]
pub struct Alert {
    id: ElementId,
    variant: AlertVariant,
    icon: Option<Icon>,
    title: Option<SharedString>,
    description: Option<SharedString>,
    children: Vec<AnyElement>,
    role: Option<Role>,
    style: StyleRefinement,
}

impl Alert {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            variant: AlertVariant::Default,
            icon: None,
            title: None,
            description: None,
            children: Vec::new(),
            role: None,
            style: StyleRefinement::default(),
        }
    }

    pub fn with_variant(mut self, variant: AlertVariant) -> Self {
        self.variant = variant;
        self
    }

    /// The destructive variant.
    pub fn destructive(self) -> Self {
        self.with_variant(AlertVariant::Destructive)
    }

    /// An icon before the text, level with the title, in the alert's text colour.
    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// The title, in the stronger weight. It wraps.
    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// The text under the title, in the quieter colour. It wraps.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// What it is to assistive technology, instead of an alert (destructive) or a status
    /// (default).
    pub fn role(mut self, role: Role) -> Self {
        self.role = Some(role);
        self
    }
}

impl Styled for Alert {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for Alert {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

impl RenderOnce for Alert {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let destructive = self.variant == AlertVariant::Destructive;
        let (text, quiet) = if destructive {
            (theme.destructive, theme.destructive.opacity(0.9))
        } else {
            (theme.card_foreground, theme.muted_foreground)
        };
        let role = self.role.unwrap_or(if destructive {
            Role::Alert
        } else {
            Role::Status
        });
        let has_description = self.description.is_some() || !self.children.is_empty();
        let mut alert = div()
            .id(self.id)
            .role(role)
            .when_some(self.title.clone(), |d, t| d.aria_label(t))
            .when_some(self.description.clone(), |d, t| d.aria_description(t))
            .debug_selector(|| "alert".into())
            .flex()
            .items_start()
            .gap(css(12.))
            .w_full()
            .min_w_0()
            .px(css(16.))
            .py(css(12.))
            .rounded(theme.radius_lg())
            .border_1()
            .border_color(theme.border)
            .bg(theme.card)
            .text_color(text)
            .text_size(theme.text.sm)
            .line_height(relative(1.5))
            .when_some(self.icon, |d, icon| {
                // Level with the title's first line: as tall as a line, the icon centred in it.
                d.child(
                    div()
                        .debug_selector(|| "alert-icon".into())
                        .flex_none()
                        .flex()
                        .items_center()
                        .h(rems(theme.text.sm.0 * 1.5))
                        .child(icon.size(css(16.)).text_color(text)),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(css(2.))
                    .when_some(self.title, |d, title| {
                        d.child(
                            div()
                                .debug_selector(|| "alert-title".into())
                                .min_w_0()
                                .font_weight(FontWeight::MEDIUM)
                                .child(title),
                        )
                    })
                    .when(has_description, |d| {
                        d.child(
                            div()
                                .debug_selector(|| "alert-description".into())
                                .flex()
                                .flex_col()
                                .gap(css(4.))
                                .min_w_0()
                                .text_color(quiet)
                                .line_height(relative(1.625))
                                .when_some(self.description, |d, t| d.child(t))
                                .children(self.children),
                        )
                    }),
            );
        alert.style().refine(&self.style);
        alert
    }
}
