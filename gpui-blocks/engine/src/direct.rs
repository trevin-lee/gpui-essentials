// Changed in Trevin Lee's fork (2026): callouts and toggles projected as block surfaces.
//! Direct projection of the editing model into the shared renderer's
//! `BlockNode`s (design §7.3): no Markdown round trip, so no render-mode
//! placeholders and no re-parse of a whole list for an edit inside it.
//! Every text leaf (paragraph, heading, code block, table cell) gets its
//! `BlockId` as `editor_key`.

use std::collections::HashMap;

use super::{
    document::{AtomData, BlockContent, EditorDocument},
    id::BlockId,
    inline::{InlineAtom, InlineText},
    kind::BlockKindData,
};
use gpui_kit::base::text::editor_host::{
    BlockNode, BlockState, CodeBlock, InlineNode, Paragraph, Span, Table,
};

/// The surface an open toggle is drawn in; see [`crate::surfaces`].
pub const TOGGLE_OPEN_SURFACE: &str = "editor-toggle-open";
/// The surface a closed toggle is drawn in.
pub const TOGGLE_CLOSED_SURFACE: &str = "editor-toggle-closed";

/// A paragraph holding `inline`, keyed by `id`.
pub(crate) fn paragraph(inline: &InlineText, id: BlockId, placeholders: bool) -> Paragraph {
    let mut paragraph = Paragraph::default();
    for (range, mark, atom) in inline.pieces() {
        match atom {
            Some(InlineAtom::Image(image)) if placeholders => {
                paragraph.push(image_placeholder(image).marks(vec![(0..1, mark.clone())]))
            }
            Some(InlineAtom::Image(image)) => paragraph.push(InlineNode::image(image.clone())),
            Some(InlineAtom::Custom(node)) => {
                let node = InlineNode::custom(node.clone());
                let len = node.text().len();
                paragraph.push(node.marks(vec![(0..len, mark.clone())]));
            }
            None => {
                let text = &inline.text()[range];
                paragraph.push(
                    InlineNode::new(text.to_string()).marks(vec![(0..text.len(), mark.clone())]),
                );
            }
        }
    }
    paragraph.with_leaf_key(id.as_u64())
}

/// An image atom shown as its alt text, never loading the image. It is a
/// custom inline object without a renderer, which draws its text fallback and
/// stays one atomic caret and selection unit.
fn image_placeholder(image: &gpui_kit::base::text::editor_host::ImageNode) -> InlineNode {
    let label: gpui::SharedString = image
        .alt
        .clone()
        .filter(|alt| !alt.is_empty())
        .unwrap_or_else(|| "Image".into());
    InlineNode::custom(
        gpui_kit::base::text::MarkdownNode::new("image-placeholder", ())
            .text(label.clone())
            .accessibility_label(label),
    )
}

/// Whether gpui-kit draws `block` as nothing in a surface: a paragraph with no inline atom
/// (an image or a custom node) and only whitespace for text, or a link definition, a break or
/// an unknown block (gpui-base's `BlockNode::draws_nothing`). Other blocks, an empty heading
/// too, draw something.
fn draws_nothing(doc: &EditorDocument, block: BlockId) -> bool {
    match doc.kind(block) {
        Some(BlockKindData::Paragraph) => {}
        Some(
            BlockKindData::Definition { .. } | BlockKindData::Break { .. } | BlockKindData::Unknown,
        ) => {
            return true;
        }
        _ => return false,
    }
    doc.inline(block).is_some_and(|inline| {
        inline
            .pieces()
            .all(|(range, _, atom)| atom.is_none() && inline.text()[range].trim().is_empty())
    })
}

/// Project one block (and its subtree), uncached.
#[cfg(test)]
pub(crate) fn project_block(doc: &EditorDocument, id: BlockId) -> Option<BlockNode> {
    NodeCache::default().node(doc, id)
}

/// Per-node projection cache (design §7.3). Every block and table row is
/// cached under its id with the block revision it was built at; `touch`
/// bubbles revisions to ancestors, so an edit rebuilds only the path from
/// the edited leaf to its top-level block and reuses every other subtree
/// (sharing its retained `InlineState`s).
#[derive(Default)]
pub(crate) struct NodeCache {
    /// Present image atoms as text placeholders: nothing is resolved,
    /// decoded or loaded (`ImagePolicy::Placeholder`).
    pub(crate) placeholders: bool,
    nodes: HashMap<BlockId, (u64, BlockNode)>,
    rows: HashMap<BlockId, (u64, Vec<Paragraph>)>,
    /// Nodes and rows built (not reused), for the perf tests (D9).
    pub(crate) built: usize,
}

impl NodeCache {
    /// Drop entries of blocks that left the document, once the cache has
    /// grown well past the document (amortized O(1) per pass).
    pub(crate) fn collect_garbage(&mut self, doc: &EditorDocument) {
        if self.nodes.len() + self.rows.len() > 2 * doc.len() + 64 {
            self.nodes.retain(|id, _| doc.get(*id).is_some());
            self.rows.retain(|id, _| doc.get(*id).is_some());
        }
    }

    pub(crate) fn clear(&mut self) {
        self.nodes.clear();
        self.rows.clear();
    }

    pub(crate) fn node(&mut self, doc: &EditorDocument, id: BlockId) -> Option<BlockNode> {
        let revision = doc.block_revision(id);
        let node = match self.nodes.get(&id) {
            Some((rev, node)) if *rev == revision => node.clone(),
            _ => {
                let node = self.build(doc, id)?;
                self.built += 1;
                self.nodes.insert(id, (revision, node.clone()));
                node
            }
        };
        Some(node)
    }

    fn row(&mut self, doc: &EditorDocument, id: BlockId) -> Vec<Paragraph> {
        let revision = doc.block_revision(id);
        match self.rows.get(&id) {
            Some((rev, row)) if *rev == revision => row.clone(),
            _ => {
                self.built += 1;
                let row: Vec<Paragraph> = doc
                    .children(id)
                    .iter()
                    .map(|cell| match doc.inline(*cell) {
                        Some(inline) => paragraph(inline, *cell, self.placeholders),
                        None => paragraph(&InlineText::new(), *cell, self.placeholders),
                    })
                    .collect();
                self.rows.insert(id, (revision, row.clone()));
                row
            }
        }
    }

    /// A surface's children as drawn. gpui-kit draws no container at all when every line in it
    /// draws nothing (a stream that has only sent a callout's marker), which would make an
    /// emptied callout or toggle vanish with its caret. So its first line, when all of them draw
    /// nothing, is drawn with a zero-width space after its text. Only the drawing has it: the
    /// model's line keeps its text, its caret offsets map to the same rendered ones (the space
    /// comes after them all), and copies and saved Markdown come from the model.
    fn drawable(
        &self,
        doc: &EditorDocument,
        id: BlockId,
        mut nodes: Vec<BlockNode>,
    ) -> Vec<BlockNode> {
        let ids = doc.children(id);
        let blank = !ids.is_empty()
            && ids
                .iter()
                .take(nodes.len())
                .all(|child| draws_nothing(doc, *child));
        if blank
            && let Some(first) = ids.first()
            && let Some(inline) = doc.inline(*first)
        {
            let mut shown = inline.clone();
            shown.push(InlineText::plain("\u{200B}", Default::default()));
            nodes[0] = BlockNode::Paragraph(paragraph(&shown, *first, self.placeholders));
        }
        nodes
    }

    fn build(&mut self, doc: &EditorDocument, id: BlockId) -> Option<BlockNode> {
        let entry = doc.get(id)?;
        let children = |this: &mut Self| -> Vec<BlockNode> {
            doc.children(id)
                .iter()
                .filter_map(|child| this.node(doc, *child))
                .collect()
        };
        let inline = || match &entry.content {
            BlockContent::Inline(inline) => paragraph(inline, id, self.placeholders),
            _ => paragraph(&InlineText::new(), id, self.placeholders),
        };
        Some(match &entry.kind {
            BlockKindData::Root => BlockNode::Root {
                children: children(self),
                span: None,
            },
            BlockKindData::Paragraph => BlockNode::Paragraph(inline()),
            BlockKindData::Heading { level } => BlockNode::Heading {
                level: *level,
                children: inline(),
                span: None,
            },
            BlockKindData::Blockquote => BlockNode::Blockquote {
                children: children(self),
                span: None,
            },
            // Drawn in the container the host registers under the callout's
            // name (`MarkdownExtensions::block_surface`). The importer and
            // `SetCallout` make callouts only of registered names.
            BlockKindData::Callout { name } => {
                let all = children(self);
                BlockNode::Surface {
                    name: name.clone(),
                    children: self.drawable(doc, id, all),
                    span: None,
                }
            }
            // A closed toggle projects only its summary, so nothing of its body
            // is laid out, measured or hit.
            BlockKindData::Toggle { open } => BlockNode::Surface {
                name: if *open {
                    TOGGLE_OPEN_SURFACE.into()
                } else {
                    TOGGLE_CLOSED_SURFACE.into()
                },
                children: if *open {
                    let all = children(self);
                    self.drawable(doc, id, all)
                } else {
                    let summary = doc
                        .children(id)
                        .iter()
                        .take(1)
                        .filter_map(|child| self.node(doc, *child))
                        .collect();
                    self.drawable(doc, id, summary)
                },
                span: None,
            },
            BlockKindData::List { ordered, start } => BlockNode::List {
                children: children(self),
                ordered: *ordered,
                start: *start,
                span: None,
            },
            BlockKindData::ListItem { checked, spread } => BlockNode::ListItem {
                children: children(self),
                spread: *spread,
                checked: *checked,
                span: None,
                task_marker: None,
            },
            BlockKindData::CodeBlock { lang, meta } => {
                let code = match &entry.content {
                    BlockContent::Plain(code) => code.clone(),
                    _ => String::new(),
                };
                let code = CodeBlock::new(code.into(), lang.clone(), None::<Span>)
                    .with_meta(meta.clone())
                    .with_leaf_key(id.as_u64());
                BlockNode::CodeBlock(code)
            }
            BlockKindData::Table { aligns } => BlockNode::Table(Table::new(
                doc.children(id)
                    .iter()
                    .map(|row| self.row(doc, *row))
                    .collect(),
                aligns.clone(),
            )),
            BlockKindData::TableRow | BlockKindData::TableCell => return None,
            BlockKindData::HorizontalRule => BlockNode::HorizontalRule { span: None },
            BlockKindData::Break { html } => BlockNode::Break {
                html: *html,
                span: None,
            },
            BlockKindData::Custom => match &entry.content {
                BlockContent::Atom(AtomData::Custom(node)) => BlockNode::Custom {
                    node: node.clone(),
                    state: BlockState::default(),
                },
                _ => BlockNode::Unknown,
            },
            BlockKindData::Definition {
                identifier,
                url,
                title,
            } => BlockNode::Definition {
                identifier: identifier.clone(),
                url: url.clone(),
                title: title.clone(),
                span: None,
            },
            BlockKindData::Unknown => BlockNode::Unknown,
        })
    }
}
