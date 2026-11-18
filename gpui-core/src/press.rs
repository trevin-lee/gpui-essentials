//! Something to press, with no look of its own: the host styles it. It takes the focus with
//! Tab, shows the theme's focus ring when the keyboard put the focus there, presses with a
//! click, a tap, Enter or Space, and tells assistive technology what it is.

use std::rc::Rc;

use gpui::{
    AnyElement, App, BoxShadow, ClickEvent, Div, ElementId, FocusHandle, Hsla, InteractiveElement,
    Interactivity, IntoElement, MouseButton, ParentElement, Pixels, RenderOnce, Role, SharedString,
    Stateful, StatefulInteractiveElement, StyleRefinement, Styled, Toggled, Window, div, point,
    prelude::FluentBuilder as _, px, relative,
};

use crate::size::{Disableable, Selectable};
use crate::theme::ActiveTheme as _;

type OnClick = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// The focus ring the theme draws around something focused from the keyboard: the ring colour
/// at 60% (3:1 against the page in both themes; the web's `ring-ring/50` isn't in the dark),
/// three pixels wide outside the element, as the web's `ring-3`, so nothing moves when it shows.
pub fn focus_ring(cx: &App) -> BoxShadow {
    BoxShadow {
        color: cx.theme().focus_ring(),
        offset: point(px(0.), px(0.)),
        blur_radius: px(0.),
        spread_radius: px(3.),
        inset: false,
    }
}

/// A ring just outside its parent's edge, drawn as an element of its own: `ring`'s colour, as
/// wide as its spread, round a parent whose corners are `radius` and whose border is `border`
/// wide (a child is placed from inside the border). Add it as the parent's child while the
/// ring shows (the parent is then `relative`, and doesn't clip its children).
///
/// GPUI draws an outer shadow under its element as well as round it, so a ring drawn as the
/// element's own shadow shows through a see-through fill (an outline or ghost button turns
/// grey). This is an inset shadow on a box that much bigger than the parent all round: a ring
/// outside the parent, with nothing of it under the parent.
pub fn ring_outside(ring: BoxShadow, radius: Pixels, border: Pixels) -> Div {
    let w = ring.spread_radius;
    let out = -(w + border);
    div()
        .absolute()
        .top(out)
        .left(out)
        .right(out)
        .bottom(out)
        .rounded(radius + w)
        .shadow(vec![BoxShadow {
            inset: true,
            offset: point(px(0.), px(0.)),
            ..ring
        }])
}

/// `ring` drawn inside its element's edge instead of outside: for a container that clips what's
/// in it (a scroll area, a table's body), where a ring outside would be cut off. An inset
/// shadow is never drawn under the element.
pub fn ring_inside(ring: BoxShadow) -> BoxShadow {
    BoxShadow {
        inset: true,
        offset: point(px(0.), px(0.)),
        ..ring
    }
}

/// Whether `focus` has the focus and the keyboard put it there (the web's `:focus-visible`).
/// GPUI draws the window again when either changes, so what's drawn from this stays right.
pub fn focus_visible(focus: &FocusHandle, window: &Window) -> bool {
    focus.is_focused(window) && window.last_input_was_keyboard()
}

/// Makes `el` the containing block for an absolutely placed child (a [`ring_outside`]), unless
/// it already is one.
pub(crate) fn hold_ring<E: Styled>(mut el: E) -> E {
    if el.style().position.is_none() {
        el = el.relative();
    }
    el
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
    min_hit: Option<Pixels>,
    focus_on_press: bool,
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
            min_hit: None,
            focus_on_press: true,
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

    /// Takes presses in an area at least `side` each way, centred on it, without changing its
    /// layout: a finger's 44 points around a small control. Each axis grows only if it's
    /// shorter than `side`, across the whole control: a 138 by 40 button takes presses 138 by
    /// 44. Where the area reaches past it, it's laid over what's around it, so don't use it on
    /// controls stacked flush against each other (a list of options): it would take their
    /// neighbours' presses.
    pub fn min_hit(mut self, side: Pixels) -> Self {
        self.min_hit = Some(side);
        self
    }

    /// Whether a pointer press gives it the focus (it does unless told not to). A grid or a
    /// toolbar that keeps the focus itself says no; Tab and the keyboard still reach it.
    pub fn focus_on_press(mut self, focus: bool) -> Self {
        self.focus_on_press = focus;
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
        let toggles = matches!(
            self.role,
            Role::CheckBox
                | Role::Switch
                | Role::RadioButton
                | Role::MenuItemCheckBox
                | Role::MenuItemRadio
        );
        let disabled = self.disabled;
        let mut el = self.base;
        // The ring, drawn outside the corners the host gave it while it's focused from the
        // keyboard.
        let show_ring = self.ring && !disabled && focus_visible(&focus, window);
        let radius = el
            .style()
            .corner_radii
            .top_left
            .map(|r| r.to_pixels(window.rem_size()))
            .unwrap_or_default();
        let border = el
            .style()
            .border_widths
            .top
            .map(|b| b.to_pixels(window.rem_size()))
            .unwrap_or_default();
        // The host's cursor, if it chose one; otherwise a pointing hand while it can be pressed.
        let host_cursor = el.style().mouse_cursor.is_some();
        el = el.role(self.role);
        crate::a11y::audit("Press", self.role, self.label.as_ref());
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
            // The hit square, if it's to be bigger than what's drawn: an invisible square
            // centred on it that presses it too.
            // The hit area, where it's to be bigger than what's drawn: max(width, side) by
            // max(height, side), centred. Its size isn't known here, so it's three invisible
            // centred rectangles whose union is exactly that: the full width by `side`, `side` by
            // the full height, and `side` by `side`.
            let hit: Vec<_> = self
                .min_hit
                .map(|side| {
                    let f = self.on_click.clone();
                    let id = self.id.clone();
                    (0..3)
                        .map(|i| {
                            let f = f.clone();
                            let el = div()
                                .id(ElementId::NamedChild(
                                    std::sync::Arc::new(id.clone()),
                                    format!("hit{i}").into(),
                                ))
                                .absolute();
                            let el = match i {
                                0 => el
                                    .left_0()
                                    .right_0()
                                    .top(relative(0.5))
                                    .mt(-side / 2.)
                                    .h(side),
                                1 => el
                                    .top_0()
                                    .bottom_0()
                                    .left(relative(0.5))
                                    .ml(-side / 2.)
                                    .w(side),
                                _ => el
                                    .left(relative(0.5))
                                    .top(relative(0.5))
                                    .ml(-side / 2.)
                                    .mt(-side / 2.)
                                    .size(side),
                            };
                            el.when(!host_cursor, |el| el.cursor_pointer())
                                .when_some(f, |el, f| {
                                    el.on_click(move |event, window, cx| {
                                        cx.stop_propagation();
                                        f(event, window, cx)
                                    })
                                })
                        })
                        .collect()
                })
                .unwrap_or_default();
            el.track_focus(&focus.tab_stop(self.tab_stop))
                .when(!self.focus_on_press, |el| {
                    // A press doesn't move the focus here (GPUI's own focus-on-press checks it).
                    el.on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                })
                .when(!hit.is_empty(), |el| el.relative().children(hit))
                .when(!host_cursor, |el| el.cursor_pointer())
                .when(show_ring, |el| {
                    hold_ring(el).child(ring_outside(ring, radius, border))
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
