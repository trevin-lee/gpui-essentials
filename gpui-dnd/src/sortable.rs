//! Sortable lists: items that reorder by dragging (mouse, touch or keyboard), and with a group,
//! move between lists (a kanban board's columns). The host owns the order: it hears a
//! [`SortableMove`] and changes its keys.
//!
//! The items keep their places in the layout while a drag is on; they're drawn moved (with
//! `window.with_element_offset`) to open the gap where the item would land, so the midpoints
//! the landing place is measured against never move under the pointer.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    AnyElement, App, Axis, Bounds, Element, ElementId, Entity, FocusHandle, GlobalElementId,
    InspectorElementId, InteractiveElement as _, IntoElement, KeyDownEvent, LayoutId,
    ParentElement as _, Pixels, Point, Rems, RenderOnce, Role, ScrollHandle, SharedString, Size,
    StatefulInteractiveElement as _, StyleRefinement, Styled, Window, div, point, px, rems,
};
use gpui_core::ActiveTheme as _;

use crate::autoscroll::{AutoScroll, AutoScrollExt as _};
use crate::element::{Dnd, dnd};
use crate::item::{DragItem, DropResult};
use crate::monitor::{DragMonitor, DropMonitor};
use crate::source::{Backend, DragSource, Preview};
use crate::state;
use crate::target::{DropOutcome, DropTarget};

/// How long the items take to slide apart, and the dropped item to settle into its place.
pub const SLIDE_DURATION: Duration = Duration::from_millis(150);

/// What a sortable item drags: its list, its key and its index there. Other targets can take
/// it too (`accept::<SortableDrag<K>>()`: a bin, say).
#[derive(Clone, Debug)]
pub struct SortableDrag<K> {
    pub list: ElementId,
    pub key: K,
    pub from: usize,
    /// The item's size, which the gap opens to.
    pub size: Size<Pixels>,
    pub(crate) group: Option<SharedString>,
}

/// An item's state, for its renderer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SortableItem {
    pub index: usize,
    /// It's the one being dragged (its place in the list, not the preview).
    pub dragging: bool,
    /// The list has the keyboard's focus and it's the focused item.
    pub focused: bool,
    /// It's drawn as the drag's preview.
    pub preview: bool,
}

/// A move the list asks the host to make.
#[derive(Clone, Debug, PartialEq)]
pub struct SortableMove<K> {
    pub key: K,
    /// The list and index it was at.
    pub from: (ElementId, usize),
    /// The list and index it goes to, counted without it: remove it from `from`, then insert
    /// it at `to`.
    pub to: (ElementId, usize),
}

impl<K> SortableMove<K> {
    /// Whether it changes anything.
    pub fn moves(&self) -> bool {
        self.from != self.to
    }
}

/// What shows where the item would land.
#[derive(Clone)]
pub enum Placeholder {
    /// The items part, a gap the dragged item's size opening where it'd land.
    Gap,
    /// A line in the theme's ring colour where it'd land; nothing moves.
    Line,
    /// The items part, and this element fills the gap.
    Custom(RenderPlaceholder),
}

/// Draws a custom placeholder of the dragged item's size.
pub type RenderPlaceholder = Rc<dyn Fn(Size<Pixels>, &mut Window, &mut App) -> AnyElement>;
pub(crate) type Rects = Rc<RefCell<Vec<Bounds<Pixels>>>>;
pub(crate) type RenderItem<K> = Rc<dyn Fn(&K, &SortableItem, &mut Window, &mut App) -> AnyElement>;
pub(crate) type RenderEmpty = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;
pub(crate) type ItemLabel<K> = Rc<dyn Fn(&K) -> SharedString>;
pub(crate) type CanMove<K> = Rc<dyn Fn(&SortableMove<K>, &App) -> bool>;
pub(crate) type OnMove<K> = Rc<dyn Fn(&SortableMove<K>, &mut Window, &mut App)>;

/// A list whose items reorder by dragging, by touch and by keyboard. With a
/// [`group`](Self::group), lists of the same group take each other's items: a kanban board is
/// a row of them.
///
/// The list is one Tab stop however long it is: the arrows move the focus between its items,
/// Space or Enter picks the focused one up, then the arrows move it one place at a time (and to
/// the neighbouring list of the group across the axis), and Space or Enter drops it.
///
/// ```ignore
/// Sortable::new("todo", self.todo.clone())
///     .group("board")
///     .label("To do")
///     .item(|card, _, _, _| card_view(card))
///     .on_move(cx.listener(|this, m: &SortableMove<CardId>, _, cx| this.apply(m, cx)))
/// ```
#[derive(IntoElement)]
pub struct Sortable<K: Clone + Eq + Hash + 'static> {
    id: ElementId,
    list: Dnd,
    keys: Vec<K>,
    axis: Axis,
    group: Option<SharedString>,
    placeholder: Placeholder,
    item: Option<RenderItem<K>>,
    empty: Option<RenderEmpty>,
    item_label: Option<ItemLabel<K>>,
    label: Option<SharedString>,
    can_move: Option<CanMove<K>>,
    on_move: Option<OnMove<K>>,
    gap: Rems,
    scroll: Option<ScrollHandle>,
}

impl<K: Clone + Eq + Hash + 'static> Sortable<K> {
    pub fn new(id: impl Into<ElementId>, keys: impl IntoIterator<Item = K>) -> Self {
        let id = id.into();
        Self {
            list: dnd(id.clone()).drop_indicator(false),
            id,
            keys: keys.into_iter().collect(),
            axis: Axis::Vertical,
            group: None,
            placeholder: Placeholder::Gap,
            item: None,
            empty: None,
            item_label: None,
            label: None,
            can_move: None,
            on_move: None,
            gap: rems(0.5),
            scroll: None,
        }
    }

    /// Vertical unless given.
    pub fn axis(mut self, axis: Axis) -> Self {
        self.axis = axis;
        self
    }

    /// Lists with the same group take each other's items.
    pub fn group(mut self, group: impl Into<SharedString>) -> Self {
        self.group = Some(group.into());
        self
    }

    /// [`Placeholder::Gap`] unless given.
    pub fn placeholder(mut self, placeholder: Placeholder) -> Self {
        self.placeholder = placeholder;
        self
    }

    /// Draws an item. It's also drawn as the drag's preview, with
    /// [`SortableItem::preview`] set.
    pub fn item(
        mut self,
        render: impl Fn(&K, &SortableItem, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.item = Some(Rc::new(render));
        self
    }

    /// Drawn when the list has no items; the list still takes them.
    pub fn empty(mut self, render: impl Fn(&mut Window, &mut App) -> AnyElement + 'static) -> Self {
        self.empty = Some(Rc::new(render));
        self
    }

    /// The list's accessible name, said as a keyboard drag moves over it ("To do").
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Each item's accessible name, said as it's picked up and dropped.
    pub fn item_label(mut self, label: impl Fn(&K) -> SharedString + 'static) -> Self {
        self.item_label = Some(Rc::new(label));
        self
    }

    /// Whether a move is allowed: no gap opens, and nothing drops, where it isn't.
    pub fn can_move(mut self, f: impl Fn(&SortableMove<K>, &App) -> bool + 'static) -> Self {
        self.can_move = Some(Rc::new(f));
        self
    }

    /// Called when an item is dropped somewhere new.
    pub fn on_move(
        mut self,
        f: impl Fn(&SortableMove<K>, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_move = Some(Rc::new(f));
        self
    }

    /// The space between items: half a rem unless given.
    pub fn gap(mut self, gap: Rems) -> Self {
        self.gap = gap;
        self
    }

    /// Makes the list scroll along its axis with this handle, and scroll by itself while a
    /// drag is near its ends.
    pub fn scroll(mut self, handle: &ScrollHandle) -> Self {
        self.scroll = Some(handle.clone());
        self
    }
}

impl<K: Clone + Eq + Hash + 'static> Styled for Sortable<K> {
    fn style(&mut self) -> &mut StyleRefinement {
        self.list.style()
    }
}

struct ListState {
    focus: FocusHandle,
    focused: usize,
    /// Each item's place in the layout, as last drawn, unmoved.
    rects: Rc<RefCell<Vec<Bounds<Pixels>>>>,
    /// The list's own bounds, as last drawn.
    bounds: Rc<Cell<Bounds<Pixels>>>,
    anims: HashMap<u64, Slide>,
    /// The keys' hashes as last drawn: when they change, the host has moved something.
    keys: Vec<u64>,
    /// An item dropped here, to settle from where its preview was let go.
    settle: Option<(u64, Point<Pixels>)>,
}

/// An item sliding from one offset to another.
#[derive(Clone, Copy)]
pub(crate) struct Slide {
    pub from: Point<Pixels>,
    pub to: Point<Pixels>,
    pub start: Instant,
}

impl Slide {
    pub fn progress(&self, now: Instant) -> f32 {
        (now.saturating_duration_since(self.start).as_secs_f32() / SLIDE_DURATION.as_secs_f32())
            .min(1.)
    }

    pub fn at(&self, now: Instant) -> Point<Pixels> {
        let t = self.progress(now);
        let eased = 1. - (1. - t).powi(3);
        self.from + (self.to - self.from) * eased
    }
}

pub(crate) fn hash<K: Hash>(key: &K) -> u64 {
    let mut h = DefaultHasher::new();
    key.hash(&mut h);
    h.finish()
}

pub(crate) fn item_id(list: &ElementId, key: u64) -> ElementId {
    ElementId::NamedChild(Arc::new(list.clone()), format!("item-{key:x}").into())
}

fn along(p: Point<Pixels>, axis: Axis) -> Pixels {
    match axis {
        Axis::Vertical => p.y,
        Axis::Horizontal => p.x,
    }
}

fn length(s: Size<Pixels>, axis: Axis) -> Pixels {
    match axis {
        Axis::Vertical => s.height,
        Axis::Horizontal => s.width,
    }
}

fn on_axis(a: Pixels, cross: Pixels, axis: Axis) -> Point<Pixels> {
    match axis {
        Axis::Vertical => point(cross, a),
        Axis::Horizontal => point(a, cross),
    }
}

fn mid(r: &Bounds<Pixels>, axis: Axis) -> Pixels {
    along(r.center(), axis)
}

/// Where an item at `at` along the axis would land, counted without `skip`.
fn index_at(rects: &[Bounds<Pixels>], skip: Option<usize>, at: Pixels, axis: Axis) -> usize {
    rects
        .iter()
        .enumerate()
        .filter(|(i, r)| Some(*i) != skip && mid(r, axis) < at)
        .count()
}

/// Each item's offset from its place, and where the gap starts, with an item `size` long landing
/// at `to` (counted without `skip`), from `origin` along the axis.
fn arrange(
    rects: &[Bounds<Pixels>],
    skip: Option<usize>,
    to: usize,
    size: Pixels,
    gap: Pixels,
    origin: Pixels,
    axis: Axis,
) -> (Vec<Pixels>, Pixels) {
    let mut offsets = vec![px(0.); rects.len()];
    let mut at = origin;
    let mut gap_start = None;
    let mut placed = 0;
    for (i, r) in rects.iter().enumerate() {
        if Some(i) == skip {
            continue;
        }
        if placed == to {
            gap_start = Some(at);
            at += size + gap;
        }
        offsets[i] = at - along(r.origin, axis);
        at += length(r.size, axis) + gap;
        placed += 1;
    }
    let gap_start = gap_start.unwrap_or(at);
    if let Some(skip) = skip {
        offsets[skip] = gap_start - along(rects[skip].origin, axis);
    }
    (offsets, gap_start)
}

/// The keyboard's places in a list: between each pair of items (counted without `skip`), and
/// before the first and after the last, on the list's middle line across the axis.
fn slots(
    rects: &[Bounds<Pixels>],
    skip: Option<usize>,
    visible: Bounds<Pixels>,
    axis: Axis,
) -> Vec<Point<Pixels>> {
    let kept: Vec<&Bounds<Pixels>> = rects
        .iter()
        .enumerate()
        .filter(|(i, _)| Some(*i) != skip)
        .map(|(_, r)| r)
        .collect();
    let cross = match axis {
        Axis::Vertical => visible.center().x,
        Axis::Horizontal => visible.center().y,
    };
    if kept.is_empty() {
        return vec![visible.center()];
    }
    let mut out = Vec::with_capacity(kept.len() + 1);
    let first = kept[0];
    out.push((along(first.origin, axis) + mid(first, axis)) / 2.);
    for pair in kept.windows(2) {
        out.push((mid(pair[0], axis) + mid(pair[1], axis)) / 2.);
    }
    let last = kept[kept.len() - 1];
    out.push((mid(last, axis) + along(last.bottom_right(), axis)) / 2.);
    out.into_iter().map(|a| on_axis(a, cross, axis)).collect()
}

impl<K: Clone + Eq + Hash + 'static> RenderOnce for Sortable<K> {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let list_id = self.id.clone();
        let state: Entity<ListState> = window.use_keyed_state(
            ElementId::NamedChild(Arc::new(list_id.clone()), "dnd-sortable".into()),
            cx,
            |_, cx| ListState {
                focus: cx.focus_handle(),
                focused: 0,
                rects: Rc::default(),
                bounds: Rc::default(),
                anims: HashMap::new(),
                keys: Vec::new(),
                settle: None,
            },
        );
        let now = cx.background_executor().now();
        let reduce = cx.reduce_motion();
        let axis = self.axis;
        let n = self.keys.len();
        let hashes: Vec<u64> = self.keys.iter().map(hash).collect();
        let gap = self.gap.to_pixels(window.rem_size());
        let monitor = DragMonitor::of(cx);
        let drag = monitor
            .item()
            .and_then(|i| i.get::<SortableDrag<K>>())
            .filter(|d| d.group == self.group || d.list == list_id)
            .cloned();
        let landing = state::sort_landing(cx)
            .filter(|(l, _)| l == &list_id)
            .map(|(_, to)| to);

        let (rects, list_bounds, focus, focused) = state.update(cx, |s, _| {
            if s.keys != hashes {
                // The host moved something: everything's at its new place already.
                s.anims.clear();
                s.keys = hashes.clone();
                if let Some((key, from)) = s.settle.take()
                    && !reduce
                {
                    s.anims.insert(
                        key,
                        Slide {
                            from,
                            to: point(px(0.), px(0.)),
                            start: now,
                        },
                    );
                }
            } else if !monitor.is_dragging() {
                s.settle = None;
            }
            s.focused = s.focused.min(n.saturating_sub(1));
            (
                s.rects.borrow().clone(),
                s.bounds.get(),
                s.focus.clone(),
                s.focused,
            )
        });
        let measured = rects.len() == n;
        // The dragged item's index here, if it's from this list.
        let skip = drag
            .as_ref()
            .filter(|d| d.list == list_id && d.from < n && hashes[d.from] == hash(&d.key))
            .map(|d| d.from);
        let shifting = !matches!(self.placeholder, Placeholder::Line);

        // Where each item is drawn, and the gap.
        let mut targets = vec![Point::<Pixels>::default(); n];
        let mut gap_at: Option<Pixels> = None;
        if let Some(d) = &drag
            && measured
        {
            let origin = rects
                .first()
                .map(|r| along(r.origin, axis))
                .unwrap_or(along(list_bounds.origin, axis));
            match landing {
                Some(to) => {
                    let (offsets, start) =
                        arrange(&rects, skip, to, length(d.size, axis), gap, origin, axis);
                    gap_at = Some(start);
                    if shifting {
                        for (t, o) in targets.iter_mut().zip(offsets) {
                            *t = on_axis(o, px(0.), axis);
                        }
                    }
                }
                // Gone to another list: this one closes the gap.
                None if skip.is_some() && shifting && state::sort_landing(cx).is_some() => {
                    let (offsets, _) = arrange(&rects, skip, n, px(0.), gap, origin, axis);
                    for (i, (t, o)) in targets.iter_mut().zip(offsets).enumerate() {
                        if Some(i) != skip {
                            *t = on_axis(o, px(0.), axis);
                        }
                    }
                }
                None => {}
            }
        }

        // Slide each item to where it's drawn.
        let mut offsets = Vec::with_capacity(n);
        let mut moving = false;
        state.update(cx, |s, _| {
            for (key, target) in hashes.iter().zip(&targets) {
                let slide = s.anims.entry(*key).or_insert(Slide {
                    from: *target,
                    to: *target,
                    start: now,
                });
                if slide.to != *target {
                    *slide = Slide {
                        from: if reduce { *target } else { slide.at(now) },
                        to: *target,
                        start: now,
                    };
                }
                let at = slide.at(now);
                moving |= slide.from != slide.to && slide.progress(now) < 1.;
                offsets.push(at);
            }
        });
        if moving {
            window.request_animation_frame();
        }

        let render = self.item.clone().unwrap_or_else(|| {
            Rc::new(|_: &K, _: &SortableItem, _: &mut Window, _: &mut App| div().into_any_element())
        });
        let list_focused = focus.is_focused(window);
        let ring = gpui_core::focus_ring(cx);
        let keyboard_ring = list_focused && window.last_input_was_keyboard();
        let dragged_style = match self.placeholder {
            Placeholder::Line => 0.4,
            _ => 0.,
        };
        let rects_cell = state.read(cx).rects.clone();
        rects_cell.borrow_mut().resize(n, Bounds::default());

        let mut children: Vec<AnyElement> = Vec::with_capacity(n + 2);
        for (index, key) in self.keys.iter().enumerate() {
            let h = hashes[index];
            let id = item_id(&list_id, h);
            let dragging = skip == Some(index);
            let info = SortableItem {
                index,
                dragging,
                focused: list_focused && focused == index && !monitor.is_dragging(),
                preview: false,
            };
            let content = render(key, &info, window, cx);
            let (k, list, group, r) = (
                key.clone(),
                list_id.clone(),
                self.group.clone(),
                rects_cell.clone(),
            );
            let preview_render = render.clone();
            let mut item = dnd(id.clone())
                .role(Role::ListItem)
                .aria_position_in_set(index + 1)
                .aria_size_of_set(n)
                .drop_indicator(false)
                .source(
                    DragSource::new(move |_| SortableDrag {
                        list: list.clone(),
                        key: k.clone(),
                        from: index,
                        size: r.borrow().get(index).map(|b| b.size).unwrap_or_default(),
                        group: group.clone(),
                    })
                    .keyboard(false)
                    .is_dragging({
                        let (list, h) = (list_id.clone(), h);
                        move |d: &[SortableDrag<K>]| d[0].list == list && hash(&d[0].key) == h
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
                .dragging_style(move |s| s.opacity(dragged_style))
                .child(content);
            if let Some(label) = &self.item_label {
                item = item.label(label(key));
            }
            if info.focused {
                item = item.aria_active_descendant();
                if keyboard_ring {
                    item = item.shadow(vec![ring.clone()]);
                }
            }
            children.push(
                Placed {
                    child: item.into_any_element(),
                    offset: offsets[index],
                    record: Some((rects_cell.clone(), index)),
                }
                .into_any_element(),
            );
        }

        // What shows where it'd land.
        if let (Some(d), Some(start), true) = (&drag, gap_at, measured) {
            let cross_origin = rects
                .first()
                .map(|r| match axis {
                    Axis::Vertical => r.origin.x,
                    Axis::Horizontal => r.origin.y,
                })
                .unwrap_or(match axis {
                    Axis::Vertical => list_bounds.origin.x,
                    Axis::Horizontal => list_bounds.origin.y,
                });
            let cross_len = rects
                .first()
                .map(|r| match axis {
                    Axis::Vertical => r.size.width,
                    Axis::Horizontal => r.size.height,
                })
                .unwrap_or(match axis {
                    Axis::Vertical => d.size.width,
                    Axis::Horizontal => d.size.height,
                });
            let to = landing.unwrap_or(0);
            let (at, size): (Point<Pixels>, Size<Pixels>) = match &self.placeholder {
                Placeholder::Line => {
                    let line = px(2.);
                    let kept: Vec<&Bounds<Pixels>> = rects
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| Some(*i) != skip)
                        .map(|(_, r)| r)
                        .collect();
                    let a = match kept.get(to) {
                        Some(r) => along(r.origin, axis) - gap / 2.,
                        None => kept
                            .last()
                            .map(|r| along(r.bottom_right(), axis) + gap / 2.)
                            .unwrap_or(start),
                    } - line / 2.;
                    let size = match axis {
                        Axis::Vertical => Size::new(cross_len, line),
                        Axis::Horizontal => Size::new(line, cross_len),
                    };
                    (on_axis(a, cross_origin, axis), size)
                }
                _ => {
                    let size = match axis {
                        Axis::Vertical => Size::new(cross_len, d.size.height),
                        Axis::Horizontal => Size::new(d.size.width, cross_len),
                    };
                    (on_axis(start, cross_origin, axis), size)
                }
            };
            let local = at - list_bounds.origin;
            let mark: Option<AnyElement> = match &self.placeholder {
                Placeholder::Gap => None,
                Placeholder::Line => Some(
                    div()
                        .debug_selector(|| "dnd-sortable-line".into())
                        .size_full()
                        .rounded_full()
                        .bg(cx.theme().ring)
                        .into_any_element(),
                ),
                Placeholder::Custom(f) => Some(f(size, window, cx)),
            };
            if let Some(mark) = mark {
                children.push(
                    div()
                        .absolute()
                        .left(local.x)
                        .top(local.y)
                        .w(size.width)
                        .h(size.height)
                        .child(mark)
                        .into_any_element(),
                );
            }
            // An item from another list makes this one longer by its size.
            if skip.is_none() && shifting {
                let extra = length(d.size, axis) + gap;
                children.push(
                    match axis {
                        Axis::Vertical => div().h(extra).w(px(1.)),
                        Axis::Horizontal => div().w(extra).h(px(1.)),
                    }
                    .flex_none()
                    .into_any_element(),
                );
            }
        }
        if n == 0
            && gap_at.is_none()
            && let Some(empty) = &self.empty
        {
            children.push(empty(window, cx));
        }

        // The list: a target, one Tab stop, and maybe a scroll container.
        let (rects_hover, rects_drop, rects_slots, rects_can) = (
            rects_cell.clone(),
            rects_cell.clone(),
            rects_cell.clone(),
            rects_cell.clone(),
        );
        let landing_for = {
            let list_id = list_id.clone();
            move |item: &DragItem, at: Point<Pixels>, rects: &[Bounds<Pixels>]| {
                let d = item.get::<SortableDrag<K>>()?;
                let skip = (d.list == list_id)
                    .then_some(d.from)
                    .filter(|f| *f < rects.len());
                Some((d.clone(), index_at(rects, skip, along(at, axis), axis)))
            }
        };
        let landing_for = Rc::new(landing_for);
        let group = self.group.clone();
        let can_move = self.can_move.clone();
        let target = {
            let (l1, l2, l3, l4) = (
                landing_for.clone(),
                landing_for.clone(),
                landing_for.clone(),
                landing_for.clone(),
            );
            let (id1, id2, id3, id4) = (
                list_id.clone(),
                list_id.clone(),
                list_id.clone(),
                list_id.clone(),
            );
            let label = self.label.clone();
            let on_move = self.on_move.clone();
            let state_drop = state.clone();
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
                    let Some((d, to)) = l1(item, at, &rects_can.borrow()) else {
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
                    if let Some((_, to)) = l2(item, at, &rects_hover.borrow()) {
                        state::shared(cx).borrow_mut().sort_landing = Some((id2.clone(), to));
                    }
                })
                .keyboard_slots(move |item, visible| {
                    let rects = rects_slots.borrow();
                    let skip = item
                        .get::<SortableDrag<K>>()
                        .filter(|d| d.list == id3)
                        .map(|d| d.from)
                        .filter(|f| *f < rects.len());
                    slots(&rects, skip, visible, axis)
                })
                .describe(move |item, m| {
                    let rects = rects_drop.borrow();
                    let Some((d, to)) = m.client_offset().and_then(|at| l3(item, at, &rects))
                    else {
                        return label.clone().unwrap_or_default();
                    };
                    let count = rects.len() + usize::from(d.list != id4);
                    match &label {
                        Some(label) => format!("{label}, position {} of {count}", to + 1).into(),
                        None => format!("position {} of {count}", to + 1).into(),
                    }
                })
                .on_drop({
                    let list_id = list_id.clone();
                    let rects = rects_cell.clone();
                    move |item, m, window, cx| {
                        let Some(at) = m.client_offset() else {
                            return DropOutcome::Pass;
                        };
                        let Some((d, to)) = l4(item, at, &rects.borrow()) else {
                            return DropOutcome::Pass;
                        };
                        let mv = SortableMove {
                            key: d.key.clone(),
                            from: (d.list.clone(), d.from),
                            to: (list_id.clone(), to),
                        };
                        if mv.moves() {
                            // It settles from where its preview was let go into its new place.
                            let r = rects.borrow();
                            let skip = (d.list == list_id)
                                .then_some(d.from)
                                .filter(|f| *f < r.len());
                            let origin = r.first().map(|b| along(b.origin, axis));
                            let cross = r.first().map(|b| match axis {
                                Axis::Vertical => b.origin.x,
                                Axis::Horizontal => b.origin.y,
                            });
                            if let (Some(origin), Some(cross), Some(preview)) =
                                (origin, cross, m.source_client_offset())
                            {
                                let (_, start) =
                                    arrange(&r, skip, to, length(d.size, axis), gap, origin, axis);
                                let place = on_axis(start, cross, axis);
                                state_drop.update(cx, |s, _| {
                                    s.settle = Some((hash(&d.key), preview - place));
                                });
                            }
                            drop(r);
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
                            window.focus(&focus, cx);
                        }
                        DropOutcome::Handled(Some(DropResult::new(mv)))
                    }
                })
        };

        let keys = self.keys.clone();
        let key_list = list_id.clone();
        let state_keys = state.clone();
        let (prev, next) = match axis {
            Axis::Vertical => ("up", "down"),
            Axis::Horizontal => ("left", "right"),
        };
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
                    k if k == prev => Some(|i, _| i.saturating_sub(1)),
                    k if k == next => Some(|i, n| (i + 1).min(n.saturating_sub(1))),
                    "home" => Some(|_, _| 0),
                    "end" => Some(|_, n| n.saturating_sub(1)),
                    _ => None,
                };
                if let Some(step) = step {
                    state_keys.update(cx, |s, cx| {
                        s.focused = step(s.focused, n);
                        cx.notify();
                    });
                    cx.stop_propagation();
                }
            })
            .relative()
            .flex()
            .gap(self.gap)
            .children(children);
        if let Some(label) = &self.label {
            list = list.label(label.clone());
        }
        list = match axis {
            Axis::Vertical => list.flex_col(),
            Axis::Horizontal => list.flex_row(),
        };
        let bounds_cell = state.read(cx).bounds.clone();
        let element = match &self.scroll {
            Some(handle) => {
                let list = match axis {
                    Axis::Vertical => list.overflow_y_scroll(),
                    Axis::Horizontal => list.overflow_x_scroll(),
                }
                .track_scroll(handle);
                let options = match axis {
                    Axis::Vertical => AutoScroll::vertical(),
                    Axis::Horizontal => AutoScroll::horizontal(),
                };
                list.auto_scroll(handle, options).into_any_element()
            }
            None => list.into_any_element(),
        };
        Placed {
            child: element,
            offset: Point::default(),
            record: None,
        }
        .measured(bounds_cell)
    }
}

/// Draws its child moved by `offset` without moving it in the layout, and notes where the
/// layout put it.
pub(crate) struct Placed {
    pub child: AnyElement,
    pub offset: Point<Pixels>,
    pub record: Option<(Rects, usize)>,
}

impl Placed {
    fn measured(self, cell: Rc<Cell<Bounds<Pixels>>>) -> Measured {
        Measured { placed: self, cell }
    }
}

impl IntoElement for Placed {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Placed {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some((rects, index)) = &self.record
            && let Some(slot) = rects.borrow_mut().get_mut(*index)
        {
            *slot = bounds;
        }
        window.with_element_offset(self.offset, |window| self.child.prepaint(window, cx));
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
    }
}

/// A [`Placed`] that notes its own bounds.
struct Measured {
    placed: Placed,
    cell: Rc<Cell<Bounds<Pixels>>>,
}

impl IntoElement for Measured {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Measured {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        self.placed.request_layout(id, inspector, window, cx)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.cell.set(bounds);
        self.placed
            .prepaint(id, inspector, bounds, state, window, cx);
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut (),
        prepaint: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.placed
            .paint(id, inspector, bounds, layout, prepaint, window, cx);
    }
}
