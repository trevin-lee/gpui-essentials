//! Something to press, with no look of its own: the host styles it. It takes the focus with
//! Tab, shows the theme's focus ring when the keyboard put the focus there, presses with a
//! click, a tap, Enter or Space, and tells assistive technology what it is.

use std::rc::Rc;

use gpui::{
    AnyElement, App, BoxShadow, ClickEvent, Div, ElementId, FocusHandle, Hsla, InteractiveElement,
    Interactivity, IntoElement, MouseButton, ParentElement, RenderOnce, Role, SharedString,
    Stateful, StatefulInteractiveElement, StyleRefinement, Styled, Toggled, Window, div, point,
    prelude::FluentBuilder as _, px,
};

use crate::size::{Disableable, Selectable};
use crate::theme::ActiveTheme as _;

type OnClick = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// The focus ring the theme draws around something focused from the keyboard: the ring colour
/// at half strength, three pixels wide outside the element (the web's `ring-3 ring-ring/50`),
/// so nothing moves when it shows.
pub fn focus_ring(cx: &App) -> BoxShadow {
    BoxShadow {
        color: cx.theme().focus_ring(),
        offset: point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(3.),
        inset: false,
    }
}

/// Something to press. Style it like a `div` (it is one), give it children, and handle
/// [`on_click`](Press::on_click). When [`disabled`](Disableable::disabled) it takes no focus
/// and ignores presses; how it looks then is the host's to say.
#[derive(IntoElement)]
pub struct Press {
    id: ElementId,
    base: Stateful<Div>,
    role: Role,
    label: Option<SharedString>,
    disabled: bool,
    selected: Option<bool>,
    on_click: Option<OnClick>,
    focus: Option<FocusHandle>,
    tab_stop: bool,
    ring: bool,
    ring_color: Option<Hsla>,
}

impl Press {
    pub fn new(id: impl Into<ElementId>) -> Self {
        let id = id.into();
        Self {
            base: div().id(id.clone()),
            id,
            role: Role::Button,
            label: None,
            disabled: false,
            selected: None,
            on_click: None,
            focus: None,
            tab_stop: true,
            ring: true,
            ring_color: None,
        }
    }

    /// What it is to assistive technology: a button unless told otherwise (a check box, a
    /// radio button, a tab, a link…).
    pub fn role(mut self, role: Role) -> Self {
        self.role = role;
        self
    }

    /// What it's called to assistive technology, when its children don't say (an icon).
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Called on a click, a tap, or Enter or Space while it has the focus.
    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }

    /// Uses this focus handle instead of its own, so the host can focus it.
    pub fn track_focus(mut self, focus: &FocusHandle) -> Self {
        self.focus = Some(focus.clone());
        self
    }

    /// Whether Tab stops on it (it does unless told not to).
    pub fn tab_stop(mut self, tab_stop: bool) -> Self {
        self.tab_stop = tab_stop;
        self
    }

    /// Whether it draws the theme's focus ring (it does unless told not to, for a host that
    /// draws its own).
    pub fn focus_ring(mut self, ring: bool) -> Self {
        self.ring = ring;
        self
    }

    /// Draws the focus ring in this colour instead of the theme's (for a host with a palette of
    /// its own). It's drawn as given: pass it at the strength it should show.
    pub fn focus_ring_color(mut self, color: impl Into<Hsla>) -> Self {
        self.ring_color = Some(color.into());
        self
    }
}

impl Disableable for Press {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Selectable for Press {
    fn selected(mut self, selected: bool) -> Self {
        self.selected = Some(selected);
        self
    }

    fn is_selected(&self) -> bool {
        self.selected.unwrap_or(false)
    }
}

impl Styled for Press {
    fn style(&mut self) -> &mut StyleRefinement {
        self.base.style()
    }
}

impl ParentElement for Press {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.base.extend(elements)
    }
}

impl InteractiveElement for Press {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.base.interactivity()
    }
}

impl StatefulInteractiveElement for Press {}

impl RenderOnce for Press {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let focus = self.focus.clone().unwrap_or_else(|| {
            window
                .use_keyed_state(self.id.clone(), cx, |_, cx| cx.focus_handle())
                .read(cx)
                .clone()
        });
        let ring = match self.ring_color {
            Some(color) => BoxShadow {
                color,
                ..focus_ring(cx)
            },
            None => focus_ring(cx),
        };
        let toggles = matches!(self.role, Role::CheckBox | Role::Switch | Role::RadioButton);
        let disabled = self.disabled;
        let mut el = self.base;
        // The host's cursor, if it chose one; otherwise a pointing hand while it can be pressed.
        let host_cursor = el.style().mouse_cursor.is_some();
        el = el.role(self.role);
        if let Some(label) = self.label {
            el = el.aria_label(label);
        }
        if let Some(selected) = self.selected {
            el = if toggles {
                el.aria_toggled(if selected {
                    Toggled::True
                } else {
                    Toggled::False
                })
            } else {
                el.aria_selected(selected)
            };
        }
        let el = if disabled {
            // No presses, and a press doesn't fall through to what's behind it. It isn't a tab
            // stop; but if it had the focus when it was disabled (a keyboard press on the last
            // Next), it keeps it until Tab moves on from here, as a browser's button does.
            let focused = focus.is_focused(window);
            el.when(focused, |el| el.track_focus(&focus.clone().tab_stop(false)))
                .block_mouse_except_scroll()
                .on_mouse_down(MouseButton::Left, |_, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                })
        } else {
            el.track_focus(&focus.tab_stop(self.tab_stop))
                .when(!host_cursor, |el| el.cursor_pointer())
                .when(self.ring, |el| {
                    el.focus_visible(move |s| s.shadow(vec![ring]))
                })
                .when_some(self.on_click, |el, f| {
                    el.on_click(move |event, window, cx| {
                        cx.stop_propagation();
                        f(event, window, cx)
                    })
                })
        };
        crate::a11y::Disabled::new(el, disabled)
    }
}
