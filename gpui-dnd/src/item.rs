//! What's dragged: the item, its type, and what a target hands back.

use std::any::{Any, TypeId};
use std::fmt;
use std::rc::Rc;

use gpui::SharedString;

/// An item's type: its Rust type. A target [`accept`](crate::DropTarget::accept)s types, as
/// react-dnd's `accept` takes type names. Two kinds of item with the same shape get newtypes,
/// which the compiler then keeps apart.
#[derive(Clone, Copy)]
pub struct ItemType {
    id: TypeId,
    name: &'static str,
}

impl ItemType {
    pub fn of<T: 'static>() -> Self {
        Self {
            id: TypeId::of::<T>(),
            name: std::any::type_name::<T>(),
        }
    }

    /// The type's path, as `std::any::type_name` gives it: for debugging, not for display.
    pub fn name(&self) -> &'static str {
        self.name
    }
}

impl PartialEq for ItemType {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for ItemType {}

impl std::hash::Hash for ItemType {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state)
    }
}

impl fmt::Debug for ItemType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name)
    }
}

/// The dragged item, its type erased: one value, or several of one type (a selection) from a
/// [`DragSource::many`](crate::DragSource::many).
#[derive(Clone)]
pub struct DragItem {
    // Always a `Vec<T>`, one long for a single item.
    items: Rc<dyn Any>,
    count: usize,
    item_type: ItemType,
    label: Option<SharedString>,
}

impl DragItem {
    pub(crate) fn new<T: 'static>(items: Vec<T>, label: Option<SharedString>) -> Self {
        Self {
            count: items.len(),
            items: Rc::new(items),
            item_type: ItemType::of::<T>(),
            label,
        }
    }

    /// The item, or the first of several, if it's a `T`.
    pub fn get<T: 'static>(&self) -> Option<&T> {
        self.all::<T>().and_then(|items| items.first())
    }

    /// Every item dragged together, if they're `T`s.
    pub fn all<T: 'static>(&self) -> Option<&[T]> {
        self.items.downcast_ref::<Vec<T>>().map(Vec::as_slice)
    }

    pub fn is<T: 'static>(&self) -> bool {
        self.item_type == ItemType::of::<T>()
    }

    /// How many items are dragged together: 1 unless the source is a
    /// [`many`](crate::DragSource::many).
    pub fn count(&self) -> usize {
        self.count
    }

    pub fn item_type(&self) -> ItemType {
        self.item_type
    }

    /// The source's accessible name, for announcements.
    pub fn label(&self) -> Option<&SharedString> {
        self.label.as_ref()
    }
}

impl fmt::Debug for DragItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DragItem")
            .field("item_type", &self.item_type)
            .field("count", &self.count)
            .field("label", &self.label)
            .finish()
    }
}

/// What a target hands back to the source on a drop: any value. The source reads it in
/// [`DragEnd::result`](crate::DragEnd::result).
#[derive(Clone)]
pub struct DropResult(Rc<dyn Any>);

impl DropResult {
    pub fn new<R: 'static>(value: R) -> Self {
        Self(Rc::new(value))
    }

    pub fn get<R: 'static>(&self) -> Option<&R> {
        self.0.downcast_ref()
    }
}

impl fmt::Debug for DropResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DropResult(..)")
    }
}
