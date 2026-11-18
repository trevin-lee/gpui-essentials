//! Drop targets: react-dnd's `useDrop`.

use std::rc::Rc;

use gpui::{App, Bounds, Pixels, Point, SharedString, Window};

use crate::item::{DragItem, DropResult, ItemType};
use crate::monitor::{DragMonitor, DropMonitor};

/// What a target did with a drop. Targets are asked innermost first.
#[derive(Clone, Debug)]
pub enum DropOutcome {
    /// Handled here; the targets outside aren't asked.
    Handled(Option<DropResult>),
    /// Handled here, and the targets outside are asked too: they see
    /// [`did_drop`](DragMonitor::did_drop) and this result (react-dnd's chaining).
    HandledContinue(Option<DropResult>),
    /// Not handled; the next target out is asked as if this one weren't there.
    Pass,
}

type CanDrop = Rc<dyn Fn(&DragItem, &DragMonitor, &App) -> bool>;
type Hover = Rc<dyn Fn(&DragItem, &DropMonitor, &mut Window, &mut App)>;
type OnDrop = Rc<dyn Fn(&DragItem, &DropMonitor, &mut Window, &mut App) -> DropOutcome>;
type Anchor = Rc<dyn Fn(Bounds<Pixels>) -> Point<Pixels>>;
type Slots = Rc<dyn Fn(&DragItem, Bounds<Pixels>) -> Vec<Point<Pixels>>>;
type Describe = Rc<dyn Fn(&DragItem, &DropMonitor) -> SharedString>;
type Step =
    Rc<dyn Fn(&DragItem, &DropMonitor, Point<f32>, &mut Window, &mut App) -> Option<Point<Pixels>>>;

/// A drop target (react-dnd's `useDrop`): which items a [`dnd`](crate::dnd) element takes, and
/// what it does with them.
#[derive(Clone, Default)]
pub struct DropTarget {
    accepts: Vec<ItemType>,
    pub(crate) can_drop: Option<CanDrop>,
    pub(crate) on_hover: Option<Hover>,
    pub(crate) on_drop: Option<OnDrop>,
    anchor: Option<Anchor>,
    slots: Option<Slots>,
    describe: Option<Describe>,
    pub(crate) step: Option<Step>,
}

impl DropTarget {
    pub fn new() -> Self {
        Self::default()
    }

    /// An item type this target takes. Call once for each.
    pub fn accept<T: 'static>(mut self) -> Self {
        self.accepts.push(ItemType::of::<T>());
        self
    }

    /// Whether it can take this item now (react-dnd's `canDrop`). Asked as the pointer moves
    /// over it, and as the drag draws, for [`can_drop_style`](crate::Dnd::can_drop_style).
    pub fn can_drop(mut self, f: impl Fn(&DragItem, &DragMonitor, &App) -> bool + 'static) -> Self {
        self.can_drop = Some(Rc::new(f));
        self
    }

    /// Called on every move while an item it accepts is over it, shallowly or deeply, whether
    /// or not it can take it (react-dnd's `hover`). Innermost target first.
    pub fn on_hover(
        mut self,
        f: impl Fn(&DragItem, &DropMonitor, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_hover = Some(Rc::new(f));
        self
    }

    /// Called on a drop, innermost target first, for each target under the pointer that can
    /// take the item. A target without it passes every drop out.
    pub fn on_drop(
        mut self,
        f: impl Fn(&DragItem, &DropMonitor, &mut Window, &mut App) -> DropOutcome + 'static,
    ) -> Self {
        self.on_drop = Some(Rc::new(f));
        self
    }

    /// Where a keyboard drag puts its pointer on this target, given the target's visible
    /// bounds: its centre unless given.
    pub fn keyboard_anchor(
        mut self,
        f: impl Fn(Bounds<Pixels>) -> Point<Pixels> + 'static,
    ) -> Self {
        self.anchor = Some(Rc::new(f));
        self
    }

    /// Several places in this target a keyboard drag can go, given the item and the target's
    /// visible bounds: a list's gaps between its items, so the arrows move one place at a time.
    /// Takes the place of [`keyboard_anchor`](Self::keyboard_anchor).
    pub fn keyboard_slots(
        mut self,
        f: impl Fn(&DragItem, Bounds<Pixels>) -> Vec<Point<Pixels>> + 'static,
    ) -> Self {
        self.slots = Some(Rc::new(f));
        self
    }

    /// Moves a keyboard drag one place within this target when an arrow is pressed over it:
    /// given the direction (a unit vector), it returns where the keyboard's pointer goes, or
    /// None to leave the target the nearest way, as if it had no step. For a list longer than
    /// its window: it can scroll the next place into view and point there, where slots would
    /// have to be drawn to be found.
    pub fn keyboard_step(
        mut self,
        f: impl Fn(&DragItem, &DropMonitor, Point<f32>, &mut Window, &mut App) -> Option<Point<Pixels>>
        + 'static,
    ) -> Self {
        self.step = Some(Rc::new(f));
        self
    }

    /// What a keyboard drag says this target is, over it and dropped on it ("To do, position 2
    /// of 4"), in place of its label.
    pub fn describe(
        mut self,
        f: impl Fn(&DragItem, &DropMonitor) -> SharedString + 'static,
    ) -> Self {
        self.describe = Some(Rc::new(f));
        self
    }

    /// Where a keyboard drag can put its pointer on this target.
    pub(crate) fn keyboard_points(
        &self,
        item: &DragItem,
        visible: Bounds<Pixels>,
    ) -> Vec<Point<Pixels>> {
        match (&self.slots, &self.anchor) {
            (Some(slots), _) => slots(item, visible),
            (None, Some(anchor)) => vec![anchor(visible)],
            (None, None) => vec![visible.center()],
        }
    }

    pub(crate) fn description(
        &self,
        item: &DragItem,
        monitor: &DropMonitor,
    ) -> Option<SharedString> {
        self.describe.as_ref().map(|f| f(item, monitor))
    }

    pub(crate) fn accepts(&self, item_type: ItemType) -> bool {
        self.accepts.contains(&item_type)
    }

    /// Accepts the item and, if it says, can take it.
    pub(crate) fn takes(&self, item: &DragItem, monitor: &DragMonitor, cx: &App) -> bool {
        self.accepts(item.item_type())
            && self.can_drop.as_ref().is_none_or(|f| f(item, monitor, cx))
    }
}
