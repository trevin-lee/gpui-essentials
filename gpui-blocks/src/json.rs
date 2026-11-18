//! A document as JSON: a tree of blocks, each with an id that stays the same while the document
//! is edited, for an app or a server to store and index.
//!
//! ```json
//! {
//!   "format": "gpui-blocks",
//!   "version": 1,
//!   "blocks": [
//!     { "id": "pk3W9cQe2LfA", "type": "heading", "level": 1, "text": "Weekly review" },
//!     { "id": "Zr81mXbT0qvN", "type": "paragraph", "text": "The plan is in [Roadmap](doc:4f2a)." },
//!     { "id": "c7HhQ2wLx5Ke", "type": "page", "doc": "9c1e", "title": "Last week" },
//!     { "id": "u3VbN6sYt1Ra", "type": "list", "ordered": false, "children": [
//!       { "id": "Qe5TnW2zLp8M", "type": "item", "checked": false, "children": [
//!         { "id": "m2XcV7bN4kJh", "type": "paragraph", "text": "Book the **dentist**" }
//!       ] }
//!     ] }
//!   ]
//! }
//! ```
//!
//! Text is inline Markdown, so marks, links, pictures and mentions are kept as the editor writes
//! them. A mention is `[title](doc:id)` in a block's text, and a sub-page is a `page` block; see
//! `docs/json.md` in the repository for every type and its fields.
//!
//! [`Document::from_markdown`] and [`Document::to_markdown`] convert without an editor, for
//! import and export. In an editor, [`Editor::document`](crate::Editor::document) and
//! [`Editor::set_document`](crate::Editor::set_document) keep the ids: a block keeps its id
//! while it's edited and moved, and a new block gets a new one.

use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasher, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use markdown_editor::{BlockEditorModel, BlockId, BlockKind, BlockRef, ColumnAlign};
use serde::{Deserialize, Serialize};

use crate::docs;

mod merge;
use crate::widget::{Widget, fenced_with_info};
pub use merge::{ApplyError, Conflict, Op, Side, apply, diff, merge};

/// The value of `format`.
pub const FORMAT: &str = "gpui-blocks";
/// The version this crate writes and reads.
pub const VERSION: u32 = 1;
/// The longest id read from JSON; a longer one is replaced.
const MAX_ID: usize = 64;

/// A whole document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Document {
    /// Always [`FORMAT`].
    pub format: String,
    /// Always [`VERSION`] when written by this crate.
    pub version: u32,
    /// YAML front matter, written back as it was, `---` lines included.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub front_matter: String,
    #[serde(deserialize_with = "blocks_of")]
    pub blocks: Vec<Block>,
}

/// One block and the blocks inside it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Block {
    /// Unique in the document, 1 to 64 of `A-Z a-z 0-9 - _`.
    pub id: String,
    #[serde(flatten)]
    pub kind: Kind,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "blocks_of"
    )]
    pub children: Vec<Block>,
}

/// The `type`s this version reads: [`Kind`]'s.
const TYPES: [&str; 20] = [
    "paragraph",
    "heading",
    "quote",
    "callout",
    "toggle",
    "list",
    "item",
    "code",
    "divider",
    "table",
    "row",
    "cell",
    "page",
    "toc",
    "embed",
    "bookmark",
    "widget",
    "columns",
    "column",
    "markdown",
];

/// A list of blocks, where a block of a type this version doesn't know (one a later version
/// writes) never fails the document: one with blocks inside is read as those blocks, in its
/// place and in order, and one without is kept as a line of Markdown that says what it was.
/// Nothing in it is lost from view, though saving writes it back in this reduced form.
fn blocks_of<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Vec<Block>, D::Error> {
    let values = Vec::<serde_json::Value>::deserialize(deserializer)?;
    let mut out = Vec::with_capacity(values.len());
    for value in values {
        read_block(value, &mut out).map_err(serde::de::Error::custom)?;
    }
    Ok(out)
}

/// Reads `value` into `out`: a block, or, for a type this version doesn't know, its children or
/// a placeholder.
fn read_block(value: serde_json::Value, out: &mut Vec<Block>) -> Result<(), serde_json::Error> {
    let kind = value
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or_default();
    if TYPES.contains(&kind) || !value.is_object() {
        out.push(serde_json::from_value(value)?);
        return Ok(());
    }
    let kind = kind.to_string();
    let mut value = value;
    let children = match value.get_mut("children").map(serde_json::Value::take) {
        Some(serde_json::Value::Array(children)) => children,
        _ => Vec::new(),
    };
    if !children.is_empty() {
        for child in children {
            read_block(child, out)?;
        }
        return Ok(());
    }
    let id = value
        .get("id")
        .and_then(|id| id.as_str())
        .unwrap_or_default()
        .to_string();
    // What it says, when it has words of its own; else what it was. HTML comment syntax inside
    // the name is taken out, so the line stays one comment.
    let text = ["markdown", "text"]
        .iter()
        .find_map(|field| value.get(*field).and_then(|v| v.as_str()))
        .map(str::to_string);
    let markdown = text.unwrap_or_else(|| {
        let name: String = kind
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | ' '))
            .take(64)
            .collect();
        format!("<!-- a {name} block, from a newer version -->")
    });
    out.push(Block {
        id,
        kind: Kind::Markdown { markdown },
        children: Vec::new(),
    });
    Ok(())
}

/// What a block is, with what it holds besides its children.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Kind {
    Paragraph {
        text: String,
    },
    Heading {
        level: u8,
        text: String,
    },
    /// Children: any blocks.
    Quote,
    /// Children: any blocks. `name` is GitHub's alert name in lowercase, such as `note`;
    /// `icon` (one emoji) and `color` (one of [`markdown_editor::CALLOUT_COLORS`]) are its own
    /// look over that kind's. An invalid icon or colour is read as none.
    Callout {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        icon: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        color: Option<String>,
    },
    /// Children: the summary (a paragraph), then the body.
    Toggle {
        open: bool,
    },
    /// Children: items.
    List {
        ordered: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start: Option<u32>,
    },
    /// Children: any blocks. `checked` is set for a to-do item; `spread` for an item written
    /// with blank lines between its blocks.
    Item {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        checked: Option<bool>,
        #[serde(default, skip_serializing_if = "is_false")]
        spread: bool,
    },
    Code {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        language: Option<String>,
        /// The rest of the info string after the language.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        meta: Option<String>,
        code: String,
    },
    Divider,
    /// Children: rows, the first of them the header. `align` is `left`, `center`, `right` or
    /// null for each column.
    Table {
        align: Vec<Option<Align>>,
    },
    /// Children: cells.
    Row,
    Cell {
        text: String,
    },
    /// A sub-page: the document `doc`, with the title it was written with.
    Page {
        doc: String,
        title: String,
    },
    /// A table of contents: the document's headings, listed where it is.
    Toc,
    /// Another document drawn here: `view` is how the app shows it (`table`, `board`,
    /// `mindmap`, `calendar`, `page`), `height` is in points.
    Embed {
        doc: String,
        title: String,
        view: String,
        height: u32,
    },
    /// A web page as a card (see [`crate::bookmark`]): its address, and its title,
    /// description (at most [`crate::bookmark::MAX_DESCRIPTION`] characters written) and
    /// picture (an `asset:<sha256>`; anything else is dropped) when known.
    Bookmark {
        url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        image: Option<String>,
    },
    /// A widget (see [`crate::widget`]): its name, the rest of its info string, and its body.
    Widget {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        meta: Option<String>,
        body: String,
    },
    /// Blocks side by side. Children: 2 to [`markdown_editor::MAX_COLUMNS`] columns. Only at
    /// the top level.
    Columns,
    /// One of a columns block's columns: `width` is its share of the row, the shares adding up
    /// to 1. Children: any blocks but columns.
    Column {
        width: f64,
    },
    /// Anything else, kept as the Markdown it was written as: a link definition, HTML, a block
    /// a later version knows.
    Markdown {
        markdown: String,
    },
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// A table column's alignment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    Left,
    Center,
    Right,
}

/// Why JSON couldn't be read.
#[derive(Debug)]
pub enum Error {
    Json(serde_json::Error),
    /// Not this format, or a newer version.
    Format {
        format: String,
        version: u32,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(e) => write!(f, "not a document: {e}"),
            Self::Format { format, version } => {
                write!(f, "can't read {format:?} version {version}")
            }
        }
    }
}

impl std::error::Error for Error {}

impl Document {
    /// Reads a document from JSON.
    pub fn from_json(json: &str) -> Result<Self, Error> {
        let doc: Self = serde_json::from_str(json).map_err(Error::Json)?;
        if doc.format != FORMAT || doc.version > VERSION {
            return Err(Error::Format {
                format: doc.format,
                version: doc.version,
            });
        }
        Ok(doc)
    }

    /// The document as JSON.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// The document as JSON, indented for people.
    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// A document from Markdown, with new ids. `widgets` are the app's (see [`crate::widget`]):
    /// fenced blocks named after one become `widget` blocks.
    pub fn from_markdown(markdown: &str, widgets: &[Arc<dyn Widget>]) -> Self {
        let (front_matter, body) = crate::editor::split_front_matter(markdown);
        let model = BlockEditorModel::from_markdown(body, crate::extensions(widgets))
            .or_else(|_| BlockEditorModel::from_markdown("", crate::extensions(widgets)))
            .expect("an empty document parses");
        let mut ids = Ids::default();
        Self::from_model(&model, front_matter, &mut ids)
    }

    pub(crate) fn from_model(
        model: &BlockEditorModel,
        front_matter: String,
        ids: &mut Ids,
    ) -> Self {
        let doc = model.document();
        // Ids of blocks no longer in the document are kept: undoing a delete brings a block
        // back as it was, and it should come back with its id.
        let blocks = doc.root().children().map(|b| block(b, ids)).collect();
        Self {
            format: FORMAT.into(),
            version: VERSION,
            front_matter,
            blocks,
        }
    }

    /// The document as Markdown. It reads back as the same blocks.
    pub fn to_markdown(&self) -> String {
        let body = join(&self.blocks);
        if self.front_matter.is_empty() {
            body
        } else if body.is_empty() {
            self.front_matter.clone()
        } else {
            format!("{}\n{body}", front_matter(&self.front_matter))
        }
    }

    /// The documents this one links to, each once, in order: mentions and sub-pages.
    pub fn links(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        visit(&self.blocks, &mut |b| {
            let found = match &b.kind {
                Kind::Page { doc, .. } | Kind::Embed { doc, .. } => vec![doc.clone()],
                Kind::Paragraph { text } | Kind::Heading { text, .. } | Kind::Cell { text } => {
                    mentions(text)
                }
                _ => Vec::new(),
            };
            for id in found {
                if seen.insert(id.clone()) {
                    out.push(id);
                }
            }
        });
        out
    }

    /// The sub-pages in this document, in order.
    pub fn pages(&self) -> Vec<String> {
        let mut out = Vec::new();
        visit(&self.blocks, &mut |b| {
            if let Kind::Page { doc, .. } = &b.kind {
                out.push(doc.clone());
            }
        });
        out
    }
}

fn visit(blocks: &[Block], f: &mut impl FnMut(&Block)) {
    for b in blocks {
        f(b);
        visit(&b.children, f);
    }
}

/// The document ids of the `doc:` links in inline Markdown.
fn mentions(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("](doc:") {
        let after = &rest[at + 2..];
        let end = after
            .find(|c: char| c == ')' || c.is_whitespace())
            .unwrap_or(after.len());
        if let Some(id) = docs::doc_id(&after[..end]) {
            out.push(id.to_string());
        }
        rest = &after[end..];
    }
    out
}

/// Front matter ending in a newline.
fn front_matter(front: &str) -> String {
    if front.ends_with('\n') {
        front.to_string()
    } else {
        format!("{front}\n")
    }
}

// Reading the editor's model.

/// The persistent ids of an editor's blocks, by the model's ids.
#[derive(Default)]
pub(crate) struct Ids(HashMap<BlockId, String>);

impl Ids {
    /// `block`'s lasting id, given now if it has none yet, as [`Document::from_model`] would.
    pub(crate) fn id_of(&mut self, block: BlockId) -> String {
        self.get_or_new(block)
    }

    /// The block with the lasting id `id`, if `model` still has it.
    pub(crate) fn block_of(&self, model: &BlockEditorModel, id: &str) -> Option<BlockId> {
        self.0
            .iter()
            .find(|(block, given)| {
                given.as_str() == id && model.document().block(**block).is_some()
            })
            .map(|(block, _)| *block)
    }

    /// Gives `to` the lasting id `from` has (a block replaced by another in its place keeps its
    /// id). `from` keeps it too, for undo, which brings it back.
    pub(crate) fn carry(&mut self, from: BlockId, to: BlockId) {
        if let Some(id) = self.0.get(&from).cloned() {
            self.0.insert(to, id);
        }
    }

    fn get_or_new(&mut self, block: BlockId) -> String {
        self.0.entry(block).or_insert_with(new_id).clone()
    }

    pub(crate) fn clear(&mut self) {
        self.0.clear();
    }

    /// Gives the model's blocks the ids in `blocks`: the model was read from `blocks`' Markdown,
    /// which can lose blocks (an empty paragraph) and join them (lists of one type side by side),
    /// so the trees are matched by kind and content, not by position. A block that matches
    /// nothing, an id used twice or an invalid id leaves that block to get a new one.
    pub(crate) fn adopt(&mut self, model: &BlockEditorModel, blocks: &[Block]) {
        self.0.clear();
        let mut used = HashSet::new();
        let root = model.document().root();
        let blocks: Vec<&Block> = blocks.iter().collect();
        self.adopt_children(root, &blocks, &mut used);
    }

    fn adopt_children(
        &mut self,
        parent: BlockRef<'_>,
        blocks: &[&Block],
        used: &mut HashSet<String>,
    ) {
        let mut next = 0;
        for b in parent.children() {
            // The first block of its kind and content, past only blocks the Markdown lost;
            // else the next block the Markdown kept, when its kind matches (its text read back
            // differently).
            let mut found = None;
            for (ix, json) in blocks.iter().enumerate().skip(next) {
                if same_kind(b, &json.kind) && same_content(b, &json.kind) {
                    found = Some(ix);
                    break;
                }
                if !lost_in_markdown(json) {
                    break;
                }
            }
            let found = found.or_else(|| {
                let ix = (next..blocks.len()).find(|ix| !lost_in_markdown(blocks[*ix]))?;
                same_kind(b, &blocks[ix].kind).then_some(ix)
            });
            let Some(ix) = found else {
                continue;
            };
            let json = blocks[ix];
            next = ix + 1;
            if valid_id(&json.id) && used.insert(json.id.clone()) {
                self.0.insert(b.id(), json.id.clone());
            }
            // Lists of its type that followed it were joined to it: their items are its own.
            let mut children: Vec<&Block> = json.children.iter().collect();
            if matches!(json.kind, Kind::List { .. }) {
                let mut k = next;
                while let Some(more) = blocks.get(k) {
                    if lost_in_markdown(more) {
                        k += 1;
                        continue;
                    }
                    if !same_list(&json.kind, &more.kind) {
                        break;
                    }
                    children.extend(more.children.iter());
                    k += 1;
                    next = k;
                }
            }
            self.adopt_children(b, &children, used);
        }
    }
}

/// Whether the Markdown drops `block`: a paragraph with no text.
fn lost_in_markdown(block: &Block) -> bool {
    matches!(&block.kind, Kind::Paragraph { text } if text.trim().is_empty())
}

/// Whether two lists are of one type, so Markdown joins them when they're side by side.
fn same_list(a: &Kind, b: &Kind) -> bool {
    matches!(
        (a, b),
        (Kind::List { ordered: x, .. }, Kind::List { ordered: y, .. }) if x == y
    )
}

/// Whether a model block holds what a JSON block of its kind holds: the same text or code.
/// Containers match on their kind alone.
fn same_content(b: BlockRef<'_>, kind: &Kind) -> bool {
    match kind {
        Kind::Paragraph { text } | Kind::Heading { text, .. } | Kind::Cell { text } => {
            b.inline_markdown().unwrap_or_default() == *text
        }
        Kind::Code { code, .. } => b.text().unwrap_or_default() == code,
        _ => true,
    }
}

fn valid_id(id: &str) -> bool {
    (1..=MAX_ID).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

/// Whether a model block is what a JSON block says it is, for matching ids.
fn same_kind(b: BlockRef<'_>, kind: &Kind) -> bool {
    matches!(
        (b.kind(), kind),
        (BlockKind::Paragraph, Kind::Paragraph { .. })
            | (BlockKind::Heading { .. }, Kind::Heading { .. })
            | (BlockKind::Blockquote, Kind::Quote)
            | (BlockKind::Callout, Kind::Callout { .. })
            | (BlockKind::Toggle { .. }, Kind::Toggle { .. })
            | (BlockKind::List { .. }, Kind::List { .. })
            | (BlockKind::ListItem { .. }, Kind::Item { .. })
            | (BlockKind::CodeBlock, Kind::Code { .. })
            | (BlockKind::HorizontalRule, Kind::Divider)
            | (BlockKind::Table, Kind::Table { .. })
            | (BlockKind::TableRow, Kind::Row)
            | (BlockKind::TableCell, Kind::Cell { .. })
            | (BlockKind::Columns, Kind::Columns)
            | (BlockKind::Column, Kind::Column { .. })
            | (
                BlockKind::Custom,
                Kind::Page { .. }
                    | Kind::Toc
                    | Kind::Embed { .. }
                    | Kind::Bookmark { .. }
                    | Kind::Widget { .. }
                    | Kind::Markdown { .. }
            )
            | (_, Kind::Markdown { .. })
    )
}

/// A new id: 12 characters from 62, about 71 bits.
fn new_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    const ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    let mut hasher = std::hash::RandomState::new().build_hasher();
    hasher.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    hasher.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default(),
    );
    let mut a = hasher.finish();
    let mut b = std::hash::RandomState::new().build_hasher().finish() ^ a.rotate_left(17);
    (0..12)
        .map(|i| {
            let n = if i < 6 { &mut a } else { &mut b };
            let c = ALPHABET[(*n % 62) as usize] as char;
            *n /= 62;
            c
        })
        .collect()
}

fn block(b: BlockRef<'_>, ids: &mut Ids) -> Block {
    let id = ids.get_or_new(b.id());
    let text = || b.inline_markdown().unwrap_or_default();
    let mut children = || b.children().map(|c| block(c, ids)).collect();
    let (kind, children) = match b.kind() {
        BlockKind::Paragraph => (Kind::Paragraph { text: text() }, Vec::new()),
        BlockKind::Heading { level } => (
            Kind::Heading {
                level,
                text: text(),
            },
            Vec::new(),
        ),
        BlockKind::Blockquote => (Kind::Quote, children()),
        BlockKind::Callout => (
            Kind::Callout {
                name: b.callout_name().unwrap_or("note").to_string(),
                icon: b.callout_icon().map(str::to_string),
                color: b.callout_color().map(str::to_string),
            },
            children(),
        ),
        BlockKind::Toggle { open } => (Kind::Toggle { open }, children()),
        BlockKind::List { ordered } => (
            Kind::List {
                ordered,
                // Only when it isn't 1, the number a list starts at unless told.
                start: b.list_start().filter(|n| *n != 1),
            },
            children(),
        ),
        BlockKind::ListItem { checked } => (
            Kind::Item {
                checked,
                spread: b.is_spread(),
            },
            children(),
        ),
        BlockKind::CodeBlock => (
            Kind::Code {
                language: b.code_language().map(str::to_string),
                meta: b.code_meta().map(str::to_string),
                code: b.text().unwrap_or_default().to_string(),
            },
            Vec::new(),
        ),
        BlockKind::HorizontalRule => (Kind::Divider, Vec::new()),
        BlockKind::Table => (
            Kind::Table {
                align: b
                    .column_aligns()
                    .unwrap_or_default()
                    .iter()
                    .map(|a| match a {
                        ColumnAlign::Left => Some(Align::Left),
                        ColumnAlign::Center => Some(Align::Center),
                        ColumnAlign::Right => Some(Align::Right),
                        _ => None,
                    })
                    .collect(),
            },
            children(),
        ),
        BlockKind::Columns => {
            // The widths as shares of 1, as the Markdown writes them.
            let mut columns: Vec<Block> = children();
            let widths: Vec<f32> = b
                .children()
                .map(|c| c.column_width().unwrap_or(1.))
                .collect();
            for (column, width) in columns
                .iter_mut()
                .zip(markdown_editor::written_widths(&widths))
            {
                if let Kind::Column { width: w } = &mut column.kind {
                    *w = f64::from(width);
                }
            }
            (Kind::Columns, columns)
        }
        BlockKind::Column => (
            Kind::Column {
                width: f64::from(b.column_width().unwrap_or(1.)),
            },
            children(),
        ),
        BlockKind::TableRow => (Kind::Row, children()),
        BlockKind::TableCell => (Kind::Cell { text: text() }, Vec::new()),
        BlockKind::Custom => (custom(&b.markdown()), Vec::new()),
        _ => (
            Kind::Markdown {
                markdown: b.markdown(),
            },
            Vec::new(),
        ),
    };
    Block { id, kind, children }
}

/// What a custom block is, from its Markdown: for the editor's keys that open one.
pub(crate) fn kind_of_custom(markdown: &str) -> Kind {
    custom(markdown)
}

/// A custom block from its Markdown: a sub-page, a widget, or kept as Markdown.
fn custom(markdown: &str) -> Kind {
    if markdown == crate::toc::MARKDOWN {
        return Kind::Toc;
    }
    if let Some((doc, title, view, height)) = docs::embed_of(markdown) {
        return Kind::Embed {
            doc,
            title,
            view,
            height,
        };
    }
    if let Some(page) = page_of(markdown) {
        return page;
    }
    if let Some(b) = crate::bookmark::Bookmark::of_markdown(markdown) {
        return Kind::Bookmark {
            url: b.url,
            title: b.title,
            description: b.description,
            image: b.image,
        };
    }
    if let Some(widget) = widget_of(markdown) {
        return widget;
    }
    Kind::Markdown {
        markdown: markdown.to_string(),
    }
}

/// `[title](doc:id "page")` read back.
fn page_of(markdown: &str) -> Option<Kind> {
    let rest = markdown.strip_prefix('[')?;
    let tail = format!(" \"{}\")", docs::PAGE_TITLE);
    let rest = rest.strip_suffix(tail.as_str())?;
    let at = rest.rfind("](")?;
    let (title, url) = (&rest[..at], &rest[at + 2..]);
    let doc = docs::doc_id(url)?;
    Some(Kind::Page {
        doc: doc.to_string(),
        title: unescape(title),
    })
}

/// A fenced widget block read back: its info string's first word, the rest, and the body.
fn widget_of(markdown: &str) -> Option<Kind> {
    let fence_len = markdown.bytes().take_while(|b| *b == b'`').count();
    if fence_len < 3 {
        return None;
    }
    let fence = &markdown[..fence_len];
    let (info, rest) = markdown[fence_len..]
        .split_once('\n')
        .unwrap_or((&markdown[fence_len..], ""));
    let body = rest.strip_suffix(fence)?.strip_suffix('\n').unwrap_or("");
    let body = if rest == fence { "" } else { body };
    let (name, meta) = match info.trim().split_once(' ') {
        Some((name, meta)) => (name, Some(meta.trim().to_string())),
        None => (info.trim(), None),
    };
    (!name.is_empty()).then(|| Kind::Widget {
        name: name.to_string(),
        meta,
        body: body.to_string(),
    })
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
}

// Writing Markdown from JSON.

fn join(blocks: &[Block]) -> String {
    blocks.iter().map(markdown).collect::<Vec<_>>().join("\n\n")
}

/// Inline Markdown on one line: a newline would end the block early.
fn line(text: &str) -> String {
    text.replace(['\r', '\n'], " ")
}

fn quoted(body: &str) -> String {
    body.lines()
        .map(|l| {
            if l.is_empty() {
                ">".into()
            } else {
                format!("> {l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn markdown(b: &Block) -> String {
    match &b.kind {
        Kind::Paragraph { text } => line(text),
        Kind::Heading { level, text } => {
            format!(
                "{} {}",
                "#".repeat((*level).clamp(1, 6) as usize),
                line(text)
            )
        }
        Kind::Quote => quoted(&join(&b.children)),
        Kind::Columns => markdown_editor::columns_markdown(
            &b.children
                .iter()
                .map(|column| match column.kind {
                    Kind::Column { width } => (width as f32, join(&column.children)),
                    // Not a column (JSON from elsewhere): a column of its own.
                    _ => (0., markdown(column)),
                })
                .collect::<Vec<_>>(),
        ),
        Kind::Column { .. } => join(&b.children),
        Kind::Callout { name, icon, color } => {
            let name: String = name.chars().filter(char::is_ascii_alphanumeric).collect();
            let marker = format!(
                "> [!{}]{}",
                name.to_ascii_uppercase(),
                markdown_editor::callout_look_comment(icon.as_deref(), color.as_deref())
            );
            let body = join(&b.children);
            if body.is_empty() {
                marker
            } else {
                format!("{marker}\n{}", quoted(&body))
            }
        }
        Kind::Toggle { open } => {
            let (summary, body) = match b.children.split_first() {
                Some((first, rest)) if matches!(first.kind, Kind::Paragraph { .. }) => {
                    (markdown(first), join(rest))
                }
                // A toggle heading.
                Some((
                    Block {
                        kind: Kind::Heading { level, text },
                        ..
                    },
                    rest,
                )) if (1..=3).contains(level) => {
                    (format!("<h{level}>{}</h{level}>", line(text)), join(rest))
                }
                _ => (String::new(), join(&b.children)),
            };
            let tag = if *open { "<details open>" } else { "<details>" };
            if body.is_empty() {
                format!("{tag}\n<summary>{summary}</summary>\n\n</details>")
            } else {
                format!("{tag}\n<summary>{summary}</summary>\n\n{body}\n\n</details>")
            }
        }
        Kind::List { ordered, start } => {
            let loose = b
                .children
                .iter()
                .any(|i| matches!(i.kind, Kind::Item { spread: true, .. }));
            let first = start.unwrap_or(1) as usize;
            b.children
                .iter()
                .enumerate()
                .map(|(ix, item)| {
                    let marker = if *ordered {
                        format!("{}. ", first + ix)
                    } else {
                        "- ".to_string()
                    };
                    list_item(item, &marker)
                })
                .collect::<Vec<_>>()
                .join(if loose { "\n\n" } else { "\n" })
        }
        Kind::Item { .. } => list_item(b, "- "),
        Kind::Code {
            language,
            meta,
            code,
        } => {
            let mut info = language.clone().unwrap_or_default();
            if let Some(meta) = meta {
                info.push(' ');
                info.push_str(meta);
            }
            // A backtick in a backtick fence's info string would end the fence.
            code_fence(&line(&info).replace('`', ""), code)
        }
        Kind::Divider => "---".into(),
        Kind::Toc => crate::toc::MARKDOWN.into(),
        Kind::Embed {
            doc,
            title,
            view,
            height,
        } => match docs::doc_id(&format!("{}{doc}", docs::SCHEME)) {
            Some(id) => docs::embed(id, title, view, *height),
            None => line(title),
        },
        Kind::Table { align } => table(b, align),
        Kind::Row | Kind::Cell { .. } => String::new(),
        Kind::Page { doc, title } => match docs::doc_id(&format!("{}{doc}", docs::SCHEME)) {
            Some(id) => docs::page(id, title),
            None => line(title),
        },
        Kind::Bookmark {
            url,
            title,
            description,
            image,
        } => {
            if crate::bookmark::web_url(url) {
                crate::bookmark::Bookmark {
                    url: url.clone(),
                    title: title.clone(),
                    description: description.clone(),
                    image: image.clone(),
                }
                .markdown()
            } else {
                // Not a web page's address: kept as its text, never a link.
                line(title.as_deref().unwrap_or(url))
            }
        }
        Kind::Widget { name, meta, body } => {
            let name: String = name
                .chars()
                .filter(|c| !c.is_whitespace() && *c != '`')
                .collect();
            let info = match meta {
                Some(meta) => format!("{name} {}", line(meta)),
                None => name,
            };
            fenced_with_info(&info, body)
        }
        Kind::Markdown { markdown } => markdown.clone(),
    }
}

fn list_item(item: &Block, marker: &str) -> String {
    let task = match item.kind {
        Kind::Item {
            checked: Some(true),
            ..
        } => "[x] ",
        Kind::Item {
            checked: Some(false),
            ..
        } => "[ ] ",
        _ => "",
    };
    let spread = matches!(item.kind, Kind::Item { spread: true, .. });
    let body = item
        .children
        .iter()
        .map(markdown)
        .collect::<Vec<_>>()
        .join(if spread { "\n\n" } else { "\n" });
    let indent = " ".repeat(marker.len());
    let mut out = String::new();
    for (ix, l) in body.split('\n').enumerate() {
        if ix == 0 {
            out.push_str(marker);
            out.push_str(task);
            out.push_str(l);
        } else {
            out.push('\n');
            if !l.is_empty() {
                out.push_str(&indent);
                out.push_str(l);
            }
        }
    }
    if body.is_empty() {
        out = format!("{marker}{task}").trim_end().to_string();
    }
    out
}

fn code_fence(info: &str, code: &str) -> String {
    let longest = code.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest.max(2) + 1);
    if code.is_empty() {
        format!("{fence}{info}\n{fence}")
    } else {
        format!("{fence}{info}\n{code}\n{fence}")
    }
}

fn table(b: &Block, align: &[Option<Align>]) -> String {
    let rows: Vec<Vec<String>> = b
        .children
        .iter()
        .map(|row| {
            row.children
                .iter()
                .map(|cell| match &cell.kind {
                    Kind::Cell { text } => line(text),
                    _ => String::new(),
                })
                .collect()
        })
        .collect();
    let cols = rows
        .iter()
        .map(Vec::len)
        .max()
        .unwrap_or(0)
        .max(align.len())
        .max(1);
    let row_md = |cells: &[String]| {
        let mut out = String::from("|");
        for ix in 0..cols {
            out.push(' ');
            out.push_str(cells.get(ix).map(String::as_str).unwrap_or(""));
            out.push_str(" |");
        }
        out
    };
    let mut lines = Vec::new();
    let header = rows.first().cloned().unwrap_or_default();
    lines.push(row_md(&header));
    let mut rule = String::from("|");
    for ix in 0..cols {
        rule.push_str(match align.get(ix).copied().flatten() {
            Some(Align::Left) => " :--- |",
            Some(Align::Center) => " :---: |",
            Some(Align::Right) => " ---: |",
            None => " --- |",
        });
    }
    lines.push(rule);
    for row in rows.iter().skip(1) {
        lines.push(row_md(row));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn widgets() -> Vec<Arc<dyn Widget>> {
        struct Chart;
        impl Widget for Chart {
            fn name(&self) -> &str {
                "chart"
            }
            fn render(&self, _: &str, _: &mut gpui::Window, _: &mut gpui::App) -> gpui::AnyElement {
                use gpui::IntoElement as _;
                gpui::div().into_any_element()
            }
        }
        vec![Arc::new(Chart)]
    }

    /// A block of a type a later version writes never fails the document: with blocks inside,
    /// it's read as those blocks, in its place and in order, at any depth; without, as a line
    /// of Markdown that says what it was, or its own words.
    #[test]
    fn a_newer_type_is_read_as_what_it_holds() {
        let json = r#"{"format":"gpui-blocks","version":1,"blocks":[
            {"id":"a","type":"paragraph","text":"before"},
            {"id":"cols","type":"split","children":[
                {"id":"c1","type":"pane","width":0.5,"children":[
                    {"id":"p1","type":"paragraph","text":"left"}]},
                {"id":"c2","type":"pane","width":0.5,"children":[
                    {"id":"p2","type":"heading","level":2,"text":"right"},
                    {"id":"p3","type":"paragraph","text":"more"}]}]},
            {"id":"q","type":"quote","children":[
                {"id":"x","type":"sparkle","children":[
                    {"id":"p4","type":"paragraph","text":"inside"}]}]},
            {"id":"e","type":"sparkle","color":"red"},
            {"id":"t","type":"sparkle","text":"its words"},
            {"id":"z","type":"paragraph","text":"after"}]}"#;
        let doc = Document::from_json(json).expect("the page reads");
        let ids: Vec<&str> = doc.blocks.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(ids, ["a", "p1", "p2", "p3", "q", "e", "t", "z"]);
        assert_eq!(doc.blocks[4].children[0].id, "p4", "nested too");
        assert_eq!(
            doc.blocks[5].kind,
            Kind::Markdown {
                markdown: "<!-- a sparkle block, from a newer version -->".into()
            }
        );
        assert_eq!(
            doc.blocks[6].kind,
            Kind::Markdown {
                markdown: "its words".into()
            }
        );
        assert_eq!(
            doc.to_markdown(),
            "before\n\nleft\n\n## right\n\nmore\n\n> inside\n\n\
             <!-- a sparkle block, from a newer version -->\n\nits words\n\nafter"
        );
        // A type's name can't end the comment early.
        let odd = r#"{"format":"gpui-blocks","version":1,"blocks":[
            {"id":"o","type":"x --> <b>"}]}"#;
        let doc = Document::from_json(odd).unwrap();
        assert_eq!(
            doc.blocks[0].kind,
            Kind::Markdown {
                markdown: "<!-- a x  b block, from a newer version -->".into()
            }
        );
        // A known type with what it needs missing is still refused.
        let bad = r#"{"format":"gpui-blocks","version":1,"blocks":[{"id":"h","type":"heading"}]}"#;
        assert!(Document::from_json(bad).is_err());
    }

    /// Columns go to JSON and back with their ids and widths, and to the Markdown agents read.
    #[test]
    fn columns_keep_their_ids_and_widths() {
        let source = "<!-- columns -->\n\n<!-- column 0.25 -->\n\nleft\n\n<!-- column 0.75 -->\n\n## right\n\nmore\n\n<!-- /columns -->";
        round_trips(source);
        let doc = Document::from_markdown(source, &[]);
        assert_eq!(doc.blocks[0].kind, Kind::Columns);
        let widths: Vec<f64> = doc.blocks[0]
            .children
            .iter()
            .map(|c| match c.kind {
                Kind::Column { width } => width,
                _ => panic!("{c:?}"),
            })
            .collect();
        assert_eq!(widths, [0.25, 0.75]);
        // JSON from elsewhere: shares that don't add up, read and written as shares of 1.
        let json = r#"{"format":"gpui-blocks","version":1,"blocks":[
            {"id":"cols","type":"columns","children":[
                {"id":"c1","type":"column","width":0.3,"children":[{"id":"p1","type":"paragraph","text":"a"}]},
                {"id":"c2","type":"column","width":0.3,"children":[{"id":"p2","type":"paragraph","text":"b"}]}]}]}"#;
        let doc = Document::from_json(json).unwrap();
        assert!(
            doc.to_markdown().contains("<!-- column 0.5 -->"),
            "{}",
            doc.to_markdown()
        );
        let model =
            BlockEditorModel::from_markdown(&doc.to_markdown(), crate::extensions(&[])).unwrap();
        let mut ids = Ids::default();
        ids.adopt(&model, &doc.blocks);
        let back = Document::from_model(&model, String::new(), &mut ids);
        fn ids_of(blocks: &[Block], out: &mut Vec<String>) {
            for b in blocks {
                out.push(b.id.clone());
                ids_of(&b.children, out);
            }
        }
        let mut got = Vec::new();
        ids_of(&back.blocks, &mut got);
        assert_eq!(got, ["cols", "c1", "p1", "c2", "p2"]);
    }

    /// A toggle heading goes to JSON (a toggle whose first child is a heading) and back, ids
    /// and all.
    #[test]
    fn toggle_headings_keep_their_level_and_ids() {
        round_trips("<details open>\n<summary><h2>Plan</h2></summary>\n\nThe body.\n\n</details>");
        let doc = Document::from_markdown(
            "<details open>\n<summary><h2>Plan</h2></summary>\n\nThe body.\n\n</details>",
            &[],
        );
        assert!(matches!(
            doc.blocks[0].children[0].kind,
            Kind::Heading { level: 2, .. }
        ));
        assert!(
            doc.to_markdown()
                .contains("<summary><h2>Plan</h2></summary>")
        );
    }

    #[test]
    fn callouts_keep_their_icon_and_colour() {
        let source = "> [!TIP] <!-- icon:💡 color:yellow -->\n> Try it.";
        round_trips(source);
        let doc = Document::from_markdown(source, &[]);
        assert_eq!(
            doc.blocks[0].kind,
            Kind::Callout {
                name: "tip".into(),
                icon: Some("💡".into()),
                color: Some("yellow".into()),
            }
        );
        round_trips("> [!NOTE] <!-- color:blue -->");
        // From elsewhere: an invalid icon or colour is read as none.
        let json = r#"{"format":"gpui-blocks","version":1,"blocks":[
            {"id":"c","type":"callout","name":"note","icon":"two words","color":"teal",
             "children":[{"id":"p","type":"paragraph","text":"a"}]}]}"#;
        let doc = Document::from_json(json).unwrap();
        assert_eq!(doc.to_markdown(), "> [!NOTE]\n> a");
    }

    #[test]
    fn bookmarks_keep_their_page() {
        let image = format!("asset:{}", "ab".repeat(32));
        let source = format!(
            "Before\n\n[A page](https://example.com/a \"bookmark\") <!-- {{\"description\":\"What it says.\",\"image\":\"{image}\"}} -->\n\n[https://example.com/b](https://example.com/b \"bookmark\")"
        );
        round_trips(&source);
        let doc = Document::from_markdown(&source, &[]);
        assert_eq!(
            doc.blocks[1].kind,
            Kind::Bookmark {
                url: "https://example.com/a".into(),
                title: Some("A page".into()),
                description: Some("What it says.".into()),
                image: Some(image.clone()),
            }
        );
        let json = doc.to_json();
        assert!(json.contains(r#""type":"bookmark""#), "{json}");
        // From elsewhere: a picture that isn't an asset is dropped, a long description is cut,
        // and an address that isn't a web page's is only text.
        let json = format!(
            r#"{{"format":"gpui-blocks","version":1,"blocks":[
            {{"id":"b","type":"bookmark","url":"https://example.com","image":"https://x.test/i.png","description":"{}"}},
            {{"id":"c","type":"bookmark","url":"javascript:alert(1)","title":"Bad"}}]}}"#,
            "d".repeat(400)
        );
        let doc = Document::from_json(&json).unwrap();
        let markdown = doc.to_markdown();
        assert!(!markdown.contains("x.test"), "{markdown}");
        assert!(
            markdown.contains(&format!("\"{}\"", "d".repeat(300))),
            "{markdown}"
        );
        assert!(markdown.ends_with("\n\nBad"), "{markdown}");
    }

    /// Every kind's `type` is one this version reads.
    #[test]
    fn every_kind_is_a_known_type() {
        let kinds = [
            Kind::Paragraph {
                text: String::new(),
            },
            Kind::Heading {
                level: 1,
                text: String::new(),
            },
            Kind::Quote,
            Kind::Callout {
                name: "note".into(),
                icon: None,
                color: None,
            },
            Kind::Toggle { open: true },
            Kind::List {
                ordered: false,
                start: None,
            },
            Kind::Item {
                checked: None,
                spread: false,
            },
            Kind::Code {
                language: None,
                meta: None,
                code: String::new(),
            },
            Kind::Divider,
            Kind::Table { align: Vec::new() },
            Kind::Row,
            Kind::Cell {
                text: String::new(),
            },
            Kind::Page {
                doc: "d".into(),
                title: String::new(),
            },
            Kind::Toc,
            Kind::Embed {
                doc: "d".into(),
                title: String::new(),
                view: "page".into(),
                height: 1,
            },
            Kind::Bookmark {
                url: "https://example.com".into(),
                title: None,
                description: None,
                image: None,
            },
            Kind::Widget {
                name: "w".into(),
                meta: None,
                body: String::new(),
            },
            Kind::Columns,
            Kind::Column { width: 0.5 },
            Kind::Markdown {
                markdown: String::new(),
            },
        ];
        assert_eq!(kinds.len(), TYPES.len());
        for kind in kinds {
            let value = serde_json::to_value(&kind).unwrap();
            let name = value["type"].as_str().unwrap().to_string();
            assert!(TYPES.contains(&name.as_str()), "{name}");
        }
    }

    /// Markdown to JSON to Markdown, and JSON to Markdown to JSON with the same ids.
    fn round_trips(source: &str) {
        let widgets = widgets();
        let doc = Document::from_markdown(source, &widgets);
        assert_eq!(doc.to_markdown(), source, "{}", doc.to_json_pretty());
        let json = doc.to_json();
        let back = Document::from_json(&json).unwrap();
        assert_eq!(back, doc);
        let model = BlockEditorModel::from_markdown(
            crate::editor::split_front_matter(&back.to_markdown()).1,
            crate::extensions(&widgets),
        )
        .unwrap();
        let mut ids = Ids::default();
        ids.adopt(&model, &back.blocks);
        let again = Document::from_model(&model, back.front_matter.clone(), &mut ids);
        assert_eq!(again, doc, "the same blocks with the same ids");
    }

    fn para(id: &str, text: &str) -> Block {
        Block {
            id: id.into(),
            kind: Kind::Paragraph { text: text.into() },
            children: Vec::new(),
        }
    }

    fn list(id: &str, items: &[(&str, &str)]) -> Block {
        Block {
            id: id.into(),
            kind: Kind::List {
                ordered: false,
                start: None,
            },
            children: items
                .iter()
                .map(|(item, text)| Block {
                    id: (*item).into(),
                    kind: Kind::Item {
                        checked: None,
                        spread: false,
                    },
                    children: vec![para(&format!("{item}p"), text)],
                })
                .collect(),
        }
    }

    /// `blocks` read back through their Markdown, as `Editor::set_document` does, and written
    /// again with the ids adopted.
    fn read_back(blocks: Vec<Block>) -> Document {
        let doc = Document {
            format: FORMAT.into(),
            version: VERSION,
            front_matter: String::new(),
            blocks,
        };
        let model =
            BlockEditorModel::from_markdown(&doc.to_markdown(), crate::extensions(&[])).unwrap();
        let mut ids = Ids::default();
        ids.adopt(&model, &doc.blocks);
        Document::from_model(&model, String::new(), &mut ids)
    }

    fn ids(blocks: &[Block]) -> Vec<String> {
        blocks
            .iter()
            .flat_map(|b| std::iter::once(b.id.clone()).chain(ids(&b.children)))
            .collect()
    }

    #[test]
    fn a_paragraph_the_markdown_drops_takes_no_ids_with_it() {
        // "a", Enter, Enter, "b": the empty paragraph isn't written, so isn't read back.
        let back = read_back(vec![
            para("A", "a"),
            para("E", ""),
            para("B", "b"),
            para("C", "c"),
        ]);
        let texts: Vec<_> = back
            .blocks
            .iter()
            .map(|b| match &b.kind {
                Kind::Paragraph { text } => text.as_str(),
                _ => "",
            })
            .collect();
        assert_eq!(texts, ["a", "b", "c"]);
        assert_eq!(ids(&back.blocks), ["A", "B", "C"], "each keeps its own id");
    }

    #[test]
    fn lists_the_markdown_joins_keep_their_items_ids() {
        // Two lists of one type side by side come back as one.
        let back = read_back(vec![
            list("L1", &[("i1", "one"), ("i2", "two")]),
            list("L2", &[("i3", "three")]),
            para("P", "after"),
        ]);
        assert_eq!(
            ids(&back.blocks),
            ["L1", "i1", "i1p", "i2", "i2p", "i3", "i3p", "P"],
            "the joined list is the first's, with every item's id; the paragraph after keeps \
             its id"
        );
    }

    #[test]
    fn an_undone_delete_brings_the_block_back_with_its_id() {
        use markdown_editor::{BlockSelection, EditorCommand};
        fn all(blocks: &[Block]) -> Vec<String> {
            blocks
                .iter()
                .flat_map(|b| std::iter::once(b.id.clone()).chain(all(&b.children)))
                .collect()
        }
        let mut model =
            BlockEditorModel::from_markdown("a\n\nb\n\nc", crate::extensions(&[])).unwrap();
        let mut ids = Ids::default();
        let before = Document::from_model(&model, String::new(), &mut ids);
        let b = model.document().root().children().nth(1).unwrap().id();
        model
            .set_selection(BlockSelection::Blocks { anchor: b, head: b })
            .unwrap();
        model.run(EditorCommand::DeleteSelection).unwrap();
        // Saved while it's gone.
        let deleted = Document::from_model(&model, String::new(), &mut ids);
        assert_eq!(deleted.blocks.len(), 2);
        assert!(model.undo());
        let after = Document::from_model(&model, String::new(), &mut ids);
        assert_eq!(
            all(&after.blocks),
            all(&before.blocks),
            "the same ids again"
        );
    }

    #[test]
    fn the_sample_round_trips_with_its_ids() {
        round_trips(include_str!("../examples/sample.md").trim_end());
    }

    #[test]
    fn every_kind_round_trips() {
        for source in [
            "# One\n\n###### Six",
            "> a\n>\n> b",
            "> [!TIP]\n> - x\n> - y",
            "<details open>\n<summary>Sum **bold**</summary>\n\nBody\n\n</details>",
            "<details>\n<summary>Empty</summary>\n\n</details>",
            "3. three\n4. four",
            "- [ ] a\n- [x] b\n  - c",
            "- a\n\n  b\n\n- c",
            "```rust title=x\nfn main() {}\n```",
            "````\n```\n````",
            "---",
            "[TOC]\n\n# Heading",
            "[Q4 board](doc:b1 \"embed board 480\")\n\n[Plain](doc:b2 \"embed page 320\")",
            "| a | b | c |\n| :--- | :---: | --- |\n| 1 | \\| | 3 |",
            "[Notes](doc:9c1e \"page\")",
            "See [Roadmap](doc:4f2a) and ![alt](https://example.com/a.png).",
            "```chart title=x\na: 1\n```",
            "```chart\n```",
            "---\ntitle: x\n---\n\n# After front matter",
        ] {
            round_trips(source);
        }
    }

    #[test]
    fn a_list_from_one_has_no_start() {
        let doc = Document::from_markdown("1. a\n2. b", &[]);
        assert_eq!(
            doc.blocks[0].kind,
            Kind::List {
                ordered: true,
                start: None
            }
        );
        assert_eq!(doc.to_markdown(), "1. a\n2. b");
    }

    #[test]
    fn links_and_pages_are_listed() {
        let doc = Document::from_markdown(
            "[A](doc:a) and [B](doc:b) and [A again](doc:a)\n\n[P](doc:p \"page\")\n\n| x |\n| --- |\n| [C](doc:c) |\n\n[bad](doc:a/b)",
            &[],
        );
        assert_eq!(doc.links(), ["a", "b", "p", "c"]);
        assert_eq!(doc.pages(), ["p"]);
    }

    #[test]
    fn ids_are_unique_and_valid() {
        let doc = Document::from_markdown(include_str!("../examples/sample.md"), &widgets());
        let mut all = Vec::new();
        visit(&doc.blocks, &mut |b| all.push(b.id.clone()));
        let unique: HashSet<_> = all.iter().collect();
        assert_eq!(unique.len(), all.len());
        assert!(all.iter().all(|id| id.len() == 12 && valid_id(id)));
    }

    #[test]
    fn hostile_json_stays_in_its_blocks() {
        let json = r#"{"format":"gpui-blocks","version":1,"blocks":[
            {"id":"a","type":"paragraph","text":"one\n\n# not a heading"},
            {"id":"a","type":"heading","level":99,"text":"dup id"},
            {"id":"bad id!","type":"callout","name":"x]\n# y","children":[{"id":"c","type":"paragraph","text":"in"}]},
            {"id":"d","type":"code","language":"rs\n```","code":"```\nstill code"},
            {"id":"e","type":"page","doc":"../etc","title":"T"},
            {"id":"f","type":"widget","name":"cha rt`","body":"x"}
        ]}"#;
        let doc = Document::from_json(json).unwrap();
        let md = doc.to_markdown();
        let model = BlockEditorModel::from_markdown(&md, crate::extensions(&widgets())).unwrap();
        let mut ids = Ids::default();
        ids.adopt(&model, &doc.blocks);
        let back = Document::from_model(&model, String::new(), &mut ids);
        let kinds: Vec<_> = back
            .blocks
            .iter()
            .map(|b| std::mem::discriminant(&b.kind))
            .collect();
        assert_eq!(kinds.len(), 6, "{md}");
        assert!(matches!(back.blocks[0].kind, Kind::Paragraph { .. }));
        assert!(matches!(
            back.blocks[1].kind,
            Kind::Heading { level: 6, .. }
        ));
        assert_eq!(back.blocks[0].id, "a");
        assert_ne!(back.blocks[1].id, "a", "a repeated id is replaced");
        assert_ne!(back.blocks[2].id, "bad id!");
        assert!(matches!(back.blocks[3].kind, Kind::Code { .. }));
        assert!(
            matches!(back.blocks[4].kind, Kind::Paragraph { .. }),
            "a bad doc id is text"
        );
    }

    #[test]
    fn other_formats_and_newer_versions_are_refused() {
        assert!(matches!(
            Document::from_json(r#"{"format":"x","version":1,"blocks":[]}"#),
            Err(Error::Format { .. })
        ));
        assert!(matches!(
            Document::from_json(r#"{"format":"gpui-blocks","version":2,"blocks":[]}"#),
            Err(Error::Format { .. })
        ));
        assert!(Document::from_json("{").is_err());
    }
}
