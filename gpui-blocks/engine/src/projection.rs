//! What the view draws from, per document revision: the top-level blocks in order, the leaves
//! that get a text layout, and each table's cells. Rebuilt once per revision, so a frame that
//! changes nothing walks no block.

use std::rc::Rc;

use super::{
    document::{BlockContent, EditorDocument},
    id::BlockId,
    kind::BlockKindData,
};

/// Geometry facts the frame needs about a caret leaf.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LeafInfo {
    pub(crate) block: BlockId,
    /// Caret offsets are 0/1 (code blocks in the spike).
    pub(crate) atomic: bool,
    /// Model text length (bytes); for atomic leaves the rendered text length.
    pub(crate) len: usize,
}

#[derive(Default)]
pub(crate) struct Snapshot {
    /// The top-level blocks, in order.
    pub(crate) blocks: Rc<[BlockId]>,
    /// Leaves that have a rendered text layout, in document order.
    pub(crate) leaves: Rc<Vec<LeafInfo>>,
    /// Tables and their keyed cells, for node selection (no caret in cells).
    pub(crate) tables: Rc<Vec<(BlockId, Vec<BlockId>)>>,
}

#[derive(Default)]
pub(crate) struct SnapshotCache {
    revision: Option<u64>,
    snapshot: Rc<Snapshot>,
    /// `cfg(test)`-checked perf counter (D9): snapshot rebuilds.
    pub(crate) rebuilds: usize,
}

impl SnapshotCache {
    pub(crate) fn snapshot(&mut self, doc: &EditorDocument) -> Rc<Snapshot> {
        if self.revision == Some(doc.revision) {
            return self.snapshot.clone();
        }
        self.rebuilds += 1;
        let blocks: Rc<[BlockId]> = doc.children(doc.root).into();
        let leaves = doc
            .leaves()
            .iter()
            .filter_map(|id| leaf_info(doc, *id))
            .collect();
        let mut tables: Vec<(BlockId, Vec<BlockId>)> = Vec::new();
        for id in doc.leaves() {
            if doc.kind(*id) != Some(&BlockKindData::TableCell) {
                continue;
            }
            let Some(table) = doc
                .path(*id)
                .into_iter()
                .rev()
                .find(|b| matches!(doc.kind(*b), Some(BlockKindData::Table { .. })))
            else {
                continue;
            };
            match tables.last_mut() {
                Some((last, cells)) if *last == table => cells.push(*id),
                _ => tables.push((table, vec![*id])),
            }
        }
        self.snapshot = Rc::new(Snapshot {
            blocks,
            leaves: Rc::new(leaves),
            tables: Rc::new(tables),
        });
        self.revision = Some(doc.revision);
        self.snapshot.clone()
    }

    /// Drops the snapshot, so the next frame builds it again.
    pub(crate) fn invalidate(&mut self) {
        self.revision = None;
    }
}

fn leaf_info(doc: &EditorDocument, id: BlockId) -> Option<LeafInfo> {
    let entry = doc.get(id)?;
    match &entry.content {
        BlockContent::Inline(inline) => Some(LeafInfo {
            block: id,
            atomic: false,
            len: inline.len(),
        }),
        BlockContent::Plain(code) => Some(LeafInfo {
            block: id,
            atomic: false,
            len: code.len(),
        }),
        _ => None,
    }
}
