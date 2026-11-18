//! The app's drag state: the registry of sources and targets per window, the drag in progress,
//! and the collectors to tell when it changes. Callbacks are never called with it borrowed.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::time::Duration;
// GPUI's clock: std's Instant, but in a browser (which has none) web-time's.
#[cfg(not(target_family = "wasm"))]
use std::time::Instant;
#[cfg(target_family = "wasm")]
use web_time::Instant;

use gpui::{
    App, Bounds, ElementId, EntityId, FocusHandle, Global, KeystrokeEvent, Modifiers, Pixels,
    Point, SharedString, Subscription, Window, WindowId,
};

use crate::item::{DragItem, DropResult};
use crate::monitor::{DragMonitor, DropMonitor, Snapshot, TargetSnap};
use crate::source::{Backend, EndInfo, PreviewSpec, SourceSpec};
use crate::target::{DropOutcome, DropTarget};

/// A source as last drawn. Its element holds the `Rc` in its element state, so it lives as long
/// as the element is drawn; the registry holds a `Weak`.
pub(crate) struct SourceRecord {
    pub id: ElementId,
    pub bounds: Bounds<Pixels>,
    pub spec: Rc<SourceSpec>,
    /// A shared source's key for this element: what its item is made from.
    pub key: Option<Rc<dyn std::any::Any>>,
    /// The element's label, the item's in announcements.
    pub label: Option<SharedString>,
    /// Where it takes a press.
    pub hitbox: Option<gpui::Hitbox>,
    /// What has the keyboard's focus when the keyboard can pick it up.
    pub focus: Option<FocusHandle>,
}

/// A target as last drawn.
pub(crate) struct TargetRecord {
    pub id: ElementId,
    pub bounds: Bounds<Pixels>,
    /// Its bounds cut by its content mask: the part a pointer can be over.
    pub visible: Bounds<Pixels>,
    /// The order it was laid out in: a later one is drawn over an earlier one.
    pub seq: u64,
    /// The nearest target it's drawn inside.
    pub parent: Option<Weak<TargetRecord>>,
    pub spec: Rc<DropTarget>,
    pub label: Option<SharedString>,
}

#[derive(Default)]
pub(crate) struct Registry {
    pub sources: Vec<Weak<SourceRecord>>,
    pub targets: Vec<Weak<TargetRecord>>,
    // When to drop the dead entries next.
    prune_at: usize,
}

impl Registry {
    pub fn source(&self, id: &ElementId) -> Option<Rc<SourceRecord>> {
        self.sources
            .iter()
            .rev()
            .filter_map(Weak::upgrade)
            .find(|s| &s.id == id)
    }

    /// The source the keyboard can pick up that has the focus.
    pub fn focused_source(&self, window: &Window) -> Option<Rc<SourceRecord>> {
        self.sources
            .iter()
            .rev()
            .filter_map(Weak::upgrade)
            .find(|s| s.spec.keyboard && s.focus.as_ref().is_some_and(|f| f.is_focused(window)))
    }

    /// The targets drawn in the last frame.
    pub fn live_targets(&self) -> Vec<Rc<TargetRecord>> {
        self.targets.iter().filter_map(Weak::upgrade).collect()
    }

    /// The live sources whose hitbox `hit` takes, the last registered (innermost) first.
    pub fn sources_where<'a>(
        &'a self,
        hit: impl Fn(&gpui::Hitbox) -> bool + 'a,
    ) -> impl Iterator<Item = Rc<SourceRecord>> + 'a {
        self.sources
            .iter()
            .rev()
            .filter_map(Weak::upgrade)
            .filter(move |s| s.hitbox.as_ref().is_some_and(&hit))
    }

    pub fn add_source(&mut self, source: &Rc<SourceRecord>) {
        self.sources.push(Rc::downgrade(source));
        self.prune();
    }

    pub fn add_target(&mut self, target: &Rc<TargetRecord>) {
        self.targets.push(Rc::downgrade(target));
        self.prune();
    }

    // Each frame adds a record per element and its last one dies, so the dead are dropped
    // whenever the lists have doubled since the last time.
    fn prune(&mut self) {
        let len = self.sources.len() + self.targets.len();
        if len >= self.prune_at.max(64) {
            self.sources.retain(|s| s.strong_count() > 0);
            self.targets.retain(|t| t.strong_count() > 0);
            self.prune_at = 2 * (self.sources.len() + self.targets.len());
        }
    }

    /// The targets that [catch](DropTarget::catch_outside) a drag of this item outside their
    /// bounds, but for those in `under`: the last drawn first.
    pub fn catchers(&self, item: &DragItem, under: &[Rc<TargetRecord>]) -> Vec<Rc<TargetRecord>> {
        let ty = item.item_type();
        let mut found: Vec<_> = self
            .targets
            .iter()
            .filter_map(Weak::upgrade)
            .filter(|t| {
                t.spec.catch_outside
                    && t.spec.accepts(ty)
                    && !under.iter().any(|u| Rc::ptr_eq(u, t))
            })
            .collect();
        found.sort_by_key(|t| std::cmp::Reverse(t.seq));
        found
    }

    /// The targets under `at` that accept the item, outermost first: the topmost accepting
    /// target whose visible part holds the point, and the accepting targets it's drawn inside
    /// that hold it too.
    pub fn targets_at(&self, at: Point<Pixels>, item: &DragItem) -> Vec<Rc<TargetRecord>> {
        let ty = item.item_type();
        let holds = |t: &TargetRecord| t.visible.contains(&at) && t.spec.accepts(ty);
        let top = self
            .targets
            .iter()
            .filter_map(Weak::upgrade)
            .filter(|t| holds(t))
            .max_by_key(|t| t.seq);
        let mut stack = Vec::new();
        let mut next = top;
        while let Some(t) = next {
            next = t.parent.as_ref().and_then(Weak::upgrade);
            if holds(&t) {
                stack.push(t);
            }
        }
        stack.reverse();
        stack
    }
}

/// A press on a source that hasn't moved far enough to be a drag yet.
pub(crate) struct Pending {
    pub source: Rc<SourceRecord>,
    pub at: Point<Pixels>,
    pub threshold: Pixels,
    pub window: WindowId,
}

pub(crate) struct ActiveDrag {
    pub window: WindowId,
    pub spec: Rc<SourceSpec>,
    pub snap: Rc<Snapshot>,
    /// The targets under the pointer, as `snap.targets`.
    pub targets: Vec<Rc<TargetRecord>>,
    /// Set once it's ended without a drop and its preview is going back to the source.
    pub returning: Option<Returning>,
    /// What a keyboard drag last said about where it is.
    pub said_over: Option<SharedString>,
}

/// How long a preview takes to go back to its source after a cancel.
pub const RETURN_DURATION: Duration = Duration::from_millis(200);

/// A preview on its way back to its source. The drag ends (the source hears `on_end`) when it
/// lands, as a drag on the Mac ends once its image has slid back.
pub(crate) struct Returning {
    pub from: Point<Pixels>,
    pub to: Point<Pixels>,
    pub started: Instant,
    pub end: EndInfo,
    /// Tells this return's timer from an earlier one's.
    pub generation: u64,
}

impl Returning {
    /// How far back it is, from 0 to 1.
    pub fn progress(&self, now: Instant) -> f32 {
        (now.saturating_duration_since(self.started).as_secs_f32() / RETURN_DURATION.as_secs_f32())
            .min(1.)
    }

    /// Where the preview is: an ease-out from where it was let go to the source.
    pub fn origin(&self, now: Instant) -> Point<Pixels> {
        let t = self.progress(now);
        let eased = 1. - (1. - t).powi(3);
        self.from + (self.to - self.from) * eased
    }
}

impl ActiveDrag {
    /// Where the preview's top-left is now.
    pub fn preview_origin(&self, now: Instant) -> Point<Pixels> {
        match &self.returning {
            Some(r) => r.origin(now),
            None => self.snap.current - self.snap.grab(),
        }
    }
}

/// What a provider keeps in its element state, the window's claim on it when it's active.
#[derive(Debug, Default)]
pub(crate) struct ProviderToken;

pub(crate) trait Collector {
    /// Works the value out again; the view to notify if it changed.
    fn update(&self, monitor: &DragMonitor) -> Option<EntityId>;
}

#[derive(Default)]
pub(crate) struct DndState {
    pub registries: HashMap<WindowId, Registry>,
    /// The view each window's working provider is drawn in: the one redrawn as the drag moves.
    pub provider_views: HashMap<WindowId, EntityId>,
    /// Whether each window's provider lets the keyboard drag, and what it says while it does.
    pub keyboard: HashMap<WindowId, bool>,
    /// Whether each window's provider lets touches drag.
    pub touch: HashMap<WindowId, bool>,
    pub announcements: HashMap<WindowId, crate::keyboard::Announcements>,
    /// What each window's live region says.
    pub said: HashMap<WindowId, SharedString>,
    pub drag: Option<ActiveDrag>,
    pub pending: Option<Pending>,
    /// A touch on an at-once source that hasn't moved past the slop yet.
    pub touch_pending: Option<Pending>,
    /// Each window's scroll containers that auto-scroll, each stamped with the frame it was
    /// drawn in: those of frames before the active provider's last paint are dropped then.
    pub zones: HashMap<WindowId, Vec<crate::autoscroll::Zone>>,
    /// The frame zones drawn now are stamped with: moved on as the active provider paints.
    pub zone_stamp: u64,
    /// Each window's active provider: the one that follows the drag, listens for its input and
    /// draws its preview. It holds the `Rc` in its element state, so it stays active while it's
    /// drawn (from cache too), and another claims the window once it isn't.
    pub active: HashMap<WindowId, Weak<ProviderToken>>,
    /// Auto-scroll under way.
    pub auto_run: Option<crate::autoscroll::Run>,
    /// The sortable list a sortable item would land in, and where: set as lists are hovered.
    pub sort_landing: Option<(ElementId, usize)>,
    pub collectors: Vec<Weak<dyn Collector>>,
    /// The targets being laid out around the element being laid out now.
    pub target_stack: Vec<Rc<TargetRecord>>,
    pub seq: u64,
    _keys: Option<Subscription>,
}

impl DndState {
    pub fn registry(&mut self, window: WindowId) -> &mut Registry {
        self.registries.entry(window).or_default()
    }

    pub fn next_seq(&mut self) -> u64 {
        self.seq += 1;
        self.seq
    }
}

struct DndGlobal(Rc<RefCell<DndState>>);

impl Global for DndGlobal {}

/// The app's drag state, made on first use.
pub(crate) fn shared(cx: &mut App) -> Rc<RefCell<DndState>> {
    if let Some(global) = cx.try_global::<DndGlobal>() {
        return global.0.clone();
    }
    let state = Rc::new(RefCell::new(DndState::default()));
    // Keys reach the drag before any binding or element: Escape cancels whatever has the
    // focus, and a keyboard drag holds the keyboard.
    let keys = cx.intercept_keystrokes(|event: &KeystrokeEvent, window, cx| {
        if crate::keyboard::key(event, window, cx) {
            cx.stop_propagation();
        }
    });
    state.borrow_mut()._keys = Some(keys);
    cx.set_global(DndGlobal(state.clone()));
    state
}

pub(crate) fn snapshot(cx: &App) -> Option<Rc<Snapshot>> {
    let global = cx.try_global::<DndGlobal>()?;
    let state = global.0.borrow();
    state.drag.as_ref().map(|d| d.snap.clone())
}

/// The drag's source's cursor, if it gives one.
pub(crate) fn drag_cursor(cx: &App) -> Option<crate::source::CursorErased> {
    let global = cx.try_global::<DndGlobal>()?;
    let state = global.0.borrow();
    state.drag.as_ref().and_then(|d| d.spec.cursor.clone())
}

/// The drag in this window, if there is one: whether its preview is on its way back.
pub(crate) fn drag_in(window: &Window, cx: &App) -> Option<bool> {
    let id = window.window_handle().window_id();
    let global = cx.try_global::<DndGlobal>()?;
    let state = global.0.borrow();
    let drag = state.drag.as_ref().filter(|d| d.window == id)?;
    Some(drag.returning.is_some())
}

pub(crate) fn said(window: &Window, cx: &App) -> Option<SharedString> {
    let global = cx.try_global::<DndGlobal>()?;
    let state = global.0.borrow();
    state.said.get(&window_id(window)).cloned()
}

/// Where a sortable item would land now.
pub(crate) fn sort_landing(cx: &App) -> Option<(ElementId, usize)> {
    let global = cx.try_global::<DndGlobal>()?;
    let state = global.0.borrow();
    state.sort_landing.clone()
}

/// Whether this window has an active provider (one drawn in this frame or the last).
pub(crate) fn has_provider(state: &DndState, window: WindowId) -> bool {
    state
        .active
        .get(&window)
        .is_some_and(|t| t.strong_count() > 0)
}

pub(crate) fn window_id(window: &Window) -> WindowId {
    window.window_handle().window_id()
}

/// Begins a drag from `source`, pressed at `initial`. False if it can't be dragged now.
pub(crate) fn begin(
    source: &SourceRecord,
    initial: Point<Pixels>,
    backend: Backend,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let spec = source.spec.clone();
    if spec.can_drag.as_ref().is_some_and(|f| !f(cx)) {
        return false;
    }
    let Some(item) = (spec.make)(source.key.as_deref(), source.label.clone(), cx) else {
        return false;
    };
    let snap = Snapshot {
        item,
        source: source.id.clone(),
        backend,
        initial,
        current: initial,
        source_origin: source.bounds.origin,
        source_size: source.bounds.size,
        targets: Vec::new(),
        modifiers: window.modifiers(),
        did_drop: false,
        drop_result: None,
        returning: false,
    };
    let state = shared(cx);
    {
        let mut state = state.borrow_mut();
        state.pending = None;
        state.drag = Some(ActiveDrag {
            window: window_id(window),
            spec: spec.clone(),
            snap: Rc::new(snap),
            targets: Vec::new(),
            returning: None,
            said_over: None,
        });
    }
    if let Some(on_begin) = &spec.on_begin {
        on_begin(&DragMonitor::of(cx), window, cx);
    }
    move_to(initial, window.modifiers(), window, cx);
    true
}

/// Moves the drag's pointer to `at`: finds the targets under it, asks them whether they can
/// take the item and tells them it's hovering, and tells the views that collect.
pub(crate) fn move_to(at: Point<Pixels>, modifiers: Modifiers, window: &mut Window, cx: &mut App) {
    let state = shared(cx);
    let (mut records, mut snap, catchers) = {
        let mut state = state.borrow_mut();
        let Some(drag) = state.drag.as_ref().filter(|d| d.returning.is_none()) else {
            return;
        };
        let window_id = drag.window;
        let item = drag.snap.item.clone();
        let mut snap = (*drag.snap).clone();
        state.sort_landing = None;
        let records = state.registry(window_id).targets_at(at, &item);
        snap.current = at;
        snap.modifiers = modifiers;
        snap.targets = records
            .iter()
            .map(|t| TargetSnap {
                id: t.id.clone(),
                can_drop: true,
                outside: false,
            })
            .collect();
        let catchers = state.registry(window_id).catchers(&item, &records);
        (records, snap, catchers)
    };
    // Each target is asked with the pointer already where it is.
    let asking = DragMonitor {
        snap: Some(Rc::new(snap.clone())),
    };
    for (t, record) in snap.targets.iter_mut().zip(&records) {
        t.can_drop = record.spec.takes(&snap.item, &asking, cx);
    }
    // None under the pointer can take it: a target that catches it outside does, as the
    // outermost.
    if !snap.targets.iter().any(|t| t.can_drop)
        && let Some(catcher) = catchers
            .into_iter()
            .find(|c| c.spec.takes(&snap.item, &asking, cx))
    {
        snap.targets.insert(
            0,
            TargetSnap {
                id: catcher.id.clone(),
                can_drop: true,
                outside: !catcher.visible.contains(&at),
            },
        );
        records.insert(0, catcher);
    }
    let snap = Rc::new(snap);
    {
        let mut state = state.borrow_mut();
        let Some(drag) = state.drag.as_mut() else {
            return;
        };
        drag.snap = snap.clone();
        drag.targets = records.clone();
    }
    let monitor = DragMonitor {
        snap: Some(snap.clone()),
    };
    for record in records.iter().rev() {
        if let Some(hover) = &record.spec.on_hover {
            hover(&snap.item, &drop_monitor(&monitor, record), window, cx);
        }
    }
    publish(cx);
    redraw(window, cx);
    crate::autoscroll::kick(window, cx);
}

fn drop_monitor(monitor: &DragMonitor, record: &TargetRecord) -> DropMonitor {
    DropMonitor {
        monitor: monitor.clone(),
        target: record.id.clone(),
        bounds: record.bounds,
    }
}

/// Lets go at `at`: the targets there that can take the item are asked, innermost first.
/// Returns whether one took it.
pub(crate) fn release(at: Point<Pixels>, window: &mut Window, cx: &mut App) -> bool {
    if drag_in(window, cx) != Some(false) {
        return false;
    }
    move_to(at, window.modifiers(), window, cx);
    let state = shared(cx);
    let Some((records, mut snap)) = state
        .borrow()
        .drag
        .as_ref()
        .map(|d| (d.targets.clone(), (*d.snap).clone()))
    else {
        return false;
    };
    let mut taken_by: Option<ElementId> = None;
    let mut taken_label: Option<SharedString> = None;
    let mut taken_said: Option<SharedString> = None;
    let mut result: Option<DropResult> = None;
    for (record, t) in records.iter().zip(snap.targets.clone()).rev() {
        if !t.can_drop {
            continue;
        }
        let Some(on_drop) = record.spec.on_drop.clone() else {
            continue;
        };
        // What the keyboard says of the target, asked while it still has the drag.
        let (label, said) = if snap.backend == Backend::Keyboard {
            (
                crate::keyboard::target_name(record, &snap.item, cx),
                crate::keyboard::target_sentence(
                    record,
                    &snap.item,
                    crate::target::Said::Dropped,
                    cx,
                ),
            )
        } else {
            (None, None)
        };
        let monitor = DragMonitor {
            snap: Some(Rc::new(snap.clone())),
        };
        let outcome = on_drop(&snap.item, &drop_monitor(&monitor, record), window, cx);
        let (handled, more) = match outcome {
            DropOutcome::Handled(r) => (Some(r), false),
            DropOutcome::HandledContinue(r) => (Some(r), true),
            DropOutcome::Pass => (None, true),
        };
        if let Some(r) = handled {
            taken_by = Some(record.id.clone());
            taken_label = label;
            taken_said = said;
            if r.is_some() {
                result = r;
            }
            snap.did_drop = true;
            snap.drop_result = result.clone();
        }
        if !more {
            break;
        }
    }
    let end = EndInfo {
        dropped: taken_by.is_some(),
        target: taken_by,
        target_label: taken_label,
        said: taken_said,
        result,
        cancelled: false,
        backend: snap.backend,
    };
    if end.dropped {
        finish(end, window, cx);
    } else {
        go_back(end, window, cx);
    }
    true
}

/// Ends the drag without a drop: the preview goes back to the source. A drag already going
/// back lands at once.
pub(crate) fn cancel(window: &mut Window, cx: &mut App) {
    let Some(returning) = drag_in(window, cx) else {
        return;
    };
    if returning {
        land(None, window, cx);
        return;
    }
    let Some(snap) = snapshot(cx) else { return };
    go_back(
        EndInfo {
            dropped: false,
            target: None,
            target_label: None,
            said: None,
            result: None,
            cancelled: true,
            backend: snap.backend,
        },
        window,
        cx,
    );
}

/// Sends the preview back to where the source is now, and ends the drag when it lands; at once
/// when motion is reduced or nothing is drawn to go back.
fn go_back(mut end: EndInfo, window: &mut Window, cx: &mut App) {
    end.cancelled = true;
    let state = shared(cx);
    let now = cx.background_executor().now();
    let animate = !cx.reduce_motion();
    let plan = {
        let state = state.borrow();
        let Some(drag) = state.drag.as_ref() else {
            return;
        };
        let drawn = !matches!(drag.spec.preview, PreviewSpec::None);
        let to = state
            .registries
            .get(&drag.window)
            .and_then(|r| r.source(&drag.snap.source))
            .map_or(drag.snap.source_origin, |s| s.bounds.origin);
        let from = drag.preview_origin(now);
        (animate && drawn && from != to).then_some((from, to))
    };
    let Some((from, to)) = plan else {
        finish(end, window, cx);
        return;
    };
    let generation = {
        let mut state = state.borrow_mut();
        let generation = state.next_seq();
        let Some(drag) = state.drag.as_mut() else {
            return;
        };
        let mut snap = (*drag.snap).clone();
        snap.targets.clear();
        snap.returning = true;
        drag.snap = Rc::new(snap);
        drag.targets.clear();
        drag.returning = Some(Returning {
            from,
            to,
            started: now,
            end,
            generation,
        });
        generation
    };
    window
        .spawn(cx, async move |cx| {
            cx.background_executor().timer(RETURN_DURATION).await;
            cx.update(|window, cx| land(Some(generation), window, cx))
                .ok();
        })
        .detach();
    publish(cx);
    redraw(window, cx);
}

/// Ends a drag whose preview was going back: the one `generation` started, or whichever.
pub(crate) fn land(generation: Option<u64>, window: &mut Window, cx: &mut App) {
    let state = shared(cx);
    let end = {
        let mut state = state.borrow_mut();
        let Some(drag) = state.drag.as_mut() else {
            return;
        };
        match &drag.returning {
            Some(r) if generation.is_none_or(|g| g == r.generation) => {}
            _ => return,
        }
        drag.returning.take().expect("matched above").end
    };
    finish(end, window, cx);
}

fn finish(end: EndInfo, window: &mut Window, cx: &mut App) {
    let state = shared(cx);
    let Some(drag) = state.borrow_mut().drag.take() else {
        return;
    };
    state.borrow_mut().sort_landing = None;
    publish(cx);
    if let Some(on_end) = &drag.spec.on_end {
        on_end(&drag.snap.item, &end, window, cx);
    }
    if end.backend == Backend::Keyboard {
        let said = crate::keyboard::announcements(window, cx);
        let text = if let Some(own) = end.said.clone().filter(|_| end.dropped) {
            own
        } else if end.dropped {
            (said.dropped)(&drag.snap.item, end.target_label.as_ref())
        } else {
            (said.cancelled)(&drag.snap.item)
        };
        crate::keyboard::announce(text, window, cx);
    }
    redraw(window, cx);
}

/// Redraws the provider's view, for the preview and the elements styled by the drag. Not
/// `window.refresh()`, which would draw every cached view anew too: a cached view that shows
/// the drag collects what it shows, and is notified when that changes.
pub(crate) fn redraw(window: &mut Window, cx: &mut App) {
    let view = shared(cx)
        .borrow()
        .provider_views
        .get(&window_id(window))
        .copied();
    match view {
        Some(view) => cx.notify(view),
        None => window.refresh(),
    }
}

/// Tells the views whose collected values changed.
pub(crate) fn publish(cx: &mut App) {
    let state = shared(cx);
    let collectors: Vec<_> = {
        let mut state = state.borrow_mut();
        state.collectors.retain(|c| c.strong_count() > 0);
        state.collectors.clone()
    };
    let monitor = DragMonitor::of(cx);
    for collector in collectors {
        if let Some(entity) = collector.upgrade().and_then(|c| c.update(&monitor)) {
            cx.notify(entity);
        }
    }
}
