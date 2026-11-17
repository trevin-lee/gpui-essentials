//! A badge, as shadcn draws it (the web app's `badge.tsx`): a small pill of text, perhaps with
//! an icon before it, in the default, secondary, destructive or outline look. It's text unless
//! it's given an [`on_click`](Badge::on_click); then it's a button, with the focus ring.
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::{Badge, IconName};
//! # fn badges() -> impl IntoElement {
//! div()
//!     .flex()
//!     .gap_2()
//!     .child(Badge::new("New"))
//!     .child(Badge::new("Draft").secondary())
//!     .child(Badge::new("Failed").destructive().icon(IconName::CircleExclamationPoint))
//!     .child(Badge::new("v2.1").outline())
//! # }
//! ```

use std::rc::Rc;

use gpui::{
    App, ClickEvent, ElementId, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    ParentElement, Refineable as _, Rems, RenderOnce, SharedString, StyleRefinement, Styled,
    Window, div, prelude::FluentBuilder as _, relative, rems,
};

use crate::icon::Icon;
use crate::press::Press;
use crate::theme::{ActiveTheme as _, Theme};

/// How a badge looks: shadcn's four.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BadgeVariant {
    /// Filled with the primary colour.
    #[default]
    Default,
    /// Filled with the secondary colour.
    Secondary,
    /// Tinted with the destructive colour.
    Destructive,
    /// A border and no fill.
    Outline,
}

/// A size in the web's CSS pixels, as rems of its 15 pixel text.
fn css(v: f32) -> Rems {
    rems(v / 15.)
}

/// A variant's fill, text and border, what a pressable one turns to under the pointer, and its
/// focus ring.
struct Paint {
    fill: Hsla,
    text: Hsla,
    border: Hsla,
    hover_fill: Hsla,
    hover_text: Hsla,
    ring: Hsla,
}

fn paint(variant: BadgeVariant, theme: &Theme) -> Paint {
    let dark = theme.mode.is_dark();
    let none = gpui::transparent_black();
    let c = &theme.colors;
    match variant {
        BadgeVariant::Default => Paint {
            fill: c.primary,
            text: c.primary_foreground,
            border: none,
            hover_fill: c.primary.opacity(0.8),
            hover_text: c.primary_foreground,
            ring: theme.focus_ring(),
        },
        BadgeVariant::Secondary => Paint {
            fill: c.secondary,
            text: c.secondary_foreground,
            border: none,
            hover_fill: c.secondary.opacity(0.8),
            hover_text: c.secondary_foreground,
            ring: theme.focus_ring(),
        },
        BadgeVariant::Destructive => Paint {
            fill: c.destructive.opacity(if dark { 0.2 } else { 0.1 }),
            text: c.destructive,
            border: none,
            hover_fill: c.destructive.opacity(if dark { 0.3 } else { 0.2 }),
            hover_text: c.destructive,
            ring: c.destructive.opacity(if dark { 0.4 } else { 0.2 }),
        },
        BadgeVariant::Outline => Paint {
            fill: none,
            text: c.foreground,
            border: c.border,
            hover_fill: c.muted,
            hover_text: c.muted_foreground,
            ring: theme.focus_ring(),
        },
    }
}

type OnClick = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// A badge: a label, and perhaps an icon before it. It keeps to one line and, given less room
/// than its label needs, ends it in an ellipsis. Like a `div`, it fills the width of a parent
/// that isn't a flex row; put it in one (or give it a width) to have it fit its label.
#[derive(IntoElement)]
pub struct Badge {
    id: Option<ElementId>,
    label: SharedString,
    icon: Option<Icon>,
    variant: BadgeVariant,
    accessibility_label: Option<SharedString>,
    on_click: Option<OnClick>,
    touch: bool,
    style: StyleRefinement,
}

impl Badge {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            id: None,
            label: label.into(),
            icon: None,
            variant: BadgeVariant::default(),
            accessibility_label: None,
            on_click: None,
            touch: false,
            style: StyleRefinement::default(),
        }
    }

    /// Its id, for a badge that can be pressed (otherwise its label is its id: give badges with
    /// the same label side by side ids of their own).
    pub fn id(mut self, id: impl Into<ElementId>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// An icon before the label, drawn at 12 pixels in the label's colour.
    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn variant(mut self, variant: BadgeVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn secondary(self) -> Self {
        self.variant(BadgeVariant::Secondary)
    }

    pub fn destructive(self) -> Self {
        self.variant(BadgeVariant::Destructive)
    }

    pub fn outline(self) -> Self {
        self.variant(BadgeVariant::Outline)
    }

    /// What it's called to assistive technology when it can be pressed, if not its label.
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.accessibility_label = Some(label.into());
        self
    }

    /// Makes it a button: called on a click, a tap, or Enter or Space while it has the focus,
    /// which Tab gives it.
    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }

    /// A badge that can be pressed takes presses 44 points each way around it, for a finger,
    /// without growing.
    pub fn touch(mut self, touch: bool) -> Self {
        self.touch = touch;
        self
    }
}

impl Styled for Badge {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// Gives a badge (a `div`, or a [`Press`] if it can be pressed) its look and its content.
fn dress<E: Styled + ParentElement + IntoElement>(
    mut el: E,
    clip: bool,
    p: &Paint,
    label: SharedString,
    icon: Option<Icon>,
    theme: &Theme,
    style: &StyleRefinement,
) -> E {
    let has_icon = icon.is_some();
    el = el
        .flex()
        // The label gives way (it ends in an ellipsis) rather than run out of the space there
        // is; the icon keeps its size.
        .flex_shrink_1()
        .min_w_0()
        .max_w_full()
        .items_center()
        .justify_center()
        .gap(css(4.))
        .h(css(20.))
        .pl(css(if has_icon { 6. } else { 8. }))
        .pr(css(8.))
        .rounded_full()
        .border_1()
        .border_color(p.border)
        .bg(p.fill)
        .text_color(p.text)
        .text_size(theme.text.xs)
        .font_weight(FontWeight::MEDIUM)
        .line_height(relative(1.))
        .whitespace_nowrap()
        // What can't fit even with the label at nothing (the icon) is clipped at its edge;
        // except where it takes presses past its edge, which a clip would cut off.
        .when(clip, |el| el.overflow_hidden());
    el.style().refine(style);
    el.when_some(icon, |el, icon| el.child(icon.size(css(12.))))
        .child(
            // Clipped to its slot with three pixels' room either side (taken from the padding)
            // for ink past its advance: an emoji, the ellipsis.
            div()
                .min_w_0()
                .overflow_hidden()
                .px(css(3.))
                .mx(css(-3.))
                .text_ellipsis()
                .child(label),
        )
}

impl RenderOnce for Badge {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let p = paint(self.variant, &theme);
        let Some(on_click) = self.on_click else {
            return dress(div(), true, &p, self.label, self.icon, &theme, &self.style)
                .into_any_element();
        };
        let id = self
            .id
            .unwrap_or_else(|| ElementId::Name(self.label.clone()));
        let (hover_fill, hover_text) = (p.hover_fill, p.hover_text);
        let touch = self.touch;
        let press = Press::new(id)
            .accessibility_label(
                self.accessibility_label
                    .unwrap_or_else(|| self.label.clone()),
            )
            .focus_ring_color(p.ring)
            .when(touch, |b| b.min_hit(theme.touch_target))
            .hover(move |s| s.bg(hover_fill).text_color(hover_text))
            .on_click(move |e, w, cx| on_click(e, w, cx));
        dress(
            press,
            !touch,
            &p,
            self.label,
            self.icon,
            &theme,
            &self.style,
        )
        .into_any_element()
    }
}
