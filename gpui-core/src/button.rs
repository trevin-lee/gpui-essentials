//! Buttons as the web app draws them (its `button.tsx`): primary, outline, secondary, ghost,
//! destructive and link variants in five sizes, with a label, an icon or both, and groups of
//! them joined edge to edge.

use std::rc::Rc;

use gpui::{
    App, ClickEvent, ElementId, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Refineable as _, Rems, RenderOnce, SharedString,
    StatefulInteractiveElement as _, StyleRefinement, Styled, Window, div,
    prelude::FluentBuilder as _, px, relative, rems,
};

use crate::icon::Icon;
use crate::press::Press;
use crate::size::{ControlSize, Disableable, Selectable, Sizable};
use crate::theme::{ActiveTheme as _, Theme};

/// How a button looks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ButtonVariant {
    /// The strongest action: filled with the primary colour.
    #[default]
    Primary,
    /// A bordered button on the page.
    Outline,
    /// Filled with the secondary colour.
    Secondary,
    /// No fill until the pointer is on it.
    Ghost,
    /// Deleting or blocking: tinted with the destructive colour.
    Destructive,
    /// Text in the primary colour, underlined under the pointer.
    Link,
}

/// Choosing a [`ButtonVariant`].
pub trait ButtonVariants: Sized {
    fn with_variant(self, variant: ButtonVariant) -> Self;

    fn primary(self) -> Self {
        self.with_variant(ButtonVariant::Primary)
    }

    fn outline(self) -> Self {
        self.with_variant(ButtonVariant::Outline)
    }

    fn secondary(self) -> Self {
        self.with_variant(ButtonVariant::Secondary)
    }

    fn ghost(self) -> Self {
        self.with_variant(ButtonVariant::Ghost)
    }

    fn destructive(self) -> Self {
        self.with_variant(ButtonVariant::Destructive)
    }

    fn link(self) -> Self {
        self.with_variant(ButtonVariant::Link)
    }
}

/// A size in the web's CSS pixels, as rems of its 15 pixel text.
fn css(v: f32) -> Rems {
    rems(v / 15.)
}

/// The measurements of one size: height, horizontal padding, gap, text and icon size, radius.
struct Metrics {
    height: Rems,
    pad: Rems,
    gap: Rems,
    text: Rems,
    icon: Rems,
    radius: Rems,
}

fn metrics(size: ControlSize, theme: &Theme) -> Metrics {
    // The web's radius-md capped at 10 and 12 pixels for its two small sizes.
    let capped = |cap: f32| rems(theme.radius_md().0.min(cap / 15.));
    match size {
        ControlSize::XSmall => Metrics {
            height: css(24.),
            pad: css(8.),
            gap: css(4.),
            text: theme.text.xs,
            icon: css(12.),
            radius: capped(10.),
        },
        ControlSize::Small => Metrics {
            height: css(28.),
            pad: css(10.),
            gap: css(4.),
            text: css(12.8),
            icon: css(14.),
            radius: capped(12.),
        },
        ControlSize::Medium | ControlSize::Touch => Metrics {
            height: css(36.),
            pad: css(10.),
            gap: css(6.),
            text: theme.text.sm,
            icon: css(16.),
            radius: theme.radius_lg(),
        },
        ControlSize::Large => Metrics {
            height: css(40.),
            pad: css(12.),
            gap: css(6.),
            text: theme.text.sm,
            icon: css(16.),
            radius: theme.radius_lg(),
        },
    }
}

/// The side of an icon-only button: a little narrower than the height, as the web's.
fn icon_side(size: ControlSize) -> Rems {
    css(match size {
        ControlSize::XSmall => 24.,
        ControlSize::Small => 28.,
        ControlSize::Medium | ControlSize::Touch => 32.,
        ControlSize::Large => 36.,
    })
}

/// A variant's fill, text, border and fill under the pointer, in the theme's mode.
struct Paint {
    fill: Hsla,
    text: Hsla,
    border: Hsla,
    hover: Hsla,
}

fn paint(variant: ButtonVariant, selected: bool, theme: &Theme) -> Paint {
    let dark = theme.mode.is_dark();
    let none = gpui::transparent_black();
    let c = &theme.colors;
    let mut p = match variant {
        ButtonVariant::Primary => Paint {
            fill: c.primary,
            text: c.primary_foreground,
            border: none,
            hover: c.primary.opacity(0.8),
        },
        ButtonVariant::Outline if dark => Paint {
            fill: c.input.opacity(0.3),
            text: c.foreground,
            border: c.input,
            hover: c.input.opacity(0.5),
        },
        ButtonVariant::Outline => Paint {
            fill: c.background,
            text: c.foreground,
            border: c.border,
            hover: c.muted,
        },
        ButtonVariant::Secondary => Paint {
            fill: c.secondary,
            text: c.secondary_foreground,
            border: none,
            hover: c.secondary.blend(c.foreground.opacity(0.05)),
        },
        ButtonVariant::Ghost => Paint {
            fill: none,
            text: c.foreground,
            border: none,
            hover: if dark { c.muted.opacity(0.5) } else { c.muted },
        },
        ButtonVariant::Destructive => Paint {
            fill: c.destructive.opacity(if dark { 0.2 } else { 0.1 }),
            text: c.destructive,
            border: none,
            hover: c.destructive.opacity(if dark { 0.3 } else { 0.2 }),
        },
        ButtonVariant::Link => Paint {
            fill: none,
            text: c.primary,
            border: none,
            hover: none,
        },
    };
    // Selected (one of a group, or a toggle that's on) reads as pressed in: the web's
    // `aria-expanded` look, muted. In the dark, muted is all but the outline's own fill
    // (the input colour at 30%), so there it's the input colour itself.
    if selected && matches!(variant, ButtonVariant::Outline | ButtonVariant::Ghost) {
        p.fill = if dark { c.input } else { c.muted };
        p.hover = p.fill;
        p.text = c.foreground;
    }
    p
}

type OnClick = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// A button: a label, an icon, or an icon and a label. Give an icon-only button an
/// [`accessibility_label`](Button::accessibility_label), since nothing else names it.
#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    label: Option<SharedString>,
    icon: Option<Icon>,
    accessibility_label: Option<SharedString>,
    variant: ButtonVariant,
    size: ControlSize,
    selected: bool,
    disabled: bool,
    on_click: Option<OnClick>,
    /// Whether what it opens is open, for assistive tech (a menu's trigger).
    expanded: Option<bool>,
    style: StyleRefinement,
    /// Where it sits in a group, which squares its inner corners.
    joined: Joined,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Joined {
    #[default]
    Alone,
    First,
    Middle,
    Last,
}

impl Button {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            label: None,
            icon: None,
            accessibility_label: None,
            variant: ButtonVariant::default(),
            size: ControlSize::default(),
            selected: false,
            disabled: false,
            on_click: None,
            expanded: None,
            style: StyleRefinement::default(),
            joined: Joined::Alone,
        }
    }

    /// Its text. It stays on one line and ends in an ellipsis where the button is too narrow.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// An icon before the label, or alone.
    pub fn icon(mut self, icon: impl Into<Icon>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// What it's called to assistive technology, when the label doesn't say (an icon only).
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.accessibility_label = Some(label.into());
        self
    }

    /// Called on a click, a tap, or Enter or Space while it has the focus.
    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }

    /// Tells assistive tech whether what it opens (a menu) is open.
    pub(crate) fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = Some(expanded);
        self
    }
}

impl ButtonVariants for Button {
    fn with_variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }
}

impl Sizable for Button {
    fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }
}

impl Selectable for Button {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    fn is_selected(&self) -> bool {
        self.selected
    }
}

impl Disableable for Button {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Styled for Button {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Button {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let m = metrics(self.size, &theme);
        let p = paint(self.variant, self.selected, &theme);
        let icon_only = self.label.is_none() && self.icon.is_some();
        let touch = self.size == ControlSize::Touch;
        let min_touch: Pixels = theme.touch_target;
        let link = self.variant == ButtonVariant::Link;
        let label = self
            .accessibility_label
            .clone()
            .or_else(|| self.label.clone());
        let radius = m.radius;
        let joined = self.joined;

        let mut press = Press::new(self.id)
            .flex()
            // A label gives way (it ends in an ellipsis) rather than run out of the space the
            // button has; an icon alone keeps its size.
            .when(icon_only, |b| b.flex_none())
            .when(!icon_only, |b| b.flex_shrink_1())
            .min_w_0()
            .max_w_full()
            .items_center()
            .justify_center()
            .gap(m.gap)
            .h(m.height)
            .when(icon_only, |b| b.w(icon_side(self.size)))
            .when(!icon_only, |b| b.px(m.pad))
            .when(touch, |b| {
                b.min_h(min_touch).when(icon_only, |b| b.min_w(min_touch))
            })
            .border_1()
            .border_color(p.border)
            .bg(p.fill)
            .text_color(p.text)
            .text_size(m.text)
            .font_weight(FontWeight::MEDIUM)
            .whitespace_nowrap()
            .overflow_hidden()
            .map(|b| match joined {
                Joined::Alone => b.rounded(radius),
                Joined::First => b.rounded_l(radius),
                Joined::Middle => b,
                Joined::Last => b.rounded_r(radius),
            })
            .when(matches!(joined, Joined::Middle | Joined::Last), |b| {
                // Neighbours share one border line.
                b.ml(px(-1.))
            })
            .selected(self.selected)
            .disabled(self.disabled)
            .when(self.disabled, |b| b.opacity(0.5));
        if !self.disabled {
            let hover = p.hover;
            press = press
                .hover(move |s| {
                    let s = s.bg(hover);
                    if link { s.underline() } else { s }
                })
                // A press nudges it down a pixel, as the web's `active:translate-y-px`.
                .relative()
                .active(|s| s.top(px(1.)));
        }
        if let Some(label) = label {
            press = press.accessibility_label(label);
        }
        if let Some(f) = self.on_click {
            press = press.on_click(move |e, w, cx| f(e, w, cx));
        }
        if let Some(expanded) = self.expanded {
            press = press.aria_expanded(expanded);
        }
        press.style().refine(&self.style);
        press
            .when_some(self.icon, |b, icon| b.child(icon.size(m.icon)))
            .when_some(self.label, |b, label| {
                b.child(
                    // Clipped to its slot, with three pixels' room either side (taken from the
                    // button's padding) for ink that reaches past its advance: an emoji, the
                    // ellipsis. Squeezed narrower than the ellipsis, it cuts it, as browsers do.
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .px(css(3.))
                        .mx(css(-3.))
                        .text_ellipsis()
                        .child(label),
                )
            })
            .line_height(relative(1.))
    }
}

type OnGroupClick = Rc<dyn Fn(&Vec<usize>, &mut Window, &mut App)>;

/// Buttons joined edge to edge, sharing a variant and a size: a segmented control. Its
/// [`on_click`](ButtonGroup::on_click) is told which button was pressed, by its place.
#[derive(IntoElement)]
pub struct ButtonGroup {
    id: ElementId,
    buttons: Vec<Button>,
    variant: Option<ButtonVariant>,
    size: Option<ControlSize>,
    disabled: bool,
    on_click: Option<OnGroupClick>,
    style: StyleRefinement,
}

impl ButtonGroup {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            buttons: Vec::new(),
            variant: None,
            size: None,
            disabled: false,
            on_click: None,
            style: StyleRefinement::default(),
        }
    }

    pub fn child(mut self, button: Button) -> Self {
        self.buttons.push(button);
        self
    }

    pub fn children(mut self, buttons: impl IntoIterator<Item = Button>) -> Self {
        self.buttons.extend(buttons);
        self
    }

    /// Called with the place of the button pressed (in a one-element list, as GPUI Kit's was).
    pub fn on_click(mut self, f: impl Fn(&Vec<usize>, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }
}

impl ButtonVariants for ButtonGroup {
    fn with_variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = Some(variant);
        self
    }
}

impl Sizable for ButtonGroup {
    fn with_size(mut self, size: ControlSize) -> Self {
        self.size = Some(size);
        self
    }
}

impl Disableable for ButtonGroup {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Styled for ButtonGroup {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for ButtonGroup {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let n = self.buttons.len();
        let on_click = self.on_click;
        // It never runs out of its space: its buttons give way first, and what can't fit even
        // then (their padding) is clipped at its edge.
        let mut row = div()
            .id(self.id)
            .flex()
            .flex_shrink_1()
            .items_center()
            .min_w_0()
            .max_w_full()
            .overflow_hidden();
        row.style().refine(&self.style);
        row.children(self.buttons.into_iter().enumerate().map(|(i, mut b)| {
            if let Some(v) = self.variant {
                b.variant = v;
            }
            if let Some(s) = self.size {
                b.size = s;
            }
            b.disabled |= self.disabled;
            b.joined = match (i, n) {
                (_, 1) => Joined::Alone,
                (0, _) => Joined::First,
                (i, n) if i + 1 == n => Joined::Last,
                _ => Joined::Middle,
            };
            if let Some(f) = on_click.clone() {
                let own = b.on_click.take();
                b = b.on_click(move |e, w, cx| {
                    if let Some(own) = &own {
                        own(e, w, cx);
                    }
                    f(&vec![i], w, cx);
                });
            }
            b
        }))
    }
}
