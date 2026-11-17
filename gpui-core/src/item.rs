//! Items, as shadcn/ui draws them (its `item.tsx`): a row with media (an icon or an image),
//! a title, a description and actions, in default, outline and muted variants and two sizes;
//! [`ItemGroup`] stacks them as a list, with [`ItemSeparator`] between them.
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::{Button, ButtonVariants as _, IconName, Item, ItemGroup, Sizable as _};
//! # struct Inbox;
//! # impl Render for Inbox {
//! fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
//!     ItemGroup::new("people").child(
//!         Item::new("ana")
//!             .outline()
//!             .icon(IconName::User)
//!             .title("Ana Ruiz")
//!             .description("Asked to move Thursday's review to Friday.")
//!             .action(Button::new("reply").label("Reply").outline().small())
//!             .on_click(|_, _, _| {}),
//!     )
//! }
//! # }
//! ```
//!
//! An item with [`on_click`](Item::on_click) is a [`Press`]: Tab reaches it, Enter, Space, a
//! click or a tap presses it, the pointer lights it up, and in the touch size it's at least
//! 44 points tall. Its title is cut short with an ellipsis and its description at two lines.

use std::rc::Rc;

use gpui::{
    AnyElement, App, ClickEvent, ElementId, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement, Refineable as _, Rems, RenderOnce, Role, SharedString,
    StatefulInteractiveElement as _, StyleRefinement, Styled, Window, div,
    prelude::FluentBuilder as _, relative, rems,
};

use crate::icon::{Icon, IconName};
use crate::press::Press;
use crate::size::{ControlSize, Disableable, Sizable};
use crate::theme::ActiveTheme as _;

/// A size in the web's CSS pixels, as rems of its 15 pixel text.
fn css(v: f32) -> Rems {
    rems(v / 15.)
}

type OnClick = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// How an item looks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ItemVariant {
    /// No fill and no border.
    #[default]
    Default,
    /// A border around it.
    Outline,
    /// A quiet fill.
    Muted,
}

enum Media {
    Icon(IconName),
    Element(AnyElement),
}

/// A row: media, a title and a description, and actions at the end. Its children go under the
/// description.
#[derive(IntoElement)]
pub struct Item {
    id: ElementId,
    variant: ItemVariant,
    size: ControlSize,
    media: Option<Media>,
    title: Option<SharedString>,
    description: Option<SharedString>,
    actions: Vec<AnyElement>,
    children: Vec<AnyElement>,
    on_click: Option<OnClick>,
    disabled: bool,
    style: StyleRefinement,
}

impl Item {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            variant: ItemVariant::Default,
            size: ControlSize::Medium,
            media: None,
            title: None,
            description: None,
            actions: Vec::new(),
            children: Vec::new(),
            on_click: None,
            disabled: false,
            style: StyleRefinement::default(),
        }
    }

    pub fn with_variant(mut self, variant: ItemVariant) -> Self {
        self.variant = variant;
        self
    }

    /// A border around it.
    pub fn outline(self) -> Self {
        self.with_variant(ItemVariant::Outline)
    }

    /// A quiet fill.
    pub fn muted(self) -> Self {
        self.with_variant(ItemVariant::Muted)
    }

    /// An icon at the start, in a small bordered square on the muted fill.
    pub fn icon(mut self, icon: IconName) -> Self {
        self.media = Some(Media::Icon(icon));
        self
    }

    /// Something of the host's at the start, as it is: an avatar, an image it has loaded.
    pub fn media(mut self, media: impl IntoElement) -> Self {
        self.media = Some(Media::Element(media.into_any_element()));
        self
    }

    /// The title, on one line, cut short with an ellipsis.
    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Quieter text under the title, cut short after two lines.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Adds something at the end: a button, a badge. Its own presses stay its own.
    pub fn action(mut self, action: impl IntoElement) -> Self {
        self.actions.push(action.into_any_element());
        self
    }

    /// Makes the whole row pressable: called on a click, a tap, or Enter or Space while it has
    /// the focus.
    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }
}

impl Sizable for Item {
    /// [`ControlSize::Small`] and smaller are the small item; [`ControlSize::Touch`] is the
    /// default one at least 44 points tall.
    fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }
}

impl Disableable for Item {
    /// A pressable item drawn dimmed, taking no focus and ignoring presses.
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Styled for Item {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for Item {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

/// What an item's row is to assistive technology when it can be pressed: a button named by its
/// title and described by its description.
fn pressable(
    press: Press,
    title: Option<SharedString>,
    description: Option<SharedString>,
    disabled: bool,
) -> Press {
    press
        .when_some(title, |p, t| p.accessibility_label(t))
        .when_some(description, |p, d| p.aria_description(d))
        .disabled(disabled)
}

impl RenderOnce for Item {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let small = matches!(self.size, ControlSize::XSmall | ControlSize::Small);
        let touch = self.size == ControlSize::Touch;
        let has_description = self.description.is_some();
        let media = self.media.map(|m| {
            let el = div()
                .debug_selector(|| "item-media".into())
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                // Beside a description it sits at the top, level with the title.
                .when(has_description, |d| d.self_start().mt(css(2.)));
            match m {
                Media::Icon(icon) => el
                    .size(css(32.))
                    .rounded(theme.radius_sm())
                    .border_1()
                    .border_color(theme.border)
                    .bg(theme.muted)
                    .child(Icon::new(icon).size(css(16.))),
                Media::Element(e) => el.child(e),
            }
        });
        let content =
            (self.title.is_some() || has_description || !self.children.is_empty()).then(|| {
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(css(4.))
                    .when_some(self.title.clone(), |d, title| {
                        d.child(
                            div()
                                .debug_selector(|| "item-title".into())
                                .min_w_0()
                                .line_height(relative(1.375))
                                .font_weight(FontWeight::MEDIUM)
                                .truncate()
                                .child(title),
                        )
                    })
                    .when_some(self.description.clone(), |d, description| {
                        d.child(
                            div()
                                .debug_selector(|| "item-description".into())
                                .min_w_0()
                                .line_height(relative(1.5))
                                .text_color(theme.muted_foreground)
                                .line_clamp(2)
                                .text_ellipsis()
                                .child(description),
                        )
                    })
                    .children(self.children)
            });
        let actions = (!self.actions.is_empty()).then(|| {
            div()
                .debug_selector(|| "item-actions".into())
                .flex_none()
                .flex()
                .items_center()
                .gap(css(8.))
                .children(self.actions)
        });
        let fill = match self.variant {
            ItemVariant::Muted => theme.muted.opacity(0.5),
            _ => gpui::transparent_black(),
        };
        let border = match self.variant {
            ItemVariant::Outline => theme.border,
            _ => gpui::transparent_black(),
        };
        let look = Look {
            small,
            radius: theme.radius_md(),
            border,
            fill,
            text: theme.text.sm,
        };
        let parts = media
            .map(IntoElement::into_any_element)
            .into_iter()
            .chain(content.map(IntoElement::into_any_element))
            .chain(actions.map(IntoElement::into_any_element));
        if let Some(f) = self.on_click {
            let disabled = self.disabled;
            let mut press = look
                .dress(pressable(
                    Press::new(self.id),
                    self.title,
                    self.description,
                    disabled,
                ))
                .when(touch, |p| p.min_h(theme.touch_target))
                .when(!disabled, |p| p.hover(|s| s.bg(theme.accent)))
                .when(disabled, |p| p.opacity(0.5))
                .on_click(move |e, w, cx| f(e, w, cx))
                .children(parts);
            press.style().refine(&self.style);
            press.into_any_element()
        } else {
            let mut el = look.dress(div().id(self.id)).children(parts);
            el.style().refine(&self.style);
            el.into_any_element()
        }
    }
}

/// How an item's row is drawn, pressable or not.
struct Look {
    small: bool,
    radius: Rems,
    border: gpui::Hsla,
    fill: gpui::Hsla,
    text: Rems,
}

impl Look {
    fn dress<E: Styled + gpui::InteractiveElement + IntoElement>(&self, el: E) -> E {
        el.debug_selector(|| "item".into())
            .flex()
            .items_center()
            .min_w_0()
            .when(self.small, |d| d.py(css(12.)).px(css(16.)).gap(css(10.)))
            .when(!self.small, |d| d.p(css(16.)).gap(css(16.)))
            .rounded(self.radius)
            .border_1()
            .border_color(self.border)
            .bg(self.fill)
            .text_size(self.text)
    }
}

/// Items stacked as a list (a list to assistive technology). Its children are the items, and
/// any [`ItemSeparator`]s between them.
#[derive(IntoElement)]
pub struct ItemGroup {
    id: ElementId,
    label: Option<SharedString>,
    children: Vec<AnyElement>,
    style: StyleRefinement,
}

impl ItemGroup {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            label: None,
            children: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    /// What the list is, to assistive technology ("People").
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl Styled for ItemGroup {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for ItemGroup {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

impl RenderOnce for ItemGroup {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let mut el = div()
            .id(self.id)
            .role(Role::List)
            .when_some(self.label, |d, l| d.aria_label(l))
            .flex()
            .flex_col()
            .min_w_0()
            .children(self.children);
        el.style().refine(&self.style);
        el
    }
}

/// A rule between the items of an [`ItemGroup`], in the border colour.
#[derive(IntoElement, Default)]
pub struct ItemSeparator {
    style: StyleRefinement,
}

impl ItemSeparator {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Styled for ItemSeparator {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for ItemSeparator {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let mut el = div()
            .debug_selector(|| "item-separator".into())
            .flex_none()
            .w_full()
            .h(gpui::px(1.))
            .bg(cx.theme().border);
        el.style().refine(&self.style);
        el
    }
}

#[cfg(test)]
mod tests {
    use gpui::{TestAppContext, accesskit};

    use super::*;

    fn node(el: &impl gpui::Element) -> accesskit::Node {
        let mut node = accesskit::Node::new(el.a11y_role().expect("a role"));
        el.write_a11y_info(&mut node);
        node
    }

    #[gpui::test]
    fn a_pressable_item_is_a_button_named_by_its_title(cx: &mut TestAppContext) {
        let vcx = cx.add_empty_window();
        vcx.update(|window, cx| {
            let el = pressable(
                Press::new("i").track_focus(&cx.focus_handle()),
                Some("Ana Ruiz".into()),
                Some("Asked to move the review.".into()),
                true,
            )
            .render(window, cx)
            .into_element();
            let n = node(&el);
            assert_eq!(n.role(), Role::Button);
            assert_eq!(n.label(), Some("Ana Ruiz"));
            assert_eq!(n.description(), Some("Asked to move the review."));
            assert!(n.is_disabled());
        });
    }

    #[gpui::test]
    fn a_group_is_a_list(cx: &mut TestAppContext) {
        let vcx = cx.add_empty_window();
        vcx.update(|window, cx| {
            let el = ItemGroup::new("g")
                .accessibility_label("People")
                .render(window, cx)
                .into_element();
            let n = node(&el);
            assert_eq!(n.role(), Role::List);
            assert_eq!(n.label(), Some("People"));
        });
    }
}
