//! A sortable list over GPUI's `uniform_list`: only the rows on screen are drawn, however many
//! there are. Every row is the same height, so where a row is, where the pointer would land and
//! where the keyboard goes next are worked out from the scroll offset and that height, rows
//! drawn or not.

use std::collections::HashMap;
use std::hash::Hash;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use gpui::{
    AnyElement, App, Bounds, ElementId, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, ParentElement as _, Pixels, Point, RenderOnce, Role, ScrollStrategy,
    SharedString, StatefulInteractiveElement as _, StyleRefinement, Styled,
    UniformListScrollHandle, Window, div, point, px, uniform_list,
};
use gpui_core::ActiveTheme as _;

use crate::autoscroll::{AutoScroll, AutoScrollExt as _};
use crate::element::{Dnd, dnd};
use crate::item::{DragItem, DropResult};
use crate::monitor::{DragMonitor, DropMonitor};
use crate::sortable::{
    CanMove, ItemLabel, OnMove, Placed, Placeholder, RenderEmpty, RenderItem, Slide, SortableDrag,
    SortableItem, SortableMove, hash, item_id,
};
use crate::source::{Backend, DragSource, Preview};
use crate::state;
use crate::target::{DropOutcome, DropTarget};

/// A sortable list of any length, drawn with GPUI's `uniform_list`: only the rows on screen are
/// drawn and measured. Every row must be the same height (the first row drawn sets it), and
/// the list has no padding of its own (pad its rows instead).
///
/// It takes the same drags as [`Sortable`](crate::Sortable) (a [`SortableDrag`]), so the two
/// kinds share items when they share a [`group`](Self::group). Its keys are an `Rc<[K]>`: give
/// it a new one when the order changes, which is how it knows the host moved something.
///
/// The list is one Tab stop; the arrows move the focus between its rows (scrolling it into
/// view), Space picks the focused row up, the arrows move it one place at a time through rows
/// that aren't drawn yet (the list scrolls ahead of it), and Space drops it. While a mouse or
/// touch drag is near its top or bottom, it scrolls.
#[derive(IntoElement)]
pub struct VirtualSortable<K: Clone + Eq + Hash + 'static> {
    id: ElementId,
    list: Dnd,
    keys: Rc<[K]>,
    scroll: UniformListScrollHandle,
    group: Option<SharedString>,
    placeholder: Placeholder,
    item: Option<RenderItem<K>>,
    empty: Option<RenderEmpty>,
    item_label: Option<ItemLabel<K>>,
    label: Option<SharedString>,
    can_move: Option<CanMove<K>>,
    on_move: Option<OnMove<K>>,
}

impl<K: Clone + Eq + Hash + 'static> VirtualSortable<K> {
    pub fn new(
        id: impl Into<ElementId>,
        keys: impl Into<Rc<[K]>>,
        scroll: &UniformListScrollHandle,
    ) -> Self {
        let id = id.into();
        Self {
            list: dnd(id.clone()).drop_indicator(false),
            id,
            keys: keys.into(),
            scroll: scroll.clone(),
            group: None,
            placeholder: Placeholder::Gap,
            item: None,
            empty: None,
            item_label: None,
            label: None,
            can_move: None,
            on_move: None,
        }
    }

    /// Lists with the same group, virtual or not, take each other's items.
    pub fn group(mut self, group: impl Into<SharedString>) -> Self {
        self.group = Some(group.into());
        self
    }

    /// [`Placeholder::Gap`] unless given; a custom placeholder is drawn in the gap.
    pub fn placeholder(mut self, placeholder: Placeholder) -> Self {
        self.placeholder = placeholder;
        self
    }

    /// Draws a row, each the same height. It's also drawn as the drag's preview.
    pub fn item(
        mut self,
        render: impl Fn(&K, &SortableItem, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.item = Some(Rc::new(render));
        self
    }

    /// Drawn when the list has no rows; the list still takes them.
    pub fn empty(mut self, render: impl Fn(&mut Window, &mut App) -> AnyElement + 'static) -> Self {
        self.empty = Some(Rc::new(render));
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn item_label(mut self, label: impl Fn(&K) -> SharedString + 'static) -> Self {
        self.item_label = Some(Rc::new(label));
        self
    }

    pub fn can_move(mut self, f: impl Fn(&SortableMove<K>, &App) -> bool + 'static) -> Self {
        self.can_move = Some(Rc::new(f));
        self
    }

    pub fn on_move(
        mut self,
        f: impl Fn(&SortableMove<K>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_move = Some(Rc::new(f));
        self
    }
}

impl<K: Clone + Eq + Hash + 'static> Styled for VirtualSortable<K> {
    fn style(&mut self) -> &mut StyleRefinement {
        self.list.style()
    }
}

/// Where the rows are, from the scroll handle: worked out, not measured row by row.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Geometry {
    /// The list's viewport in the window.
    pub viewport: Bounds<Pixels>,
    /// Where row 0's top is in the window, scrolled.
    pub top: Pixels,
    pub row: Pixels,
    pub n: usize,
}

impl Geometry {
    pub fn of(handle: &UniformListScrollHandle, n: usize) -> Option<Self> {
        let state = handle.0.borrow();
        let viewport = state.base_handle.bounds();
        // An empty list still takes an item, at its top.
        if n == 0 {
            return Some(Self {
                viewport,
                top: viewport.top(),
                row: px(1.),
                n,
            });
        }
        // GPUI notes the list's viewport (`item`) and all its rows together (`contents`).
        let row = state.last_item_size?.contents.height / n as f32;
        if row <= px(0.) {
            return None;
        }
        let offset = state.base_handle.offset();
        Some(Self {
            viewport,
            top: viewport.top() + offset.y,
            row,
            n,
        })
    }

    fn mid(&self, i: usize) -> Pixels {
        self.top + self.row * i as f32 + self.row / 2.
    }

    /// Where an item at window height `y` would land, counted without `skip`.
    pub fn index_at(&self, y: Pixels, skip: Option<usize>) -> usize {
        let x = (f32::from(y - self.top) - f32::from(self.row) / 2.) / f32::from(self.row);
        let all = (x.ceil().max(0.) as usize).min(self.n);
        match skip {
            Some(s) if s < all => all - 1,
            _ => all,
        }
    }

    /// How many rows there are without `skip`.
    fn kept(&self, skip: Option<usize>) -> usize {
        self.n - usize::from(skip.is_some_and(|s| s < self.n))
    }

    /// The row a place without `skip` is.
    fn natural(&self, r: usize, skip: Option<usize>) -> usize {
        r + usize::from(skip.is_some_and(|s| r >= s))
    }

    /// A window height that lands at `to` (counted without `skip`).
    pub fn slot_y(&self, to: usize, skip: Option<usize>) -> Pixels {
        let m = self.kept(skip);
        if m == 0 {
            return self.top + self.row / 2.;
        }
        if to == 0 {
            return self.top + self.row * self.natural(0, skip) as f32 + self.row / 4.;
        }
        if to >= m {
            return self.mid(self.natural(m - 1, skip)) + self.row / 4.;
        }
        (self.mid(self.natural(to - 1, skip)) + self.mid(self.natural(to, skip))) / 2.
    }

    /// Where row `i` is drawn, in rows from its own place, with an item landing at `to` (or
    /// none), the dragged row `skip` from this list.
    fn shift(i: usize, skip: Option<usize>, to: Option<usize>) -> isize {
        let v = match (skip, to) {
            (Some(s), Some(to)) if i == s => to,
            (Some(s), Some(to)) => {
                let r = i - usize::from(i > s);
                r + usize::from(r >= to)
            }
            (Some(s), None) if i == s => i,
            (Some(s), None) => i - usize::from(i > s),
            (None, Some(to)) => i + usize::from(i >= to),
            (None, None) => i,
        };
        v as isize - i as isize
    }
}

struct VirtualState {
    focus: FocusHandle,
    focused: usize,
    slides: HashMap<u64, Slide>,
    keys: Option<usize>,
    settle: Option<(u64, Point<Pixels>)>,
}

/// What the rows need to know to be drawn, worked out once a frame.
#[derive(Clone)]
struct Plan<K> {
    list: ElementId,
    group: Option<SharedString>,
    skip: Option<usize>,
    /// Landing here: at this place.
    to: Option<usize>,
    /// Gone to another list: this one closes up.
    closing: bool,
    shifting: bool,
    row: Pixels,
    width: Pixels,
    n: usize,
    focused: Option<usize>,
    keyboard_ring: bool,
    dragging: bool,
    dragged_opacity: f32,
    keys: Rc<[K]>,
}

impl<K: Clone + Eq + Hash + 'static> RenderOnce for VirtualSortable<K> {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let list_id = self.id.clone();
        let state: Entity<VirtualState> = window.use_keyed_state(
            ElementId::NamedChild(Arc::new(list_id.clone()), "dnd-virtual".into()),
            cx,
            |_, cx| VirtualState {
                focus: cx.focus_handle(),
                focused: 0,
                slides: HashMap::new(),
                keys: None,
                settle: None,
            },
        );
        let now = cx.background_executor().now();
        let reduce = cx.reduce_motion();
        let n = self.keys.len();
        let keys_at = Rc::as_ptr(&self.keys) as *const () as usize;
        let monitor = DragMonitor::of(cx);
        let drag = monitor
            .item()
            .and_then(|i| i.get::<SortableDrag<K>>())
            .filter(|d| d.group == self.group || d.list == list_id)
            .cloned();
        let any_landing = state::sort_landing(cx);
        let landing = any_landing
            .as_ref()
            .filter(|(l, _)| l == &list_id)
            .map(|(_, to)| *to);
        let geometry = Geometry::of(&self.scroll, n);
        let shifting = !matches!(self.placeholder, Placeholder::Line);

        let (focus, focused) = state.update(cx, |s, _| {
            if s.keys != Some(keys_at) {
                // The host gave new keys: everything's at its new place already.
                s.slides.clear();
                s.keys = Some(keys_at);
                if let Some((key, from)) = s.settle.take()
                    && !reduce
                {
                    s.slides.insert(
                        key,
                        Slide {
                            from,
                            to: Point::default(),
                            start: now,
                        },
                    );
                }
            } else if !monitor.is_dragging() {
                s.settle = None;
            }
            s.focused = s.focused.min(n.saturating_sub(1));
            (s.focus.clone(), s.focused)
        });
        let skip = drag
            .as_ref()
            .filter(|d| d.list == list_id && d.from < n && hash(&self.keys[d.from]) == hash(&d.key))
            .map(|d| d.from);
        let plan = Plan {
            list: list_id.clone(),
            group: self.group.clone(),
            skip,
            to: drag.as_ref().and(landing),
            closing: drag.is_some() && landing.is_none() && any_landing.is_some(),
            shifting,
            row: geometry.map_or(px(0.), |g| g.row),
            width: geometry.map_or(px(0.), |g| g.viewport.size.width),
            n,
            focused: (focus.is_focused(window) && !monitor.is_dragging()).then_some(focused),
            keyboard_ring: focus.is_focused(window) && window.last_input_was_keyboard(),
            dragging: monitor.is_dragging(),
            dragged_opacity: if shifting { 0. } else { 0.4 },
            keys: self.keys.clone(),
        };

        let render = self.item.clone().unwrap_or_else(|| {
            Rc::new(|_: &K, _: &SortableItem, _: &mut Window, _: &mut App| div().into_any_element())
        });
        let item_label = self.item_label.clone();
        let rows = {
            let (state, render, plan) = (state.clone(), render.clone(), plan.clone());
            uniform_list(
                ElementId::NamedChild(Arc::new(list_id.clone()), "rows".into()),
                n,
                move |range, window, cx| {
                    rows(
                        &plan,
                        &state,
                        &render,
                        item_label.as_ref(),
                        range,
                        now,
                        reduce,
                        window,
                        cx,
                    )
                },
            )
            .track_scroll(&self.scroll)
            .size_full()
        };

        // A line where it'd land, or the custom placeholder in the gap.
        let mark = match (&self.placeholder, geometry, plan.to, &drag) {
            (Placeholder::Gap, ..) | (_, None, ..) | (_, _, None, _) | (_, _, _, None) => None,
            (placeholder, Some(g), Some(to), Some(d)) => {
                let natural = g.natural(to, skip);
                let y = if to >= g.kept(skip) && g.kept(skip) > 0 {
                    g.top + g.row * (g.natural(to - 1, skip) + 1) as f32
                } else {
                    g.top + g.row * natural as f32
                };
                let local = y - g.viewport.top();
                Some(match placeholder {
                    Placeholder::Line => div()
                        .debug_selector(|| "dnd-sortable-line".into())
                        .absolute()
                        .left_0()
                        .right_0()
                        .top(local - px(1.))
                        .h(px(2.))
                        .bg(cx.theme().ring)
                        .into_any_element(),
                    Placeholder::Custom(f) => div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .top(local)
                        .h(g.row)
                        .child(f(
                            gpui::size(g.viewport.size.width, d.size.height),
                            window,
                            cx,
                        ))
                        .into_any_element(),
                    Placeholder::Gap => unreachable!(),
                })
            }
        };
        let empty = (n == 0 && plan.to.is_none())
            .then(|| self.empty.as_ref().map(|f| f(window, cx)))
            .flatten();

        let target = target(&self, &state);
        let (keys, key_list, state_keys, handle) = (
            self.keys.clone(),
            list_id.clone(),
            state.clone(),
            self.scroll.clone(),
        );
        let mut list = self
            .list
            .target(target)
            .role(Role::List)
            .track_focus(&focus.tab_stop(true))
            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                if DragMonitor::of(cx).is_dragging() || event.keystroke.modifiers.modified() {
                    return;
                }
                let n = keys.len();
                let key = event.keystroke.key.as_str();
                if key == "space" || key == "enter" {
                    let index = state_keys.read(cx).focused;
                    if let Some(k) = keys.get(index) {
                        crate::keyboard::pick_up(&item_id(&key_list, hash(k)), window, cx);
                    }
                    cx.stop_propagation();
                    return;
                }
                let step: Option<fn(usize, usize) -> usize> = match key {
                    "up" => Some(|i, _| i.saturating_sub(1)),
                    "down" => Some(|i, n| (i + 1).min(n.saturating_sub(1))),
                    "home" => Some(|_, _| 0),
                    "end" => Some(|_, n| n.saturating_sub(1)),
                    _ => None,
                };
                if let Some(step) = step {
                    let index = state_keys.update(cx, |s, cx| {
                        s.focused = step(s.focused, n);
                        cx.notify();
                        s.focused
                    });
                    handle.scroll_to_item(index, ScrollStrategy::Nearest);
                    cx.stop_propagation();
                }
            })
            .relative()
            .child(rows)
            .children(mark)
            .children(empty);
        if let Some(label) = &self.label {
            list = list.label(label.clone());
        }
        list.auto_scroll(&self.scroll, AutoScroll::vertical())
    }
}

/// The rows on screen, each drawn moved to where the drag puts it.
#[allow(clippy::too_many_arguments)]
fn rows<K: Clone + Eq + Hash + 'static>(
    plan: &Plan<K>,
    state: &Entity<VirtualState>,
    render: &RenderItem<K>,
    item_label: Option<&ItemLabel<K>>,
    range: std::ops::Range<usize>,
    now: Instant,
    reduce: bool,
    window: &mut Window,
    cx: &mut App,
) -> Vec<AnyElement> {
    let ring = gpui_core::focus_ring(cx);
    let mut moving = false;
    let mut out = Vec::with_capacity(range.len());
    for index in range {
        let Some(key) = plan.keys.get(index) else {
            continue;
        };
        let h = hash(key);
        let rows = if !plan.shifting || !plan.dragging {
            0
        } else if plan.to.is_some() || plan.closing {
            Geometry::shift(index, plan.skip, plan.to)
        } else {
            0
        };
        let target = point(px(0.), plan.row * rows as f32);
        let offset = state.update(cx, |s, _| {
            let slide = s.slides.entry(h).or_insert(Slide {
                from: target,
                to: target,
                start: now,
            });
            if slide.to != target {
                *slide = Slide {
                    from: if reduce { target } else { slide.at(now) },
                    to: target,
                    start: now,
                };
            }
            moving |= slide.from != slide.to && slide.progress(now) < 1.;
            slide.at(now)
        });
        let info = SortableItem {
            index,
            dragging: plan.skip == Some(index),
            focused: plan.focused == Some(index),
            preview: false,
        };
        let content = render(key, &info, window, cx);
        let (k, list, group, size) = (
            key.clone(),
            plan.list.clone(),
            plan.group.clone(),
            gpui::size(plan.width, plan.row),
        );
        let preview_render = render.clone();
        let (dragged_list, opacity) = (plan.list.clone(), plan.dragged_opacity);
        let mut row = dnd(item_id(&plan.list, h))
            .role(Role::ListItem)
            .aria_position_in_set(index + 1)
            .aria_size_of_set(plan.n)
            .drop_indicator(false)
            .source(
                DragSource::new(move |_| SortableDrag {
                    list: list.clone(),
                    key: k.clone(),
                    from: index,
                    size,
                    group: group.clone(),
                })
                .keyboard(false)
                .is_dragging(move |d: &[SortableDrag<K>]| {
                    d[0].list == dragged_list && hash(&d[0].key) == h
                })
                .preview(Preview::build(
                    move |d: &[SortableDrag<K>], window, cx| {
                        let info = SortableItem {
                            index: d[0].from,
                            preview: true,
                            ..SortableItem::default()
                        };
                        div()
                            .w(d[0].size.width)
                            .h(d[0].size.height)
                            .shadow_lg()
                            .opacity(0.95)
                            .child(preview_render(&d[0].key, &info, window, cx))
                            .into_any_element()
                    },
                )),
            )
            .dragging_style(move |s| s.opacity(opacity))
            .child(content);
        if let Some(label) = item_label {
            row = row.label(label(key));
        }
        if info.focused {
            row = row.aria_active_descendant();
            if plan.keyboard_ring {
                row = row.shadow(vec![ring.clone()]);
            }
        }
        out.push(
            Placed {
                child: row.into_any_element(),
                offset,
                record: None,
            }
            .into_any_element(),
        );
    }
    if moving {
        window.request_animation_frame();
    }
    out
}

/// The list as a drop target: where an item lands is worked out from the pointer's height.
fn target<K: Clone + Eq + Hash + 'static>(
    list: &VirtualSortable<K>,
    state: &Entity<VirtualState>,
) -> DropTarget {
    let id = list.id.clone();
    let n = list.keys.len();
    let handle = list.scroll.clone();
    // The item and where it'd land, if it can come here.
    let landing = {
        let (id, handle) = (id.clone(), handle.clone());
        Rc::new(move |item: &DragItem, at: Point<Pixels>| {
            let d = item.get::<SortableDrag<K>>()?;
            let g = Geometry::of(&handle, n)?;
            let skip = (d.list == id).then_some(d.from).filter(|f| *f < n);
            Some((d.clone(), g.index_at(at.y, skip), g, skip))
        })
    };
    let group = list.group.clone();
    let can_move = list.can_move.clone();
    let on_move = list.on_move.clone();
    let label = list.label.clone();
    let (l1, l2, l3, l4) = (
        landing.clone(),
        landing.clone(),
        landing.clone(),
        landing.clone(),
    );
    let (id1, id2, id3) = (id.clone(), id.clone(), id.clone());
    let (state_drop, handle_step, handle_drop) = (state.clone(), handle.clone(), handle.clone());
    DropTarget::new()
        .accept::<SortableDrag<K>>()
        .can_drop(move |item, m, cx| {
            let Some(d) = item.get::<SortableDrag<K>>() else {
                return false;
            };
            if d.list != id1 && (d.group.is_none() || d.group != group) {
                return false;
            }
            let (Some(cm), Some(at)) = (&can_move, m.client_offset()) else {
                return true;
            };
            let Some((d, to, ..)) = l1(item, at) else {
                return false;
            };
            cm(
                &SortableMove {
                    key: d.key.clone(),
                    from: (d.list.clone(), d.from),
                    to: (id1.clone(), to),
                },
                cx,
            )
        })
        .on_hover(move |item, m: &DropMonitor, _, cx| {
            if !m.can_drop() {
                return;
            }
            let Some(at) = m.client_offset() else { return };
            if let Some((_, to, ..)) = l2(item, at) {
                state::shared(cx).borrow_mut().sort_landing = Some((id2.clone(), to));
            }
        })
        .keyboard_step(move |item, m, direction, _, _| {
            // Along the list: one place, scrolling it into view first; across it: away.
            if direction.x != 0. {
                return None;
            }
            let at = m.client_offset()?;
            let (_, to, g, skip) = l3(item, at)?;
            let next = if direction.y < 0. {
                to.checked_sub(1)?
            } else {
                to + 1
            };
            if next > g.kept(skip) {
                return None;
            }
            let content = g.slot_y(next, skip) - g.top;
            let base = handle_step.0.borrow().base_handle.clone();
            let offset = base.offset();
            let view = g.viewport.size.height;
            let max = base.max_offset().y.max(px(0.));
            let margin = g.row.min(view / 2.);
            let mut y = offset.y;
            if content - margin < -y {
                y = -(content - margin);
            } else if content + margin > -y + view {
                y = -(content + margin - view);
            }
            let y = y.clamp(-max, px(0.));
            if y != offset.y {
                base.set_offset(point(offset.x, y));
            }
            Some(point(g.viewport.center().x, g.viewport.top() + y + content))
        })
        .describe(move |item, m| {
            let Some((_, to, g, skip)) = m.client_offset().and_then(|at| l4(item, at)) else {
                return label.clone().unwrap_or_default();
            };
            let count = g.kept(skip) + 1;
            match &label {
                Some(label) => format!("{label}, position {} of {count}", to + 1).into(),
                None => format!("position {} of {count}", to + 1).into(),
            }
        })
        .on_drop(move |item, m, window, cx| {
            let Some(at) = m.client_offset() else {
                return DropOutcome::Pass;
            };
            let Some((d, to, g, _)) = landing(item, at) else {
                return DropOutcome::Pass;
            };
            let mv = SortableMove {
                key: d.key.clone(),
                from: (d.list.clone(), d.from),
                to: (id3.clone(), to),
            };
            if mv.moves() {
                if let Some(preview) = m.source_client_offset() {
                    let place = point(g.viewport.left(), g.top + g.row * to as f32);
                    state_drop.update(cx, |s, _| s.settle = Some((hash(&d.key), preview - place)));
                }
                if let Some(f) = &on_move {
                    f(&mv, window, cx);
                }
            }
            if m.backend() == Some(Backend::Keyboard) {
                let focus = state_drop.update(cx, |s, cx| {
                    s.focused = to;
                    cx.notify();
                    s.focus.clone()
                });
                handle_drop.scroll_to_item(to, ScrollStrategy::Nearest);
                window.focus(&focus, cx);
            }
            DropOutcome::Handled(Some(DropResult::new(mv)))
        })
}
