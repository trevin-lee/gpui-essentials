// Changed in Trevin Lee's fork (2026): callouts and toggles imported; parsed from the
// Markdown parser's syntax tree directly, with the engine's own extensions.
//! Markdown into the editor model: the `markdown` crate's syntax tree (mdast), read with the
//! engine's [`Extensions`].

use std::collections::HashMap;
use std::sync::Arc;

use gpui::{Hsla, SharedString};
use markdown::mdast::{self, Node};
use markdown::{ParseOptions, to_mdast};

/// `source`'s syntax tree, or why it has none. The parser (`markdown` 1.0) panics on some
/// input, such as a list item opening an unclosed fence followed by a list of the other kind
/// (`"- a\n  ```\n1. a\n"`): that's an error here too, so pasted, imported or written Markdown
/// can't bring the app down.
pub fn parse(source: &str, options: &ParseOptions) -> Result<Node, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| to_mdast(source, options)))
        .map_err(|_| "the Markdown parser failed on this text".to_string())?
        .map_err(|e| e.to_string())
}

/// `source` as Markdown that reads back as its text alone, in paragraphs: every ASCII
/// punctuation mark escaped and lines' leading space dropped, so no construct forms. For text
/// the parser fails on, which is kept rather than lost.
pub(crate) fn literal(source: &str) -> String {
    let mut out = String::with_capacity(source.len() * 2);
    for line in source.lines() {
        for c in line.trim_start().chars() {
            if c.is_ascii_punctuation() {
                out.push('\\');
            }
            out.push(c);
        }
        out.push('\n');
    }
    out
}

use super::{
    document::{AtomData, BlockContent, EditorDocument},
    extension::{CustomNode, Extensions},
    inline::{InlineAtom, InlineText},
    kind::BlockKindData,
    step::{DetachedBlock, DetachedContent},
    transaction::TransactionBuilder,
};
use crate::kind::ColumnAlign;
use crate::text_mark::{ImageNode, LinkMark, TextMark};

/// The name of a block kept as the Markdown source it was (raw HTML, a footnote), and of an
/// inline piece kept so.
pub(crate) const SOURCE_BLOCK: &str = "editor-source-block";
pub(crate) const SOURCE_INLINE: &str = "editor-source-inline";

fn options(math: bool) -> ParseOptions {
    let mut options = ParseOptions::gfm();
    // Math is read so an extension can claim it; unclaimed, it's read again as prose.
    options.constructs.math_text = math;
    options.constructs.math_flow = math;
    options
}

pub(crate) fn parse_document(
    source: &str,
    extensions: Arc<Extensions>,
) -> Result<EditorDocument, SharedString> {
    // Text the parser fails on is read as plain text instead.
    let fallback;
    let (source, root) = match parse(source, &options(true)) {
        Ok(root) => (source, root),
        Err(_) => {
            fallback = literal(source);
            let root = parse(&fallback, &options(true)).map_err(SharedString::from)?;
            (fallback.as_str(), root)
        }
    };
    let reader = Reader::new(source, &extensions, &root);
    let nodes = root.children().map(Vec::as_slice).unwrap_or_default();
    let blocks = group_columns(reader.blocks(nodes));
    let mut doc = EditorDocument::empty(extensions.clone());
    let root_id = doc.root;
    // Drop the placeholder paragraph of the empty document.
    doc.detach(root_id, 0);
    for (ix, block) in blocks.into_iter().enumerate() {
        doc.attach(root_id, ix, block);
    }
    // Same structural rules as after every transaction (empty list items
    // get a paragraph, an empty document gets one leaf). The steps of this
    // pass are not history.
    let mut tx = TransactionBuilder::new(&mut doc);
    tx.normalize()
        .map_err(|e| SharedString::from(format!("{e:?}")))?;
    drop(tx);
    doc.structure_dirty = true;
    doc.refresh_order();
    // Markdown parsers omit final blank paragraphs. Reconstruct that editable
    // boundary as real document nodes, never a mutable raw-source shadow.
    // The last block's span tells exterior footer trivia from line endings
    // owned by an unterminated code fence/body.
    let exterior_start = nodes
        .last()
        .and_then(Node::position)
        .map_or(0, |p| p.end.offset);
    let exterior = source.get(exterior_start..).unwrap_or_default();
    let trimmed_spaces = exterior.trim_end_matches([' ', '\t']);
    let stripped = trimmed_spaces.trim_end_matches(['\r', '\n']);
    let line_ends = &trimmed_spaces[stripped.len()..];
    let ending_count = if line_ends.contains('\n') {
        line_ends.bytes().filter(|b| *b == b'\n').count()
    } else {
        line_ends.len()
    };
    for _ in 0..ending_count.div_ceil(2) {
        let index = doc.children(root_id).len();
        doc.attach(
            root_id,
            index,
            DetachedBlock::leaf(
                BlockKindData::Paragraph,
                BlockContent::Inline(InlineText::default()),
            ),
        );
    }
    doc.refresh_order();

    // CommonMark ignores terminal paragraph spaces, but in a live editing
    // buffer they separate the next typed/dictated word. Restore that trivia
    // into the document, not a second mutable source buffer. Code retains its
    // exact own body and must not inherit trivia after its closing fence.
    let trimmed = source.trim_end_matches([' ', '\t']);
    let mut tail = &source[trimmed.len()..];
    let last = *doc.leaves().last().unwrap();
    if (matches!(doc.kind(last), Some(BlockKindData::Heading { .. }))
        || doc.index_in_parent(last).is_some_and(|(parent, _)| {
            matches!(doc.kind(parent), Some(BlockKindData::ListItem { .. }))
        }))
        && doc.inline(last).is_some_and(|inline| inline.is_empty())
    {
        // One space is the empty heading's structural marker separator.
        tail = tail.strip_prefix(' ').unwrap_or(tail);
    }
    if !tail.is_empty()
        && let Some(entry) = doc.get_mut(last)
        && let BlockContent::Inline(inline) = &mut entry.content
    {
        inline.push(InlineText::plain(tail, TextMark::default()));
    }
    doc.revision = 0;
    Ok(doc)
}

/// `markdown` as blocks, toggles grouped, for inserting or pasting: `None` when it doesn't
/// parse.
pub(crate) fn parse_blocks(markdown: &str, extensions: &Extensions) -> Option<Vec<DetachedBlock>> {
    // Text the parser fails on is pasted as plain text instead.
    let fallback;
    let (markdown, root) = match parse(markdown, &options(true)) {
        Ok(root) => (markdown, root),
        Err(_) => {
            fallback = literal(markdown);
            let root = parse(&fallback, &options(true)).ok()?;
            (fallback.as_str(), root)
        }
    };
    let reader = Reader::new(markdown, extensions, &root);
    Some(group_columns(reader.blocks(
        root.children().map(Vec::as_slice).unwrap_or_default(),
    )))
}

/// Link reference definitions, by their normalized identifier.
type Definitions = HashMap<String, (SharedString, Option<SharedString>)>;

/// Reads one source's syntax tree.
struct Reader<'a> {
    source: &'a str,
    extensions: &'a Extensions,
    definitions: Arc<Definitions>,
}

impl<'a> Reader<'a> {
    fn new(source: &'a str, extensions: &'a Extensions, root: &Node) -> Self {
        let mut definitions = Definitions::new();
        collect_definitions(root, &mut definitions);
        Self {
            source,
            extensions,
            definitions: Arc::new(definitions),
        }
    }

    /// Another source (a toggle's summary, math read as prose) with this one's definitions.
    fn sub<'b>(&'b self, source: &'b str) -> Reader<'b> {
        Reader {
            source,
            extensions: self.extensions,
            definitions: self.definitions.clone(),
        }
    }

    fn node_source(&self, node: &Node) -> Option<&'a str> {
        let p = node.position()?;
        self.source.get(p.start.offset..p.end.offset)
    }

    fn blocks(&self, nodes: &[Node]) -> Vec<DetachedBlock> {
        let mut out = Vec::new();
        for node in nodes {
            self.block(node, &mut out);
        }
        self.group_toggles(out)
    }

    fn custom_block(&self, node: &Node) -> Option<DetachedBlock> {
        self.extensions
            .iter()
            .filter(|e| e.is_block())
            .find_map(|e| e.parse(node))
            .map(|custom| {
                DetachedBlock::leaf(
                    BlockKindData::Custom,
                    BlockContent::Atom(AtomData::Custom(custom)),
                )
            })
    }

    fn block(&self, node: &Node, out: &mut Vec<DetachedBlock>) {
        if let Some(block) = self.custom_block(node) {
            out.push(block);
            return;
        }
        let leaf = DetachedBlock::leaf;
        let block = match node {
            Node::Paragraph(p) => leaf(
                BlockKindData::Paragraph,
                BlockContent::Inline(self.inline(&p.children)),
            ),
            Node::Heading(h) => leaf(
                BlockKindData::Heading { level: h.depth },
                BlockContent::Inline(self.inline(&h.children)),
            ),
            Node::Blockquote(q) => self.quote(q),
            Node::List(list) => DetachedBlock::container(
                BlockKindData::List {
                    ordered: list.ordered,
                    start: list.start,
                },
                self.blocks(&list.children),
            ),
            Node::ListItem(item) => {
                let mut children = self.blocks(&item.children);
                // A parsed bare marker is an editable empty item, not an empty
                // container to prune during normalization.
                if children.is_empty() {
                    children.push(DetachedBlock::paragraph(InlineText::new()));
                }
                DetachedBlock::container(
                    BlockKindData::ListItem {
                        checked: item.checked,
                        spread: item.spread,
                    },
                    children,
                )
            }
            Node::Code(code) => leaf(
                BlockKindData::CodeBlock {
                    lang: code.lang.clone().map(Into::into),
                    meta: code.meta.clone().map(Into::into),
                },
                BlockContent::Plain(code.value.clone()),
            ),
            // Math no extension claimed: its source, read as prose.
            Node::Math(_) => {
                let Some(text) = self.node_source(node) else {
                    return;
                };
                if let Ok(root) = parse(text, &options(false)) {
                    let sub = self.sub(text);
                    for child in root.children().into_iter().flatten() {
                        sub.block(child, out);
                    }
                }
                return;
            }
            Node::Html(html) if is_break_tag(&html.value) => leaf(
                BlockKindData::Break { html: true },
                BlockContent::Atom(AtomData::None),
            ),
            Node::Html(_) | Node::FootnoteDefinition(_) => match self.source_block(node) {
                Some(block) => block,
                None => return,
            },
            Node::Table(table) => {
                let aligns: Vec<ColumnAlign> =
                    table.align.iter().map(|a| ColumnAlign::from(*a)).collect();
                let rows = table
                    .children
                    .iter()
                    .filter_map(|row| match row {
                        Node::TableRow(row) => Some(row),
                        _ => None,
                    })
                    .map(|row| {
                        // GFM fixes the column count with the header row.
                        let cells = row
                            .children
                            .iter()
                            .take(aligns.len())
                            .map(|cell| {
                                let children =
                                    cell.children().map(Vec::as_slice).unwrap_or_default();
                                leaf(
                                    BlockKindData::TableCell,
                                    BlockContent::Inline(self.inline(children)),
                                )
                            })
                            .collect();
                        DetachedBlock::container(BlockKindData::TableRow, cells)
                    })
                    .collect();
                DetachedBlock::container(BlockKindData::Table { aligns }, rows)
            }
            Node::ThematicBreak(_) => leaf(
                BlockKindData::HorizontalRule,
                BlockContent::Atom(AtomData::None),
            ),
            Node::Definition(def) => leaf(
                BlockKindData::Definition {
                    identifier: def.identifier.clone().into(),
                    url: def.url.clone().into(),
                    title: def.title.clone().map(Into::into),
                },
                BlockContent::Atom(AtomData::None),
            ),
            _ => leaf(BlockKindData::Unknown, BlockContent::Atom(AtomData::None)),
        };
        out.push(block);
    }

    /// Raw HTML or a footnote, kept as the source it was. The source's later lines still hold
    /// their containers' markers (`> `), which the document writes again; those are dropped.
    fn source_block(&self, node: &Node) -> Option<DetachedBlock> {
        let source = self.node_source(node)?;
        // Block spans may include separator line endings, which the root owns.
        let source = source.trim_end_matches(['\r', '\n']);
        let start = node.position()?.start.offset;
        let line_start = self.source[..start].rfind('\n').map_or(0, |ix| ix + 1);
        let prefix = &self.source[line_start..start];
        let continuation: String = prefix
            .chars()
            .map(|ch| if ch == '>' { '>' } else { ' ' })
            .collect();
        let source = source
            .split('\n')
            .enumerate()
            .map(|(ix, line)| {
                if ix == 0 || continuation.is_empty() {
                    line
                } else if line.trim_end() == continuation.trim_end() {
                    ""
                } else {
                    line.strip_prefix(&continuation).unwrap_or(line)
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        Some(DetachedBlock::leaf(
            BlockKindData::Custom,
            BlockContent::Atom(AtomData::Custom(
                CustomNode::new(SOURCE_BLOCK, ())
                    .text(source.clone())
                    .markdown(source),
            )),
        ))
    }

    /// A blockquote, or a callout when its first line is a registered callout's marker alone
    /// (`> [!NOTE]`), as GitHub and Obsidian write it.
    fn quote(&self, quote: &mdast::Blockquote) -> DetachedBlock {
        if let Some((name, (icon, color), children)) = self.take_callout_marker(quote) {
            let mut blocks = self.blocks(&children);
            if blocks.is_empty() {
                blocks.push(DetachedBlock::paragraph(InlineText::new()));
            }
            return DetachedBlock::container(BlockKindData::Callout { name, icon, color }, blocks);
        }
        DetachedBlock::container(BlockKindData::Blockquote, self.blocks(&quote.children))
    }

    /// The callout's name, its look and its children without the marker, when the quote opens
    /// with one. The look is a comment on the marker's line (`> [!TIP] <!-- icon:💡 -->`); one
    /// that isn't a valid look, or has more after it, stays as the callout's first paragraph.
    #[allow(clippy::type_complexity)]
    fn take_callout_marker(
        &self,
        quote: &mdast::Blockquote,
    ) -> Option<(
        SharedString,
        (Option<SharedString>, Option<SharedString>),
        Vec<Node>,
    )> {
        let Some(Node::Paragraph(paragraph)) = quote.children.first() else {
            return None;
        };
        let Some(Node::Text(text)) = paragraph.children.first() else {
            return None;
        };
        let (line, rest) = match text.value.split_once('\n') {
            Some((line, rest)) => (line.strip_suffix('\r').unwrap_or(line), Some(rest)),
            None => (text.value.as_str(), None),
        };
        // Written as is: not escaped (`\[!NOTE]`), not from an entity.
        let start = text.position.as_ref()?.start.offset;
        if self.source.get(..start)?.ends_with('\\') || !self.source.get(start..)?.starts_with(line)
        {
            return None;
        }
        // A comment after the marker: its look, or else its first paragraph.
        let commented = rest.is_none()
            && line.ends_with(' ')
            && matches!(paragraph.children.get(1), Some(Node::Html(html)) if html.value.starts_with("<!--"));
        let line = if commented { line.trim_end() } else { line };
        let written = line.strip_prefix("[!")?.strip_suffix(']')?;
        if !self.extensions.is_callout(written) {
            return None;
        }
        let name: SharedString = written.to_ascii_lowercase().into();
        let mut first = paragraph.clone();
        let mut look = (None, None);
        if commented {
            let Some(Node::Html(html)) = paragraph.children.get(1) else {
                return None;
            };
            let parsed = crate::callout::read_callout_look(&html.value);
            match (parsed, first.children.get(2)) {
                (Some(parsed), None) => {
                    look = parsed;
                    first.children.clear();
                }
                (Some(parsed), Some(Node::Break(_))) => {
                    look = parsed;
                    first.children.drain(..3);
                }
                (Some(parsed), Some(Node::Text(next))) if next.value.starts_with('\n') => {
                    look = parsed;
                    let after = next.value[1..].to_string();
                    first.children.drain(..2);
                    match first.children.first_mut() {
                        Some(Node::Text(text)) if !after.is_empty() => text.value = after,
                        _ => {
                            first.children.remove(0);
                        }
                    }
                }
                // Not a look, or more after it on the line: the callout's first paragraph.
                _ => {
                    first.children.remove(0);
                }
            }
            let mut children: Vec<Node> = quote.children.clone();
            if first.children.is_empty() {
                children.remove(0);
            } else {
                children[0] = Node::Paragraph(first);
            }
            return Some((name, look, children));
        }
        match rest {
            Some(rest) => {
                if let Some(Node::Text(text)) = first.children.first_mut() {
                    text.value = rest.to_string();
                    if text.value.is_empty() {
                        first.children.remove(0);
                    }
                }
            }
            None => match first.children.get(1) {
                None => first.children.clear(),
                Some(Node::Break(_)) => {
                    first.children.drain(..2);
                }
                // More on the marker's line: not a callout.
                Some(_) => return None,
            },
        }
        let mut children: Vec<Node> = quote.children.clone();
        if first.children.is_empty() {
            children.remove(0);
        } else {
            children[0] = Node::Paragraph(first);
        }
        Some((name, look, children))
    }

    // Inline content.

    fn inline(&self, nodes: &[Node]) -> InlineText {
        let mut out = InlineText::new();
        self.inline_into(nodes, &TextMark::default(), &mut out);
        out
    }

    fn inline_into(&self, nodes: &[Node], mark: &TextMark, out: &mut InlineText) {
        let mut ix = 0;
        while ix < nodes.len() {
            // `<u>…</u>` and the like: a mark over what's between.
            if let Some((false, name, attrs)) = inline_html_tag(&nodes[ix])
                && let Some(tag_mark) = inline_html_mark(&name, attrs)
                && let Some(close) = matching_close_tag(nodes, ix, &name)
            {
                self.inline_into(&nodes[ix + 1..close], &with(mark, tag_mark), out);
                ix = close + 1;
                continue;
            }
            self.inline_node(&nodes[ix], mark, out);
            ix += 1;
        }
    }

    fn inline_node(&self, node: &Node, mark: &TextMark, out: &mut InlineText) {
        if let Some(custom) = self
            .extensions
            .iter()
            .filter(|e| !e.is_block())
            .find_map(|e| e.parse(node))
        {
            // Written back as it was read, unless it says otherwise.
            let custom = if custom.as_markdown().is_empty() {
                let source = self.node_source(node).unwrap_or_default().to_string();
                custom.markdown(source)
            } else {
                custom
            };
            out.push(InlineText::atom(InlineAtom::Custom(custom), mark.clone()));
            return;
        }
        match node {
            Node::Text(t) => {
                let text = t.value.replace("\r\n", " ").replace(['\n', '\r'], " ");
                out.push(InlineText::plain(&text, mark.clone()));
            }
            Node::Emphasis(n) => {
                self.inline_into(&n.children, &with(mark, TextMark::default().italic()), out)
            }
            Node::Strong(n) => {
                self.inline_into(&n.children, &with(mark, TextMark::default().bold()), out)
            }
            Node::Delete(n) => self.inline_into(
                &n.children,
                &with(mark, TextMark::default().strikethrough()),
                out,
            ),
            Node::InlineCode(code) => out.push(InlineText::plain(
                &code.value,
                with(mark, TextMark::default().code()),
            )),
            Node::Link(link) => {
                let linked = TextMark {
                    link: Some(LinkMark {
                        url: link.url.clone().into(),
                        identifier: None,
                        title: link.title.clone().map(Into::into),
                    }),
                    ..TextMark::default()
                };
                self.inline_into(&link.children, &with(mark, linked), out)
            }
            // A reference link written out in full: the canonical output is an inline link.
            Node::LinkReference(link) => match self.definitions.get(&link.identifier) {
                Some((url, title)) => {
                    let linked = TextMark {
                        link: Some(LinkMark {
                            url: url.clone(),
                            identifier: None,
                            title: title.clone(),
                        }),
                        ..TextMark::default()
                    };
                    self.inline_into(&link.children, &with(mark, linked), out)
                }
                None => self.inline_into(&link.children, mark, out),
            },
            Node::Image(image) => out.push(InlineText::atom(
                InlineAtom::Image(image_node(&image.url, image.title.clone(), &image.alt)),
                mark.clone(),
            )),
            Node::ImageReference(image) => {
                if let Some((url, title)) = self.definitions.get(&image.identifier) {
                    let mut node = image_node(url, None, &image.alt);
                    node.title = title.clone();
                    out.push(InlineText::atom(InlineAtom::Image(node), mark.clone()));
                }
            }
            Node::Break(_) => out.push(InlineText::plain("\n", mark.clone())),
            // Math no extension claimed: its source, read as prose.
            Node::InlineMath(_) => {
                let Some(text) = self.node_source(node) else {
                    return;
                };
                if let Ok(root) = parse(text, &options(false)) {
                    let sub = self.sub(text);
                    for child in root.children().into_iter().flatten() {
                        if let Some(children) = child.children() {
                            sub.inline_into(children, mark, out);
                        }
                    }
                }
            }
            Node::Html(html) if is_break_tag(&html.value) => {
                out.push(InlineText::plain("\n", mark.clone()))
            }
            // A mark's tag without its partner: nothing to draw.
            Node::Html(_)
                if inline_html_tag(node)
                    .is_some_and(|(_, name, attrs)| inline_html_mark(&name, attrs).is_some()) => {}
            Node::Html(_) | Node::FootnoteReference(_) => {
                if let Some(source) = self.node_source(node) {
                    out.push(InlineText::atom(
                        InlineAtom::Custom(
                            CustomNode::new(SOURCE_INLINE, ())
                                .text(source.to_string())
                                .markdown(source.to_string()),
                        ),
                        mark.clone(),
                    ));
                }
            }
            other => {
                if let Some(children) = other.children() {
                    self.inline_into(children, mark, out);
                }
            }
        }
    }

    // Toggles.

    /// The summary's inline Markdown, as a paragraph's, with the document's definitions. A
    /// summary that isn't one paragraph of Markdown (`1. one`, `# h`) is kept as the literal
    /// text it is.
    fn summary_inline(&self, summary: &str) -> InlineText {
        let literal = || InlineText::plain(summary, TextMark::default());
        if summary.is_empty() {
            return InlineText::default();
        }
        // A reference link resolves only where its definition is, so the document's are
        // written after the summary.
        let mut source = summary.to_string();
        if summary.contains('[') {
            let bad = |s: &str| s.contains(['[', ']', '<', '>', '\n', '\r']);
            for (label, (url, _)) in self.definitions.iter() {
                if !bad(label) && !bad(url) {
                    source.push_str(&format!("\n\n[{label}]: <{url}>"));
                }
            }
        }
        let Ok(root) = parse(&source, &options(true)) else {
            return literal();
        };
        let sub = self.sub(&source);
        let nodes: Vec<&Node> = root
            .children()
            .into_iter()
            .flatten()
            .filter(|n| !matches!(n, Node::Definition(_)))
            .collect();
        match nodes.as_slice() {
            [Node::Paragraph(p)] => sub.inline(&p.children),
            _ => literal(),
        }
    }

    /// Pair `<details>` and `</details>` blocks among siblings into toggles,
    /// the blocks between them the body. A toggle's body is written at its own
    /// level, so a nested toggle is a pair inside the pair; a stack matches them.
    /// A tag without its partner stays the raw HTML it was.
    fn group_toggles(&self, blocks: Vec<DetachedBlock>) -> Vec<DetachedBlock> {
        if !blocks
            .iter()
            .any(|b| matches!(details_tag(b), Some(DetailsTag::Open { .. })))
        {
            return blocks;
        }
        let toggle = |open: bool, summary: &str, body: Vec<DetachedBlock>| {
            // A toggle heading's summary is `<h1>`…`<h3>` around its text.
            let first = match summary_heading(summary) {
                Some((level, text)) => DetachedBlock::leaf(
                    BlockKindData::Heading { level },
                    BlockContent::Inline(self.summary_inline(text)),
                ),
                None => DetachedBlock::paragraph(self.summary_inline(summary)),
            };
            let mut children = vec![first];
            children.extend(body);
            DetachedBlock::container(BlockKindData::Toggle { open }, children)
        };
        // Each frame: the opening tag's block and what it holds so far.
        let mut stack: Vec<(DetachedBlock, bool, String, Vec<DetachedBlock>)> = Vec::new();
        let mut out = Vec::new();
        for block in blocks {
            match details_tag(&block) {
                Some(DetailsTag::Open {
                    open,
                    summary,
                    closed: true,
                }) => {
                    let t = toggle(open, &summary, Vec::new());
                    stack.last_mut().map_or(&mut out, |f| &mut f.3).push(t);
                }
                Some(DetailsTag::Open { open, summary, .. }) => {
                    stack.push((block, open, summary, Vec::new()))
                }
                Some(DetailsTag::Close) if !stack.is_empty() => {
                    let (_, open, summary, body) = stack.pop().unwrap();
                    let t = toggle(open, &summary, body);
                    stack.last_mut().map_or(&mut out, |f| &mut f.3).push(t);
                }
                _ => stack.last_mut().map_or(&mut out, |f| &mut f.3).push(block),
            }
        }
        // Unclosed: the opening tag and its blocks go back as they were.
        while let Some((tag, _, _, body)) = stack.pop() {
            let target = stack.last_mut().map_or(&mut out, |f| &mut f.3);
            target.push(tag);
            target.extend(body);
        }
        out
    }
}

fn with(mark: &TextMark, add: TextMark) -> TextMark {
    let mut out = mark.clone();
    out.merge(add);
    out
}

fn image_node(url: &str, title: Option<String>, alt: &str) -> ImageNode {
    ImageNode {
        url: url.to_string().into(),
        title: title.map(Into::into),
        alt: Some(alt.to_string().into()),
    }
}

fn collect_definitions(node: &Node, out: &mut Definitions) {
    if let Node::Definition(def) = node {
        out.entry(def.identifier.clone())
            .or_insert_with(|| (def.url.clone().into(), def.title.clone().map(Into::into)));
    }
    for child in node.children().into_iter().flatten() {
        collect_definitions(child, out);
    }
}

fn is_break_tag(html: &str) -> bool {
    let tag = html.trim().to_ascii_lowercase();
    matches!(tag.as_str(), "<br>" | "<br/>" | "<br />")
}

/// An inline HTML tag: whether it closes, its lowercase name, and its attributes.
fn inline_html_tag(node: &Node) -> Option<(bool, String, &str)> {
    let Node::Html(html) = node else {
        return None;
    };
    let inner = html.value.trim().strip_prefix('<')?.strip_suffix('>')?;
    if inner.ends_with('/') {
        return None;
    }
    let (closing, rest) = match inner.strip_prefix('/') {
        Some(rest) => (true, rest),
        None => (false, inner),
    };
    let name_len = rest.chars().take_while(char::is_ascii_alphanumeric).count();
    let name = rest[..name_len].to_ascii_lowercase();
    let attrs = rest[name_len..].trim();
    (!name.is_empty()).then_some((closing, name, attrs))
}

fn inline_html_attr<'a>(attrs: &'a str, name: &str) -> Option<&'a str> {
    let mut rest = attrs;
    while !rest.is_empty() {
        rest = rest.trim_start();
        let key_len = rest
            .find(|c: char| c.is_whitespace() || matches!(c, '=' | '/' | '>'))
            .unwrap_or(rest.len());
        if key_len == 0 {
            break;
        }
        let key = &rest[..key_len];
        rest = rest[key_len..].trim_start();
        let value = if let Some(after_eq) = rest.strip_prefix('=') {
            rest = after_eq.trim_start();
            if let Some(quote) = rest.chars().next().filter(|c| matches!(c, '"' | '\'')) {
                rest = &rest[1..];
                let end = rest.find(quote)?;
                let value = &rest[..end];
                rest = &rest[end + 1..];
                value
            } else {
                let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
                let value = &rest[..end];
                rest = &rest[end..];
                value
            }
        } else {
            ""
        };
        if key.eq_ignore_ascii_case(name) {
            return Some(value);
        }
    }
    None
}

/// The mark an inline HTML tag stands for.
fn inline_html_mark(name: &str, attrs: &str) -> Option<TextMark> {
    Some(match name {
        "strong" | "b" => TextMark::default().bold(),
        "em" | "i" => TextMark::default().italic(),
        "u" => TextMark::default().underline(),
        "s" | "del" | "strike" => TextMark::default().strikethrough(),
        "mark" => {
            let color = inline_html_attr(attrs, "color")
                .and_then(|c| mark_color(c.trim()))
                .or_else(|| {
                    let style = inline_html_attr(attrs, "style")?;
                    style.split(';').find_map(|decl| {
                        let (key, value) = decl.split_once(':')?;
                        key.trim()
                            .eq_ignore_ascii_case("background-color")
                            .then(|| mark_color(value.trim()))
                            .flatten()
                    })
                })
                .unwrap_or_else(super::marks::default_highlight);
            TextMark::default().highlight(color)
        }
        // A span is a mark only when it colours the text; any other is kept as it was written.
        "span" => {
            let style = inline_html_attr(attrs, "style")?;
            let color = style.split(';').find_map(|decl| {
                let (key, value) = decl.split_once(':')?;
                key.trim()
                    .eq_ignore_ascii_case("color")
                    .then(|| mark_color(value.trim()))
                    .flatten()
            })?;
            TextMark::default().color(color)
        }
        _ => return None,
    })
}

fn mark_color(value: &str) -> Option<Hsla> {
    if value.starts_with('#') {
        return gpui::Rgba::try_from(value).ok().map(Into::into);
    }
    match value.to_ascii_lowercase().as_str() {
        "black" => Some(gpui::rgb(0x000000).into()),
        "white" => Some(gpui::rgb(0xffffff).into()),
        "blue" => Some(gpui::rgb(0x3b82f6).into()),
        "yellow" => Some(gpui::rgb(0xfacc15).into()),
        _ => None,
    }
}

fn matching_close_tag(nodes: &[Node], open: usize, name: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (ix, child) in nodes.iter().enumerate().skip(open + 1) {
        match inline_html_tag(child) {
            Some((false, tag, _)) if tag == name => depth += 1,
            Some((true, tag, _)) if tag == name => {
                if depth == 0 {
                    return Some(ix);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// Whether `name` can be written in a callout marker and read back: an ASCII
/// letter, then ASCII letters, digits, `-` and `_`.
pub(crate) fn is_callout_name(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// What a raw HTML block says about a toggle: `<details>` (with `open` or
/// not) and its `<summary>`, or `</details>`.
enum DetailsTag {
    Open {
        open: bool,
        summary: String,
        closed: bool,
    },
    Close,
}

/// Top-level blocks with their column markers made columns (see `columns.rs`). Only the top
/// level: markers inside a quote or a list stay the HTML they are.
fn group_columns(blocks: Vec<DetachedBlock>) -> Vec<DetachedBlock> {
    crate::columns::group(
        blocks,
        |block| crate::columns::marker(source_of(block)?),
        |columns| {
            let columns = columns
                .into_iter()
                .map(|(width, mut blocks)| {
                    if blocks.is_empty() {
                        blocks.push(DetachedBlock::paragraph(InlineText::new()));
                    }
                    DetachedBlock::container(BlockKindData::Column { width }, blocks)
                })
                .collect();
            DetachedBlock::container(BlockKindData::Columns, columns)
        },
    )
}

/// The source of a block kept as the HTML (or footnote) it was written as.
fn source_of(block: &DetachedBlock) -> Option<&str> {
    let DetachedContent::Leaf(BlockContent::Atom(AtomData::Custom(node))) = &block.content else {
        return None;
    };
    (node.name() == SOURCE_BLOCK).then(|| node.as_markdown())
}

/// A toggle heading's summary, `<h2>Title</h2>`: its level (1 to 3) and its text.
pub(crate) fn summary_heading(summary: &str) -> Option<(u8, &str)> {
    let summary = summary.trim();
    let rest = summary
        .strip_prefix("<h")
        .or_else(|| summary.strip_prefix("<H"))?;
    let level = rest.chars().next()?.to_digit(10)?;
    if !(1..=3).contains(&level) {
        return None;
    }
    let inner = rest[1..].strip_prefix('>')?;
    let close = format!("</h{level}>");
    let text = inner
        .strip_suffix(close.as_str())
        .or_else(|| inner.strip_suffix(close.to_uppercase().as_str()))?;
    Some((level as u8, text.trim()))
}

fn details_tag(block: &DetachedBlock) -> Option<DetailsTag> {
    let DetachedContent::Leaf(BlockContent::Atom(AtomData::Custom(node))) = &block.content else {
        return None;
    };
    if node.name() != SOURCE_BLOCK {
        return None;
    }
    let source = node.as_markdown().trim();
    if source.eq_ignore_ascii_case("</details>") {
        return Some(DetailsTag::Close);
    }
    let lower = source.to_ascii_lowercase();
    let attrs = lower.strip_prefix("<details")?;
    if !attrs.starts_with(['>', ' ', '\t', '\n']) {
        return None;
    }
    let (open, tag_end) = scan_attributes(attrs)?;
    let after_tag = source.len() - attrs.len() + tag_end + 1;
    let rest = source[after_tag..].trim_start();
    let inner = rest
        .get(..9)
        .filter(|s| s.eq_ignore_ascii_case("<summary>"))
        .map(|_| &rest[9..])?;
    let close = inner.to_ascii_lowercase().find("</summary>")?;
    let summary = inner[..close].trim().to_string();
    let after = inner[close + 10..].trim();
    let closed = after.eq_ignore_ascii_case("</details>");
    (after.is_empty() || closed).then_some(DetailsTag::Open {
        open,
        summary,
        closed,
    })
}

/// Reads a start tag's attributes, from after its name: whether one is named
/// `open` (with any value, or none, as HTML has it), and where the `>` that
/// ends the tag is. `None` when the tag doesn't end.
fn scan_attributes(attrs: &str) -> Option<(bool, usize)> {
    let bytes = attrs.as_bytes();
    let mut open = false;
    let mut i = 0;
    loop {
        while bytes.get(i)?.is_ascii_whitespace() || bytes[i] == b'/' {
            i += 1;
        }
        if bytes[i] == b'>' {
            return Some((open, i));
        }
        let start = i;
        while !matches!(bytes.get(i)?, b'=' | b'>' | b'/') && !bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        open |= &attrs[start..i] == "open";
        while bytes.get(i)?.is_ascii_whitespace() {
            i += 1;
        }
        if bytes[i] != b'=' {
            continue;
        }
        i += 1;
        while bytes.get(i)?.is_ascii_whitespace() {
            i += 1;
        }
        match bytes[i] {
            quote @ (b'"' | b'\'') => {
                i += 1;
                while *bytes.get(i)? != quote {
                    i += 1;
                }
                i += 1;
            }
            _ => {
                while !matches!(bytes.get(i)?, b'>') && !bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
            }
        }
    }
}
