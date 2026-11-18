//! Toggles, as shadcn draws them (its `toggle.tsx` and `toggle-group.tsx`): a button that stays
//! pressed in or comes back out (Bold in a toolbar), in a plain or an outlined look, with an
//! icon, a label or both; and a row of them joined edge to edge, of which one or several can be
//! pressed in, sharing one Tab stop.
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::{IconName, Toggle, ToggleGroup};
//! # struct Editor { bold: bool, align: Vec<SharedString> }
//! # impl Render for Editor {
//! fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
//!     div()
//!         .child(
//!             Toggle::new("bold")
//!                 .icon(IconName::Italic)
//!                 .accessibility_label("Italic")
//!                 .pressed(self.bold)
//!                 .on_click(cx.listener(|this, pressed: &bool, _, cx| {
//!                     this.bold = *pressed;
//!                     cx.notify();
//!                 })),
//!         )
//!         .child(
//!             ToggleGroup::new("align")
//!                 .accessibility_label("Alignment")
//!                 .item("start", "Left")
//!                 .item("center", "Centre")
//!                 .item("end", "Right")
//!                 .selected(self.align.clone())
//!                 .on_change(cx.listener(|this, ids: &Vec<SharedString>, _, cx| {
//!                     this.align = ids.clone();
//!                     cx.notify();
//!                 })),
//!         )
//! }
//! # }
//! ```

use std::rc::Rc;

use gpui::{
    App, ElementId, FocusHandle, FontWeight, Hsla, InteractiveElement as _, IntoElement,
    KeyBinding, ParentElement as _, Rems, RenderOnce, Role, SharedString,
    StatefulInteractiveElement as _, StyleRefinement, Styled, Toggled, Window, actions, div,
    prelude::FluentBuilder as _, px, relative, rems,
};

use crate::base::{
    ActiveTheme as _, ControlSize, Disableable, Icon, IconName, Press, Sizable, Theme,
};

actions!(
    gpui_core_toggle_group,
    [
        /// The keyboard to the toggle before (the last from the first).
        Previous,
        /// The keyboard to the toggle after (the first from the last).
        Next,
        First,
        Last
    ]
);

const CONTEXT: &str = "GpuiCoreToggleGroup";

pub(crate) fn init(cx: &mut App) {
    let c = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("left", Previous, c),
        KeyBinding::new("up", Previous, c),
        KeyBinding::new("right", Next, c),
        KeyBinding::new("down", Next, c),
        KeyBinding::new("home", First, c),
        KeyBinding::new("end", Last, c),
    ]);
}

fn css(v: f32) -> Rems {
    rems(v / 15.)
}

/// How a toggle looks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ToggleVariant {
    /// No fill or border until the pointer is on it or it's pressed in.
    #[default]
    Default,
    /// A border, as an outline button.
    Outline,
}

/// Height, padding and the side of a square one, for a size: shadcn's `h-9 px-2 min-w-9`, `h-8
/// px-1.5 min-w-8` and `h-10 px-2.5 min-w-10`.
fn metrics(size: ControlSize) -> (Rems, Rems) {
    match size {
        ControlSize::XSmall => (css(24.), css(4.)),
        ControlSize::Small => (css(32.), css(6.)),
        ControlSize::Medium | ControlSize::Touch => (css(36.), css(8.)),
        ControlSize::Large => (css(40.), css(10.)),
    }
}

/// Fill, text and the fill under the pointer. Pressed in, it's shadcn's `data-[state=on]`
/// accent look; in the dark, where accent is all but the page, the input colour, as a selected
/// `Button` is.
fn paint(variant: ToggleVariant, pressed: bool, theme: &Theme) -> (Hsla, Hsla, Hsla) {
    let dark = theme.mode.is_dark();
    let none = gpui::transparent_black();
    if pressed {
        let fill = if dark { theme.input } else { theme.accent };
        return (fill, theme.accent_foreground, fill);
    }
    let hover = match (variant, dark) {
        (ToggleVariant::Outline, true) => theme.input.opacity(0.5),
        (ToggleVariant::Outline, false) => theme.accent,
        (ToggleVariant::Default, true) => theme.muted.opacity(0.5),
        (ToggleVariant::Default, false) => theme.muted,
    };
    let fill = if variant == ToggleVariant::Outline && dark {
        theme.input.opacity(0.3)
    } else {
        none
    };
    (fill, theme.foreground, hover)
}

/// Where a toggle sits in a group, which squares its inner corners.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Joined {
    #[default]
    Alone,
    First,
    Middle,
    Last,
}

type OnClick = Rc<dyn Fn(&bool, &mut Window, &mut App)>;

/// A button that's pressed in or not. Its [`on_click`](Toggle::on_click) is told the state it
/// should take; the host keeps it and passes it back with [`pressed`](Toggle::pressed). Give an
/// icon-only one an [`accessibility_label`](Toggle::accessibility_label).
#[derive(IntoElement)]
pub struct Toggle {
    id: ElementId,
    label: Option<SharedString>,
    icon: Option<IconName>,
    accessibility_label: Option<SharedString>,
    pressed: bool,
    variant: ToggleVariant,
    size: ControlSize,
    disabled: bool,
    on_click: Option<OnClick>,
    style: StyleRefinement,
    // In a group: the role it has there (a radio for a one-choice group), its place, its Tab
    // stop and its focus handle.
    role: Role,
    joined: Joined,
    tab_stop: bool,
    focus: Option<FocusHandle>,
    fill: bool,
}

impl Toggle {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            label: None,
            icon: None,
            accessibility_label: None,
            pressed: false,
            variant: ToggleVariant::Default,
            size: ControlSize::Medium,
            disabled: false,
            on_click: None,
            style: StyleRefinement::default(),
            role: Role::Button,
            joined: Joined::Alone,
            tab_stop: true,
            focus: None,
            fill: false,
        }
    }

    /// Its text. It stays on one line and ends in an ellipsis where it's too narrow.
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// An icon before the label, or alone.
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    /// What it's called to assistive technology, when the label doesn't say (an icon only).
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.accessibility_label = Some(label.into());
        self
    }

    /// Whether it's pressed in.
    pub fn pressed(mut self, pressed: bool) -> Self {
        self.pressed = pressed;
        self
    }

    pub fn variant(mut self, variant: ToggleVariant) -> Self {
        self.variant = variant;
        self
    }

    /// The outlined look.
    pub fn outline(self) -> Self {
        self.variant(ToggleVariant::Outline)
    }

    /// Called with the state it should take, on a click, a tap, or Space or Enter.
    pub fn on_click(mut self, f: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }
}

impl Disableable for Toggle {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Sizable for Toggle {
    fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }
}

impl Styled for Toggle {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Toggle {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let (height, pad) = metrics(self.size);
        let (fill, text, hover) = paint(self.variant, self.pressed, &theme);
        let touch = self.size == ControlSize::Touch;
        let icon_only = self.label.is_none();
        let outline = self.variant == ToggleVariant::Outline;
        let radius = theme.radius_md();
        let joined = self.joined;
        let name = self
            .accessibility_label
            .clone()
            .or_else(|| self.label.clone());
        let pressed = self.pressed;
        let on_click = self.on_click;
        let mut press = Press::new(self.id.clone())
            .role(self.role)
            .aria_toggled(if pressed {
                Toggled::True
            } else {
                Toggled::False
            })
            .tab_stop(self.tab_stop)
            .when_some(self.focus, |p, f| p.track_focus(&f))
            .disabled(self.disabled)
            .when_some(name, |p, n| p.accessibility_label(n))
            .when_some(on_click, |p, f| {
                p.on_click(move |_, w, cx| f(&!pressed, w, cx))
            })
            .debug_selector({
                let id = self.id.clone();
                move || format!("toggle-{id}")
            })
            .flex()
            .items_center()
            .justify_center()
            .gap(css(8.))
            .h(height)
            .px(pad)
            .max_w_full()
            .map(|p| {
                if self.fill {
                    // In a group: the toggles share its width, giving way alike.
                    p.flex_1().min_w_0()
                } else if icon_only {
                    p.flex_none().min_w(height)
                } else {
                    // A label gives way (it ends in an ellipsis) rather than run out of the
                    // space it has.
                    p.flex_shrink_1().min_w(height)
                }
            })
            .when(touch, |p| {
                p.min_h(theme.touch_target)
                    .when(icon_only, |p| p.min_w(theme.touch_target))
            })
            .when(outline, |p| p.border_1().border_color(theme.input))
            .bg(fill)
            .text_color(text)
            .text_size(theme.text.sm)
            .font_weight(FontWeight::MEDIUM)
            .whitespace_nowrap()
            .overflow_hidden()
            .line_height(relative(1.))
            .map(|p| match joined {
                Joined::Alone => p.rounded(radius),
                Joined::First => p.rounded_l(radius),
                Joined::Middle => p,
                Joined::Last => p.rounded_r(radius),
            })
            .when(
                outline && matches!(joined, Joined::Middle | Joined::Last),
                // Neighbours share one border line.
                |p| p.ml(px(-1.)),
            )
            .when(self.disabled, |p| p.opacity(0.5))
            .when(!self.disabled, |p| p.hover(move |s| s.bg(hover)));
        gpui::Refineable::refine(press.style(), &self.style);
        let press = press
            .when_some(self.icon, |p, icon| {
                p.child(Icon::new(icon).size(css(16.)).color(text))
            })
            .when_some(self.label, |p, label| {
                p.child(
                    // Three pixels' room above and below for descenders and accents, which
                    // reach past a line height of 1.
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .py(css(3.))
                        .my(css(-3.))
                        .text_ellipsis()
                        .child(label),
                )
            });
        // Drawn here rather than as a component of its own, so its role and state are this
        // element's.
        RenderOnce::render(press, window, cx)
    }
}

type OnChange = Rc<dyn Fn(&Vec<SharedString>, &mut Window, &mut App)>;

struct GroupItem {
    id: SharedString,
    label: Option<SharedString>,
    name: SharedString,
    icon: Option<IconName>,
    disabled: bool,
}

/// Toggles joined edge to edge, sharing a look and a size: one of them pressed in at most (a
/// text's alignment), or with [`multiple`](ToggleGroup::multiple) any of them (bold, italic,
/// underline). Hand it what's pressed with [`selected`](ToggleGroup::selected) and change it in
/// [`on_change`](ToggleGroup::on_change), which is told everything that should be pressed.
///
/// It's one Tab stop (the toggle last moved to, else the first pressed); the arrows, Home and
/// End move the keyboard along (going round at the ends) without pressing anything, and Space
/// or Enter presses the one with the keyboard. Pressing the one that's in, in a one-choice
/// group, lets it out (none pressed), as shadcn's does.
#[derive(IntoElement)]
pub struct ToggleGroup {
    id: ElementId,
    items: Vec<GroupItem>,
    selected: Vec<SharedString>,
    multiple: bool,
    variant: ToggleVariant,
    size: ControlSize,
    disabled: bool,
    label: Option<SharedString>,
    on_change: Option<OnChange>,
    style: StyleRefinement,
}

impl ToggleGroup {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            items: Vec::new(),
            selected: Vec::new(),
            multiple: false,
            variant: ToggleVariant::Default,
            size: ControlSize::Medium,
            disabled: false,
            label: None,
            on_change: None,
            style: StyleRefinement::default(),
        }
    }

    /// A toggle with a label: `id` is what [`on_change`](Self::on_change) is told.
    pub fn item(mut self, id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        let label = label.into();
        self.items.push(GroupItem {
            id: id.into(),
            name: label.clone(),
            label: Some(label),
            icon: None,
            disabled: false,
        });
        self
    }

    /// A toggle with an icon before its label.
    pub fn item_with_icon(
        mut self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        icon: IconName,
    ) -> Self {
        let label = label.into();
        self.items.push(GroupItem {
            id: id.into(),
            name: label.clone(),
            label: Some(label),
            icon: Some(icon),
            disabled: false,
        });
        self
    }

    /// A toggle that's an icon alone; `name` is what assistive tech calls it.
    pub fn icon_item(
        mut self,
        id: impl Into<SharedString>,
        icon: IconName,
        name: impl Into<SharedString>,
    ) -> Self {
        self.items.push(GroupItem {
            id: id.into(),
            name: name.into(),
            label: None,
            icon: Some(icon),
            disabled: false,
        });
        self
    }

    /// Shows the toggle `id` dimmed and passes over it.
    pub fn disable(mut self, id: impl Into<SharedString>) -> Self {
        let id = id.into();
        if let Some(i) = self.items.iter_mut().find(|i| i.id == id) {
            i.disabled = true;
        }
        self
    }

    /// The toggles pressed in (in a one-choice group, the first of them counts).
    pub fn selected(mut self, ids: impl IntoIterator<Item = impl Into<SharedString>>) -> Self {
        self.selected = ids.into_iter().map(Into::into).collect();
        self
    }

    /// Whether any number can be pressed in at once (one at most unless told).
    pub fn multiple(mut self, multiple: bool) -> Self {
        self.multiple = multiple;
        self
    }

    pub fn variant(mut self, variant: ToggleVariant) -> Self {
        self.variant = variant;
        self
    }

    /// The outlined look, the toggles sharing their borders.
    pub fn outline(self) -> Self {
        self.variant(ToggleVariant::Outline)
    }

    /// What it's for, to assistive tech ("Alignment").
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Told what should be pressed in after a press: the ids in the order the toggles are.
    pub fn on_change(
        mut self,
        f: impl Fn(&Vec<SharedString>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }
}

impl Disableable for ToggleGroup {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Sizable for ToggleGroup {
    fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }
}

impl Styled for ToggleGroup {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// What a group remembers between frames: a focus handle per toggle, and its Tab stop.
struct GroupState {
    handles: Vec<FocusHandle>,
    stop: Option<SharedString>,
}

impl RenderOnce for ToggleGroup {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::base::init_soon(cx);
        let n = self.items.len();
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| GroupState {
            handles: Vec::new(),
            stop: None,
        });
        state.update(cx, |s, cx| {
            while s.handles.len() < n {
                s.handles.push(cx.focus_handle());
            }
            s.handles.truncate(n);
        });
        let handles: Rc<Vec<FocusHandle>> = Rc::new(state.read(cx).handles.clone());
        let ids: Rc<Vec<SharedString>> = Rc::new(self.items.iter().map(|i| i.id.clone()).collect());
        let group_disabled = self.disabled;
        let enabled: Rc<Vec<bool>> = Rc::new(
            self.items
                .iter()
                .map(|i| !i.disabled && !group_disabled)
                .collect(),
        );
        let multiple = self.multiple;
        let mut selected = self.selected.clone();
        if !multiple {
            selected.truncate(1);
        }
        let on: Vec<bool> = ids.iter().map(|id| selected.contains(id)).collect();
        // The one Tab stop: where the keyboard last was, else the first pressed, else the first
        // that can be.
        let remembered = state
            .read(cx)
            .stop
            .as_ref()
            .and_then(|id| ids.iter().position(|i| i == id));
        let stop = remembered
            .filter(|i| enabled[*i])
            .or_else(|| (0..n).find(|i| on[*i] && enabled[*i]))
            .or_else(|| enabled.iter().position(|e| *e));
        // Moves the keyboard to the toggle `to` gives from the one that has it.
        let mover = |to: fn(&[bool], usize) -> Option<usize>| {
            let (handles, ids, enabled, state) =
                (handles.clone(), ids.clone(), enabled.clone(), state.clone());
            move |window: &mut Window, cx: &mut App| {
                let Some(at) = handles.iter().position(|h| h.is_focused(window)).or(stop) else {
                    return;
                };
                if let Some(i) = to(&enabled, at) {
                    state.update(cx, |s, _| s.stop = Some(ids[i].clone()));
                    window.focus(&handles[i], cx);
                }
            }
        };
        let prev = mover(|e, at| round(e, at, -1));
        let next = mover(|e, at| round(e, at, 1));
        let first = mover(|e, _| e.iter().position(|x| *x));
        let last = mover(|e, _| e.iter().rposition(|x| *x));
        let mut row = div()
            .id(self.id)
            .key_context(CONTEXT)
            .role(if multiple {
                Role::Group
            } else {
                Role::RadioGroup
            })
            .when_some(self.label, |d, l| d.aria_label(l))
            .on_action(move |_: &Previous, w, cx| prev(w, cx))
            .on_action(move |_: &Next, w, cx| next(w, cx))
            .on_action(move |_: &First, w, cx| first(w, cx))
            .on_action(move |_: &Last, w, cx| last(w, cx))
            .flex()
            .items_center()
            // It never runs out of its space: its toggles give way (their labels end in an
            // ellipsis).
            .min_w_0()
            .max_w_full();
        gpui::Refineable::refine(row.style(), &self.style);
        let on_change = self.on_change;
        let (variant, size) = (self.variant, self.size);
        row.children(self.items.into_iter().enumerate().map(|(i, item)| {
            let pressed = on[i];
            let next: Vec<SharedString> = if multiple {
                ids.iter()
                    .enumerate()
                    .filter(|(j, _)| if *j == i { !pressed } else { on[*j] })
                    .map(|(_, id)| id.clone())
                    .collect()
            } else if pressed {
                Vec::new()
            } else {
                vec![item.id.clone()]
            };
            let on_change = on_change.clone();
            let state = state.clone();
            let id = item.id.clone();
            let mut t = Toggle::new(ElementId::NamedChild(
                std::sync::Arc::new(ElementId::Name("toggle-item".into())),
                item.id.clone(),
            ))
            .accessibility_label(item.name)
            .pressed(pressed)
            .variant(variant)
            .with_size(size)
            .disabled(item.disabled || group_disabled)
            .on_click(move |_, window, cx| {
                state.update(cx, |s, _| s.stop = Some(id.clone()));
                if let Some(f) = &on_change {
                    f(&next, window, cx);
                }
            });
            if let Some(label) = item.label {
                t = t.label(label);
            }
            if let Some(icon) = item.icon {
                t = t.icon(icon);
            }
            t.role = if multiple {
                Role::Button
            } else {
                Role::RadioButton
            };
            t.focus = Some(handles[i].clone());
            t.tab_stop = stop == Some(i);
            t.fill = true;
            t.joined = match (i, n) {
                (_, 1) => Joined::Alone,
                (0, _) => Joined::First,
                (i, n) if i + 1 == n => Joined::Last,
                _ => Joined::Middle,
            };
            t
        }))
    }
}

/// The next toggle from `from` in `step`'s direction that can be pressed, going round.
fn round(enabled: &[bool], from: usize, step: isize) -> Option<usize> {
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
