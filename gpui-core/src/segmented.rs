//! A segmented control: a few options side by side, one of them on (a theme's System, Light and
//! Dark; a page's views). It's a radio group: one Tab stop (the option that's on), the arrows,
//! Home and End moving to another and turning it on, a press turning one on.
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::SegmentedControl;
//! # struct Settings { theme: SharedString }
//! # impl Render for Settings {
//! fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
//!     SegmentedControl::new("theme")
//!         .accessibility_label("Theme")
//!         .segment("system", "System")
//!         .segment("light", "Light")
//!         .segment("dark", "Dark")
//!         .selected(self.theme.clone())
//!         .on_change(cx.listener(|this, id: &SharedString, _, cx| {
//!             this.theme = id.clone();
//!             cx.notify();
//!         }))
//! }
//! # }
//! ```

use std::rc::Rc;

use gpui::{
    App, ElementId, FocusHandle, InteractiveElement as _, IntoElement, KeyBinding,
    ParentElement as _, RenderOnce, Role, SharedString, StatefulInteractiveElement as _,
    StyleRefinement, Styled, Window, actions, div, prelude::FluentBuilder as _, rems,
};

use crate::base::{ActiveTheme as _, Disableable as _, Icon, IconName, Press, Selectable as _};

actions!(
    gpui_core_segmented,
    [
        /// The option before the one with the keyboard, turned on.
        Previous,
        /// The option after it, turned on.
        Next,
        First,
        Last
    ]
);

const CONTEXT: &str = "GpuiCoreSegmented";

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

type OnChange = Rc<dyn Fn(&SharedString, &mut Window, &mut App)>;

struct Segment {
    id: SharedString,
    label: SharedString,
    icon: Option<IconName>,
    disabled: bool,
}

/// Options side by side, one on. Hand it the one that's on with [`selected`](Self::selected)
/// and change it in [`on_change`](Self::on_change).
///
/// It's as wide as its labels, whole, wherever it sits (beside a spacer in a toolbar, say).
/// Given a width (`w(…)`, `w_full()`) or a share of a row (`flex_1()`), its options split it
/// equally instead, and a label too long for its share is cut short.
#[derive(IntoElement)]
pub struct SegmentedControl {
    id: ElementId,
    segments: Vec<Segment>,
    selected: Option<SharedString>,
    on_change: Option<OnChange>,
    label: Option<SharedString>,
    touch: bool,
    style: StyleRefinement,
}

impl SegmentedControl {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            segments: Vec::new(),
            selected: None,
            on_change: None,
            label: None,
            touch: false,
            style: StyleRefinement::default(),
        }
    }

    /// An option: `id` is what [`on_change`](Self::on_change) is told.
    pub fn segment(mut self, id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        self.segments.push(Segment {
            id: id.into(),
            label: label.into(),
            icon: None,
            disabled: false,
        });
        self
    }

    /// An option with an icon before its label.
    pub fn segment_with_icon(
        mut self,
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        icon: IconName,
    ) -> Self {
        self.segments.push(Segment {
            id: id.into(),
            label: label.into(),
            icon: Some(icon),
            disabled: false,
        });
        self
    }

    /// Shows the option `id` and passes over it: no press or key turns it on.
    pub fn disable(mut self, id: impl Into<SharedString>) -> Self {
        let id = id.into();
        if let Some(s) = self.segments.iter_mut().find(|s| s.id == id) {
            s.disabled = true;
        }
        self
    }

    /// The option that's on.
    pub fn selected(mut self, id: impl Into<SharedString>) -> Self {
        self.selected = Some(id.into());
        self
    }

    /// Told the option a press or a key turned on. The host makes it the selected one.
    pub fn on_change(mut self, f: impl Fn(&SharedString, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// What it chooses, to assistive tech ("Theme").
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Options 44 points tall, for touch.
    pub fn touch(mut self, touch: bool) -> Self {
        self.touch = touch;
        self
    }
}

impl Styled for SegmentedControl {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

fn css(v: f32) -> gpui::Rems {
    rems(v / 15.)
}

/// The next option from `from` in `step`'s direction that can be turned on, not wrapping.
fn step(enabled: &[bool], from: usize, step: isize) -> Option<usize> {
    let mut i = from as isize;
    loop {
        i += step;
        if i < 0 || i as usize >= enabled.len() {
            return None;
        }
        if enabled[i as usize] {
            return Some(i as usize);
        }
    }
}

impl RenderOnce for SegmentedControl {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::base::init_soon(cx);
        let theme = cx.theme().clone();
        // A focus handle for each option, kept between frames.
        let n = self.segments.len();
        let handles = window.use_keyed_state(self.id.clone(), cx, |_, _| Vec::<FocusHandle>::new());
        handles.update(cx, |h, cx| {
            while h.len() < n {
                h.push(cx.focus_handle());
            }
            h.truncate(n);
        });
        let handles: Rc<Vec<FocusHandle>> = Rc::new(handles.read(cx).clone());
        let ids: Rc<Vec<SharedString>> =
            Rc::new(self.segments.iter().map(|s| s.id.clone()).collect());
        let enabled: Rc<Vec<bool>> = Rc::new(self.segments.iter().map(|s| !s.disabled).collect());
        let on = self
            .selected
            .as_ref()
            .and_then(|id| ids.iter().position(|i| i == id));
        // The one Tab stop: the option that's on, else the first that can be.
        let stop = on
            .filter(|i| enabled[*i])
            .or_else(|| enabled.iter().position(|e| *e));
        let on_change = self.on_change.clone();
        // Moves to the option `to` would give from the one with the keyboard, turning it on.
        let mover = |to: fn(&[bool], usize) -> Option<usize>| {
            let (handles, ids, enabled, on_change) = (
                handles.clone(),
                ids.clone(),
                enabled.clone(),
                on_change.clone(),
            );
            move |window: &mut Window, cx: &mut App| {
                let at = handles
                    .iter()
                    .position(|h| h.is_focused(window))
                    .or(stop)
                    .unwrap_or(0);
                if let Some(i) = to(&enabled, at) {
                    window.focus(&handles[i], cx);
                    if let Some(f) = &on_change {
                        f(&ids[i], window, cx);
                    }
                }
            }
        };
        let prev = mover(|e, at| step(e, at, -1));
        let next = mover(|e, at| step(e, at, 1));
        let first = mover(|e, _| e.iter().position(|x| *x));
        let last = mover(|e, _| e.iter().rposition(|x| *x));
        // Given a width or a share of a row, the options split it; otherwise each is as wide as
        // its label.
        let stretch =
            self.style.size.width.is_some() || self.style.flex_grow.is_some_and(|g| g > 0.);
        let height = if self.touch {
            theme.touch_target.into()
        } else {
            gpui::DefiniteLength::from(css(36.))
        };
        let mut track = div()
            .id(self.id)
            .key_context(CONTEXT)
            .role(Role::RadioGroup)
            .when_some(self.label, |d, l| d.aria_label(l))
            .on_action(move |_: &Previous, w, cx| prev(w, cx))
            .on_action(move |_: &Next, w, cx| next(w, cx))
            .on_action(move |_: &First, w, cx| first(w, cx))
            .on_action(move |_: &Last, w, cx| last(w, cx))
            .flex()
            .items_center()
            .when(stretch, |d| d.min_w_0().max_w_full())
            .when(!stretch, |d| d.flex_none())
            .h(height)
            .p(css(3.))
            .gap(css(2.))
            .rounded(theme.radius_lg())
            .bg(theme.muted);
        gpui::Refineable::refine(track.style(), &self.style);
        track.children(self.segments.into_iter().enumerate().map(|(i, s)| {
            let selected = on == Some(i);
            let on_change = on_change.clone();
            let id = s.id.clone();
            Press::new(ElementId::NamedChild(
                std::sync::Arc::new(ElementId::Name("segment".into())),
                s.id.clone(),
            ))
            .role(Role::RadioButton)
            .selected(selected)
            .disabled(s.disabled)
            .accessibility_label(s.label.clone())
            .track_focus(&handles[i])
            .tab_stop(stop == Some(i))
            .debug_selector({
                let id = s.id.clone();
                move || format!("segment-{id}")
            })
            .flex()
            .when(stretch, |p| p.flex_1().min_w_0())
            .when(!stretch, |p| p.flex_none())
            .h_full()
            .items_center()
            .justify_center()
            .gap(css(6.))
            .px(css(12.))
            .rounded(theme.radius_md())
            .text_size(theme.text.sm)
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(if selected {
                theme.foreground
            } else {
                theme.muted_foreground
            })
            .when(selected, |p| p.bg(theme.background).shadow_sm())
            .when(s.disabled, |p| p.opacity(0.5))
            .when_some(s.icon, |p, icon| {
                p.child(Icon::new(icon).color(if selected {
                    theme.foreground
                } else {
                    theme.muted_foreground
                }))
            })
            .child(
                div()
                    .when(stretch, |d| d.min_w_0())
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .child(s.label),
            )
            .on_click(move |_, window, cx| {
                if !selected && let Some(f) = &on_change {
                    f(&id, window, cx);
                }
            })
        }))
    }
}
