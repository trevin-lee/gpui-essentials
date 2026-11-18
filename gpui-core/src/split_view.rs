//! A split view: two or three panes side by side (a sidebar, a list, a detail), with a handle
//! between each two. Drag a handle to resize the pane before it, within that pane's minimum and
//! maximum; the last pane takes the rest. A handle takes the focus with Tab: Left and Right move
//! it a step (with Shift, five), Home and End to the pane's minimum and maximum, and a double
//! press puts the pane back at its default width. Below a width the host sets it shows one pane
//! at a time, the one the host says, and tells the host it's narrow so it can draw a back button.
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::{Pane, SplitView};
//! # struct Mail { sidebar: bool, open: bool, narrow: bool, widths: Vec<Pixels> }
//! # impl Render for Mail {
//! fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
//!     SplitView::new("mail")
//!         .pane(Pane::new("Mailboxes").width(px(200.)).min(px(150.)).max(px(320.))
//!             .collapsed(!self.sidebar).accessibility_label("Mailboxes"))
//!         .pane(Pane::new("Messages").width(px(320.)).min(px(240.)).accessibility_label("Messages"))
//!         .pane(Pane::new("Message").min(px(300.)).accessibility_label("Message"))
//!         .widths(self.widths.clone())
//!         .on_resize(cx.listener(|this, widths: &[Pixels], _, cx| {
//!             this.widths = widths.to_vec();
//!             cx.notify();
//!         }))
//!         .narrow_below(px(700.))
//!         .narrow_shows(if self.open { 2 } else { 1 })
//!         .on_narrow_change(cx.listener(|this, narrow: &bool, _, cx| {
//!             this.narrow = *narrow;
//!             cx.notify();
//!         }))
//! }
//! # }
//! ```

use std::rc::Rc;

use gpui::{
    AnyElement, App, Bounds, CursorStyle, DispatchPhase, ElementId, Entity, FocusHandle,
    InteractiveElement as _, IntoElement, KeyBinding, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ParentElement as _, Pixels, RenderOnce, Role, SharedString,
    StatefulInteractiveElement as _, StyleRefinement, Styled, Window, accesskit, actions, canvas,
    deferred, div, prelude::FluentBuilder as _, px,
};

use crate::base::{ActiveTheme as _, focus_ring};

actions!(
    gpui_core_split_view,
    [
        /// Moves the focused handle a step left.
        Shrink,
        /// Moves it a step right.
        Grow,
        /// Moves it five steps left.
        ShrinkMore,
        /// Moves it five steps right.
        GrowMore,
        /// Moves it to its pane's minimum.
        ToMin,
        /// Moves it to its pane's maximum.
        ToMax
    ]
);

const CONTEXT: &str = "GpuiCoreSplitView";

/// What Left and Right move a handle by; with Shift, five times as far.
const STEP: Pixels = px(10.);
const BIG_STEP: Pixels = px(50.);

/// The drawn line between two panes.
const LINE: Pixels = px(1.);

/// How wide a handle takes presses, when not for touch.
const HIT: Pixels = px(8.);

pub(crate) fn init(cx: &mut App) {
    let c = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("left", Shrink, c),
        KeyBinding::new("right", Grow, c),
        KeyBinding::new("shift-left", ShrinkMore, c),
        KeyBinding::new("shift-right", GrowMore, c),
        KeyBinding::new("home", ToMin, c),
        KeyBinding::new("end", ToMax, c),
    ]);
}

type OnResize = Rc<dyn Fn(&[Pixels], &mut Window, &mut App)>;
type OnNarrow = Rc<dyn Fn(&bool, &mut Window, &mut App)>;

/// One pane of a [`SplitView`]: its content, and how wide it may be.
pub struct Pane {
    child: AnyElement,
    width: Pixels,
    min: Pixels,
    max: Option<Pixels>,
    collapsed: bool,
    label: Option<SharedString>,
}

impl Pane {
    /// A pane holding `child`, 240 pixels wide to begin with, at least 120, with no maximum.
    /// The pane clips its content; give the child `size_full` (or `flex_1`, the pane being a
    /// column) to fill it.
    pub fn new(child: impl IntoElement) -> Self {
        Self {
            child: child.into_any_element(),
            width: px(240.),
            min: px(120.),
            max: None,
            collapsed: false,
            label: None,
        }
    }

    /// Its default width: what it starts at and what a double press on its handle puts it back
    /// to. The last pane shown takes the rest of the room instead.
    pub fn width(mut self, width: Pixels) -> Self {
        self.width = width;
        self
    }

    /// The narrowest a drag or a key makes it (for the last pane: the room its handle leaves it).
    pub fn min(mut self, min: Pixels) -> Self {
        self.min = min;
        self
    }

    /// The widest a drag or a key makes it.
    pub fn max(mut self, max: Pixels) -> Self {
        self.max = Some(max);
        self
    }

    /// Hidden, its room given to the others (a sidebar the host toggles). Its content is kept,
    /// not drawn.
    pub fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }

    /// What it is to assistive technology ("Mailboxes"); its handle is called the same.
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

/// Panes side by side with handles between them, one at a time when narrow: the widths, the
/// handles' keys and the narrow form are described with each builder method below.
#[derive(IntoElement)]
pub struct SplitView {
    id: ElementId,
    panes: Vec<Pane>,
    widths: Option<Vec<Pixels>>,
    narrow_below: Option<Pixels>,
    narrow_shows: usize,
    on_resize: Option<OnResize>,
    on_narrow: Option<OnNarrow>,
    touch: bool,
    style: StyleRefinement,
}

impl SplitView {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            panes: Vec::new(),
            widths: None,
            narrow_below: None,
            narrow_shows: 0,
            on_resize: None,
            on_narrow: None,
            touch: false,
            style: StyleRefinement::default(),
        }
    }

    /// The next pane, left to right.
    pub fn pane(mut self, pane: Pane) -> Self {
        self.panes.push(pane);
        self
    }

    /// Widths the host kept (what [`on_resize`](Self::on_resize) told it), one for each pane but
    /// the last. They replace the panes' widths whenever they differ from the ones passed last,
    /// so passing the same ones on every draw keeps them without undoing a drag.
    pub fn widths(mut self, widths: impl IntoIterator<Item = Pixels>) -> Self {
        self.widths = Some(widths.into_iter().collect());
        self
    }

    /// Told the panes' widths, one for each pane but the last, when a drag ends, a key moves a
    /// handle or a double press resets one (and only if they changed), for the host to keep.
    pub fn on_resize(mut self, f: impl Fn(&[Pixels], &mut Window, &mut App) + 'static) -> Self {
        self.on_resize = Some(Rc::new(f));
        self
    }

    /// Below this width it shows one pane at a time, filling it: the one
    /// [`narrow_shows`](Self::narrow_shows) says.
    pub fn narrow_below(mut self, width: Pixels) -> Self {
        self.narrow_below = Some(width);
        self
    }

    /// The pane shown when narrow, counted from 0 (the detail when something is selected, the
    /// list otherwise). The first unless told.
    pub fn narrow_shows(mut self, index: usize) -> Self {
        self.narrow_shows = index;
        self
    }

    /// Told whether it's narrow, once it first knows and whenever that changes, so the host can
    /// draw its back button.
    pub fn on_narrow_change(mut self, f: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_narrow = Some(Rc::new(f));
        self
    }

    /// Handles that take presses 44 points wide, for a finger.
    pub fn touch(mut self, touch: bool) -> Self {
        self.touch = touch;
        self
    }
}

impl Styled for SplitView {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// What a split view keeps between frames.
#[derive(Default)]
struct State {
    /// Each pane's width, where a drag, a key or the host set it.
    widths: Vec<Option<Pixels>>,
    /// The widths the host passed last.
    seeded: Option<Vec<Pixels>>,
    /// The focus handle of the handle after each pane.
    focus: Vec<FocusHandle>,
    drag: Option<Drag>,
    /// The width it was laid out at last, and each pane's.
    width: Option<Pixels>,
    drawn: Vec<Pixels>,
    /// Whether it last told the host it was narrow.
    told_narrow: Option<bool>,
}

#[derive(Clone, Copy)]
struct Drag {
    pane: usize,
    /// Where the press was, and the pane's width then.
    x: Pixels,
    width: Pixels,
    /// The pane's kept width before the drag, to tell whether it changed.
    before: Pixels,
}

/// The panes' limits as drawn this frame.
struct Limits {
    defaults: Vec<Pixels>,
    mins: Vec<Pixels>,
    maxs: Vec<Option<Pixels>>,
    /// The panes shown with a handle after them.
    sized: Vec<usize>,
    /// The minimum of the pane taking the rest.
    rest_min: Pixels,
    lines: Pixels,
}

impl Limits {
    /// `w` within pane `i`'s minimum and maximum.
    fn bound(&self, i: usize, w: Pixels) -> Pixels {
        let lo = self.mins[i];
        let hi = self.maxs[i].map_or(w.max(lo), |m| m.max(lo));
        w.clamp(lo, hi)
    }

    /// Pane `i`'s kept width.
    fn kept(&self, i: usize, s: &State) -> Pixels {
        self.bound(
            i,
            s.widths
                .get(i)
                .copied()
                .flatten()
                .unwrap_or(self.defaults[i]),
        )
    }

    /// Pane `i`'s width as drawn (its kept width if it hasn't been).
    fn shown(&self, i: usize, s: &State) -> Pixels {
        s.drawn
            .get(i)
            .copied()
            .filter(|w| *w > Pixels::ZERO)
            .unwrap_or_else(|| self.kept(i, s))
    }

    /// The widest pane `i` can be: its maximum, and no more than leaves the last pane its
    /// minimum beside the others as drawn.
    fn most(&self, i: usize, s: &State) -> Pixels {
        let lo = self.mins[i];
        let mut hi = self.maxs[i].unwrap_or(Pixels::MAX).max(lo);
        if let Some(total) = s.width {
            let others: Pixels = self
                .sized
                .iter()
                .filter(|j| **j != i)
                .map(|j| self.shown(*j, s))
                .sum();
            hi = hi.min(total - self.lines - others - self.rest_min).max(lo);
        }
        hi
    }

    /// `w` as pane `i` can be.
    fn fit(&self, i: usize, w: Pixels, s: &State) -> Pixels {
        w.clamp(self.mins[i], self.most(i, s))
    }

    /// The kept widths, one for each pane but the last.
    fn report(&self, s: &State) -> Vec<Pixels> {
        let n = self.defaults.len().saturating_sub(1);
        (0..n).map(|i| self.kept(i, s)).collect()
    }
}

/// Sets pane `i`'s width and redraws; whether it changed.
fn set(state: &Entity<State>, limits: &Limits, i: usize, w: Pixels, cx: &mut App) -> bool {
    state.update(cx, |s, cx| {
        let w = limits.fit(i, w, s);
        if limits.kept(i, s) == w {
            return false;
        }
        s.widths[i] = Some(w);
        cx.notify();
        true
    })
}

fn tell(
    state: &Entity<State>,
    limits: &Limits,
    on_resize: &Option<OnResize>,
    window: &mut Window,
    cx: &mut App,
) {
    if let Some(f) = on_resize {
        let widths = limits.report(state.read(cx));
        f(&widths, window, cx);
    }
}

impl RenderOnce for SplitView {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::base::init_soon(cx);
        let theme = cx.theme().clone();
        let n = self.panes.len();
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| State::default());

        // Narrow by the width it was laid out at last; before that, by the window's.
        let width = state.read(cx).width.unwrap_or(window.viewport_size().width);
        let narrow = self.narrow_below.is_some_and(|below| width < below);
        let shows = self.narrow_shows.min(n.saturating_sub(1));
        let visible: Vec<bool> = self
            .panes
            .iter()
            .enumerate()
            .map(|(i, p)| if narrow { i == shows } else { !p.collapsed })
            .collect();
        // The last pane shown takes the rest; each shown before it has a handle after it.
        let rest = visible.iter().rposition(|v| *v);
        let sized: Vec<usize> = (0..n).filter(|i| visible[*i] && Some(*i) != rest).collect();
        let mins: Vec<Pixels> = self.panes.iter().map(|p| p.min).collect();
        let limits = Rc::new(Limits {
            defaults: self.panes.iter().map(|p| p.width).collect(),
            maxs: self.panes.iter().map(|p| p.max).collect(),
            rest_min: match rest {
                Some(r) if !narrow => mins[r],
                _ => Pixels::ZERO,
            },
            mins,
            lines: LINE * sized.len(),
            sized,
        });
        state.update(cx, |s, cx| {
            while s.focus.len() < n {
                s.focus.push(cx.focus_handle().tab_stop(true));
            }
            s.widths.resize(n, None);
            // A drag on a handle that's gone (its pane collapsed, or narrow) ends.
            if s.drag.is_some_and(|d| !limits.sized.contains(&d.pane)) {
                s.drag = None;
            }
            if let Some(widths) = &self.widths
                && s.seeded.as_ref() != Some(widths)
            {
                for (i, w) in widths.iter().enumerate().take(n) {
                    s.widths[i] = Some(limits.bound(i, *w));
                }
                s.seeded = Some(widths.clone());
            }
        });

        let (widths, focus, drag) = {
            let s = state.read(cx);
            let widths: Vec<Pixels> = (0..n).map(|i| limits.kept(i, s)).collect();
            (widths, s.focus.clone(), s.drag)
        };
        let hit = if self.touch { theme.touch_target } else { HIT };
        let ring = focus_ring(cx);
        let on_resize = self.on_resize.clone();

        let mut children: Vec<AnyElement> = Vec::new();
        // Which child each pane is, for measuring.
        let mut pane_child = Vec::with_capacity(n);
        for (i, pane) in self.panes.into_iter().enumerate() {
            pane_child.push(children.len());
            let el = div()
                .id(("split-pane", i))
                .role(Role::Group)
                .when_some(pane.label.clone(), |d, l| d.aria_label(l))
                .debug_selector({
                    let id = self.id.clone();
                    move || format!("{id}-pane-{i}")
                })
                .relative()
                .flex()
                .flex_col()
                .h_full()
                .min_w_0()
                .overflow_hidden();
            let el = if !visible[i] {
                // Laid out as nothing, its content kept for when it's shown again.
                el.hidden()
            } else if Some(i) == rest {
                el.flex_1().when(!narrow, |d| d.min_w(limits.mins[i]))
            } else {
                el.flex_none()
                    .flex_shrink(1.)
                    .w(widths[i])
                    .min_w(limits.mins[i])
            };
            children.push(el.child(pane.child).into_any_element());
            if !limits.sized.contains(&i) {
                continue;
            }

            // The handle after it: a line in the flow, and drawn over the panes on either side
            // (so its hit area isn't under the next one) the focusable part with its hit area.
            let state_ = state.clone();
            let limits_ = limits.clone();
            let on_resize_ = on_resize.clone();
            let key = move |to: fn(Pixels, Pixels, Pixels) -> Pixels| {
                let (state, limits, on_resize) =
                    (state_.clone(), limits_.clone(), on_resize_.clone());
                move |window: &mut Window, cx: &mut App| {
                    let (now, lo, hi) = {
                        let s = state.read(cx);
                        (limits.shown(i, s), limits.mins[i], limits.most(i, s))
                    };
                    if set(&state, &limits, i, to(now, lo, hi), cx) {
                        tell(&state, &limits, &on_resize, window, cx);
                    }
                }
            };
            let shrink = key(|w, _, _| w - STEP);
            let grow = key(|w, _, _| w + STEP);
            let shrink_more = key(|w, _, _| w - BIG_STEP);
            let grow_more = key(|w, _, _| w + BIG_STEP);
            let to_min = key(|_, lo, _| lo);
            let to_max = key(|_, _, hi| hi);
            let (now, most) = {
                let s = state.read(cx);
                (limits.shown(i, s), limits.most(i, s))
            };
            let dragging = drag.is_some_and(|d| d.pane == i);
            let press = {
                let (state, limits, on_resize) = (state.clone(), limits.clone(), on_resize.clone());
                move |e: &MouseDownEvent, window: &mut Window, cx: &mut App| {
                    // A drag doesn't take the focus from what has it (a field in a pane).
                    window.prevent_default();
                    cx.stop_propagation();
                    if e.click_count >= 2 {
                        state.update(cx, |s, _| s.drag = None);
                        if set(&state, &limits, i, limits.defaults[i], cx) {
                            tell(&state, &limits, &on_resize, window, cx);
                        }
                        return;
                    }
                    state.update(cx, |s, cx| {
                        s.drag = Some(Drag {
                            pane: i,
                            x: e.position.x,
                            width: limits.shown(i, s),
                            before: limits.kept(i, s),
                        });
                        cx.notify();
                    });
                }
            };
            let handle = div()
                .id(("split-handle", i))
                .key_context(CONTEXT)
                .track_focus(&focus[i])
                .role(Role::Splitter)
                .aria_orientation(accesskit::Orientation::Vertical)
                .when_some(pane.label, |d, l| d.aria_label(l))
                .aria_numeric_value(f32::from(now).round() as f64)
                .aria_min_numeric_value(f32::from(limits.mins[i]).round() as f64)
                .aria_max_numeric_value(f32::from(most).round() as f64)
                .debug_selector({
                    let id = self.id.clone();
                    move || format!("{id}-handle-{i}")
                })
                .on_action(move |_: &Shrink, w, cx| shrink(w, cx))
                .on_action(move |_: &Grow, w, cx| grow(w, cx))
                .on_action(move |_: &ShrinkMore, w, cx| shrink_more(w, cx))
                .on_action(move |_: &GrowMore, w, cx| grow_more(w, cx))
                .on_action(move |_: &ToMin, w, cx| to_min(w, cx))
                .on_action(move |_: &ToMax, w, cx| to_max(w, cx))
                .absolute()
                .top_0()
                .bottom_0()
                .left_0()
                .w(LINE)
                .when(dragging, |d| d.bg(theme.ring))
                .focus_visible({
                    let ring = ring.clone();
                    move |s| s.shadow(vec![ring])
                })
                .child(
                    div()
                        .id(("split-hit", i))
                        .debug_selector({
                            let id = self.id.clone();
                            move || format!("{id}-hit-{i}")
                        })
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left((LINE - hit) / 2.)
                        .w(hit)
                        .cursor(CursorStyle::ResizeLeftRight)
                        .on_mouse_down(MouseButton::Left, press),
                );
            children.push(
                div()
                    .flex_none()
                    .relative()
                    .w(LINE)
                    .h_full()
                    .bg(theme.border)
                    .child(deferred(handle))
                    .into_any_element(),
            );
        }

        // Follows a drag past the handle until the button is let go.
        let measure = children.len();
        children.push(
            canvas(|_, _, _| {}, {
                let (state, limits, on_resize) = (state.clone(), limits.clone(), on_resize.clone());
                move |_, _, window, cx| {
                    let Some(drag) = state.read(cx).drag else {
                        return;
                    };
                    window.set_window_cursor_style(CursorStyle::ResizeLeftRight);
                    let (s, l) = (state.clone(), limits.clone());
                    window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| {
                        if phase == DispatchPhase::Bubble
                            && e.pressed_button == Some(MouseButton::Left)
                        {
                            set(&s, &l, drag.pane, drag.width + (e.position.x - drag.x), cx);
                        }
                    });
                    let (s, l, on_resize) = (state.clone(), limits.clone(), on_resize.clone());
                    window.on_mouse_event(move |_: &MouseUpEvent, phase, window, cx| {
                        if phase != DispatchPhase::Bubble {
                            return;
                        }
                        let changed = s.update(cx, |st, cx| {
                            st.drag = None;
                            cx.notify();
                            l.kept(drag.pane, st) != drag.before
                        });
                        if changed {
                            tell(&s, &l, &on_resize, window, cx);
                        }
                    });
                }
            })
            .absolute()
            .size_full()
            .into_any_element(),
        );

        let on_narrow = self.on_narrow.clone();
        let narrow_below = self.narrow_below;
        let measured = state.clone();
        let mut row = div()
            .on_children_prepainted(move |bounds: Vec<Bounds<Pixels>>, window, cx| {
                let Some(total) = bounds.get(measure).map(|b| b.size.width) else {
                    return;
                };
                let drawn = pane_child
                    .iter()
                    .map(|k| bounds.get(*k).map_or(Pixels::ZERO, |b| b.size.width))
                    .collect();
                let now = narrow_below.is_some_and(|below| total < below);
                let tell_narrow = measured.update(cx, |s, _| {
                    s.width = Some(total);
                    s.drawn = drawn;
                    let tell = narrow_below.is_some() && s.told_narrow != Some(now);
                    if tell {
                        s.told_narrow = Some(now);
                    }
                    tell
                });
                // Drawn by a guess that turned out wrong (the first frame, or a resize across
                // the line): draw again, after this frame.
                if now != narrow {
                    let state = measured.clone();
                    window.defer(cx, move |_, cx| state.update(cx, |_, cx| cx.notify()));
                }
                if tell_narrow && let Some(f) = on_narrow.clone() {
                    window.defer(cx, move |window, cx| f(&now, window, cx));
                }
            })
            .id(self.id)
            .relative()
            .flex()
            .flex_row()
            .size_full()
            .min_w_0()
            .overflow_hidden();
        gpui::Refineable::refine(row.style(), &self.style);
        row.children(children)
    }
}
