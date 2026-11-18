//! A radio group, as shadcn draws it (its `radio-group.tsx`): options one above the other (or
//! side by side), each a small circle with a label, one of them on. One Tab stop (the option
//! that's on), the arrows moving to the next option and turning it on, going round from the
//! last to the first as radios do, and a press on an option or its label turning it on.
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::RadioGroup;
//! # struct Settings { plan: SharedString }
//! # impl Render for Settings {
//! fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
//!     RadioGroup::new("plan")
//!         .accessibility_label("Plan")
//!         .option("free", "Free")
//!         .option("pro", "Pro")
//!         .option("team", "Team")
//!         .selected(self.plan.clone())
//!         .on_change(cx.listener(|this, id: &SharedString, _, cx| {
//!             this.plan = id.clone();
//!             cx.notify();
//!         }))
//! }
//! # }
//! ```

use std::rc::Rc;

use gpui::{
    App, ElementId, FocusHandle, FontWeight, InteractiveElement as _, IntoElement, KeyBinding,
    Orientation, ParentElement as _, Rems, RenderOnce, Role, SharedString,
    StatefulInteractiveElement as _, StyleRefinement, Styled, Window, actions, div,
    prelude::FluentBuilder as _, rems,
};

use crate::base::{
    ActiveTheme as _, ControlSize, Disableable, Press, Selectable as _, Sizable, focus_ring,
};

actions!(
    gpui_core_radio_group,
    [
        /// The option before the one with the keyboard (the last from the first), turned on.
        Previous,
        /// The option after it (the first from the last), turned on.
        Next
    ]
);

const CONTEXT: &str = "GpuiCoreRadioGroup";

pub(crate) fn init(cx: &mut App) {
    let c = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("left", Previous, c),
        KeyBinding::new("up", Previous, c),
        KeyBinding::new("right", Next, c),
        KeyBinding::new("down", Next, c),
    ]);
}

type OnChange = Rc<dyn Fn(&SharedString, &mut Window, &mut App)>;

struct RadioOption {
    id: SharedString,
    label: SharedString,
    disabled: bool,
}

fn css(v: f32) -> Rems {
    rems(v / 15.)
}

/// Options with one on. Hand it the one that's on with [`selected`](Self::selected) and change
/// it in [`on_change`](Self::on_change).
#[derive(IntoElement)]
pub struct RadioGroup {
    id: ElementId,
    options: Vec<RadioOption>,
    selected: Option<SharedString>,
    on_change: Option<OnChange>,
    label: Option<SharedString>,
    horizontal: bool,
    disabled: bool,
    invalid: bool,
    size: ControlSize,
    style: StyleRefinement,
}

impl RadioGroup {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            options: Vec::new(),
            selected: None,
            on_change: None,
            label: None,
            horizontal: false,
            disabled: false,
            invalid: false,
            size: ControlSize::Medium,
            style: StyleRefinement::default(),
        }
    }

    /// An option: `id` is what [`on_change`](Self::on_change) is told, `label` is drawn beside
    /// its circle (wrapping when it's too long for its line) and names it to assistive tech.
    pub fn option(mut self, id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        self.options.push(RadioOption {
            id: id.into(),
            label: label.into(),
            disabled: false,
        });
        self
    }

    /// Shows the option `id` dimmed and passes over it: no press or key turns it on.
    pub fn disable(mut self, id: impl Into<SharedString>) -> Self {
        let id = id.into();
        if let Some(o) = self.options.iter_mut().find(|o| o.id == id) {
            o.disabled = true;
        }
        self
    }

    /// The option that's on (none until one is given).
    pub fn selected(mut self, id: impl Into<SharedString>) -> Self {
        self.selected = Some(id.into());
        self
    }

    /// Told the option a press or a key turned on. The host makes it the selected one.
    pub fn on_change(mut self, f: impl Fn(&SharedString, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// What it chooses, to assistive tech ("Plan").
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Lays the options out side by side, wrapping onto more rows where they don't fit (they're
    /// one above the other unless told).
    pub fn horizontal(mut self, horizontal: bool) -> Self {
        self.horizontal = horizontal;
        self
    }

    /// Drawn with the destructive colour's border, as a field that failed its check.
    pub fn invalid(mut self, invalid: bool) -> Self {
        self.invalid = invalid;
        self
    }
}

impl Disableable for RadioGroup {
    /// Every option dimmed and passed over.
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Sizable for RadioGroup {
    /// Only [`ControlSize::Touch`] changes it: each option takes presses 44 points tall.
    fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }
}

impl Styled for RadioGroup {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// The next option from `from` in `step`'s direction that can be turned on, going round.
fn step(enabled: &[bool], from: usize, step: isize) -> Option<usize> {
    let n = enabled.len() as isize;
    let mut i = from as isize;
    for _ in 0..n {
        i = (i + step).rem_euclid(n);
        if enabled[i as usize] {
            return Some(i as usize);
        }
    }
    None
}

impl RenderOnce for RadioGroup {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::base::init_soon(cx);
        let theme = cx.theme().clone();
        let dark = theme.mode.is_dark();
        let touch = self.size == ControlSize::Touch;
        // A focus handle for each option, kept between frames.
        let n = self.options.len();
        let handles = window.use_keyed_state(self.id.clone(), cx, |_, _| Vec::<FocusHandle>::new());
        handles.update(cx, |h, cx| {
            while h.len() < n {
                h.push(cx.focus_handle());
            }
            h.truncate(n);
        });
        let handles: Rc<Vec<FocusHandle>> = Rc::new(handles.read(cx).clone());
        let ids: Rc<Vec<SharedString>> =
            Rc::new(self.options.iter().map(|o| o.id.clone()).collect());
        let group_disabled = self.disabled;
        let enabled: Rc<Vec<bool>> = Rc::new(
            self.options
                .iter()
                .map(|o| !o.disabled && !group_disabled)
                .collect(),
        );
        let on = self
            .selected
            .as_ref()
            .and_then(|id| ids.iter().position(|i| i == id));
        // The one Tab stop: the option that's on, else the first that can be.
        let stop = on
            .filter(|i| enabled[*i])
            .or_else(|| enabled.iter().position(|e| *e));
        let on_change = self.on_change.clone();
        // Moves `by` from the option with the keyboard, turning on what it reaches.
        let mover = |by: isize| {
            let (handles, ids, enabled, on_change) = (
                handles.clone(),
                ids.clone(),
                enabled.clone(),
                on_change.clone(),
            );
            move |window: &mut Window, cx: &mut App| {
                let Some(at) = handles.iter().position(|h| h.is_focused(window)).or(stop) else {
                    return;
                };
                if let Some(i) = step(&enabled, at, by) {
                    window.focus(&handles[i], cx);
                    if Some(i) != on
                        && let Some(f) = &on_change
                    {
                        f(&ids[i], window, cx);
                    }
                }
            }
        };
        let prev = mover(-1);
        let next = mover(1);

        let mut group = div()
            .id(self.id)
            .key_context(CONTEXT)
            .role(Role::RadioGroup)
            .aria_orientation(if self.horizontal {
                Orientation::Horizontal
            } else {
                Orientation::Vertical
            })
            .when_some(self.label, |d, l| d.aria_label(l))
            .on_action(move |_: &Previous, w, cx| prev(w, cx))
            .on_action(move |_: &Next, w, cx| next(w, cx))
            .flex()
            .min_w_0()
            .max_w_full()
            .map(|d| {
                if self.horizontal {
                    d.flex_row().flex_wrap().gap_x(css(24.)).gap_y(css(12.))
                } else {
                    d.flex_col().gap(if touch { css(0.) } else { css(12.) })
                }
            });
        gpui::Refineable::refine(group.style(), &self.style);
        let invalid = self.invalid;
        group.children(self.options.into_iter().enumerate().map(|(i, o)| {
            let checked = on == Some(i);
            let disabled = o.disabled || group_disabled;
            let ring = !touch && focus_visible(&handles[i], window);
            let border = if invalid && !checked {
                if dark {
                    theme.destructive.opacity(0.5)
                } else {
                    theme.destructive
                }
            } else if checked {
                theme.primary
            } else if ring {
                theme.ring
            } else {
                theme.input
            };
            let mut shadows = Vec::new();
            if ring {
                shadows.push(focus_ring(cx));
            } else if invalid {
                let mut r = focus_ring(cx);
                r.color = theme.destructive.opacity(if dark { 0.4 } else { 0.2 });
                shadows.push(r);
            }
            let circle = div()
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .size(css(16.))
                .rounded_full()
                .border_1()
                .border_color(border)
                .when(checked, |d| d.bg(theme.primary))
                .when(!checked && dark, |d| d.bg(theme.input.opacity(0.3)))
                .shadow(shadows)
                .when(checked, |d| {
                    d.child(
                        div()
                            .size(css(8.))
                            .rounded_full()
                            .bg(theme.primary_foreground),
                    )
                });
            let on_change = on_change.clone();
            let id = o.id.clone();
            Press::new(ElementId::NamedChild(
                std::sync::Arc::new(ElementId::Name("radio".into())),
                o.id.clone(),
            ))
            .role(Role::RadioButton)
            .selected(checked)
            .disabled(disabled)
            .focus_ring(false)
            .accessibility_label(o.label.clone())
            .aria_position_in_set(i + 1)
            .aria_size_of_set(n)
            .track_focus(&handles[i])
            .tab_stop(stop == Some(i))
            .debug_selector({
                let id = o.id.clone();
                move || format!("radio-{id}")
            })
            .flex()
            .items_start()
            .gap(css(8.))
            .min_w_0()
            .max_w_full()
            .when(disabled, |p| p.opacity(0.5))
            .when(touch, |p| p.min_h(theme.touch_target).items_center())
            .child(circle)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(theme.text.sm)
                    .line_height(css(16.))
                    .font_weight(FontWeight::MEDIUM)
                    .debug_selector({
                        let id = o.id.clone();
                        move || format!("radio-label-{id}")
                    })
                    .child(o.label),
            )
            .on_click(move |_, window, cx| {
                if !checked && let Some(f) = &on_change {
                    f(&id, window, cx);
                }
            })
        }))
    }
}

/// Whether `focus` has the focus and the keyboard put it there (the web's `:focus-visible`).
/// GPUI redraws the window when the focus or the kind of input changes, so this is current.
fn focus_visible(focus: &FocusHandle, window: &Window) -> bool {
    focus.is_focused(window) && window.last_input_was_keyboard()
}
