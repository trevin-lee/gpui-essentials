//! Drag and drop for GPUI in the manner of react-dnd.
//!
//! - **[`DndProvider`]** wraps the window's content (react-dnd's `DndProvider`): it follows the
//!   drag and draws its preview above everything.
//! - **[`dnd`]** makes a div that's a drag source, a drop target, or both. A
//!   [`DragSource`] (react-dnd's `useDrag`) says what's dragged and hears how it ended; a
//!   [`DropTarget`] (`useDrop`) says which item types it takes, and is told as one hovers and
//!   when one drops. Targets nest: the innermost one under the pointer is asked first, and can
//!   pass the drop out.
//! - **[`DragMonitor`]** is the drag as it is now: the item, the offsets, the targets under the
//!   pointer. [`CollectExt::collect_dnd`] keeps a value worked out from it in a view's state,
//!   and notifies the view only when that value changes (react-dnd's `collect`).
//!
//! - **[`press_at`]** and **[`begin_at`]** start a drag from a press the host recognised itself,
//!   on something that isn't a `dnd` element (an event drawn on a canvas); a target that
//!   [catches](DropTarget::catch_outside) its drag outside its bounds and
//!   [`auto_scroll_zone`] serve such a canvas too.
//!
//! An item's type is its Rust type: a target `accept::<Card>()`s and reads the card back with
//! `item.get::<Card>()`. A source can drag several items of one type together
//! ([`DragSource::many`]).
//!
//! ```ignore
//! DndProvider::new(
//!     div()
//!         .child(
//!             dnd("card")
//!                 .source(DragSource::item(Card(1)).on_end(|end, _, _| {
//!                     if end.dropped { /* … */ }
//!                 }))
//!                 .dragging_style(|s| s.opacity(0.4))
//!                 .child("Card"),
//!         )
//!         .child(
//!             dnd("done")
//!                 .target(DropTarget::new().accept::<Card>().on_drop(|item, _, _, _| {
//!                     let card = item.get::<Card>().unwrap();
//!                     DropOutcome::Handled(None)
//!                 }))
//!                 .child("Done"),
//!         ),
//! )
//! ```
//!
//! gpui-dnd keeps its own drag state beside GPUI's `on_drag`/`on_drop` (see `DESIGN.md` for
//! why). Nothing runs while no drag is moving: no timers and no frames.

mod autoscroll;
mod collect;
mod element;
mod host;
mod item;
mod keyboard;
mod monitor;
mod provider;
mod sortable;
mod source;
mod state;
mod target;
mod touch;
mod virtual_list;

pub use autoscroll::{AutoScroll, AutoScrollArea, AutoScrollExt, Scrollable, auto_scroll_zone};
pub use collect::{CollectExt, Collected};
pub use element::{Dnd, PICK_UP_HINT, dnd};
pub use host::{Press, begin_at, cancel, drag_cursor, press_at};
pub use item::{DragItem, DropResult, ItemType};
pub use keyboard::{Announcements, SayAt, announcement, item_name};
pub use monitor::{DragLayerMonitor, DragMonitor, DropMonitor, Over};
pub use provider::{DndProvider, PREVIEW_PRIORITY};
pub use sortable::{
    Placeholder, SLIDE_DURATION, Sortable, SortableDrag, SortableItem, SortableMove,
};
pub use source::{Backend, DragEnd, DragSource, Preview, SharedSource, TouchActivation};
pub use state::RETURN_DURATION;
pub use target::{DropOutcome, DropTarget};
pub use virtual_list::VirtualSortable;
