//! Block drags on gpui-dnd. The editor still decides what a press means (a grip or edge
//! handle, a long press off the text, the block sheet's Move), which blocks it drags, where
//! they'd drop (the `SortableState` gap, `side_drop`'s rows and columns) and the one undoable move; it
//! draws the lifted blocks and the drop line. gpui-dnd runs the drag's life: the pointer and the
//! finger while it's on, Escape and a lost touch, the release (in the window or out of it), and
//! the cursor. The page's edge scrolling stays the host's (it follows `block_drag_pointer` and
//! goes on with `continue_block_drag`).
//!
//! A press passes the editor's own threshold, then [`start`] begins the drag with `begin_at`
//! where the pointer is now, outside the editor's update: gpui-dnd calls back into the editor as
//! a drag begins. The editor's content is the drag's target, registered from the element's
//! prepaint, catching it anywhere in the window.

use gpui::{App, Context, ElementId, Entity, EntityId, Pixels, Point, Window};
use gpui_dnd::{Backend, DragSource, DropOutcome, DropTarget, Preview};

use super::state::BlockEditorState;

/// What gpui-dnd carries for a block drag: whose editor it is. The drag itself (the blocks,
/// where they'd drop) stays the editor's `BlockDrag`.
#[derive(Clone, Debug)]
pub(crate) struct DraggedBlocks {
    editor: EntityId,
}

/// A drag the editor made, for gpui-dnd to begin once the editor's update is over.
pub(crate) struct Begin {
    at: Point<Pixels>,
    id: ElementId,
    source: DragSource<DraggedBlocks>,
    press: gpui_dnd::Press,
}

/// What a move with the button (or a long-pressed finger) down is to a block drag.
pub(crate) enum Moved {
    /// No block drag: the move is the text selection's.
    Selection,
    /// The block drag's, nothing more to do: gpui-dnd follows it, or it's a press short of
    /// starting, which is the handle's and selects nothing.
    Taken,
    /// A press past its threshold: the drag begins.
    Begin(Box<Begin>),
}

impl BlockEditorState {
    /// The drag of the pending `block_drag`, begun with the pointer at `at` (where it is now; the
    /// drag measures from its own `start`) by `backend`. `relative`: the next
    /// finger carries it by how far it moves (a block lifted in place).
    pub(crate) fn dnd_begin(
        &self,
        at: Point<Pixels>,
        backend: Backend,
        relative: bool,
        cx: &mut Context<Self>,
    ) -> Begin {
        let (began, ended) = (cx.entity().downgrade(), cx.entity().downgrade());
        let source = DragSource::item(DraggedBlocks {
            editor: cx.entity_id(),
        })
        // The editor draws the lifted blocks itself, and the others springing aside.
        .preview(Preview::None)
        .keyboard(false)
        .on_begin(move |_, _, cx| {
            began.update(cx, |state, cx| state.dnd_began(cx)).ok();
        })
        .on_end(move |end, _, cx| {
            let cancelled = end.cancelled;
            ended
                .update(cx, |state, cx| state.dnd_ended(cancelled, cx))
                .ok();
        });
        Begin {
            at,
            id: ElementId::NamedInteger("gpui-blocks-drag".into(), cx.entity_id().as_u64()),
            source,
            press: gpui_dnd::Press {
                relative,
                ..gpui_dnd::Press::new(backend)
            },
        }
    }

    /// The pointer moved with the button down: a pending press past 4 points begins its drag.
    pub(crate) fn pressed_move(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) -> Moved {
        let Some(drag) = self.live_block_drag() else {
            return Moved::Selection;
        };
        if drag.active {
            return Moved::Taken;
        }
        // Short of the threshold the press is still the handle's: no text selection.
        let d = at - drag.start;
        if d.x.abs().max(d.y.abs()) < gpui::px(4.) {
            return Moved::Taken;
        }
        Moved::Begin(Box::new(self.dnd_begin(at, Backend::Mouse, false, cx)))
    }

    /// gpui-dnd began the drag: its blocks are picked up.
    fn dnd_began(&mut self, cx: &mut Context<Self>) {
        if let Some(mut drag) = self.block_drag.filter(|d| !d.active) {
            self.activate_block_drag(&mut drag, cx);
            self.block_drag = Some(drag);
            cx.notify();
        }
    }

    /// gpui-dnd's drag is over. Dropped here, the drop has been made; cancelled (Escape, a lost
    /// touch, a tap on a lifted block) or begun and not taken, the blocks spring back.
    fn dnd_ended(&mut self, _cancelled: bool, cx: &mut Context<Self>) {
        if self.block_drag.take().is_some() {
            self.reorder.cancel_drag();
            cx.notify();
        }
    }
}

/// Begins the drag `begin` (out of the editor's update); one that can't begin is forgotten.
pub(crate) fn start(
    editor: &Entity<BlockEditorState>,
    begin: Begin,
    window: &mut Window,
    cx: &mut App,
) {
    if !gpui_dnd::begin_at(window, cx, begin.id, begin.at, begin.source, begin.press) {
        editor.update(cx, |state, cx| state.dnd_ended(true, cx));
    }
}

/// The editor's content as its block drags' target: its own drags only, caught anywhere in the
/// window (a release over the page's margin, or outside the window, drops where the pointer
/// is, as before).
pub(crate) fn target(editor: &Entity<BlockEditorState>) -> DropTarget {
    let id = editor.entity_id();
    let (hover, drop) = (editor.downgrade(), editor.downgrade());
    DropTarget::new()
        .accept::<DraggedBlocks>()
        .catch_outside(true)
        .can_drop(move |item, _, _| item.get::<DraggedBlocks>().is_some_and(|d| d.editor == id))
        .on_hover(move |_, m, window, cx| {
            let Some(at) = m.client_offset() else {
                return;
            };
            let live = hover
                .update(cx, |state, cx| state.block_drag_to(at, cx))
                .unwrap_or(false);
            // The document changed under the drag (it measured blocks that moved or went):
            // the editor dropped it, and gpui-dnd's goes too.
            if !live {
                window.defer(cx, gpui_dnd::cancel);
            }
        })
        .on_drop(move |_, _, _, cx| {
            drop.update(cx, |state, cx| state.finish_block_drag(cx))
                .ok();
            DropOutcome::Handled(None)
        })
}
