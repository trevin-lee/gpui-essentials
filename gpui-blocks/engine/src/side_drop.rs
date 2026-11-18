//! Dragging blocks beside others, as Notion makes columns: dropped on a top-level block's left or
//! right edge, the dragged blocks and that block become a row of two columns; on a row's edge or
//! in the gap between two of its columns, a new column (up to `MAX_COLUMNS`); inside a column,
//! among its blocks. Blocks dragged out of a column drop into the top level between blocks. A
//! vertical line marks a new column, a horizontal one a place among blocks.
//!
//! Each drop is one undo step, and normalization keeps the rules after it: the column the blocks
//! left goes when it's empty, and a row left with one column unwraps. Kept apart from the
//! vertical reordering (gpui-dnd's `SortableState`).

use gpui::{Bounds, Pixels, Point, point, px, size};

use crate::command::CommandError;
use crate::editor::BlockEditorModel;
use crate::id::BlockId;
use crate::kind::{BlockKindData, MAX_COLUMNS};
use crate::state::BlockEditorState;
use crate::transaction::EditOrigin;

/// Where dragged blocks drop, other than a gap between top-level blocks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SideDrop {
    /// Beside the top-level block `target`, to its right or left: the two make a row.
    Beside { target: BlockId, right: bool },
    /// A new column at `at` in the columns block `columns`.
    NewColumn { columns: BlockId, at: usize },
    /// Into `column`, before its `index`th block not being dragged.
    IntoColumn { column: BlockId, index: usize },
    /// Out of a column into the top level, before its `index`th block.
    TopLevel { index: usize },
}

/// A drop and the line that shows it, in window coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Target {
    pub(crate) drop: SideDrop,
    pub(crate) line: Bounds<Pixels>,
}

/// How far into a block from its left or right side a drop makes a row.
fn edge(block: &Bounds<Pixels>) -> Pixels {
    (block.size.width * 0.2).clamp(px(24.), px(80.))
}

fn vertical(x: Pixels, top: Pixels, bottom: Pixels) -> Bounds<Pixels> {
    Bounds::new(
        point(x - px(1.5), top),
        size(px(3.), (bottom - top).max(px(8.))),
    )
}

fn horizontal(y: Pixels, left: Pixels, right: Pixels) -> Bounds<Pixels> {
    Bounds::new(
        point(left, y - px(1.5)),
        size((right - left).max(px(8.)), px(3.)),
    )
}

impl BlockEditorState {
    /// Where each child of `parent` was last drawn: the union of its lines, in window
    /// coordinates, in order. A child with nothing drawn is left out.
    pub fn children_bounds(&self, parent: BlockId) -> Vec<(BlockId, Bounds<Pixels>)> {
        let doc = &self.model.doc;
        let mut out: Vec<(BlockId, Option<Bounds<Pixels>>)> =
            doc.children(parent).iter().map(|id| (*id, None)).collect();
        let frame = self.frame.borrow();
        for leaf in &frame.leaves {
            let path = doc.path(leaf.info.block);
            let Some(at) = path.iter().position(|id| *id == parent) else {
                continue;
            };
            let Some(child) = path.get(at + 1) else {
                continue;
            };
            if let Some((_, bounds)) = out.iter_mut().find(|(id, _)| id == child) {
                let b = leaf.bounds();
                *bounds = Some(bounds.map_or(b, |a| a.union(&b)));
            }
        }
        out.into_iter()
            .filter_map(|(id, b)| Some((id, b?)))
            .collect()
    }

    /// Where `run` (the dragged blocks, siblings, in order) drops with the pointer at `p`, when
    /// it isn't a gap between top-level blocks: `None` leaves that to the `SortableState` gap, or, for blocks
    /// dragged out of a column (`nested`), to [`SideDrop::TopLevel`].
    pub(crate) fn side_target(
        &self,
        run: &[BlockId],
        p: Point<Pixels>,
        nested: bool,
    ) -> Option<Target> {
        let doc = &self.model.doc;
        // A columns block never goes into columns.
        let movable = !run
            .iter()
            .any(|id| matches!(doc.kind(*id), Some(BlockKindData::Columns)));
        let tops = self.top_level_bounds();
        for (_, id, b) in &tops {
            if run.contains(id) || p.y < b.top() - px(6.) || p.y >= b.bottom() + px(6.) {
                continue;
            }
            if !movable {
                return None;
            }
            if matches!(doc.kind(*id), Some(BlockKindData::Columns)) {
                return self.target_in_columns(*id, run, p);
            }
            // Over its left edge (left of it is the handles' margin), or its right edge or
            // past it.
            let reach = edge(b);
            if (p.x >= b.left() && p.x < b.left() + reach) || p.x > b.right() - reach {
                let right = p.x > b.right() - reach;
                let x = if right {
                    b.right() + px(6.)
                } else {
                    b.left() - px(6.)
                };
                return Some(Target {
                    drop: SideDrop::Beside { target: *id, right },
                    line: vertical(x, b.top(), b.bottom()),
                });
            }
            break;
        }
        if !nested {
            return None;
        }
        // Out of a column: the gap between top-level blocks the pointer is in.
        let index = tops.iter().filter(|(_, _, b)| b.center().y < p.y).count();
        let (left, right) = tops.iter().fold(
            (None, None),
            |(l, r): (Option<Pixels>, Option<Pixels>), (_, _, b)| {
                (
                    Some(l.map_or(b.left(), |l| l.min(b.left()))),
                    Some(r.map_or(b.right(), |r| r.max(b.right()))),
                )
            },
        );
        let y = match (
            index.checked_sub(1).and_then(|i| tops.get(i)),
            tops.get(index),
        ) {
            (Some((_, _, above)), Some((_, _, below))) => (above.bottom() + below.top()) / 2.,
            (Some((_, _, above)), None) => above.bottom() + px(4.),
            (None, Some((_, _, below))) => below.top() - px(4.),
            (None, None) => p.y,
        };
        Some(Target {
            drop: SideDrop::TopLevel { index },
            line: horizontal(y, left?, right?),
        })
    }

    /// A drop over the columns block `columns`: a new column at its edges or in a gap between
    /// columns, else a place among the blocks of the column under the pointer.
    fn target_in_columns(
        &self,
        columns: BlockId,
        run: &[BlockId],
        p: Point<Pixels>,
    ) -> Option<Target> {
        let doc = &self.model.doc;
        let cols = self.children_bounds(columns);
        let (first, last) = (cols.first()?.1, cols.last()?.1);
        let side_by_side = cols.windows(2).all(|w| {
            (w[0].1.top() - w[1].1.top()).abs() < px(4.) && w[1].1.left() > w[0].1.right()
        });
        let (top, bottom) = cols
            .iter()
            .fold((first.top(), first.bottom()), |(t, b), (_, c)| {
                (t.min(c.top()), b.max(c.bottom()))
            });
        // The columns the run would leave: a column of only dragged blocks goes when they do.
        let staying = cols
            .iter()
            .filter(|(c, _)| doc.children(*c).iter().any(|b| !run.contains(b)))
            .count();
        let room = staying < MAX_COLUMNS;
        if side_by_side && room {
            let reach = px(16.);
            let new = |at: usize, x: Pixels| Target {
                drop: SideDrop::NewColumn { columns, at },
                line: vertical(x, top, bottom),
            };
            if p.x < first.left() + reach {
                return Some(new(0, first.left() - px(6.)));
            }
            if p.x > last.right() - reach {
                return Some(new(cols.len(), last.right() + px(6.)));
            }
            for (ix, w) in cols.windows(2).enumerate() {
                if p.x > w[0].1.right() - reach / 2. && p.x < w[1].1.left() + reach / 2. {
                    return Some(new(ix + 1, (w[0].1.right() + w[1].1.left()) / 2.));
                }
            }
        }
        // The column under the pointer (stacked: the one level with it).
        let (column, bounds) = cols
            .iter()
            .find(|(_, b)| {
                if side_by_side {
                    p.x >= b.left() && p.x < b.right()
                } else {
                    p.y >= b.top() - px(6.) && p.y < b.bottom() + px(6.)
                }
            })
            .copied()
            .or_else(|| cols.last().copied())?;
        let blocks: Vec<(BlockId, Bounds<Pixels>)> = self
            .children_bounds(column)
            .into_iter()
            .filter(|(b, _)| !run.contains(b))
            .collect();
        let index = blocks.iter().filter(|(_, b)| b.center().y < p.y).count();
        let y = match (
            index.checked_sub(1).and_then(|i| blocks.get(i)),
            blocks.get(index),
        ) {
            (Some((_, above)), Some((_, below))) => (above.bottom() + below.top()) / 2.,
            (Some((_, above)), None) => above.bottom() + px(4.),
            (None, Some((_, below))) => below.top() - px(4.),
            (None, None) => bounds.top(),
        };
        Some(Target {
            drop: SideDrop::IntoColumn { column, index },
            line: horizontal(y, bounds.left(), bounds.right()),
        })
    }
}

impl BlockEditorModel {
    /// Moves `run` (sibling blocks, in order) to `drop`, as one undo step; the blocks stay
    /// selected. Normalization then takes away a column they emptied, and unwraps a row left
    /// with one column.
    pub(crate) fn drop_run(
        &mut self,
        run: &[BlockId],
        drop: SideDrop,
    ) -> Result<bool, CommandError> {
        let doc = &self.doc;
        let (Some(first), Some(last)) = (run.first().copied(), run.last().copied()) else {
            return Ok(false);
        };
        let parent = doc.parent(first);
        if run.iter().any(|b| doc.parent(*b) != parent) {
            return Err(CommandError::Unsupported("blocks of different parents"));
        }
        let columns_in_run = run
            .iter()
            .any(|b| matches!(doc.kind(*b), Some(BlockKindData::Columns)));
        match drop {
            SideDrop::Beside { target, .. } if run.contains(&target) || columns_in_run => {
                return Err(CommandError::Unsupported("beside itself"));
            }
            SideDrop::Beside { target, .. }
                if doc.parent(target) != Some(doc.root)
                    || matches!(doc.kind(target), Some(BlockKindData::Columns)) =>
            {
                return Err(CommandError::Unsupported("beside that"));
            }
            SideDrop::NewColumn { columns, .. }
                if columns_in_run || doc.kind(columns) != Some(&BlockKindData::Columns) =>
            {
                return Err(CommandError::Unsupported("a new column there"));
            }
            SideDrop::IntoColumn { column, .. }
                if columns_in_run
                    || !matches!(doc.kind(column), Some(BlockKindData::Column { .. })) =>
            {
                return Err(CommandError::Unsupported("into that"));
            }
            _ => {}
        }
        let root = doc.root;
        let selection = crate::selection::BlockSelection::Blocks {
            anchor: first,
            head: last,
        };
        self.transact(EditOrigin::Command("drop_blocks"), |tx| {
            let mut moved = Vec::with_capacity(run.len());
            for block in run {
                moved.push(tx.remove_block(*block)?);
            }
            match drop {
                SideDrop::Beside { target, right } => {
                    let (_, index) = tx
                        .doc
                        .index_in_parent(target)
                        .ok_or(crate::step::StepError::UnknownBlock(target))?;
                    let target = tx.remove_block(target)?;
                    let column = |blocks| {
                        crate::step::DetachedBlock::container(
                            BlockKindData::Column { width: 0.5 },
                            blocks,
                        )
                    };
                    let (a, b) = (column(vec![target]), column(moved));
                    let pair = if right { vec![a, b] } else { vec![b, a] };
                    tx.insert(
                        root,
                        index,
                        vec![crate::step::DetachedBlock::container(
                            BlockKindData::Columns,
                            pair,
                        )],
                    )?;
                }
                SideDrop::NewColumn { columns, at } => {
                    let ids = tx.doc.children(columns).to_vec();
                    let n = ids.len() as f32;
                    // The new column takes an even share; the others make room in proportion.
                    for id in &ids {
                        if let Some(BlockKindData::Column { width }) = tx.doc.kind(*id).cloned() {
                            tx.set_kind(
                                *id,
                                BlockKindData::Column {
                                    width: width * n / (n + 1.),
                                },
                            )?;
                        }
                    }
                    let sum: f32 = ids
                        .iter()
                        .filter_map(|id| match tx.doc.kind(*id) {
                            Some(BlockKindData::Column { width }) => Some(*width),
                            _ => None,
                        })
                        .sum();
                    let width = if sum > 0. { sum / n } else { 1. };
                    let column = crate::step::DetachedBlock::container(
                        BlockKindData::Column { width },
                        moved,
                    );
                    tx.insert(columns, at.min(ids.len()), vec![column])?;
                }
                SideDrop::IntoColumn { column, index } => {
                    let len = tx.doc.children(column).len();
                    tx.insert(column, index.min(len), moved)?;
                }
                SideDrop::TopLevel { index } => {
                    let len = tx.doc.children(root).len();
                    tx.insert(root, index.min(len), moved)?;
                }
            }
            Ok(Some(selection))
        })
    }
}
