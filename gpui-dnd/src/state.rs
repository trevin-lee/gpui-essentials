//! The app's drag state: the registry of sources and targets per window, the drag in progress,
//! and the collectors to tell when it changes. Callbacks are never called with it borrowed.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use gpui::{
    App, Bounds, ElementId, EntityId, Global, KeystrokeEvent, Modifiers, Pixels, Point,
    Subscription, Window, WindowId,
};

use crate::item::{DragItem, DropResult};
use crate::monitor::{DragMonitor, DropMonitor, Snapshot, TargetSnap};
use crate::source::{Backend, EndInfo, SourceSpec};
use crate::target::{DropOutcome, DropTarget};

/// A source as last drawn. Its element holds the `Rc` in its element state, so it lives as long
/// as the element is drawn; the registry holds a `Weak`.
pub(crate) struct SourceRecord {
    pub id: ElementId,
    pub bounds: Bounds<Pixels>,
    pub spec: Rc<SourceSpec>,
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
}

#[derive(Default)]
pub(crate) struct Registry {
    pub sources: Vec<Weak<SourceRecord>>,
    pub targets: Vec<Weak<TargetRecord>>,
    // When to drop the dead entries next.
    prune_at: usize,
}

impl Registry {
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
}

pub(crate) trait Collector {
    /// Works the value out again; the view to notify if it changed.
    fn update(&self, monitor: &DragMonitor) -> Option<EntityId>;
}

#[derive(Default)]
pub(crate) struct DndState {
    pub registries: HashMap<WindowId, Registry>,
    /// The view each window's working provider is drawn in: the one redrawn as the drag moves.
    pub provider_views: HashMap<WindowId, EntityId>,
    pub drag: Option<ActiveDrag>,
    pub pending: Option<Pending>,
    pub collectors: Vec<Weak<dyn Collector>>,
    /// How many providers are being laid out around the element being laid out now.
    pub provider_depth: usize,
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
    // Escape cancels a drag whatever has the focus, before any binding sees it.
    let keys = cx.intercept_keystrokes(|event: &KeystrokeEvent, window, cx| {
        if event.keystroke.key == "escape" && is_dragging_in(window, cx) {
            cancel(window, cx);
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

fn is_dragging_in(window: &Window, cx: &App) -> bool {
    let id = window.window_handle().window_id();
    cx.try_global::<DndGlobal>()
        .is_some_and(|g| g.0.borrow().drag.as_ref().is_some_and(|d| d.window == id))
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
    let Some(item) = (spec.make)(cx) else {
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
    let (records, mut snap) = {
        let mut state = state.borrow_mut();
        let Some(drag) = state.drag.as_ref() else {
            return;
        };
        let window_id = drag.window;
        let item = drag.snap.item.clone();
        let mut snap = (*drag.snap).clone();
        let records = state.registry(window_id).targets_at(at, &item);
        snap.current = at;
        snap.modifiers = modifiers;
        snap.targets = records
            .iter()
            .map(|t| TargetSnap {
                id: t.id.clone(),
                can_drop: true,
            })
            .collect();
        (records, snap)
    };
    // Each target is asked with the pointer already where it is.
    let asking = DragMonitor {
        snap: Some(Rc::new(snap.clone())),
    };
    for (t, record) in snap.targets.iter_mut().zip(&records) {
        t.can_drop = record.spec.takes(&snap.item, &asking, cx);
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
    if !is_dragging_in(window, cx) {
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
    let mut result: Option<DropResult> = None;
    for (record, t) in records.iter().zip(snap.targets.clone()).rev() {
        if !t.can_drop {
            continue;
        }
        let Some(on_drop) = record.spec.on_drop.clone() else {
            continue;
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
    let dropped = taken_by.is_some();
    finish(
        EndInfo {
            dropped,
            target: taken_by,
            result,
            cancelled: !dropped,
            backend: snap.backend,
        },
        window,
        cx,
    );
    true
}

/// Ends the drag without a drop.
pub(crate) fn cancel(window: &mut Window, cx: &mut App) {
    let backend = match snapshot(cx) {
        Some(snap) => snap.backend,
        None => return,
    };
    finish(
        EndInfo {
            dropped: false,
            target: None,
            result: None,
            cancelled: true,
            backend,
        },
        window,
        cx,
    );
}

fn finish(end: EndInfo, window: &mut Window, cx: &mut App) {
    let state = shared(cx);
    let Some(drag) = state.borrow_mut().drag.take() else {
        return;
    };
    publish(cx);
    if let Some(on_end) = &drag.spec.on_end {
        on_end(&drag.snap.item, &end, window, cx);
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
