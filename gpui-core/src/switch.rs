//! An on/off switch, as the web app draws it (its `switch.tsx`): a pill in the primary colour
//! when on and the input colour when off, with a thumb that slides between the two.

use std::rc::Rc;
use std::time::Duration;

use gpui::{
    Animation, AnimationExt as _, App, ElementId, IntoElement, ParentElement as _, Rems,
    RenderOnce, Role, SharedString, StyleRefinement, Styled, Window, div, ease_out_quint,
    prelude::FluentBuilder as _, rems,
};

use crate::press::Press;
use crate::size::{ControlSize, Disableable, Selectable as _, Sizable};
use crate::theme::ActiveTheme as _;

type OnClick = Rc<dyn Fn(&bool, &mut Window, &mut App)>;

fn css(v: f32) -> Rems {
    rems(v / 15.)
}

/// Remembers, per switch, whether it was on when last drawn, so the thumb slides only when it
/// changes, not when it first appears.
struct Seen {
    checked: bool,
    changes: usize,
}

/// A switch. Its [`on_click`](Switch::on_click) is told the state it should take (the opposite
/// of the one it shows); the host keeps the state and passes it back with
/// [`checked`](Switch::checked).
#[derive(IntoElement)]
pub struct Switch {
    id: ElementId,
    checked: bool,
    disabled: bool,
    size: ControlSize,
    label: Option<SharedString>,
    on_click: Option<OnClick>,
    style: StyleRefinement,
}

impl Switch {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            checked: false,
            disabled: false,
            size: ControlSize::Medium,
            label: None,
            on_click: None,
            style: StyleRefinement::default(),
        }
    }

    /// Whether it's on.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    /// What it turns on, to assistive technology (the label beside it, usually).
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Called with the state it should take, on a click, a tap, or Space or Enter.
    pub fn on_click(mut self, f: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }
}

impl Disableable for Switch {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Sizable for Switch {
    fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }
}

impl Styled for Switch {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Switch {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let dark = theme.mode.is_dark();
        let small = matches!(self.size, ControlSize::XSmall | ControlSize::Small);
        // The web's: 32 by 18.4 with a 16 pixel thumb, or 24 by 14 with a 12 pixel one, inside a
        // one pixel border.
        let (w, h, thumb) = if small {
            (24., 14., 12.)
        } else {
            (32., 18.4, 16.)
        };
        let travel = w - 2. - thumb;
        let checked = self.checked;

        let seen = window.use_keyed_state(
            ElementId::NamedChild(std::sync::Arc::new(self.id.clone()), "switch-seen".into()),
            cx,
            |_, _| Seen {
                checked,
                changes: 0,
            },
        );
        let changes = seen.update(cx, |s, _| {
            if s.checked != checked {
                s.checked = checked;
                s.changes += 1;
            }
            s.changes
        });

        let track = if checked {
            theme.primary
        } else if dark {
            theme.input.opacity(0.8)
        } else {
            theme.input
        };
        let thumb_color = match (dark, checked) {
            (true, true) => theme.primary_foreground,
            (true, false) => theme.foreground,
            (false, _) => theme.background,
        };
        let at = move |t: f32| {
            css(if checked {
                travel * t
            } else {
                travel * (1. - t)
            })
        };
        let knob = div()
            .absolute()
            .top(css((h - 2. - thumb) / 2.))
            .size(css(thumb))
            .rounded_full()
            .bg(thumb_color);
        let knob = if changes == 0 {
            knob.left(at(1.)).into_any_element()
        } else {
            knob.with_animation(
                ElementId::NamedInteger("switch-thumb".into(), changes as u64),
                Animation::new(Duration::from_millis(150)).with_easing(ease_out_quint()),
                move |k, t| k.left(at(t)),
            )
            .into_any_element()
        };

        let touch = self.size == ControlSize::Touch;
        let on_click = self.on_click;
        let mut press = Press::new(self.id)
            .role(Role::Switch)
            .selected(checked)
            .disabled(self.disabled)
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            // On a touch screen the target is at least 44 points each way around the track.
            .when(touch, |p| {
                p.min_w(theme.touch_target).min_h(theme.touch_target)
            })
            .when(self.disabled, |p| p.opacity(0.5))
            .when_some(self.label, |p, l| p.accessibility_label(l))
            .when_some(on_click, |p, f| {
                p.on_click(move |_, w, cx| f(&!checked, w, cx))
            })
            .focus_ring(!touch)
            .rounded_full()
            .child(
                div()
                    .relative()
                    .flex_none()
                    .w(css(w))
                    .h(css(h))
                    .rounded_full()
                    .border_1()
                    .border_color(gpui::transparent_black())
                    .bg(track)
                    .child(knob),
            );
        gpui::Refineable::refine(press.style(), &self.style);
        press
    }
}
