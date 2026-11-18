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

use gpui::{App, Context, ElementId, Entity, EntityId, Pixels, Point, SharedString, Window, point};
use gpui_dnd::{Backend, DragSource, DropOutcome, DropTarget, Preview, Said};

use super::id::BlockId;
use super::kind::BlockKindData;
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
        self.key_drop = None;
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
    let (step, say) = (editor.downgrade(), editor.downgrade());
    DropTarget::new()
        .accept::<DraggedBlocks>()
        .catch_outside(true)
        .can_drop(move |item, _, _| item.get::<DraggedBlocks>().is_some_and(|d| d.editor == id))
        .keyboard_step(move |_, m, dir, _, cx| {
            step.update(cx, |state, cx| state.key_step(dir, cx))
                .ok()
                .flatten()
                .or(m.client_offset())
        })
        .say(move |_, _, said, cx| {
            say.upgrade()
                .map(|state| state.read(cx).spoken(said))
                .unwrap_or_default()
        })
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

/// A keyboard drag's places: the top-level blocks it isn't moving, as they were when it was
/// picked up (in content y), and where among them it is now.
pub(crate) struct KeyDrop {
    /// The other top-level blocks, in order: each one's top, middle and bottom in content y.
    others: Vec<(BlockId, f32, f32, f32)>,
    /// The gap the run is in now (0 is above the first of `others`), and the one it started in.
    at: usize,
    from: usize,
    /// Beside the block above the gap (the first, at the top), as a new column.
    beside: bool,
    /// What the run is called: its kind and first words, or "3 blocks".
    label: String,
}

/// What a keyboard pick-up says: "Picked up {label}. Arrow keys move it, Space drops it, Escape
/// cancels.", "them" for a run of blocks.
fn picked_up(label: &str, count: usize) -> String {
    let it = if count > 1 { "them" } else { "it" };
    format!("Picked up {label}. Arrow keys move {it}, Space drops {it}, Escape cancels.")
}

/// How many of a block's first words a label says.
const LABEL_WORDS: usize = 6;

impl BlockEditorState {
    /// Space with blocks selected: the selected run picked up by the keyboard. The arrows
    /// move it (up and down a place among the top-level blocks, right into a new column beside
    /// the block above, left back out), Space or Enter drops it as one undoable move, and Escape
    /// puts it back, each step said in the editor's words.
    pub fn pick_up_blocks(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_readonly() || self.block_drag.is_some() {
            return;
        }
        let doc = &self.model.doc;
        let Some((parent, range)) = self.model.selected_blocks() else {
            return;
        };
        let run: Vec<BlockId> = doc.children(parent)[range].to_vec();
        let Some(&first) = run.first() else {
            return;
        };
        // The top-level block the run is in: itself, or (in a column) its row.
        let row = doc.path(first).get(1).copied();
        let tops = self.top_level_bounds();
        // Where the run is drawn: its blocks' union (in a column, the row's).
        let run_bounds = self
            .children_bounds(parent)
            .into_iter()
            .filter(|(id, _)| run.contains(id))
            .map(|(_, b)| b)
            .reduce(|a, b| a.union(&b));
        let Some(bounds) = run_bounds.or_else(|| {
            tops.iter()
                .find(|(_, id, _)| *id == first)
                .map(|(_, _, b)| *b)
        }) else {
            return;
        };
        self.begin_block_drag(first, bounds.center());
        let Some(drag) = self.block_drag.as_mut() else {
            return;
        };
        drag.by_keys = true;
        let nested = drag.nested;
        let top = self.content_top();
        let others: Vec<(BlockId, f32, f32, f32)> = tops
            .iter()
            .filter(|(_, id, _)| !run.contains(id))
            .map(|(_, id, b)| {
                (
                    *id,
                    f32::from(b.top() - top),
                    f32::from(b.center().y - top),
                    f32::from(b.bottom() - top),
                )
            })
            .collect();
        // Its place: below the blocks above it (out of a column, below its row).
        let from = match row.filter(|_| nested) {
            Some(row) => others
                .iter()
                .position(|(id, ..)| *id == row)
                .map_or(0, |ix| ix + 1),
            None => {
                let middle = f32::from(bounds.center().y - top);
                others.iter().filter(|(_, _, m, _)| *m < middle).count()
            }
        };
        let label = if run.len() == 1 {
            self.block_label(first)
        } else {
            format!("{} blocks", run.len())
        };
        let begin = self.dnd_begin(bounds.center(), Backend::Keyboard, false, cx);
        let begin = Begin {
            press: gpui_dnd::Press {
                label: Some(label.clone().into()),
                // The whole sentence is the editor's: "them" for a run.
                picked_up: Some(picked_up(&label, run.len()).into()),
                ..begin.press
            },
            ..begin
        };
        self.key_drop = Some(KeyDrop {
            others,
            at: from,
            from,
            beside: false,
            label,
        });
        let editor = cx.entity();
        // gpui-dnd calls back into the editor as the drag begins: not from inside its update.
        window.defer(cx, move |window, cx| start(&editor, begin, window, cx));
        cx.notify();
    }

    /// What a block is called: its kind and its first words ("Heading Plans for Q3").
    pub(crate) fn block_label(&self, block: BlockId) -> String {
        let doc = &self.model.doc;
        let kind = match doc.kind(block) {
            Some(BlockKindData::Heading { .. }) => "Heading",
            Some(BlockKindData::Paragraph) => "Paragraph",
            Some(BlockKindData::List { .. }) => "List",
            Some(BlockKindData::ListItem { .. }) => "List item",
            Some(BlockKindData::Blockquote) => "Quote",
            Some(BlockKindData::Callout { .. }) => "Callout",
            Some(BlockKindData::Toggle { .. }) => "Toggle",
            Some(BlockKindData::CodeBlock { .. }) => "Code",
            Some(BlockKindData::Table { .. }) => "Table",
            Some(BlockKindData::Columns) => "Columns",
            Some(BlockKindData::HorizontalRule) => "Divider",
            _ => "Block",
        };
        let mut words: Vec<&str> = Vec::new();
        let mut stack = vec![block];
        while let Some(id) = stack.pop() {
            if let Some(text) = doc.leaf_text(id) {
                words.extend(text.split_whitespace().filter(|w| *w != "\u{fffc}"));
                if words.len() >= LABEL_WORDS {
                    break;
                }
            }
            stack.extend(doc.children(id).iter().rev());
        }
        words.truncate(LABEL_WORDS);
        if words.is_empty() {
            kind.to_string()
        } else {
            format!("{kind} {}", words.join(" "))
        }
    }

    /// An arrow during a keyboard drag: the run's new place, set as its drop, and the point the
    /// drag is at now (the other blocks open the gap for it there).
    fn key_step(&mut self, dir: Point<f32>, cx: &mut Context<Self>) -> Option<Point<Pixels>> {
        let mut drag = self.live_block_drag().filter(|d| d.by_keys)?;
        let key = self.key_drop.as_mut()?;
        let n = key.others.len();
        let (dx, dy) = (dir.x.round() as i32, dir.y.round() as i32);
        match (dx, dy) {
            (0, -1) if !key.beside => key.at = key.at.saturating_sub(1),
            (0, 1) if !key.beside => key.at = (key.at + 1).min(n),
            (1, 0) if !drag.nested && n > 0 => key.beside = true,
            (-1, 0) => key.beside = false,
            _ => {}
        }
        let (at, beside, from) = (key.at, key.beside, key.from);
        // The block the run would be beside or below: the one above the gap (the first, at
        // the top).
        let neighbour = key.others.get(at.saturating_sub(1)).copied();
        let gap_y = match (at, key.others.get(at.saturating_sub(1)), key.others.get(at)) {
            (0, _, Some(below)) => below.1 - 1.,
            (_, Some(above), Some(below)) if at > 0 => (above.2 + below.2) / 2.,
            (_, Some(above), None) => above.3 + 1.,
            _ => 0.,
        };
        let top = self.content_top();
        let held = self.dragged_blocks(&drag);
        let p = if beside {
            let (id, ..) = neighbour?;
            let b = self
                .top_level_bounds()
                .into_iter()
                .find(|(_, i, _)| *i == id)?
                .2;
            let p = point(b.right() - gpui::px(4.), b.center().y);
            drag.side = self.side_target(&held, p, false);
            drag.drop = None;
            p
        } else {
            let p = point(drag.start.x, top + gpui::px(gap_y));
            if drag.nested {
                drag.side = self.top_level_gap_target(at);
                drag.drop = None;
            } else {
                drag.side = None;
                // In `move_run_to`'s terms: an index with the run still in the order.
                drag.drop = Some(if at > from { at + drag.count } else { at });
            }
            p
        };
        drag.at = p;
        self.block_drag = Some(drag);
        cx.notify();
        Some(p)
    }

    /// What a keyboard drag says, in the editor's words: "{label}, below {neighbour}, 4 of 12"
    /// as it moves, "{label}, in a new column beside {neighbour}", "{label}, out of the
    /// columns, below {neighbour}"; "{label}, moved below {neighbour}" (or "moved to the top")
    /// when dropped; "{label}, put back" on Escape.
    pub(crate) fn spoken(&self, said: Said) -> SharedString {
        let (Some(drag), Some(key)) = (self.block_drag, self.key_drop.as_ref()) else {
            return SharedString::default();
        };
        let label = &key.label;
        let neighbour = key
            .others
            .get(key.at.saturating_sub(1))
            .map(|(id, ..)| self.block_label(*id));
        let below = match (&neighbour, key.at) {
            (Some(n), at) if at > 0 => format!("below {n}"),
            _ => "at the top".to_string(),
        };
        let place = format!("{} of {}", key.at + 1, key.others.len() + 1);
        let beside = neighbour
            .clone()
            .or_else(|| key.others.first().map(|(id, ..)| self.block_label(*id)));
        let text = match said {
            Said::Cancelled => format!("{label}, put back"),
            Said::Over if key.beside => format!(
                "{label}, in a new column beside {}",
                beside.unwrap_or_default()
            ),
            Said::Over if drag.nested => format!("{label}, out of the columns, {below}"),
            Said::Over => format!("{label}, {below}, {place}"),
            Said::Dropped if key.beside => {
                format!(
                    "{label}, moved into a new column beside {}",
                    beside.unwrap_or_default()
                )
            }
            Said::Dropped if key.at == 0 => format!("{label}, moved to the top"),
            Said::Dropped => format!("{label}, moved {below}"),
        };
        text.into()
    }
}
