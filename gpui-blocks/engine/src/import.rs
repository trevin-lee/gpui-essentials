//! `markdown::parse` output (`BlockNode`) into the editor model.

use std::collections::HashMap;
use std::sync::Arc;

use gpui::SharedString;

use super::{
    document::{AtomData, BlockContent, EditorDocument},
    inline::{InlineAtom, InlineText},
    kind::BlockKindData,
    step::{DetachedBlock, DetachedContent},
    transaction::TransactionBuilder,
};
use gpui_kit::base::text::{
    MarkdownExtensions,
    editor_host::{BlockNode, InlineNode, LinkMark, Paragraph, Span, TextMark, parse_markdown},
};

pub(crate) fn parse_document(
    source: &str,
    extensions: Arc<MarkdownExtensions>,
) -> Result<EditorDocument, SharedString> {
    let parsed = parse_markdown(source, extensions.clone())?;
    let mut doc = EditorDocument::empty(extensions.clone());
    let root = doc.root;
    // Drop the placeholder paragraph of the empty document.
    doc.detach(root, 0);
    let mut defs = Defs::with_source(source).with_extensions(extensions.clone());
    collect_definitions(&parsed, &mut defs);
    let mut blocks = Vec::new();
    for node in parsed.iter() {
        import_block(node, &defs, &mut blocks);
    }
    let blocks = group_toggles(blocks, &defs);
    for (ix, block) in blocks.into_iter().enumerate() {
        doc.attach(root, ix, block);
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
    // Primary AST spans distinguish exterior footer trivia from line endings
    // owned by an unterminated code fence/body.
    let exterior_start = parsed
        .last()
        .and_then(BlockNode::span)
        .map_or(0, |span| span.end);
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
        let index = doc.children(root).len();
        doc.attach(
            root,
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

fn leaf(kind: BlockKindData, content: BlockContent) -> DetachedBlock {
    DetachedBlock::leaf(kind, content)
}

fn import_children(nodes: &[BlockNode], defs: &Defs) -> Vec<DetachedBlock> {
    let mut out = Vec::new();
    for node in nodes {
        import_block(node, defs, &mut out);
    }
    group_toggles(out, defs)
}

/// A callout's children: never empty, since an empty container is pruned.
fn callout(name: SharedString, mut children: Vec<DetachedBlock>) -> DetachedBlock {
    if children.is_empty() {
        children.push(DetachedBlock::paragraph(InlineText::new()));
    }
    DetachedBlock::container(BlockKindData::Callout { name }, children)
}

/// A callout marker's name (`[!NOTE]` gives `note`), or `None` when `text`
/// isn't one.
fn callout_name(text: &str) -> Option<SharedString> {
    let name = text.strip_prefix("[!")?.strip_suffix(']')?;
    let valid = name.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    valid.then(|| name.to_ascii_lowercase().into())
}

/// The callout name on a blockquote's first line, when the marker is alone
/// there as GitHub and Obsidian write it. Read from the source, since the
/// parsed text has lost the line ending (and an escaped `\[` reads the same).
fn marker_on_first_line(source: Option<&str>, span: Option<Span>) -> Option<SharedString> {
    let source = source?.get(span?.start..)?;
    let line = source.split('\n').next()?.trim_end_matches('\r');
    let line = line.trim_start().strip_prefix('>')?;
    let line = line.strip_prefix(' ').unwrap_or(line);
    callout_name(line.trim_end())
}

/// A blockquote whose first line is a callout marker becomes a callout, the
/// marker dropped from its first paragraph. Callouts at the top level arrive
/// as surfaces when the host registered their name; this catches the rest
/// (nested ones, and names nothing registered).
fn quote_or_callout(mut children: Vec<DetachedBlock>, marker: Option<SharedString>) -> DetachedBlock {
    // The marker and the soft line break after it, which arrives as a space.
    let marker = marker.and_then(|name| match children.first() {
        Some(DetachedBlock {
            kind: BlockKindData::Paragraph,
            content: DetachedContent::Leaf(BlockContent::Inline(inline)),
            ..
        }) => {
            // The text must start with the marker as written. When it doesn't
            // (a `[!note]: url` definition made the marker a link), the
            // quote stays a quote rather than losing text.
            let text = inline.text();
            let len = "[!]".len() + name.len();
            let written = text.get(..len)?;
            if !written.starts_with("[!")
                || !written.ends_with(']')
                || !written[2..len - 1].eq_ignore_ascii_case(&name)
            {
                return None;
            }
            let len = if text[len..].starts_with(' ') { len + 1 } else { len };
            Some((name, len, inline.len()))
        }
        _ => None,
    });
    let Some((name, len, total)) = marker else {
        return DetachedBlock::container(BlockKindData::Blockquote, children);
    };
    if len == total {
        children.remove(0);
    } else if let DetachedContent::Leaf(BlockContent::Inline(inline)) = &mut children[0].content {
        *inline = inline.slice(len..total);
    }
    callout(name, children)
}

/// What a raw HTML block says about a toggle: `<details>` (with `open` or
/// not) and its `<summary>`, or `</details>`.
enum DetailsTag {
    Open { open: bool, summary: String, closed: bool },
    Close,
}

fn details_tag(block: &DetachedBlock) -> Option<DetailsTag> {
    let DetachedContent::Leaf(BlockContent::Atom(AtomData::Custom(node))) = &block.content else {
        return None;
    };
    if node.name() != "editor-source-block" {
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
    let tag_end = attrs.find('>')?;
    let open = attrs[..tag_end].split_ascii_whitespace().any(|a| a == "open");
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

/// The summary's inline Markdown, as a paragraph's, parsed with the
/// document's extensions. The document's link reference definitions are
/// written after it, so a reference link in it resolves as it would in a
/// paragraph. A summary that isn't one paragraph of Markdown (`1. one`,
/// `# h`) is kept as the literal text it is.
fn summary_inline(summary: &str, defs: &Defs) -> InlineText {
    let literal = || InlineText::plain(summary, TextMark::default());
    if summary.is_empty() {
        return InlineText::default();
    }
    let mut source = summary.to_string();
    if summary.contains('[') {
        let bad = |s: &str| s.contains(['[', ']', '<', '>', '\n', '\r']);
        for (label, (url, _)) in defs.iter() {
            if !bad(label) && !bad(url) {
                source.push_str(&format!("\n\n[{label}]: <{url}>"));
            }
        }
    }
    let Ok(nodes) = parse_markdown(&source, defs.extensions.clone()) else {
        return literal();
    };
    let mut paragraphs = nodes
        .iter()
        .filter(|node| !matches!(node, BlockNode::Definition { .. }));
    match (paragraphs.next(), paragraphs.next()) {
        (Some(BlockNode::Paragraph(paragraph)), None) => import_inline(paragraph, defs),
        _ => literal(),
    }
}

/// Pair `<details>` and `</details>` blocks among siblings into toggles,
/// the blocks between them the body. A toggle's body is written at its own
/// level, so a nested toggle is a pair inside the pair; a stack matches them.
/// A tag without its partner stays the raw HTML it was.
pub(crate) fn group_toggles(blocks: Vec<DetachedBlock>, defs: &Defs) -> Vec<DetachedBlock> {
    if !blocks.iter().any(|b| matches!(details_tag(b), Some(DetailsTag::Open { .. }))) {
        return blocks;
    }
    let toggle = |open: bool, summary: &str, body: Vec<DetachedBlock>| {
        let mut children = vec![DetachedBlock::paragraph(summary_inline(summary, defs))];
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

pub(crate) fn import_block(node: &BlockNode, defs: &Defs, out: &mut Vec<DetachedBlock>) {
    let block = match node {
        // An HTML container: splice its children in place.
        BlockNode::Root { children, .. } => {
            for child in children {
                import_block(child, defs, out);
            }
            return;
        }
        BlockNode::Paragraph(paragraph) => leaf(
            BlockKindData::Paragraph,
            BlockContent::Inline(import_inline(paragraph, defs)),
        ),
        BlockNode::Heading {
            level, children, ..
        } => leaf(
            BlockKindData::Heading { level: *level },
            BlockContent::Inline(import_inline(children, defs)),
        ),
        BlockNode::Blockquote { children, span } => quote_or_callout(
            import_children(children, defs),
            marker_on_first_line(defs.source, *span),
        ),
        BlockNode::Surface { name, children, .. } => {
            callout(name.clone(), import_children(children, defs))
        }
        BlockNode::List {
            children,
            ordered,
            start,
            ..
        } => DetachedBlock::container(
            BlockKindData::List {
                ordered: *ordered,
                start: *start,
            },
            import_children(children, defs),
        ),
        BlockNode::ListItem {
            children,
            spread,
            checked,
            ..
        } => {
            let mut children = import_children(children, defs);
            // A parsed bare marker is an editable empty item, not an empty
            // container to prune during normalization.
            if children.is_empty() {
                children.push(DetachedBlock::paragraph(InlineText::new()));
            }
            DetachedBlock::container(
                BlockKindData::ListItem {
                    checked: *checked,
                    spread: *spread,
                },
                children,
            )
        }
        BlockNode::CodeBlock(code) => {
            let text = code.code().to_string();
            leaf(
                BlockKindData::CodeBlock {
                    lang: code.lang(),
                    meta: code.meta(),
                },
                BlockContent::Plain(text),
            )
        }
        BlockNode::Custom { node, .. } => leaf(
            BlockKindData::Custom,
            BlockContent::Atom(AtomData::Custom(node.clone())),
        ),
        BlockNode::Table(table) => {
            let rows = table
                .rows()
                .map(|row| {
                    DetachedBlock::container(
                        BlockKindData::TableRow,
                        row.map(|cell| {
                            leaf(
                                BlockKindData::TableCell,
                                BlockContent::Inline(import_inline(cell, defs)),
                            )
                        })
                        .collect(),
                    )
                })
                .collect();
            DetachedBlock::container(
                BlockKindData::Table {
                    aligns: table.column_aligns().to_vec(),
                },
                rows,
            )
        }
        BlockNode::Break { html, .. } => leaf(
            BlockKindData::Break { html: *html },
            BlockContent::Atom(AtomData::None),
        ),
        BlockNode::HorizontalRule { .. } => leaf(
            BlockKindData::HorizontalRule,
            BlockContent::Atom(AtomData::None),
        ),
        BlockNode::Definition {
            identifier,
            url,
            title,
            ..
        } => leaf(
            BlockKindData::Definition {
                identifier: identifier.clone(),
                url: url.clone(),
                title: title.clone(),
            },
            BlockContent::Atom(AtomData::None),
        ),
        BlockNode::Unknown | _ => leaf(BlockKindData::Unknown, BlockContent::Atom(AtomData::None)),
    };
    out.push(block);
}

/// Convert a paragraph's inline nodes into normalized runs. `InlineNode.marks`
/// may overlap (`**b*c*d**` gives bold 0..3 and italic 1..2), so split at
/// every boundary and merge the marks covering each piece.
///
/// Reference links (`[text][id]`) arrive with an empty url and an
/// identifier; they are resolved against the document's definitions so the
/// canonical output is an inline link. Unresolved references (and footnote
/// links) lose their link mark: documented lossiness.
pub(crate) fn import_inline(paragraph: &Paragraph, defs: &Defs) -> InlineText {
    let mut out = InlineText::new();
    for node in paragraph.children() {
        out.push(import_inline_node(node));
    }
    let len = out.len();
    out.map_marks(0..len, |mark| match &mark.link {
        Some(link) if link.url.is_empty() => {
            let link = link
                .identifier
                .as_ref()
                .and_then(|id| defs.get(&SharedString::from(id.to_lowercase())))
                .map(|(url, title)| LinkMark {
                    url: url.clone(),
                    identifier: None,
                    title: title.clone(),
                });
            TextMark {
                link,
                ..mark.clone()
            }
        }
        _ => mark.clone(),
    });
    out
}

/// What importing needs besides the parsed tree: link reference definitions,
/// and the source when there is one, to tell what the tree no longer shows
/// (a soft line break arrives as a space).
#[derive(Default)]
pub(crate) struct Defs<'a> {
    map: HashMap<SharedString, (SharedString, Option<SharedString>)>,
    source: Option<&'a str>,
    /// The document's extensions, for what is parsed again on its own (a
    /// toggle's summary) and for the callout names it registers.
    extensions: Arc<MarkdownExtensions>,
}

impl<'a> Defs<'a> {
    pub(crate) fn with_source(source: &'a str) -> Self {
        Self {
            map: HashMap::new(),
            source: Some(source),
            extensions: Default::default(),
        }
    }

    pub(crate) fn with_extensions(mut self, extensions: Arc<MarkdownExtensions>) -> Self {
        self.extensions = extensions;
        self
    }
}

impl std::ops::Deref for Defs<'_> {
    type Target = HashMap<SharedString, (SharedString, Option<SharedString>)>;
    fn deref(&self) -> &Self::Target {
        &self.map
    }
}

impl std::ops::DerefMut for Defs<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.map
    }
}

pub(crate) fn collect_definitions(nodes: &[BlockNode], defs: &mut Defs) {
    for node in nodes {
        match node {
            BlockNode::Definition {
                identifier,
                url,
                title,
                ..
            } => {
                defs.insert(
                    identifier.to_lowercase().into(),
                    (url.clone(), title.clone()),
                );
            }
            BlockNode::Root { children, .. }
            | BlockNode::Blockquote { children, .. }
            | BlockNode::Surface { children, .. }
            | BlockNode::List { children, .. }
            | BlockNode::ListItem { children, .. } => collect_definitions(children, defs),
            _ => {}
        }
    }
}

fn import_inline_node(node: &InlineNode) -> InlineText {
    let merged_marks = || {
        let mut mark = TextMark::default();
        for (_, layer) in node.mark_runs() {
            mark.merge(layer.clone());
        }
        mark
    };
    if let Some(custom) = node.as_custom() {
        return InlineText::atom(InlineAtom::Custom(custom.clone()), merged_marks());
    }
    if let Some(image) = node.as_image() {
        return InlineText::atom(InlineAtom::Image(image.clone()), merged_marks());
    }
    let text: &str = node.text();
    let mut cuts = vec![0, text.len()];
    for (range, _) in node.mark_runs() {
        cuts.push(range.start.min(text.len()));
        cuts.push(range.end.min(text.len()));
    }
    cuts.sort_unstable();
    cuts.dedup();
    let mut out = InlineText::new();
    for pair in cuts.windows(2) {
        let (lo, hi) = (pair[0], pair[1]);
        if lo >= hi || !text.is_char_boundary(lo) || !text.is_char_boundary(hi) {
            continue;
        }
        let mut mark = TextMark::default();
        for (range, layer) in node.mark_runs() {
            if range.start <= lo && hi <= range.end {
                mark.merge(layer.clone());
            }
        }
        out.push(InlineText::plain(&text[lo..hi], mark));
    }
    out
}

/// Footnotes and unsupported HTML are outside the editable block vocabulary. Keep their original
/// source in opaque, selectable atoms rather than accepting the display
/// parser's decorated-prose projection. Uses the same AST/parser pipeline as
/// every other editor node; no second parser or mutable source mirror.
pub(crate) fn preserve_source_extensions(extensions: MarkdownExtensions) -> MarkdownExtensions {
    use ::markdown::mdast::Node;
    use gpui_kit::base::text::{MarkdownNode, MarkdownParseContext, MarkdownPlugin};

    struct SourceAtom {
        block: bool,
    }
    impl MarkdownPlugin for SourceAtom {
        fn name(&self) -> &str {
            if self.block {
                "editor-source-block"
            } else {
                "editor-source-inline"
            }
        }
        fn is_block(&self) -> bool {
            self.block
        }
        fn parse(&self, node: &Node, cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
            let known_html = if let Node::Html(html) = node {
                let lower = html.value.to_ascii_lowercase();
                let tag = lower
                    .trim_start_matches('<')
                    .trim_start_matches('/')
                    .split(|ch: char| !ch.is_ascii_alphabetic())
                    .next()
                    .unwrap_or_default();
                matches!(tag, "u" | "mark" | "em" | "strong" | "s" | "del" | "br")
            } else {
                false
            };
            let accepts = if self.block {
                matches!(node, Node::FootnoteDefinition(_))
                    || (matches!(node, Node::Html(_)) && !known_html)
            } else {
                matches!(node, Node::FootnoteReference(_))
                    || (matches!(node, Node::Html(_)) && !known_html)
            };
            if !accepts {
                return None;
            }
            let source = cx.node_source(node)?;
            // Block spans may include separator line endings. The root owns
            // those boundaries; retaining them here adds a blank line on every
            // export/import cycle. Internal content remains verbatim.
            let source = if self.block {
                source.trim_end_matches(['\r', '\n'])
            } else {
                source
            };
            let source = if self.block {
                // The AST span starts after the first container marker, but
                // later source lines still include it. Strip only matching
                // continuation prefixes, preserving body indentation and lazy
                // lines. The root serializer will add the containers once.
                let start = node.position()?.start.offset;
                let line_start = cx.source()[..start].rfind('\n').map_or(0, |ix| ix + 1);
                let prefix = &cx.source()[line_start..start];
                let continuation = prefix
                    .chars()
                    .map(|ch| if ch == '>' { '>' } else { ' ' })
                    .collect::<String>();
                source
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
                    .join("\n")
            } else {
                source.to_owned()
            };
            Some(
                MarkdownNode::new(self.name(), ())
                    .text(source.clone())
                    .markdown(source),
            )
        }
    }
    extensions
        .plugin(SourceAtom { block: true })
        .plugin(SourceAtom { block: false })
}
