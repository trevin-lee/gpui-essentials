//! A check box, as shadcn draws it (its `checkbox.tsx`): a small rounded square, filled with the
//! primary colour and a check when it's on, or a dash when it's partly on (indeterminate), with
//! an optional label beside it that a press on toggles it too.
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::Checkbox;
//! # struct Settings { terms: bool }
//! # impl Render for Settings {
//! fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
//!     Checkbox::new("terms")
//!         .label("Accept terms and conditions")
//!         .checked(self.terms)
//!         .on_click(cx.listener(|this, checked: &bool, _, cx| {
//!             this.terms = *checked;
//!             cx.notify();
//!         }))
//! }
//! # }
//! ```

use std::rc::Rc;

use gpui::{
    App, ElementId, FocusHandle, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, Rems, RenderOnce, Role, SharedString, StatefulInteractiveElement as _,
    StyleRefinement, Styled, Toggled, Window, div, prelude::FluentBuilder as _, rems,
};

use crate::base::{
    ActiveTheme as _, ControlSize, Disableable, Icon, IconName, Press, Sizable, focus_ring,
};

type OnClick = Rc<dyn Fn(&bool, &mut Window, &mut App)>;

fn css(v: f32) -> Rems {
    rems(v / 15.)
}

/// A check box, with a label beside it if given. Its [`on_click`](Checkbox::on_click) is told
/// the state it should take: the opposite of the one it shows, or on from indeterminate. The
/// host keeps the state and passes it back with [`checked`](Checkbox::checked).
#[derive(IntoElement)]
pub struct Checkbox {
    id: ElementId,
    checked: bool,
    indeterminate: bool,
    invalid: bool,
    disabled: bool,
    size: ControlSize,
    label: Option<SharedString>,
    accessibility_label: Option<SharedString>,
    on_click: Option<OnClick>,
    style: StyleRefinement,
}

impl Checkbox {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            checked: false,
            indeterminate: false,
            invalid: false,
            disabled: false,
            size: ControlSize::Medium,
            label: None,
            accessibility_label: None,
            on_click: None,
            style: StyleRefinement::default(),
        }
    }

    /// Whether it's on.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    /// Partly on (a "select all" over a list where only some are): drawn with a dash and
    /// announced as mixed, whatever [`checked`](Self::checked) says. A press turns it on.
    pub fn indeterminate(mut self, indeterminate: bool) -> Self {
        self.indeterminate = indeterminate;
        self
    }

    /// Text beside the box. A press on it toggles the box, and it names the box to assistive
    /// technology. It wraps where it's too long for its line.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// What it's called to assistive technology, when there's no label or the label doesn't say.
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.accessibility_label = Some(label.into());
        self
    }

    /// Drawn with the destructive colour's border, as a field that failed its check.
    pub fn invalid(mut self, invalid: bool) -> Self {
        self.invalid = invalid;
        self
    }

    /// Called with the state it should take, on a click or a tap on the box or its label, or
    /// Space while it has the focus.
    pub fn on_click(mut self, f: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }
}

impl Disableable for Checkbox {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Sizable for Checkbox {
    /// Only [`ControlSize::Touch`] changes it: the box stays 16 points, but a finger's 44 by
    /// 44 around it (and its label) presses it.
    fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }
}

impl Styled for Checkbox {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Checkbox {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let dark = theme.mode.is_dark();
        let on = self.checked || self.indeterminate;
        let touch = self.size == ControlSize::Touch;
        // Its own focus handle, so the box can draw the ring while the whole row has the focus.
        let focus = window
            .use_keyed_state(
                ElementId::NamedChild(std::sync::Arc::new(self.id.clone()), "focus".into()),
                cx,
                |_, cx| cx.focus_handle(),
            )
            .read(cx)
            .clone();
        // Focus rings show only for the keyboard, as `focus-visible`; GPUI redraws the window
        // when the focus or the kind of input changes, so this is current.
        let ring = !touch && focus_visible(&focus, window);

        let border = if self.invalid && !on {
            if dark {
                theme.destructive.opacity(0.5)
            } else {
                theme.destructive
            }
        } else if on {
            theme.primary
        } else if ring {
            theme.ring
        } else {
            theme.input
        };
        let mut shadows = Vec::new();
        if ring {
            shadows.push(focus_ring(cx));
        } else if self.invalid {
            let mut r = focus_ring(cx);
            r.color = theme.destructive.opacity(if dark { 0.4 } else { 0.2 });
            shadows.push(r);
        }
        let mark = if self.indeterminate {
            Some(IconName::Minus)
        } else if self.checked {
            Some(IconName::Check)
        } else {
            None
        };
        let check_box = div()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .size(css(16.))
            .rounded(css(4.))
            .border_1()
            .border_color(border)
            .when(on, |d| d.bg(theme.primary))
            .when(!on && dark, |d| d.bg(theme.control.opacity(0.3)))
            // The ring (or an invalid box's red one) drawn just outside it, not under it.
            .relative()
            .children({
                let radius = css(4.).to_pixels(window.rem_size());
                shadows
                    .into_iter()
                    .map(move |s| crate::base::ring_outside(s, radius, gpui::px(1.)))
            })
            .when_some(mark, |d, m| {
                d.child(Icon::new(m).size(css(14.)).color(theme.primary_foreground))
            });

        let toggled = if self.indeterminate {
            Toggled::Mixed
        } else if self.checked {
            Toggled::True
        } else {
            Toggled::False
        };
        let has_label = self.label.is_some();
        let name = self.accessibility_label.or_else(|| self.label.clone());
        let on_click = self.on_click;
        let next = self.indeterminate || !self.checked;
        let mut press = Press::new(self.id.clone())
            .role(Role::CheckBox)
            .aria_toggled(toggled)
            .track_focus(&focus)
            .disabled(self.disabled)
            .focus_ring(false)
            .when_some(name, |p, n| p.accessibility_label(n))
            .when_some(on_click, |p, f| p.on_click(move |_, w, cx| f(&next, w, cx)))
            .debug_selector({
                let id = self.id.clone();
                move || format!("checkbox-{id}")
            })
            .flex()
            .items_start()
            .gap(css(8.))
            .min_w_0()
            .max_w_full()
            .when(self.disabled, |p| p.opacity(0.5))
            // On a touch screen it takes presses 44 points tall, and as wide with no label.
            .when(touch, |p| {
                p.min_h(theme.touch_target)
                    .items_center()
                    .when(!has_label, |p| p.min_w(theme.touch_target).justify_center())
            })
            .child(check_box)
            .when_some(self.label, |p, label| {
                p.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(theme.text.sm)
                        .line_height(css(16.))
                        .font_weight(FontWeight::MEDIUM)
                        .debug_selector({
                            let id = self.id.clone();
                            move || format!("checkbox-label-{id}")
                        })
                        .child(label),
                )
            });
        gpui::Refineable::refine(press.style(), &self.style);
        // Drawn here rather than as a component of its own, so the element this gives is the
        // check box itself (its role and state), for a host that wraps it.
        RenderOnce::render(press, window, cx)
    }
}

/// Whether `focus` has the focus and the keyboard put it there (the web's `:focus-visible`).
pub(crate) fn focus_visible(focus: &FocusHandle, window: &Window) -> bool {
    focus.is_focused(window) && window.last_input_was_keyboard()
}
