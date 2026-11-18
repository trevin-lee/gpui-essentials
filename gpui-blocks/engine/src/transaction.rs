// Changed in Trevin Lee's fork (2026): a toggle's summary is normalized to come first.
use std::ops::Range;

use super::{
    document::{BlockContent, EditorDocument},
    id::BlockId,
    inline::InlineText,
    kind::BlockKindData,
    selection::BlockSelection,
    step::{DetachedBlock, Step, StepError, StepMap},
};

/// Why an edit happened; drives undo coalescing.
#[non_exhaustive]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EditOrigin {
    Typing,
    Backspace,
    DeleteForward,
    /// A word delete backwards (D21); coalesces only with word deletes.
    WordBackspace,
    /// A word delete forwards (D21); coalesces only with word deletes.
    WordDelete,
    Ime,
    Paste,
    Cut,
    Command(&'static str),
    InputRule(&'static str),
    Api,
    Undo,
    Redo,
}

/// A committed group of steps. Opaque.
#[derive(Clone, Debug)]
pub struct Transaction {
    pub(crate) steps: Vec<Step>,
    pub(crate) inverse: Vec<Step>,
    pub(crate) maps: Vec<StepMap>,
    pub(crate) selection_before: BlockSelection,
    pub(crate) selection_after: BlockSelection,
    pub(crate) origin: EditOrigin,
}

impl Transaction {
    /// Map a selection through every step; `None` if an endpoint's block
    /// was removed.
    pub(crate) fn map_selection(&self, selection: &BlockSelection) -> Option<BlockSelection> {
        let map = |mut p| {
            for step_map in &self.maps {
                p = step_map.map(p)?;
            }
            Some(p)
        };
        match selection {
            BlockSelection::Text { anchor, head } => Some(BlockSelection::Text {
                anchor: map(*anchor)?,
                head: map(*head)?,
            }),
            BlockSelection::Blocks { .. } | BlockSelection::Cells { .. } => Some(selection.clone()),
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// Re-apply recorded steps (redo) or their inverses (undo).
    pub(crate) fn replay(steps: &[Step], doc: &mut EditorDocument) -> Result<(), StepError> {
        for step in steps {
            step.apply(doc)?;
        }
        doc.refresh_order();
        doc.revision += 1;
        Ok(())
    }
}

/// Builds a transaction against the live document. Every change goes
/// through [`TransactionBuilder::step`], which records the step and its
/// inverse.
pub(crate) struct TransactionBuilder<'a> {
    pub(crate) doc: &'a mut EditorDocument,
    steps: Vec<Step>,
    inverse: Vec<Step>,
    maps: Vec<StepMap>,
}

impl<'a> TransactionBuilder<'a> {
    pub(crate) fn new(doc: &'a mut EditorDocument) -> Self {
        Self {
            doc,
            steps: Vec::new(),
            inverse: Vec::new(),
            maps: Vec::new(),
        }
    }

    pub(crate) fn step(&mut self, step: Step) -> Result<StepMap, StepError> {
        let (inverse, map) = step.apply(self.doc)?;
        self.steps.push(step);
        self.inverse.push(inverse);
        self.maps.push(map.clone());
        Ok(map)
    }

    pub(crate) fn replace_text(
        &mut self,
        block: BlockId,
        range: Range<usize>,
        insert: InlineText,
    ) -> Result<(), StepError> {
        if range.is_empty() && insert.is_empty() {
            return Ok(());
        }
        self.step(Step::ReplaceText {
            block,
            range,
            insert,
        })
        .map(|_| ())
    }

    pub(crate) fn replace_plain(
        &mut self,
        block: BlockId,
        range: Range<usize>,
        insert: &str,
    ) -> Result<(), StepError> {
        if range.is_empty() && insert.is_empty() {
            return Ok(());
        }
        self.step(Step::ReplacePlain {
            block,
            range,
            insert: insert.to_string(),
        })
        .map(|_| ())
    }

    pub(crate) fn insert(
        &mut self,
        parent: BlockId,
        index: usize,
        blocks: Vec<DetachedBlock>,
    ) -> Result<(), StepError> {
        if blocks.is_empty() {
            return Ok(());
        }
        self.step(Step::InsertBlocks {
            parent,
            index,
            blocks,
        })
        .map(|_| ())
    }

    /// Remove blocks and return copies of them (ids kept) so callers can
    /// re-insert them elsewhere: a move is remove + insert.
    pub(crate) fn remove(
        &mut self,
        parent: BlockId,
        range: Range<usize>,
    ) -> Result<Vec<DetachedBlock>, StepError> {
        if range.is_empty() {
            return Ok(Vec::new());
        }
        self.step(Step::RemoveBlocks { parent, range })?;
        match self.inverse.last() {
            Some(Step::InsertBlocks { blocks, .. }) => Ok(blocks.clone()),
            _ => Err(StepError::OutOfRange),
        }
    }

    pub(crate) fn remove_block(&mut self, id: BlockId) -> Result<DetachedBlock, StepError> {
        let (parent, index) = self
            .doc
            .index_in_parent(id)
            .ok_or(StepError::UnknownBlock(id))?;
        self.remove(parent, index..index + 1)?
            .pop()
            .ok_or(StepError::OutOfRange)
    }

    /// Move `id` (keeping ids) to `index` of `parent`, where `index` counts
    /// the target's children *after* the block has been taken out.
    pub(crate) fn move_block(
        &mut self,
        id: BlockId,
        parent: BlockId,
        index: usize,
    ) -> Result<(), StepError> {
        let block = self.remove_block(id)?;
        self.insert(parent, index, vec![block])
    }

    pub(crate) fn set_kind(
        &mut self,
        block: BlockId,
        kind: BlockKindData,
    ) -> Result<(), StepError> {
        if self.doc.kind(block) == Some(&kind) {
            return Ok(());
        }
        self.step(Step::SetKind { block, kind }).map(|_| ())
    }

    /// Structural normalization (design §5.1), appended as ordinary steps so
    /// undo stays exact:
    /// - a `List` holds only `ListItem`s,
    /// - a `ListItem` starts with a leaf,
    /// - no container other than `Root` is empty,
    /// - adjacent sibling lists of the same type are merged (Markdown cannot
    ///   write them apart),
    /// - a `Columns` is at the top level and holds 2 to `MAX_COLUMNS` `Column`s, and a `Column`
    ///   is only in a `Columns` (so columns never nest),
    /// - there is at least one caret leaf.
    pub(crate) fn normalize(&mut self) -> Result<(), StepError> {
        loop {
            let Some(fix) = self.find_violation() else {
                return Ok(());
            };
            match fix {
                Fix::RemoveEmpty(id) => {
                    self.remove_block(id)?;
                }
                Fix::WrapInItem(id) => {
                    let (parent, index) = self
                        .doc
                        .index_in_parent(id)
                        .ok_or(StepError::UnknownBlock(id))?;
                    let block = self.remove_block(id)?;
                    let item = DetachedBlock::container(
                        BlockKindData::ListItem {
                            checked: None,
                            spread: false,
                        },
                        vec![block],
                    );
                    self.insert(parent, index, vec![item])?;
                }
                Fix::LeadingParagraph(item) => {
                    self.insert(item, 0, vec![DetachedBlock::paragraph(InlineText::new())])?;
                }
                Fix::MergeLists(first, second) => {
                    let count = self.doc.children(second).len();
                    let items = self.remove(second, 0..count)?;
                    let len = self.doc.children(first).len();
                    self.insert(first, len, items)?;
                    self.remove_block(second)?;
                }
                Fix::Unwrap(id) => {
                    let (parent, index) = self
                        .doc
                        .index_in_parent(id)
                        .ok_or(StepError::UnknownBlock(id))?;
                    let count = self.doc.children(id).len();
                    let children = self.remove(id, 0..count)?;
                    self.remove_block(id)?;
                    self.insert(parent, index, children)?;
                }
                Fix::IntoColumn(stray) => {
                    let (columns, index) = self
                        .doc
                        .index_in_parent(stray)
                        .ok_or(StepError::UnknownBlock(stray))?;
                    let siblings = self.doc.children(columns).to_vec();
                    let before = siblings[..index]
                        .iter()
                        .rev()
                        .find(|c| matches!(self.doc.kind(**c), Some(BlockKindData::Column { .. })));
                    let after = siblings[index + 1..]
                        .iter()
                        .find(|c| matches!(self.doc.kind(**c), Some(BlockKindData::Column { .. })));
                    let block = self.remove_block(stray)?;
                    match (before, after) {
                        (Some(column), _) => {
                            let len = self.doc.children(*column).len();
                            self.insert(*column, len, vec![block])?;
                        }
                        (None, Some(column)) => self.insert(*column, 0, vec![block])?,
                        // No column at all: a column of its own, and the rules go on from there.
                        (None, None) => {
                            let column = DetachedBlock::container(
                                BlockKindData::Column { width: 1. },
                                vec![block],
                            );
                            self.insert(columns, index, vec![column])?;
                        }
                    }
                }
                Fix::MergeLastColumn(columns) => {
                    let ids = self.doc.children(columns).to_vec();
                    let (last, before) = (ids[ids.len() - 1], ids[ids.len() - 2]);
                    let count = self.doc.children(last).len();
                    let blocks = self.remove(last, 0..count)?;
                    let len = self.doc.children(before).len();
                    self.insert(before, len, blocks)?;
                    self.remove_block(last)?;
                }
                Fix::NoLeaf => {
                    let root = self.doc.root;
                    let len = self.doc.children(root).len();
                    self.insert(root, len, vec![DetachedBlock::paragraph(InlineText::new())])?;
                }
            }
        }
    }

    fn find_violation(&self) -> Option<Fix> {
        let doc = &*self.doc;
        let mut stack = vec![doc.root];
        while let Some(id) = stack.pop() {
            let entry = doc.get(id)?;
            if let BlockContent::Children(children) = &entry.content {
                if children.is_empty() && entry.kind != BlockKindData::Root {
                    return Some(Fix::RemoveEmpty(id));
                }
                if let BlockKindData::List { .. } = entry.kind
                    && let Some(stray) = children
                        .iter()
                        .find(|c| !matches!(doc.kind(**c), Some(BlockKindData::ListItem { .. })))
                {
                    return Some(Fix::WrapInItem(*stray));
                }
                if let BlockKindData::ListItem { .. } = entry.kind
                    && !children.first().is_some_and(|c| doc.is_caret_leaf(*c))
                {
                    return Some(Fix::LeadingParagraph(id));
                }
                if let BlockKindData::Columns = entry.kind {
                    if entry.parent != Some(doc.root) || children.len() < 2 {
                        return Some(Fix::Unwrap(id));
                    }
                    if let Some(stray) = children
                        .iter()
                        .find(|c| !matches!(doc.kind(**c), Some(BlockKindData::Column { .. })))
                    {
                        return Some(Fix::IntoColumn(*stray));
                    }
                    if children.len() > crate::kind::MAX_COLUMNS {
                        return Some(Fix::MergeLastColumn(id));
                    }
                }
                if let BlockKindData::Column { .. } = entry.kind
                    && entry
                        .parent
                        .and_then(|p| doc.kind(p))
                        .is_none_or(|k| *k != BlockKindData::Columns)
                {
                    return Some(Fix::Unwrap(id));
                }
                // A toggle's first child is its summary, a line of text.
                if let BlockKindData::Toggle { .. } = entry.kind
                    && !children.first().is_some_and(|c| {
                        matches!(
                            doc.kind(*c),
                            Some(BlockKindData::Paragraph | BlockKindData::Heading { .. })
                        )
                    })
                {
                    return Some(Fix::LeadingParagraph(id));
                }
                for pair in children.windows(2) {
                    if let (
                        Some(BlockKindData::List { ordered: a, .. }),
                        Some(BlockKindData::List { ordered: b, .. }),
                    ) = (doc.kind(pair[0]), doc.kind(pair[1]))
                        && a == b
                    {
                        return Some(Fix::MergeLists(pair[0], pair[1]));
                    }
                }
                stack.extend(children.iter().copied());
            }
        }
        if doc.walk_leaves().is_empty() {
            return Some(Fix::NoLeaf);
        }
        None
    }

    pub(crate) fn finish(
        mut self,
        origin: EditOrigin,
        selection_before: BlockSelection,
        selection_after: BlockSelection,
    ) -> Result<Transaction, StepError> {
        self.normalize()?;
        self.doc.refresh_order();
        if !self.steps.is_empty() {
            self.doc.revision += 1;
        }
        debug_assert_eq!(self.doc.check_invariants(), Ok(()));
        let mut inverse = self.inverse;
        inverse.reverse();
        Ok(Transaction {
            steps: self.steps,
            inverse,
            maps: self.maps,
            selection_before,
            selection_after,
            origin,
        })
    }

    /// Undo whatever was applied so far (used when a command fails halfway).
    pub(crate) fn rollback(self) {
        for step in self.inverse.iter().rev() {
            let _ = step.apply(self.doc);
        }
        self.doc.refresh_order();
    }
}

enum Fix {
    RemoveEmpty(BlockId),
    /// A container that can't be where it is: its children take its place.
    Unwrap(BlockId),
    /// A block in a columns block that isn't a column: into the column before it (or after).
    IntoColumn(BlockId),
    /// A columns block with too many columns: its last column's blocks join the one before.
    MergeLastColumn(BlockId),
    WrapInItem(BlockId),
    LeadingParagraph(BlockId),
    MergeLists(BlockId, BlockId),
    NoLeaf,
}
