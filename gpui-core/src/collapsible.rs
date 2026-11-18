//! A collapsible: a trigger that shows or hides the content under it, as shadcn/ui's (Radix's)
//! does. It's open or closed: the host can keep the state ([`open`](Collapsible::open) and
//! [`on_toggle`](Collapsible::on_toggle)), or leave it to the collapsible, which remembers it
//! under its id ([`default_open`](Collapsible::default_open)).
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::Collapsible;
//! # struct Order;
//! # impl Render for Order {
//! fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
//!     Collapsible::new("details")
//!         .label("Order details")
//!         .child("Three items, shipped on Monday.")
//! }
//! # }
//! ```
//!
//! The trigger is a [`Press`]: Tab reaches it, Enter, Space, a click or a tap toggles it, and
//! assistive technology hears whether it's expanded. Hidden content isn't drawn at all. It
//! doesn't animate: the content appears and goes in one frame.

use std::rc::Rc;

use gpui::{
    AnyElement, App, ElementId, FontWeight, InteractiveElement as _, IntoElement, ParentElement,
    Refineable as _, Rems, RenderOnce, SharedString, StatefulInteractiveElement as _,
    StyleRefinement, Styled, Window, div, prelude::FluentBuilder as _, rems,
};

use crate::base::{ActiveTheme as _, Disableable, Icon, IconName, Press};

/// A size in the web's CSS pixels, as rems of its 15 pixel text.
fn css(v: f32) -> Rems {
    rems(v / 15.)
}

type OnToggle = Rc<dyn Fn(&bool, &mut Window, &mut App)>;

enum Trigger {
    None,
    Label(SharedString),
    Press(Box<Press>),
    With(Box<dyn FnOnce(bool) -> Press>),
}

/// A trigger and the content it shows or hides. Its children are the content.
#[derive(IntoElement)]
pub struct Collapsible {
    id: ElementId,
    trigger: Trigger,
    open: Option<bool>,
    default_open: bool,
    on_toggle: Option<OnToggle>,
    disabled: bool,
    touch: bool,
    content: Vec<AnyElement>,
    style: StyleRefinement,
}

impl Collapsible {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            trigger: Trigger::None,
            open: None,
            default_open: false,
            on_toggle: None,
            disabled: false,
            touch: false,
            content: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    /// A trigger drawn for you: the label, and a chevron pointing down when closed and up when
    /// open, across the collapsible's width.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.trigger = Trigger::Label(label.into());
        self
    }

    /// The host's own trigger. The collapsible handles its presses (any `on_click` it had is
    /// replaced) and tells assistive technology whether it's expanded.
    pub fn trigger(mut self, trigger: Press) -> Self {
        self.trigger = Trigger::Press(Box::new(trigger));
        self
    }

    /// The host's own trigger, made for the state it's in (open or not): for a trigger that
    /// changes with it when the collapsible keeps the state.
    pub fn trigger_with(mut self, trigger: impl FnOnce(bool) -> Press + 'static) -> Self {
        self.trigger = Trigger::With(Box::new(trigger));
        self
    }

    /// Whether it's open, for a host that keeps the state: it changes only when the host
    /// passes a new one (after [`on_toggle`](Self::on_toggle)).
    pub fn open(mut self, open: bool) -> Self {
        self.open = Some(open);
        self
    }

    /// Whether it starts open, when the collapsible keeps the state itself (no
    /// [`open`](Self::open)).
    pub fn default_open(mut self, open: bool) -> Self {
        self.default_open = open;
        self
    }

    /// Told the state it should take, on a press, Enter or Space on the trigger. A host that
    /// keeps the state passes it back with [`open`](Self::open); otherwise it's for
    /// information, and the collapsible has already changed.
    pub fn on_toggle(mut self, f: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_toggle = Some(Rc::new(f));
        self
    }

    /// A [`label`](Self::label) trigger at least 44 points tall, for touch.
    pub fn touch(mut self, touch: bool) -> Self {
        self.touch = touch;
        self
    }
}

impl Disableable for Collapsible {
    /// The trigger ignores presses and takes no focus; the content stays as it is.
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Styled for Collapsible {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl ParentElement for Collapsible {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.content.extend(elements)
    }
}

/// What the collapsible adds to its trigger: whether it's expanded, and disabled.
fn wire(trigger: Press, open: bool, disabled: bool) -> Press {
    trigger
        .aria_expanded(open)
        .when(disabled, |p| p.disabled(true))
}

impl RenderOnce for Collapsible {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        // The state, when the collapsible keeps it.
        let kept = self.open.is_none().then(|| {
            let default = self.default_open;
            window.use_keyed_state(
                ElementId::NamedChild(
                    std::sync::Arc::new(self.id.clone()),
                    "collapsible-open".into(),
                ),
                cx,
                move |_, _| default,
            )
        });
        let open = match (&kept, self.open) {
            (_, Some(open)) => open,
            (Some(kept), None) => *kept.read(cx),
            (None, None) => false,
        };
        let on_toggle = self.on_toggle.clone();
        let toggle = move |_: &gpui::ClickEvent, window: &mut Window, cx: &mut App| {
            if let Some(kept) = &kept {
                kept.update(cx, |o, cx| {
                    *o = !open;
                    cx.notify();
                });
            }
            if let Some(f) = &on_toggle {
                f(&!open, window, cx);
            }
        };
        let trigger_id =
            ElementId::NamedChild(std::sync::Arc::new(self.id.clone()), "trigger".into());
        let trigger = match self.trigger {
            Trigger::None => None,
            Trigger::Press(p) => Some(*p),
            Trigger::With(f) => Some(f(open)),
            Trigger::Label(label) => Some(
                Press::new(trigger_id)
                    // Its words are its name: the label text alone isn't announced as one.
                    .accessibility_label(label.clone())
                    .debug_selector(|| "collapsible-trigger".into())
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(css(8.))
                    .w_full()
                    .min_w_0()
                    .px(css(8.))
                    .py(css(6.))
                    .when(self.touch, |p| p.min_h(theme.touch_target))
                    .rounded(theme.radius_md())
                    .text_size(theme.text.sm)
                    .font_weight(FontWeight::MEDIUM)
                    .when(!self.disabled, |p| p.hover(|s| s.bg(theme.accent)))
                    .when(self.disabled, |p| p.opacity(0.5))
                    .child(div().min_w_0().truncate().child(label))
                    .child(
                        Icon::new(if open {
                            IconName::ChevronUp
                        } else {
                            IconName::ChevronDown
                        })
                        .size(css(16.))
                        .text_color(theme.muted_foreground),
                    ),
            ),
        };
        let disabled = self.disabled;
        let trigger = trigger.map(|p| wire(p, open, disabled).on_click(toggle));
        let mut el = div()
            .id(self.id)
            .flex()
            .flex_col()
            .min_w_0()
            .children(trigger)
            .when(open, |d| {
                d.child(
                    div()
                        .debug_selector(|| "collapsible-content".into())
                        .min_w_0()
                        .children(self.content),
                )
            });
        el.style().refine(&self.style);
        el
    }
}

#[cfg(test)]
mod tests {
    use gpui::{Element as _, Role, TestAppContext, accesskit};

    use super::*;

    #[gpui::test]
    fn the_trigger_says_whether_it_is_expanded(cx: &mut TestAppContext) {
        let vcx = cx.add_empty_window();
        let node = |open: bool, disabled: bool, vcx: &mut gpui::VisualTestContext| {
            vcx.update(|window, cx| {
                let el = wire(
                    Press::new("t").track_focus(&cx.focus_handle()),
                    open,
                    disabled,
                )
                .render(window, cx)
                .into_element();
                let mut node = accesskit::Node::new(el.a11y_role().unwrap());
                el.write_a11y_info(&mut node);
                node
            })
        };
        let open = node(true, false, vcx);
        assert_eq!(open.role(), Role::Button);
        assert_eq!(open.is_expanded(), Some(true));
        assert_eq!(node(false, false, vcx).is_expanded(), Some(false));
        assert!(node(false, true, vcx).is_disabled());
    }
}
