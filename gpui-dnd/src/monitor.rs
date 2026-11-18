//! The drag as it is now, for views and callbacks to read.

use std::ops::Deref;
use std::rc::Rc;

use gpui::{App, Bounds, ElementId, Modifiers, Pixels, Point, Size};

use crate::item::{DragItem, DropResult, ItemType};
use crate::source::Backend;
use crate::state;

/// One target under the pointer, as the monitor sees it.
#[derive(Clone, Debug)]
pub(crate) struct TargetSnap {
    pub id: ElementId,
    pub can_drop: bool,
    /// A target that [catches](crate::DropTarget::catch_outside) the drag outside its bounds.
    pub outside: bool,
}

/// The drag's state at one moment. A new one is made on every change, so a monitor holds no
/// borrow and can be kept across a callback.
#[derive(Clone, Debug)]
pub(crate) struct Snapshot {
    pub item: DragItem,
    pub source: ElementId,
    pub backend: Backend,
    pub initial: Point<Pixels>,
    pub current: Point<Pixels>,
    pub source_origin: Point<Pixels>,
    pub source_size: Size<Pixels>,
    /// Outermost first; the last is the one the pointer is shallowly over.
    pub targets: Vec<TargetSnap>,
    pub modifiers: Modifiers,
    pub did_drop: bool,
    pub drop_result: Option<DropResult>,
    /// Ended without a drop, the preview going back to the source.
    pub returning: bool,
}

impl Snapshot {
    /// Where the grab point was in the source.
    pub fn grab(&self) -> Point<Pixels> {
        self.initial - self.source_origin
    }
}

/// Whether a target counts as over when the pointer is over a target inside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Over {
    /// Only the innermost target under the pointer (react-dnd's `isOver({ shallow: true })`).
    Shallow,
    /// It or any target inside it.
    Deep,
}

/// The drag as it is now: react-dnd's drag source, drop target and drag layer monitors in one.
/// Read it anywhere with [`DragMonitor::of`]; it's a snapshot, so holding it borrows nothing.
/// Positions are in window coordinates.
#[derive(Clone, Debug, Default)]
pub struct DragMonitor {
    pub(crate) snap: Option<Rc<Snapshot>>,
}

impl DragMonitor {
    pub fn of(cx: &App) -> Self {
        Self {
            snap: state::snapshot(cx),
        }
    }

    pub fn is_dragging(&self) -> bool {
        self.snap.is_some()
    }

    /// Whether the drag started from this source.
    pub fn is_dragging_source(&self, source: &ElementId) -> bool {
        self.snap.as_ref().is_some_and(|s| &s.source == source)
    }

    pub fn item(&self) -> Option<&DragItem> {
        self.snap.as_ref().map(|s| &s.item)
    }

    pub fn item_type(&self) -> Option<ItemType> {
        self.item().map(DragItem::item_type)
    }

    pub fn source(&self) -> Option<&ElementId> {
        self.snap.as_ref().map(|s| &s.source)
    }

    pub fn backend(&self) -> Option<Backend> {
        self.snap.as_ref().map(|s| s.backend)
    }

    /// Where the drag began: the press, the long press or the focused source's centre.
    pub fn initial_client_offset(&self) -> Option<Point<Pixels>> {
        self.snap.as_ref().map(|s| s.initial)
    }

    /// Where the pointer (or the keyboard's anchor) is.
    pub fn client_offset(&self) -> Option<Point<Pixels>> {
        self.snap.as_ref().map(|s| s.current)
    }

    pub fn difference_from_initial_offset(&self) -> Option<Point<Pixels>> {
        self.snap.as_ref().map(|s| s.current - s.initial)
    }

    /// The source's top-left when the drag began.
    pub fn initial_source_client_offset(&self) -> Option<Point<Pixels>> {
        self.snap.as_ref().map(|s| s.source_origin)
    }

    /// Where the source's top-left would be if it moved with the pointer: where a preview
    /// is drawn.
    pub fn source_client_offset(&self) -> Option<Point<Pixels>> {
        self.snap.as_ref().map(|s| s.current - s.grab())
    }

    /// The source's size when the drag began.
    pub fn source_size(&self) -> Option<Size<Pixels>> {
        self.snap.as_ref().map(|s| s.source_size)
    }

    /// The targets under the pointer that accept the item, outermost first.
    pub fn target_ids(&self) -> Vec<ElementId> {
        self.snap
            .as_ref()
            .map(|s| {
                s.targets
                    .iter()
                    .filter(|t| !t.outside)
                    .map(|t| t.id.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn is_over(&self, target: &ElementId, depth: Over) -> bool {
        let Some(s) = &self.snap else { return false };
        let mut under = s.targets.iter().filter(|t| !t.outside);
        match depth {
            Over::Shallow => under.next_back().is_some_and(|t| &t.id == target),
            Over::Deep => under.any(|t| &t.id == target),
        }
    }

    /// Whether this target, under the pointer (or [catching](crate::DropTarget::catch_outside)
    /// it outside), can take the item. False for any other target.
    pub fn can_drop_on(&self, target: &ElementId) -> bool {
        self.snap
            .as_ref()
            .is_some_and(|s| s.targets.iter().any(|t| &t.id == target && t.can_drop))
    }

    /// Whether it ended without a drop and its preview is going back to the source. Still
    /// dragging until it lands; no target is under it meanwhile.
    pub fn is_returning(&self) -> bool {
        self.snap.as_ref().is_some_and(|s| s.returning)
    }

    /// During a drop: whether a target inside this one already handled it.
    pub fn did_drop(&self) -> bool {
        self.snap.as_ref().is_some_and(|s| s.did_drop)
    }

    /// During a drop: what the targets inside returned.
    pub fn drop_result(&self) -> Option<&DropResult> {
        self.snap.as_ref().and_then(|s| s.drop_result.as_ref())
    }

    /// The modifier keys held at the last move.
    pub fn modifiers(&self) -> Modifiers {
        self.snap.as_ref().map(|s| s.modifiers).unwrap_or_default()
    }
}

/// A target's own view of the drag, given to its callbacks.
#[derive(Clone, Debug)]
pub struct DropMonitor {
    pub(crate) monitor: DragMonitor,
    pub(crate) target: ElementId,
    pub(crate) bounds: Bounds<Pixels>,
}

impl DropMonitor {
    pub fn target(&self) -> &ElementId {
        &self.target
    }

    /// The target's bounds in window coordinates, as last drawn.
    pub fn bounds(&self) -> Bounds<Pixels> {
        self.bounds
    }

    pub fn is_over(&self, depth: Over) -> bool {
        self.monitor.is_over(&self.target, depth)
    }

    pub fn can_drop(&self) -> bool {
        self.monitor.can_drop_on(&self.target)
    }

    /// The pointer's place in the target, from its top-left.
    pub fn offset_in_target(&self) -> Option<Point<Pixels>> {
        self.monitor.client_offset().map(|p| p - self.bounds.origin)
    }
}

impl Deref for DropMonitor {
    type Target = DragMonitor;

    fn deref(&self) -> &DragMonitor {
        &self.monitor
    }
}

/// The drag layer's view of the drag: the monitor, and where the preview's top-left is now
/// (following the pointer, or on its way back to the source).
#[derive(Clone, Debug)]
pub struct DragLayerMonitor {
    pub(crate) monitor: DragMonitor,
    pub(crate) origin: Point<Pixels>,
}

impl DragLayerMonitor {
    /// Where to draw the preview's top-left, in window coordinates.
    pub fn preview_origin(&self) -> Point<Pixels> {
        self.origin
    }
}

impl Deref for DragLayerMonitor {
    type Target = DragMonitor;

    fn deref(&self) -> &DragMonitor {
        &self.monitor
    }
}
