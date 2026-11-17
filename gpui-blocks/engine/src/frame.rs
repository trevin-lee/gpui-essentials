// Changed in Trevin Lee's fork (2026): it records the rem size and each unit's bounds; a leaf's
// geometry is the engine's own.
//! Per-frame geometry. During the editor's prepaint every text leaf the engine draws reports
//! where it laid out (`draw::OwnLeaf`); after prepaint those become the editor's
//! [`FrameLayout`], which handlers hit-test against until the next frame. Everything here maps
//! between the model's offsets and the leaves' own indices.

use std::{collections::HashMap, ops::Range, rc::Rc};

use gpui::{Bounds, Pixels, Point, point, px, size};

use super::{draw::OwnLeaf, id::BlockId, position::BlockPosition, projection::LeafInfo};

/// Builds the frame from the leaves the engine drew.
pub(crate) fn into_frame(
    mut geometries: HashMap<u64, OwnLeaf>,
    leaves: Rc<Vec<LeafInfo>>,
    tables: &[(BlockId, Vec<BlockId>)],
) -> FrameLayout {
    let mut frame = FrameLayout::default();
    for (table, cells) in tables {
        let bounds = cells
            .iter()
            .filter_map(|cell| geometries.get(&cell.as_u64()).map(|g| g.bounds()))
            .reduce(|a, b| a.union(&b));
        if let Some(bounds) = bounds {
            frame.tables.push((*table, bounds));
        }
    }
    for info in leaves.iter() {
        if let Some(geometry) = geometries.remove(&info.block.as_u64()) {
            frame.index.insert(info.block, frame.leaves.len());
            frame.leaves.push(LeafGeom {
                info: *info,
                geometry,
            });
        }
    }
    frame
}

pub(crate) struct LeafGeom {
    pub(crate) info: LeafInfo,
    geometry: OwnLeaf,
}

impl LeafGeom {
    pub(crate) fn bounds(&self) -> Bounds<Pixels> {
        self.geometry.bounds()
    }

    pub(crate) fn line_height(&self) -> Pixels {
        self.geometry.line_height()
    }

    /// Height of the caret: the font's ascent + descent (like a text
    /// input's caret), not the whole line box, which includes leading.
    pub(crate) fn caret_height(&self) -> Pixels {
        self.geometry.caret_height()
    }

    /// Rendered index for a model offset.
    fn index(&self, offset: usize) -> usize {
        let len = self.geometry.len();
        if self.info.atomic {
            if offset == 0 { 0 } else { len }
        } else {
            offset.min(self.info.len).min(len)
        }
    }

    /// Top of the caret at `offset`, window coordinates.
    pub(crate) fn caret_point(&self, offset: usize) -> Point<Pixels> {
        self.geometry.position_for_index(self.index(offset))
    }

    /// Closest model offset for a window point.
    pub(crate) fn offset_at(&self, at: Point<Pixels>) -> usize {
        let index = self.geometry.index_for_position(at);
        if self.info.atomic {
            usize::from(index * 2 > self.geometry.len().max(1))
        } else {
            index.min(self.info.len)
        }
    }

    /// The inline atom drawn under `at`, as its leaf byte range.
    pub(crate) fn atom_at(&self, at: Point<Pixels>) -> Option<Range<usize>> {
        self.geometry.atom_at(at)
    }
}

/// The last complete frame's leaf geometry, in document order.
#[derive(Default)]
pub(crate) struct FrameLayout {
    pub(crate) leaves: Vec<LeafGeom>,
    pub(crate) index: HashMap<BlockId, usize>,
    /// Table bounds (union of its cells), for node selection by click.
    pub(crate) tables: Vec<(BlockId, Bounds<Pixels>)>,
    /// Reconstructed table grids (column / row boxes), see `layout_tables`.
    pub(crate) grids: Vec<TableGeom>,
    /// Cell boxes by cell id, for hit-testing between cells.
    pub(crate) cell_boxes: HashMap<BlockId, Bounds<Pixels>>,
    /// The editor's content area: tables span its width, while their cells'
    /// text bounds only cover the text.
    pub(crate) content: Option<Bounds<Pixels>>,
    /// The rem size the editor was drawn at, for widths given in rems (a
    /// toggle's gutter).
    pub(crate) rem_size: Pixels,
    /// Each laid-out cull unit (a top-level block, or a chunk of a long
    /// list) and the bounds it was drawn in: where a block with no text,
    /// such as a rule or a widget, is.
    pub(crate) units: Vec<(BlockId, Bounds<Pixels>)>,
}

impl FrameLayout {
    pub(crate) fn leaf(&self, block: BlockId) -> Option<&LeafGeom> {
        self.index.get(&block).map(|ix| &self.leaves[*ix])
    }

    pub(crate) fn caret_bounds(&self, pos: BlockPosition, width: Pixels) -> Option<Bounds<Pixels>> {
        let leaf = self.leaf(pos.block)?;
        let (line, h) = (leaf.line_height(), leaf.caret_height());
        let mut origin = leaf.caret_point(pos.offset);
        // GPUI centres the glyphs in the line box: so does the caret.
        origin.y += (line - h) / 2.;
        Some(Bounds::new(origin, size(width, h)))
    }

    /// The table under a window point.
    /// Kept for the block handle (not built yet); clicks in a table place the caret.
    #[allow(dead_code)]
    pub(crate) fn table_at(&self, at: Point<Pixels>) -> Option<BlockId> {
        self.tables
            .iter()
            .find(|(_, bounds)| bounds.contains(&at))
            .map(|(id, _)| *id)
    }

    /// Nearest caret position to a window point.
    pub(crate) fn hit_test(&self, at: Point<Pixels>) -> Option<BlockPosition> {
        // Inside a table cell's box (padding and slack included): that
        // cell. Leaf bottoms are not monotonic across a table row whose
        // cells wrap to different heights, so the search below can miss.
        if let Some((cell, _)) = self.cell_boxes.iter().find(|(_, b)| b.contains(&at))
            && let Some(leaf) = self.leaf(*cell)
        {
            return Some(BlockPosition::new(*cell, leaf.offset_at(at)));
        }
        let last = self.leaves.len().checked_sub(1)?;
        let ix = self
            .leaves
            .partition_point(|leaf| leaf.bounds().bottom() <= at.y)
            .min(last);
        let ix = self.nearest_in_band(ix, at.x);
        let leaf = &self.leaves[ix];
        Some(BlockPosition::new(leaf.info.block, leaf.offset_at(at)))
    }

    /// Leaves side by side with `ix` (table cells in one row share a
    /// vertical band): the one horizontally nearest to `x`.
    fn nearest_in_band(&self, ix: usize, x: Pixels) -> usize {
        let band = self.leaves[ix].bounds();
        let overlaps = |i: usize| {
            let b = self.leaves[i].bounds();
            b.top() < band.bottom() && b.bottom() > band.top()
        };
        let mut lo = ix;
        while lo > 0 && overlaps(lo - 1) {
            lo -= 1;
        }
        let mut hi = ix;
        while hi + 1 < self.leaves.len() && overlaps(hi + 1) {
            hi += 1;
        }
        let distance = |i: usize| {
            let leaf = &self.leaves[i];
            let b = self
                .cell_boxes
                .get(&leaf.info.block)
                .copied()
                .unwrap_or_else(|| leaf.bounds());
            if x < b.left() {
                b.left() - x
            } else if x > b.right() {
                x - b.right()
            } else {
                px(0.)
            }
        };
        (lo..=hi)
            .min_by(|a, b| {
                distance(*a)
                    .partial_cmp(&distance(*b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or(ix)
    }

    /// One visual line above/below `pos`, keeping `x`.
    pub(crate) fn vertical(
        &self,
        pos: BlockPosition,
        x: Pixels,
        down: bool,
    ) -> Option<BlockPosition> {
        let ix = *self.index.get(&pos.block)?;
        let leaf = &self.leaves[ix];
        let caret = leaf.caret_point(pos.offset);
        let lh = leaf.line_height();
        let y = if down {
            caret.y + lh * 1.5
        } else {
            caret.y - lh * 0.5
        };
        let bounds = leaf.bounds();
        if y >= bounds.top() && y < bounds.bottom() {
            return Some(BlockPosition::new(
                leaf.info.block,
                leaf.offset_at(point(x, y)),
            ));
        }
        // The next leaf above/below that is not side by side with this one
        // (skips the rest of a table row), then the nearest in its band.
        let side_by_side = |i: usize| {
            let b = self.leaves[i].bounds();
            b.top() < bounds.bottom() && b.bottom() > bounds.top()
        };
        let mut j = ix;
        loop {
            j = if down {
                Some(j + 1).filter(|j| *j < self.leaves.len())?
            } else {
                j.checked_sub(1)?
            };
            if !side_by_side(j) {
                break;
            }
        }
        let next = &self.leaves[self.nearest_in_band(j, x)];
        let b = next.bounds();
        let y = if down {
            b.top() + next.line_height() / 2.
        } else {
            b.bottom() - next.line_height() / 2.
        };
        Some(BlockPosition::new(
            next.info.block,
            next.offset_at(point(x, y)),
        ))
    }

    /// Start or end of the visual (soft-wrapped) line holding `pos`.
    pub(crate) fn line_edge(&self, pos: BlockPosition, end: bool) -> Option<BlockPosition> {
        let leaf = self.leaf(pos.block)?;
        let caret = leaf.caret_point(pos.offset);
        let b = leaf.bounds();
        let x = if end {
            b.right() + px(10000.)
        } else {
            b.left() - px(1.)
        };
        let y = caret.y + leaf.line_height() / 2.;
        Some(BlockPosition::new(pos.block, leaf.offset_at(point(x, y))))
    }
}

/// One table's laid-out grid, rebuilt from its cells' text geometry.
#[derive(Clone, Debug)]
pub(crate) struct TableGeom {
    pub(crate) table: BlockId,
    /// Column `(left, right)` edges, slack and divider included.
    pub(crate) cols: Vec<(Pixels, Pixels)>,
    /// Row `(top, bottom)` edges, padding included.
    pub(crate) rows: Vec<(Pixels, Pixels)>,
    /// The table's frame.
    pub(crate) bounds: Bounds<Pixels>,
}

/// A table as the layout pass needs it: its cell ids by row.
pub(crate) struct TableShape {
    pub(crate) table: BlockId,
    pub(crate) grid: Vec<Vec<BlockId>>,
}

/// The width of the lines between a table's rows and round it (`draw.rs`).
const RULE: f32 = 1.;

impl FrameLayout {
    /// Rebuild each table's column and row boxes from where its cells' text was laid out: each
    /// cell's text sits `pad` inside its box, a line `RULE` wide closes every row but the last,
    /// and the table's border is round them all (`draw::table_element`).
    pub(crate) fn layout_tables(&mut self, shapes: &[TableShape], pad: Pixels) {
        self.grids.clear();
        self.cell_boxes.clear();
        let rule = px(RULE);
        for shape in shapes {
            let cols = shape.grid.iter().map(Vec::len).max().unwrap_or(0);
            // Each column's (left, right), from the first of its cells laid out.
            let mut spans: Vec<Option<(Pixels, Pixels)>> = vec![None; cols];
            for row in &shape.grid {
                for (c, cell) in row.iter().enumerate() {
                    if spans[c].is_none()
                        && let Some(leaf) = self.leaf(*cell)
                    {
                        let b = leaf.bounds();
                        spans[c] = Some((b.left() - pad, b.right() + pad));
                    }
                }
            }
            let Some(col_spans) = spans.into_iter().collect::<Option<Vec<_>>>() else {
                continue;
            };
            if col_spans.is_empty() {
                continue;
            }
            let row_spans: Vec<(Pixels, Pixels)> = shape
                .grid
                .iter()
                .enumerate()
                .filter_map(|(r, row)| {
                    let bounds: Vec<Bounds<Pixels>> = row
                        .iter()
                        .filter_map(|c| self.leaf(*c).map(|l| l.bounds()))
                        .collect();
                    let top = bounds.iter().map(|b| b.top()).reduce(|a, b| a.min(b))?;
                    let bottom = bounds.iter().map(|b| b.bottom()).reduce(|a, b| a.max(b))?;
                    let last = r + 1 == shape.grid.len();
                    Some((top - pad, bottom + pad + if last { px(0.) } else { rule }))
                })
                .collect();
            if row_spans.len() != shape.grid.len() {
                continue;
            }
            for (r, row) in shape.grid.iter().enumerate() {
                for (c, cell) in row.iter().enumerate() {
                    let (l, rt) = col_spans[c];
                    let (t, b) = row_spans[r];
                    self.cell_boxes
                        .insert(*cell, Bounds::from_corners(point(l, t), point(rt, b)));
                }
            }
            let (l, r) = (col_spans[0].0, col_spans[cols - 1].1);
            let (t, b) = (row_spans[0].0, row_spans[row_spans.len() - 1].1);
            self.grids.push(TableGeom {
                table: shape.table,
                cols: col_spans,
                rows: row_spans,
                bounds: Bounds::from_corners(point(l - rule, t - rule), point(r + rule, b + rule)),
            });
        }
    }
}
