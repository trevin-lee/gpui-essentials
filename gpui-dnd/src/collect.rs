//! Collected state: react-dnd's `collect`, for a view's own state.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use gpui::{Context, EntityId};

use crate::monitor::DragMonitor;
use crate::state::{self, Collector};

/// A value worked out from the [`DragMonitor`] whenever the drag changes, that notifies its
/// view only when it changes (react-dnd's `collect`). Dropping it stops it.
pub struct Collected<C> {
    cell: Rc<Cell<C>>,
}

struct Cell<C> {
    value: RefCell<C>,
    collect: Box<dyn Fn(&DragMonitor) -> C>,
    entity: EntityId,
}

impl<C: PartialEq> Collector for Cell<C> {
    fn update(&self, monitor: &DragMonitor) -> Option<EntityId> {
        let value = (self.collect)(monitor);
        let mut current = self.value.borrow_mut();
        if *current == value {
            return None;
        }
        *current = value;
        Some(self.entity)
    }
}

impl<C: Clone> Collected<C> {
    /// The value as of the last change.
    pub fn get(&self) -> C {
        self.cell.value.borrow().clone()
    }
}

/// [`collect_dnd`](CollectExt::collect_dnd) on a view's context.
pub trait CollectExt {
    /// Works `collect` out from the monitor now and on every change of the drag, and notifies
    /// this view when its value changes:
    ///
    /// ```ignore
    /// let over = cx.collect_dnd(|m| m.is_over(&"todo".into(), Over::Shallow));
    /// // in render: if over.get() { … }
    /// ```
    fn collect_dnd<C: PartialEq + Clone + 'static>(
        &mut self,
        collect: impl Fn(&DragMonitor) -> C + 'static,
    ) -> Collected<C>;
}

impl<V: 'static> CollectExt for Context<'_, V> {
    fn collect_dnd<C: PartialEq + Clone + 'static>(
        &mut self,
        collect: impl Fn(&DragMonitor) -> C + 'static,
    ) -> Collected<C> {
        let value = collect(&DragMonitor::of(self));
        let cell = Rc::new(Cell {
            value: RefCell::new(value),
            collect: Box::new(collect),
            entity: self.entity_id(),
        });
        let weak: Weak<dyn Collector> = Rc::downgrade(&(cell.clone() as Rc<dyn Collector>));
        state::shared(self).borrow_mut().collectors.push(weak);
        Collected { cell }
    }
}
