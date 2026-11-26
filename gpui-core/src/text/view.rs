//! TextView: Markdown drawn natively, selectable, with links as events for the host.

use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    AbsoluteLength, AnyElement, App, AvailableSpace, Bounds, ClipboardItem, Corners, CursorStyle,
    DefiniteLength, DispatchPhase, Element, ElementId, Fill, FocusHandle, FontWeight,
    GlobalElementId, HitboxBehavior, Hsla, InspectorElementId, InteractiveElement as _,
    IntoElement, KeyBinding, LayoutId, Length, Modifiers, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, ParentElement as _, Pixels, Point, Rems, RenderOnce, Role,
    SharedString, StatefulInteractiveElement as _, Style, StyleRefinement, Styled as _, Window,
    actions, div, fill, point, px, quad, relative, rems, size, transparent_black,
};
use unicode_segmentation::UnicodeSegmentation as _;

use gpui::TextAlign;
use gpui::prelude::FluentBuilder as _;

use super::markdown::{Align, Block, MATH_LANGUAGE, MATH_SOURCE_LANGUAGE, parse_with};
use super::paragraph::{Affinity, CodeStyle, Marks, Paragraph, ParagraphLayout, ParagraphStyle};
use crate::base::ScrollExt as _;
use crate::theme::{ActiveTheme as _, Theme};

actions!(
    gpui_core_text,
    [CopySelection, SelectAllText, ClearSelection]
);

const CONTEXT: &str = "GpuiCoreTextView";

pub(crate) fn init(cx: &mut App) {
    let c = Some(CONTEXT);
    if cfg!(any(target_os = "macos", target_os = "ios")) {
        cx.bind_keys([
            KeyBinding::new("cmd-c", CopySelection, c),
            KeyBinding::new("cmd-a", SelectAllText, c),
        ]);
    } else {
        cx.bind_keys([
            KeyBinding::new("ctrl-c", CopySelection, c),
            KeyBinding::new("ctrl-a", SelectAllText, c),
        ]);
    }
    cx.bind_keys([KeyBinding::new("escape", ClearSelection, c)]);
}

type HeadingSize = Arc<dyn Fn(u8, Pixels) -> Pixels + Send + Sync + 'static>;

/// What kind of block a [`TextView`] lays out, for [`TextViewStyle::block_gap`].
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockKind {
    Paragraph,
    /// Its level, 1 to 6.
    Heading(u8),
    List {
        ordered: bool,
    },
    Code,
    Quote,
    Rule,
    Table,
}

/// The space between two blocks one after the other, or `None` for the view's own rule.
pub type BlockGap = Arc<dyn Fn(BlockKind, BlockKind) -> Option<Pixels> + Send + Sync + 'static>;

fn kind(b: &Block) -> BlockKind {
    match b {
        Block::Paragraph(_) => BlockKind::Paragraph,
        Block::Heading(level, _) => BlockKind::Heading(*level),
        Block::List { start, .. } => BlockKind::List {
            ordered: start.is_some(),
        },
        Block::Code { .. } => BlockKind::Code,
        Block::Quote(_) => BlockKind::Quote,
        Block::Rule => BlockKind::Rule,
        Block::Table { .. } => BlockKind::Table,
    }
}

/// A heading's space above and below from its level (1 to 6) and its font size.
pub type HeadingMargins = Arc<dyn Fn(u8, Pixels) -> (Pixels, Pixels) + Send + Sync + 'static>;

/// How a TextView lays its blocks out, beyond the theme.
#[derive(Clone)]
pub struct TextViewStyle {
    /// The space between blocks.
    pub paragraph_gap: Rems,
    /// The size headings are measured from (their text is bold): the size of the text around
    /// the view unless given, so it follows the host's zoom.
    pub heading_base_font_size: Option<Pixels>,
    /// A heading's size from its level (1 to 6) and the base. By default: 1.6, 1.35 and 1.15
    /// times the base for levels 1 to 3, the base for the rest.
    pub heading_font_size: Option<HeadingSize>,
    /// A code block's box: its fill, border, corners, padding and text.
    pub code_block: StyleRefinement,
    /// How far a list's items are set in from its left edge (1.2 rems for bullets, 1.6 for
    /// numbers unless given; never more than a quarter of the width).
    pub list_indent: Option<Pixels>,
    /// The space between a list's items (0.4 of the paragraph gap unless given).
    pub list_item_gap: Option<Pixels>,
    /// The space between an item's bullet or number and its text. Markers end that far before
    /// the text, as the web's do (0.3 of the indent unless given).
    pub list_marker_gap: Option<Pixels>,
    /// The space above a heading and below it (the paragraph gap unless given).
    pub heading_margin_top: Option<Pixels>,
    pub heading_margin_bottom: Option<Pixels>,
    /// The space above and below a heading by its level and font size, over the two above: for
    /// margins in ems, as the web's `1.1em` above an h1. Between two headings the larger of the
    /// first's bottom and the second's top applies, and the first block in the view or in a
    /// list item, quote or cell never has space above it.
    pub heading_margins: Option<HeadingMargins>,
    /// A heading's line height as a multiple of its size (the text's line height, scaled to the
    /// heading's size, unless given).
    pub heading_line_height: Option<f32>,
    /// A table's corners (the theme's `radius_md` unless given).
    pub table_radius: Option<Pixels>,
    /// The space round a table cell's text (half a rem unless given).
    pub table_cell_padding: Option<Pixels>,
    /// A table's text size (the body's unless given): the web's 14px tables in 16px Markdown.
    pub table_text_size: Option<Pixels>,
    /// A table's header row's weight (semibold unless given).
    pub table_header_weight: Option<FontWeight>,
    /// The space between two blocks by their kinds, over every rule above: the web's margins
    /// depend on the pair (a heading has less below it before a paragraph than before a list).
    /// `None` for a pair keeps the view's own rule. It applies between siblings: a list's items
    /// keep [`list_item_gap`](Self::list_item_gap), and the first block anywhere has no space
    /// above it.
    pub block_gap: Option<BlockGap>,
    /// Colours a fenced code block in its language (```` ```rust ````), from the host: gpui-core's
    /// `highlight` feature makes one (`Highlighter::into_fn`), or a host brings its own. Ranges
    /// out of order, overlapping, past the code or off a character's edge are passed over.
    pub highlight: Option<super::HighlightFn>,
    /// Inline code: its family (the theme's mono family unless given), its size as a share of
    /// the text's (the text's unless given), its padding, corners, fill and colour.
    pub inline_code: InlineCodeStyle,
    /// Colours over the theme's, for a host with a palette of its own.
    pub colors: TextViewColors,
}

/// Inline code's look in a [`TextView`]; every part optional.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InlineCodeStyle {
    pub family: Option<SharedString>,
    /// Its size as a share of the text's (0.875, as the web's `0.875em`).
    pub size: Option<f32>,
    /// Space either side of it, inside its fill.
    pub padding: Option<Pixels>,
    /// Space above and below its ink, inside its fill (half the side padding unless given).
    /// Negative draws the fill inside the font's ascent and descent, towards its cap height.
    /// The fill isn't held to the line: past it, it reaches into the lines' leading, as an
    /// inline box's background does on the web.
    pub padding_y: Option<Pixels>,
    /// A border round the fill, outside the padding (none unless both are given).
    pub border_width: Option<Pixels>,
    pub border_color: Option<Hsla>,
    pub radius: Option<Pixels>,
    pub background: Option<Hsla>,
    pub color: Option<Hsla>,
}

/// A [`TextView`]'s colours, each over the theme's when given. The text's own colour is the
/// colour of the text around the view.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextViewColors {
    /// Links (the theme's foreground).
    pub link: Option<Hsla>,
    /// List bullets, numbers and task boxes (the theme's muted foreground).
    pub list_marker: Option<Hsla>,
    /// Horizontal rules (the theme's border).
    pub rule: Option<Hsla>,
    /// A quote's bar (the theme's border).
    pub quote_bar: Option<Hsla>,
    /// A table's lines (the theme's border).
    pub table_line: Option<Hsla>,
    /// A table's header row's fill (none unless given).
    pub table_header: Option<Hsla>,
    /// A table's header row's text (the text's colour unless given).
    pub table_header_text: Option<Hsla>,
    /// Every other body row's fill, from the second (none unless given).
    pub table_stripe: Option<Hsla>,
    /// The selection (the theme's ring at 30%).
    pub selection: Option<Hsla>,
    /// An image's description and other quiet text (the theme's muted foreground).
    pub muted: Option<Hsla>,
}

impl Default for TextViewStyle {
    fn default() -> Self {
        Self {
            paragraph_gap: rems(0.75),
            heading_base_font_size: None,
            heading_font_size: None,
            code_block: StyleRefinement::default(),
            list_indent: None,
            list_item_gap: None,
            list_marker_gap: None,
            heading_margin_top: None,
            heading_margin_bottom: None,
            heading_margins: None,
            heading_line_height: None,
            table_radius: None,
            table_cell_padding: None,
            table_text_size: None,
            table_header_weight: None,
            block_gap: None,
            highlight: None,
            inline_code: InlineCodeStyle::default(),
            colors: TextViewColors::default(),
        }
    }
}

type OnLink = Rc<dyn Fn(&str, &Modifiers, &mut Window, &mut App)>;
type CodeOverlay = Rc<dyn Fn(&CodeBlock, &mut Window, &mut App) -> AnyElement>;

/// A fenced code block, for [`TextView::code_block_overlay`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeBlock {
    /// The key of the source it's in ([`TextView::blocks`]; 0 for [`TextView::markdown`]).
    pub key: u64,
    /// Its place among its source's code blocks, from 0.
    pub index: usize,
    /// Its fence's info string (```` ```rust ````), if any.
    pub language: Option<SharedString>,
    /// Its code, without the fences or a trailing line end.
    pub code: SharedString,
}

/// Markdown, drawn. Nothing in it fetches or opens anything: a link press goes to
/// [`on_link_click`](TextView::on_link_click), and an image shows as its description.
#[derive(IntoElement)]
pub struct TextView {
    id: ElementId,
    sources: Vec<(u64, SharedString)>,
    style: TextViewStyle,
    selectable: bool,
    on_link: Option<OnLink>,
    caret: Option<Hsla>,
    code_overlay: Option<CodeOverlay>,
}

impl TextView {
    pub fn markdown(id: impl Into<ElementId>, source: impl Into<SharedString>) -> Self {
        Self::blocks(id, [(0, source.into())])
    }

    /// One document from several Markdown sources, each under a key of the host's (unique in
    /// the view), in order: a chat reply's settled blocks and its live tail, say. It's drawn,
    /// spaced, selected and copied as one `markdown` view of the sources one after another
    /// would be, but a source is read again only when its text changes, and each of its
    /// top-level blocks laid out again only when that block changes: a streamed reply's last
    /// source changing costs that source's last block, not the reply.
    ///
    /// A link like `[text][label]` resolves to a definition (`[label]: https://…`) in any of the
    /// sources; a source's own definition wins over another's. When a source's definitions
    /// change, the sources that could refer to them are read again.
    ///
    /// A selection stays while the sources before it and its own are unchanged, so text that's
    /// settled can be selected and copied while the last source streams.
    pub fn blocks(
        id: impl Into<ElementId>,
        sources: impl IntoIterator<Item = (u64, SharedString)>,
    ) -> Self {
        Self {
            id: id.into(),
            sources: sources.into_iter().collect(),
            style: TextViewStyle::default(),
            selectable: false,
            on_link: None,
            caret: None,
            code_overlay: None,
        }
    }

    /// Draws `f`'s element over each fenced code block, laid out at the block's box and placed
    /// on it, over the text: a Copy button in its corner, say. It's built for the blocks in
    /// view as the view is drawn, so it costs no frames of its own. Its listeners run before
    /// the view's: a press it stops (`cx.stop_propagation()`) doesn't start a selection.
    ///
    /// A display equation's block (language [`MATH_LANGUAGE`](super::MATH_LANGUAGE)) is the
    /// exception: there the element is laid out beside the TeX, at the box's right and at its
    /// own width (so give it one: the button, not a layer the size of the box), and the TeX
    /// scrolls in the room left, so the element never covers it.
    pub fn code_block_overlay(
        mut self,
        f: impl Fn(&CodeBlock, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.code_overlay = Some(Rc::new(f));
        self
    }

    /// A caret in `color` after the text's last character (the last source's, with
    /// [`blocks`](Self::blocks), or the one before it when that source is empty), as a reply
    /// being written shows. It's only drawn, never laid out, so taking it away can't move the
    /// text. It doesn't blink: a host that wants it to sets and clears it on a timer of its own.
    pub fn trailing_caret(mut self, color: Option<Hsla>) -> Self {
        self.caret = color;
        self
    }

    pub fn style(mut self, style: TextViewStyle) -> Self {
        self.style = style;
        self
    }

    /// Whether its text can be selected with the pointer and copied.
    pub fn selectable(mut self, selectable: bool) -> Self {
        self.selectable = selectable;
        self
    }

    /// Called with a link's address when it's pressed (and not dragged across).
    pub fn on_link_click(
        mut self,
        f: impl Fn(&str, &Modifiers, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_link = Some(Rc::new(f));
        self
    }
}

/// A place in the text: a paragraph of the layout and an offset in it.
type Pos = (usize, usize);

/// One top-level block of a source, and its layout at the last width and style it was laid out
/// in, at the view's top left. It's kept while the block is the same as the one at its place
/// when the source is read again. (A long list or table could keep a piece per item or row the
/// same way, so a growing last item is all that's laid out again.)
struct Piece {
    block: Block,
    laid: Option<Rc<ViewLayout>>,
    /// A list's items laid out on their own, kept while the list changes: a list growing as
    /// it's streamed lays out only its new or changed items.
    items: Vec<ItemLaid>,
    /// A code block's lines laid out on their own, kept while the block changes: a fence
    /// growing as it's streamed lays out only its new or changed lines.
    lines: Vec<LineLaid>,
    /// A table's rows laid out on their own, kept while the table changes: a table growing
    /// as it's streamed lays out only its new or changed rows.
    rows: Vec<RowLaid>,
}

/// A table row's cells laid out on their own, and what they were laid out for.
#[derive(Default)]
struct RowLaid {
    /// Its cells, as many as the table has columns.
    cells: Vec<Paragraph>,
    header: bool,
    align: Vec<Align>,
    /// A cell's width inside its padding.
    width: Pixels,
    key: StyleKey,
    laid: Vec<Rc<ParagraphLayout>>,
}

/// A line of a code block laid out on its own, and what it was laid out for.
struct LineLaid {
    line: Paragraph,
    width: Pixels,
    key: StyleKey,
    laid: Rc<ParagraphLayout>,
}

/// A list item laid out on its own, at the list's top left, and what it was laid out for.
struct ItemLaid {
    item: super::markdown::Item,
    /// Its place in the list and the list's start: what its marker says.
    n: usize,
    start: Option<u32>,
    width: Pixels,
    key: StyleKey,
    laid: Rc<ViewLayout>,
}

/// A source of [`TextView::blocks`], read.
struct KeyState {
    key: u64,
    source: SharedString,
    /// Whether it could refer to a link definition (it has a `]`), so is read with the others'.
    refers: bool,
    /// The hash of the definitions it was read with.
    read_with: u64,
    /// Its own link reference definitions, as lines for the others to be read with.
    definitions: String,
    pieces: Vec<Piece>,
    /// Its blocks one after another, while none of them is laid out again.
    laid: Option<Rc<SourceLaid>>,
}

impl KeyState {
    /// Reads `source`, with the definitions of the sources before it and after it if it could
    /// refer to one.
    fn read(
        key: u64,
        source: SharedString,
        (before, after): (&str, &str),
        previous: Option<Vec<Piece>>,
    ) -> Self {
        let refers = source.contains(']');
        let parsed = if refers {
            parse_with(&source, before, after)
        } else {
            parse_with(&source, "", "")
        };
        let mut k = KeyState {
            key,
            refers,
            read_with: if refers {
                context_hash(before, after)
            } else {
                0
            },
            definitions: parsed.definitions,
            source,
            pieces: Vec::new(),
            laid: None,
        };
        k.set_blocks(parsed.blocks, previous.unwrap_or_default());
        k
    }

    /// Its blocks, keeping the layout of each that's the same as the one at its place before.
    fn set_blocks(&mut self, blocks: Vec<Block>, mut previous: Vec<Piece>) {
        previous.truncate(blocks.len());
        let mut previous = previous.into_iter();
        self.pieces = blocks
            .into_iter()
            .map(|block| match previous.next() {
                Some(p) if p.block == block => Piece {
                    block,
                    laid: p.laid,
                    items: p.items,
                    lines: p.lines,
                    rows: p.rows,
                },
                // A code block that changed keeps its lines' layouts, for the lines that didn't.
                Some(p)
                    if matches!(p.block, Block::Code { .. })
                        && matches!(block, Block::Code { .. }) =>
                {
                    Piece {
                        block,
                        laid: None,
                        items: Vec::new(),
                        lines: p.lines,
                        rows: Vec::new(),
                    }
                }
                // A list that changed keeps its items' layouts, for the items that didn't.
                Some(p)
                    if matches!(p.block, Block::List { .. })
                        && matches!(block, Block::List { .. }) =>
                {
                    Piece {
                        block,
                        laid: None,
                        items: p.items,
                        lines: Vec::new(),
                        rows: Vec::new(),
                    }
                }
                // A table that changed keeps its rows' layouts, for the rows that didn't.
                Some(p)
                    if matches!(p.block, Block::Table { .. })
                        && matches!(block, Block::Table { .. }) =>
                {
                    Piece {
                        block,
                        laid: None,
                        items: Vec::new(),
                        lines: Vec::new(),
                        rows: p.rows,
                    }
                }
                _ => Piece {
                    block,
                    laid: None,
                    items: Vec::new(),
                    lines: Vec::new(),
                    rows: Vec::new(),
                },
            })
            .collect();
    }

    /// Its paragraphs while every block is laid out; `None` before.
    fn leaf_count(&self) -> Option<usize> {
        if let Some(l) = &self.laid {
            return Some(l.leaves.len());
        }
        self.pieces
            .iter()
            .map(|p| p.laid.as_ref().map(|l| l.leaves.len()))
            .sum()
    }
}

/// Whether two texts are the same: at once when they're the same string, as a host's unchanged
/// source is from one draw to the next, and character by character otherwise.
fn same_text(a: &SharedString, b: &SharedString) -> bool {
    let (a, b): (&str, &str) = (a, b);
    std::ptr::eq(a, b) || a == b
}

/// The hash of the definitions a source is read with, before it and after it.
fn context_hash(before: &str, after: &str) -> u64 {
    hash_of(before) ^ hash_of(after).rotate_left(17)
}

fn hash_of(text: &str) -> u64 {
    use std::hash::{Hash as _, Hasher as _};
    if text.is_empty() {
        return 0;
    }
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish().max(1)
}

/// What a view keeps between draws.
struct ViewState {
    keys: Vec<KeyState>,
    /// Counts the changes to the sources, so a layout of older ones isn't taken for theirs.
    generation: u64,
    focus: FocusHandle,
    anchor: Option<Pos>,
    head: Option<Pos>,
    selecting: bool,
    /// A press that hasn't moved yet: a link press if it lets go here.
    press: Option<(Point<Pixels>, Pos)>,
    layout: Option<Rc<Assembled>>,
    bounds: Bounds<Pixels>,
    /// The width it was last drawn at: what a measure with no definite width (a flex item's
    /// content size) is answered at, so its layout is the cached one.
    width: Option<Pixels>,
}

impl ViewState {
    /// Takes the host's sources: reads those that are new or changed (and those whose
    /// definitions to refer to have), keeps the layout of every block that's the same, and keeps
    /// the selection if it's in what's unchanged.
    fn sync(&mut self, sources: &[(u64, SharedString)]) {
        let same = self.keys.len() == sources.len()
            && self
                .keys
                .iter()
                .zip(sources)
                .all(|(k, (key, src))| k.key == *key && same_text(&k.source, src));
        if same {
            return;
        }
        let before: Vec<u64> = self.keys.iter().map(|k| k.key).collect();
        // The sources' own definitions first: each is read alone, or kept if unchanged.
        let mut changed = vec![false; sources.len()];
        let in_place = before.len() <= sources.len()
            && before.iter().zip(sources).all(|(k, (key, _))| k == key);
        let mut keys = if in_place {
            // The same sources in the same places, some changed and maybe more after them (as
            // while a reply streams): each changed one read again where it is.
            let mut keys = std::mem::take(&mut self.keys);
            for (i, (key, src)) in sources.iter().enumerate() {
                match keys.get_mut(i) {
                    Some(k) if same_text(&k.source, src) => {}
                    Some(k) => {
                        let previous = std::mem::take(&mut k.pieces);
                        *k = KeyState::read(*key, src.clone(), ("", ""), Some(previous));
                        changed[i] = true;
                    }
                    None => {
                        keys.push(KeyState::read(*key, src.clone(), ("", ""), None));
                        changed[i] = true;
                    }
                }
            }
            keys
        } else {
            let mut old: std::collections::HashMap<u64, KeyState> =
                self.keys.drain(..).map(|k| (k.key, k)).collect();
            let mut keys: Vec<KeyState> = Vec::with_capacity(sources.len());
            for (i, (key, src)) in sources.iter().enumerate() {
                keys.push(match old.remove(key) {
                    Some(k) if same_text(&k.source, src) => k,
                    prev => {
                        changed[i] = true;
                        KeyState::read(*key, src.clone(), ("", ""), prev.map(|k| k.pieces))
                    }
                });
            }
            keys
        };
        // Then those that could refer to a definition, with the definitions of the sources
        // before them and after them, in the document's order (the first of a label wins).
        let any = keys.iter().any(|k| !k.definitions.is_empty());
        let defs: Vec<String> = if any {
            keys.iter().map(|k| k.definitions.clone()).collect()
        } else {
            Vec::new()
        };
        for (i, k) in keys.iter_mut().enumerate() {
            if !k.refers {
                continue;
            }
            let (before, after) = if any {
                (defs[..i].concat(), defs[i + 1..].concat())
            } else {
                (String::new(), String::new())
            };
            if k.read_with != context_hash(&before, &after) {
                let pieces = std::mem::take(&mut k.pieces);
                *k = KeyState::read(k.key, k.source.clone(), (&before, &after), Some(pieces));
                changed[i] = true;
            }
        }
        // The paragraphs before the first change keep their places: a selection among them
        // stays.
        let mut stable = 0;
        for (i, k) in keys.iter().enumerate() {
            let kept = !changed[i] && before.get(i).is_some_and(|key| *key == k.key);
            if kept && let Some(n) = k.leaf_count() {
                stable += n;
                continue;
            }
            for p in &k.pieces {
                match &p.laid {
                    Some(l) => stable += l.leaves.len(),
                    None => break,
                }
            }
            break;
        }
        let inside = |p: Option<Pos>| p.is_none_or(|(leaf, _)| leaf < stable);
        if !(inside(self.anchor) && inside(self.head)) {
            self.anchor = None;
            self.head = None;
            self.selecting = false;
        }
        self.press = None;
        self.keys = keys;
        self.generation += 1;
        self.layout = None;
    }

    fn selection(&self) -> Option<(Pos, Pos)> {
        let (a, h) = (self.anchor?, self.head?);
        (a != h).then(|| if a <= h { (a, h) } else { (h, a) })
    }

    fn selected_text(&self) -> Option<String> {
        let ((l0, o0), (l1, o1)) = self.selection()?;
        let layout = self.layout.as_ref()?;
        let mut out = String::new();
        let mut above: Option<Pixels> = None;
        for i in l0..=l1 {
            let leaf = layout.leaf(i)?;
            let t = &leaf.paragraph.text;
            let (s, e) = (
                if i == l0 { o0 } else { 0 },
                if i == l1 { o1 } else { t.len() },
            );
            if let Some(bottom) = above {
                // A display equation between this paragraph and the one before (it isn't a
                // paragraph of the view's): its TeX, as it was written.
                for tex in layout.math_between(bottom, leaf.origin.y) {
                    out.push_str("\n\n$$");
                    out.push_str(&tex);
                    out.push_str("$$");
                }
                out.push_str(leaf.separator);
            }
            leaf.copy(s.min(t.len()), e.min(t.len()), &mut out);
            above = Some(leaf.origin.y + leaf.layout.height);
        }
        Some(out)
    }
}

/// A paragraph placed in the view.
#[derive(Clone)]
struct Leaf {
    origin: Point<Pixels>,
    layout: Rc<ParagraphLayout>,
    paragraph: Rc<Paragraph>,
    /// What goes before it when copied with the one above.
    separator: &'static str,
    /// Its inline formulas typeset, each in an inline box of the paragraph's.
    maths: Rc<Vec<InlineMath>>,
}

/// An inline formula typeset in a paragraph's inline box (at `offset`, its U+FFFC).
#[derive(Debug)]
struct InlineMath {
    offset: usize,
    tex: SharedString,
    #[cfg(feature = "math")]
    formula: Rc<super::math::Formula>,
    /// Pixels an em, and the colour it's drawn in.
    #[cfg(feature = "math")]
    em: Pixels,
    #[cfg(feature = "math")]
    color: Hsla,
}

/// How much bigger than the text around it math is set, as KaTeX's stylesheet sets it.
#[cfg(feature = "math")]
const MATH_SCALE: f32 = 1.21;

#[derive(Clone)]
enum Deco {
    Box {
        bounds: Bounds<Pixels>,
        fill: Hsla,
        border: Hsla,
        border_width: Pixels,
        radius: Corners<Pixels>,
    },
    Text {
        origin: Point<Pixels>,
        layout: Rc<ParagraphLayout>,
    },
    /// Where a fenced code block's box is, and what's in it: for an overlay. Not painted (a
    /// display equation's TeX is drawn by its own overlay, which scrolls sideways).
    Code {
        bounds: Bounds<Pixels>,
        language: Option<SharedString>,
        code: SharedString,
        math: Option<Rc<MathLook>>,
    },
}

/// How a display equation's TeX is drawn: its lines, each on one line however wide, in the code
/// block's font, inside its box's padding.
#[derive(Debug)]
struct MathLook {
    lines: Vec<SharedString>,
    font: gpui::Font,
    size: Pixels,
    line_height: Pixels,
    color: Hsla,
    /// Top, right, bottom and left, inside the border.
    pad: [Pixels; 4],
    /// The box's border, inside which the TeX is clipped as it scrolls.
    border: Pixels,
    /// The equation typeset, at pixels an em and in a colour; None to show its TeX.
    #[cfg(feature = "math")]
    formula: Option<(Rc<super::math::Formula>, Pixels, Hsla)>,
}

impl Deco {
    /// Where it's drawn.
    fn bounds(&self) -> Bounds<Pixels> {
        match self {
            Deco::Box { bounds, .. } | Deco::Code { bounds, .. } => *bounds,
            Deco::Text { origin, layout } => {
                Bounds::new(*origin, size(layout.width, layout.height))
            }
        }
    }

    fn moved_down(&self, by: Pixels) -> Deco {
        let mut d = self.clone();
        match &mut d {
            Deco::Box { bounds, .. } => bounds.origin.y += by,
            Deco::Text { origin, .. } => origin.y += by,
            Deco::Code { bounds, .. } => bounds.origin.y += by,
        }
        d
    }
}

/// A block (or a list item) laid out from its top left: its paragraphs and what's drawn
/// around them.
struct ViewLayout {
    width: Pixels,
    key: StyleKey,
    height: Pixels,
    leaves: Vec<Leaf>,
    decos: Vec<Deco>,
}

/// Where the things placed down a layout reach, to find those in a band of it by binary search
/// rather than by looking at each: for each, the furthest down any reaches up to and with it,
/// and the highest any starts from it on. Both only grow, whatever order the things are in.
#[derive(Default)]
struct Reach {
    bottoms: Vec<Pixels>,
    tops: Vec<Pixels>,
}

impl Reach {
    /// From each thing's top and bottom, in order.
    fn new(spans: impl Iterator<Item = (Pixels, Pixels)>) -> Self {
        let mut bottoms = Vec::new();
        let mut tops = Vec::new();
        let mut low = px(f32::MIN);
        for (top, bottom) in spans {
            low = low.max(bottom);
            bottoms.push(low);
            tops.push(top);
        }
        let mut high = px(f32::MAX);
        for t in tops.iter_mut().rev() {
            high = high.min(*t);
            *t = high;
        }
        Reach { bottoms, tops }
    }

    /// The things that could reach into `top..bottom`: each before them ends above it, and each
    /// after them starts below it.
    fn within(&self, top: Pixels, bottom: Pixels) -> std::ops::Range<usize> {
        let start = self.bottoms.partition_point(|b| *b < top);
        let end = self.tops.partition_point(|t| *t <= bottom);
        start..end.max(start)
    }

    /// From the highest any starts to the furthest any reaches; `None` with nothing in it.
    fn span(&self) -> Option<(Pixels, Pixels)> {
        Some((*self.tops.first()?, *self.bottoms.last()?))
    }
}

/// A source's blocks one after another from its top, kept while none of them is laid out
/// again: a draw places it where it goes in the view without copying what's in it, and finds
/// what's in view in it by binary search.
struct SourceLaid {
    /// The width and style its blocks were laid out at.
    width: Pixels,
    key: StyleKey,
    height: Pixels,
    leaves: Vec<Leaf>,
    decos: Vec<Deco>,
    /// Its code blocks' boxes, in order, with its key and their places among its code blocks.
    /// A display equation's has how its TeX is drawn.
    codes: Vec<(CodeBlock, Bounds<Pixels>, Option<Rc<MathLook>>)>,
    leaf_reach: Reach,
    deco_reach: Reach,
    code_reach: Reach,
    /// From the highest anything in it starts to the furthest anything reaches.
    span: (Pixels, Pixels),
}

impl SourceLaid {
    /// `k`'s blocks, each laid out at `width` in `key`, one after another with the gaps
    /// between them.
    fn new(k: &KeyState, width: Pixels, key: StyleKey, c: &Ctx) -> Self {
        let mut leaves: Vec<Leaf> = Vec::new();
        let mut decos = Vec::new();
        let mut codes = Vec::new();
        let mut y = px(0.);
        let mut above: Option<&Block> = None;
        for piece in &k.pieces {
            let Some(laid) = &piece.laid else { continue };
            if let Some(a) = above {
                y += gap(a, &piece.block, c);
            }
            for (j, leaf) in laid.leaves.iter().enumerate() {
                let mut leaf = leaf.clone();
                leaf.origin.y += y;
                // A block's first paragraph is a new one when copied after another's. (The
                // view's very first is never copied after anything.)
                if j == 0 && leaf.separator.is_empty() {
                    leaf.separator = "\n\n";
                }
                leaves.push(leaf);
                #[cfg(test)]
                tests::ASSEMBLED.with(|n| n.set(n.get() + 1));
            }
            for d in &laid.decos {
                let d = d.moved_down(y);
                if let Deco::Code {
                    bounds,
                    language,
                    code,
                    math,
                } = &d
                {
                    codes.push((
                        CodeBlock {
                            key: k.key,
                            index: codes.len(),
                            language: language.clone(),
                            code: code.clone(),
                        },
                        *bounds,
                        math.clone(),
                    ));
                }
                decos.push(d);
            }
            y += laid.height;
            above = Some(&piece.block);
        }
        // A paragraph's ink reaches past its box by up to a share of its line (as it's
        // clipped when painted).
        let leaf_reach = Reach::new(leaves.iter().map(|l| {
            let ink = l.layout.line_height * 0.3;
            (l.origin.y - ink, l.origin.y + l.layout.height + ink)
        }));
        let deco_reach = Reach::new(decos.iter().map(|d| {
            let b = d.bounds();
            (b.top(), b.bottom())
        }));
        let code_reach = Reach::new(codes.iter().map(|(_, b, _)| (b.top(), b.bottom())));
        let spans = [leaf_reach.span(), deco_reach.span(), code_reach.span()];
        let span = spans
            .into_iter()
            .flatten()
            .reduce(|(t0, b0), (t1, b1)| (t0.min(t1), b0.max(b1)))
            .unwrap_or_default();
        SourceLaid {
            width,
            key,
            height: y,
            leaves,
            decos,
            codes,
            leaf_reach,
            deco_reach,
            code_reach,
            span,
        }
    }
}

/// A source's blocks where they go in the view, and its first paragraph's place among the
/// view's.
struct Chunk {
    y: Pixels,
    first: usize,
    laid: Rc<SourceLaid>,
}

impl Chunk {
    /// Its decorations that could reach into `top..bottom`, from the view's top.
    fn decos_in(&self, top: Pixels, bottom: Pixels) -> &[Deco] {
        &self.laid.decos[self.laid.deco_reach.within(top - self.y, bottom - self.y)]
    }
}

/// The view laid out: each source's blocks where they go, one after another.
struct Assembled {
    width: Pixels,
    key: StyleKey,
    /// The sources' generation it was laid out from.
    generation: u64,
    height: Pixels,
    chunks: Vec<Chunk>,
    /// Where each chunk reaches, from the view's top.
    reach: Reach,
    /// How many paragraphs there are in all.
    leaf_count: usize,
    /// The last source's paragraphs, and where its first line starts (for a trailing caret
    /// when it has none) and that line's height.
    tail: Option<(std::ops::Range<usize>, Point<Pixels>, Pixels)>,
}

/// Everything layout needs from the theme, the window and the host's style.
struct Ctx<'a> {
    theme: &'a Theme,
    style: &'a TextViewStyle,
    base: ParagraphStyle,
    rem: Pixels,
    gap: Pixels,
}

fn abs(l: AbsoluteLength, rem: Pixels) -> Pixels {
    l.to_pixels(rem)
}

fn def(l: DefiniteLength, rem: Pixels) -> Pixels {
    match l {
        DefiniteLength::Absolute(a) => abs(a, rem),
        DefiniteLength::Fraction(_) => px(0.),
    }
}

impl Ctx<'_> {
    fn heading_size(&self, level: u8) -> Pixels {
        let base = self.style.heading_base_font_size.unwrap_or(self.base.size);
        match &self.style.heading_font_size {
            Some(f) => f(level, base),
            None => {
                base * match level {
                    1 => 1.6,
                    2 => 1.35,
                    3 => 1.15,
                    _ => 1.,
                }
            }
        }
    }

    /// A heading's text style.
    fn heading(&self, level: u8) -> ParagraphStyle {
        let size = self.heading_size(level);
        let mut s = self.with_size(size, true);
        if let Some(r) = self.style.heading_line_height {
            s.line_height = size * r;
        }
        s
    }

    /// The space above a heading and below it.
    fn heading_margins(&self, level: u8) -> (Pixels, Pixels) {
        match &self.style.heading_margins {
            Some(f) => f(level, self.heading_size(level)),
            None => (
                self.style.heading_margin_top.unwrap_or(self.gap),
                self.style.heading_margin_bottom.unwrap_or(self.gap),
            ),
        }
    }

    fn with_size(&self, size: Pixels, bold: bool) -> ParagraphStyle {
        let mut s = self.base.clone();
        s.line_height *= size / s.size;
        s.size = size;
        if bold {
            s.font.weight = FontWeight::SEMIBOLD;
        }
        s
    }

    /// The code block's text style and box, from the host's refinement.
    fn code(&self) -> (ParagraphStyle, Hsla, Hsla, Pixels, Pixels, [Pixels; 4]) {
        let r = &self.style.code_block;
        let mut s = self.base.clone();
        s.font.family = r
            .text
            .font_family
            .clone()
            .unwrap_or_else(|| self.theme.mono_font_family.clone());
        if let Some(size) = r.text.font_size {
            let px_ = size.to_pixels(self.rem);
            s.line_height *= px_ / s.size;
            s.size = px_;
        } else {
            s.size *= 0.9;
        }
        if let Some(lh) = r.text.line_height {
            s.line_height = lh.to_pixels(s.size.into(), self.rem);
        }
        if let Some(c) = r.text.color {
            s.color = c;
        }
        s.code_background = transparent_black();
        let fill_ = match &r.background {
            Some(Fill::Color(bg)) => bg.as_solid().unwrap_or(self.theme.muted),
            _ => self.theme.muted,
        };
        let border = r.border_color.unwrap_or(self.theme.border);
        let bw = r
            .border_widths
            .left
            .map(|w| abs(w, self.rem))
            .unwrap_or(px(0.));
        let radius = r
            .corner_radii
            .top_left
            .map(|w| abs(w, self.rem))
            .unwrap_or(self.theme.radius_md().to_pixels(self.rem));
        let p = &r.padding;
        let pad = [
            p.top.map(|l| def(l, self.rem)).unwrap_or(px(10.)),
            p.right.map(|l| def(l, self.rem)).unwrap_or(px(12.)),
            p.bottom.map(|l| def(l, self.rem)).unwrap_or(px(10.)),
            p.left.map(|l| def(l, self.rem)).unwrap_or(px(12.)),
        ];
        (s, fill_, border, bw, radius, pad)
    }
}

/// The space between two blocks one after the other: the host's for the pair, else the larger
/// of a heading's margin and the next's, else the paragraph gap.
fn gap(above: &Block, below: &Block, cx: &Ctx) -> Pixels {
    let margins = |b: &Block| match b {
        Block::Heading(level, _) => Some(cx.heading_margins(*level)),
        _ => None,
    };
    let host = cx
        .style
        .block_gap
        .as_ref()
        .and_then(|f| f(kind(above), kind(below)));
    host.unwrap_or_else(|| match (margins(above), margins(below)) {
        (Some((_, below)), Some((above, _))) => below.max(above),
        (Some((_, below)), None) => below,
        (None, Some((above, _))) => above,
        (None, None) => cx.gap,
    })
}

fn lay(
    blocks: &[Block],
    x: Pixels,
    width: Pixels,
    mut y: Pixels,
    cx: &Ctx,
    out: &mut ViewLayout,
    window: &mut Window,
) -> Pixels {
    let width = width.max(px(8.));
    for (i, b) in blocks.iter().enumerate() {
        if i > 0 {
            y += gap(&blocks[i - 1], b, cx);
        }
        let sep = if out.leaves.is_empty() { "" } else { "\n\n" };
        match b {
            Block::Paragraph(p) => y = leaf(p, &cx.base, x, width, y, sep, out, window),
            Block::Heading(level, p) => {
                let s = cx.heading(*level);
                y = leaf(p, &s, x, width, y, sep, out, window);
            }
            Block::Code { text, lang } => {
                y = lay_code(
                    text,
                    lang.as_deref(),
                    x,
                    width,
                    y,
                    sep,
                    cx,
                    out,
                    window,
                    None,
                );
            }
            Block::Quote(inner) => {
                let top = y;
                let bar = px(3.);
                // An indent never takes more than a quarter of the width it's taken from.
                let indent = (cx.rem * 0.9).min(width * 0.25);
                let deco_ix = out.decos.len();
                out.decos.push(Deco::Box {
                    bounds: Bounds::default(),
                    fill: cx.style.colors.quote_bar.unwrap_or(cx.theme.border),
                    border: transparent_black(),
                    border_width: px(0.),
                    radius: Corners::all(px(1.5)),
                });
                y = lay(inner, x + indent, width - indent, y, cx, out, window);
                out.decos[deco_ix] = Deco::Box {
                    bounds: Bounds::new(point(x, top), size(bar, y - top)),
                    fill: cx.style.colors.quote_bar.unwrap_or(cx.theme.border),
                    border: transparent_black(),
                    border_width: px(0.),
                    radius: Corners::all(px(1.5)),
                };
            }
            Block::List { start, items } => {
                y = lay_list(*start, items, x, width, y, cx, out, window, None);
            }
            Block::Rule => {
                let mid = y + cx.base.line_height * 0.5;
                out.decos.push(Deco::Box {
                    bounds: Bounds::new(point(x, mid), size(width, px(1.))),
                    fill: cx.style.colors.rule.unwrap_or(cx.theme.border),
                    border: transparent_black(),
                    border_width: px(0.),
                    radius: Corners::all(px(0.)),
                });
                y += cx.base.line_height;
            }
            Block::Table { align, rows } => {
                y = lay_table(align, rows, x, width, y, cx, out, window, None);
            }
        }
    }
    y
}

/// A table from `y`: its rows of cells, each cell a paragraph of its own (copied with a tab
/// between cells and a line end between rows). With `cache` (a top-level block's), a row laid
/// out before at its place, the same and at the same width and style, isn't laid out again.
#[allow(clippy::too_many_arguments)]
fn lay_table(
    align: &[Align],
    rows: &[Vec<Paragraph>],
    x: Pixels,
    width: Pixels,
    mut y: Pixels,
    cx: &Ctx,
    out: &mut ViewLayout,
    window: &mut Window,
    mut cache: Option<&mut Vec<RowLaid>>,
) -> Pixels {
    let cols = rows.iter().map(|r| r.len()).max().unwrap_or(0).max(1);
    let pad = cx.style.table_cell_padding.unwrap_or(cx.rem * 0.5);
    let col_w = width / cols as f32;
    let top = y;
    let radius = cx
        .style
        .table_radius
        .unwrap_or(cx.theme.radius_md().to_pixels(cx.rem));
    let inner = (col_w - pad * 2.).max(px(8.));
    let mut previous = cache.as_deref_mut().map(std::mem::take).unwrap_or_default();
    let mut kept_rows = Vec::new();
    for (ri, row) in rows.iter().enumerate() {
        // The row at this place before, if it's the same row laid out the same way.
        let cells: Vec<Paragraph> = (0..cols)
            .map(|c| row.get(c).cloned().unwrap_or_default())
            .collect();
        let kept = previous.get_mut(ri).map(std::mem::take).filter(|r| {
            r.header == (ri == 0)
                && r.width == inner
                && r.key == out.key
                && r.align.as_slice() == align
                && r.cells == cells
        });
        let mut laid = Vec::new();
        let row_top = y;
        let mut row_bottom = y;
        // The header's fill, or a striped body row's.
        let row_fill = match ri {
            0 => cx.style.colors.table_header,
            _ if ri % 2 == 0 => cx.style.colors.table_stripe,
            _ => None,
        };
        let header_ix = row_fill.is_some().then(|| {
            out.decos.push(Deco::Box {
                bounds: Bounds::default(),
                fill: transparent_black(),
                border: transparent_black(),
                border_width: px(0.),
                radius: Corners::all(px(0.)),
            });
            out.decos.len() - 1
        });
        let size_ = cx.style.table_text_size.unwrap_or(cx.base.size);
        let s = if ri == 0 {
            let mut s = cx.with_size(size_, true);
            if let Some(w) = cx.style.table_header_weight {
                s.font.weight = w;
            }
            if let Some(c) = cx.style.colors.table_header_text {
                s.color = c;
            }
            s
        } else {
            cx.with_size(size_, false)
        };
        for (c, p) in cells.iter().enumerate() {
            let p = p.clone();
            let cx0 = x + col_w * c as f32 + pad;
            let sep = if out.leaves.is_empty() {
                ""
            } else if c == 0 {
                "\n"
            } else {
                "\t"
            };
            // Centred and right-aligned columns lay their text out so.
            let mut cs = s.clone();
            cs.align = match align.get(c) {
                Some(Align::Center) => TextAlign::Center,
                Some(Align::Right) => TextAlign::Right,
                _ => TextAlign::Left,
            };
            // A cell's inline math typeset, as a paragraph's is (the same each time for the
            // same cell and style, so a kept layout is of this paragraph).
            let (tp, maths) = with_maths(&p, &cs);
            // Only a cell with math is a new paragraph; any other is `p` itself, moved below
            // rather than cloned each draw.
            let typeset = match tp {
                std::borrow::Cow::Owned(tp) => Some(tp),
                std::borrow::Cow::Borrowed(_) => None,
            };
            let l = match kept.as_ref().and_then(|k| k.laid.get(c)) {
                Some(l) => l.clone(),
                None => {
                    #[cfg(test)]
                    tests::CELLS_LAID.with(|k| k.set(k.get() + 1));
                    let tp = typeset.as_ref().unwrap_or(&p);
                    Rc::new(ParagraphLayout::new(tp, &cs, inner, window))
                }
            };
            let h = l.height;
            if kept.is_none() {
                laid.push(l.clone());
            }
            out.leaves.push(Leaf {
                origin: point(cx0, row_top + pad),
                layout: l,
                paragraph: Rc::new(typeset.unwrap_or(p)),
                separator: sep,
                maths: Rc::new(maths),
            });
            row_bottom = row_bottom.max(row_top + pad * 2. + h);
        }
        y = row_bottom;
        kept_rows.push(match kept {
            Some(r) => r,
            None => RowLaid {
                cells,
                header: ri == 0,
                align: align.to_vec(),
                width: inner,
                key: out.key.clone(),
                laid,
            },
        });
        if let (Some(ix), Some(fill_)) = (header_ix, row_fill) {
            // Inside the table's border, rounded where it meets its corners.
            let first = ri == 0;
            let last = ri + 1 == rows.len();
            let round = |on: bool| if on { radius } else { px(0.) };
            out.decos[ix] = Deco::Box {
                bounds: Bounds::new(point(x, row_top), size(width, y - row_top)),
                fill: fill_,
                border: transparent_black(),
                border_width: px(0.),
                radius: Corners {
                    top_left: round(first),
                    top_right: round(first),
                    bottom_left: round(last),
                    bottom_right: round(last),
                },
            };
        }
        // Lines between rows; the table's border is the last one's.
        if ri + 1 < rows.len() {
            out.decos.push(Deco::Box {
                bounds: Bounds::new(point(x, y), size(width, px(1.))),
                fill: cx.style.colors.table_line.unwrap_or(cx.theme.border),
                border: transparent_black(),
                border_width: px(0.),
                radius: Corners::all(px(0.)),
            });
        }
    }
    out.decos.push(Deco::Box {
        bounds: Bounds::new(point(x, top), size(width, y - top)),
        fill: transparent_black(),
        border: cx.style.colors.table_line.unwrap_or(cx.theme.border),
        border_width: px(1.),
        radius: Corners::all(radius),
    });
    if let Some(c) = cache {
        *c = kept_rows;
    }
    y
}

/// A fenced code block from `y`: its box, and its code a line at a time, each line a paragraph
/// of its own (copied with a line end between). With `cache` (a top-level block's), a line laid
/// out before at the same width and style is taken from it rather than laid out again.
#[allow(clippy::too_many_arguments)]
fn lay_code(
    text: &str,
    lang: Option<&str>,
    x: Pixels,
    width: Pixels,
    mut y: Pixels,
    sep: &'static str,
    cx: &Ctx,
    out: &mut ViewLayout,
    window: &mut Window,
    mut cache: Option<&mut Vec<LineLaid>>,
) -> Pixels {
    if lang == Some(MATH_LANGUAGE) {
        return lay_math(text, true, x, width, y, cx, out);
    }
    if lang == Some(MATH_SOURCE_LANGUAGE) {
        return lay_math(text, false, x, width, y, cx, out);
    }
    let (s, fill_, border, bw, radius, [pt, pr, pb, pl]) = cx.code();
    let top = y;
    let inner = (width - pl - pr - bw * 2.).max(px(8.));
    let deco_ix = out.decos.len();
    out.decos.push(Deco::Box {
        bounds: Bounds::default(),
        fill: fill_,
        border,
        border_width: bw,
        radius: Corners::all(radius),
    });
    let p = match (&cx.style.highlight, lang) {
        (Some(f), Some(lang)) => highlighted(text, &f(lang, text)),
        _ => Paragraph::plain(text.to_string()),
    };
    y += pt + bw;
    let mut kept = Vec::new();
    let mut old: Vec<Option<LineLaid>> = cache
        .as_deref_mut()
        .map(std::mem::take)
        .unwrap_or_default()
        .into_iter()
        .map(Some)
        .collect();
    for (i, line) in split_lines(&p).into_iter().enumerate() {
        // The line at its place first: a fence growing at its end keeps its lines there. Else,
        // with the lines laid out at this width and style, one that moved (a line put in
        // above it); at another width or style, none is looked for.
        let same = old
            .get(i)
            .and_then(Option::as_ref)
            .is_some_and(|l| probe() && l.width == inner && l.key == out.key);
        let cached = if !same {
            None
        } else if old[i].as_ref().is_some_and(|l| l.line == line) {
            old[i].take()
        } else {
            old.iter()
                .position(|l| probe() && l.as_ref().is_some_and(|l| l.line == line))
                .and_then(|at| old[at].take())
        };
        let laid = match cached {
            Some(l) => l,
            None => {
                #[cfg(test)]
                tests::LINES_LAID.with(|k| k.set(k.get() + 1));
                LineLaid {
                    laid: Rc::new(ParagraphLayout::new(&line, &s, inner, window)),
                    line,
                    width: inner,
                    key: out.key.clone(),
                }
            }
        };
        out.leaves.push(Leaf {
            origin: point(x + pl + bw, y),
            layout: laid.laid.clone(),
            paragraph: Rc::new(laid.line.clone()),
            // The code's lines join with a line end when copied.
            separator: if i == 0 { sep } else { "\n" },
            maths: Rc::default(),
        });
        y += laid.laid.height;
        kept.push(laid);
    }
    if let Some(c) = cache {
        *c = kept;
    }
    y += pb + bw;
    out.decos[deco_ix] = Deco::Box {
        bounds: Bounds::new(point(x, top), size(width, y - top)),
        fill: fill_,
        border,
        border_width: bw,
        radius: Corners::all(radius),
    };
    out.decos.push(Deco::Code {
        bounds: Bounds::new(point(x, top), size(width, y - top)),
        language: lang.map(|l| l.to_string().into()),
        // Without a trailing line end, whatever the parser kept.
        code: text.strip_suffix('\n').unwrap_or(text).to_string().into(),
        math: None,
    });
    y
}

/// A display equation from `y`: the code block's box, as tall as its TeX's lines, each kept on
/// one line. The TeX isn't a paragraph of the view's (it's drawn by an overlay that scrolls
/// sideways when it's wider than the box, so it never wraps mid-formula, clips or overlaps), so
/// it isn't selected with the text around it; its Copy button copies it.
fn lay_math(
    text: &str,
    typeset: bool,
    x: Pixels,
    width: Pixels,
    mut y: Pixels,
    cx: &Ctx,
    out: &mut ViewLayout,
) -> Pixels {
    // Nothing is shaped here (the overlay lays the TeX out as it's drawn), so a top-level
    // block's line cache, which `lay_code` takes, has nothing to keep.
    let (s, fill_, border, bw, radius, [pt, pr, pb, pl]) = cx.code();
    let top = y;
    let code = text.trim_end_matches('\n');
    let lines: Vec<SharedString> = code.split('\n').map(|l| l.to_string().into()).collect();
    // Typeset (with the `math` feature), at the size of the text around it; else its TeX.
    #[cfg(feature = "math")]
    let formula = typeset
        .then(|| super::math::typeset(code, true))
        .flatten()
        .map(|f| (f, cx.base.size * MATH_SCALE, cx.base.color));
    #[cfg(not(feature = "math"))]
    let _ = typeset;
    let source = s.line_height * lines.len() as f32;
    #[cfg(feature = "math")]
    let tall = formula
        .as_ref()
        .map_or(source, |(f, em, _)| f.ascent(*em) + f.descent(*em));
    #[cfg(not(feature = "math"))]
    let tall = source;
    y += pt + bw * 2. + pb + tall;
    let bounds = Bounds::new(point(x, top), size(width, y - top));
    out.decos.push(Deco::Box {
        bounds,
        fill: fill_,
        border,
        border_width: bw,
        radius: Corners::all(radius),
    });
    out.decos.push(Deco::Code {
        bounds,
        language: Some(MATH_LANGUAGE.into()),
        code: code.to_string().into(),
        math: Some(Rc::new(MathLook {
            lines,
            font: s.font.clone(),
            size: s.size,
            line_height: s.line_height,
            color: s.color,
            pad: [pt, pr, pb, pl],
            border: bw,
            #[cfg(feature = "math")]
            formula,
        })),
    });
    y
}

/// A display equation's TeX in its box: a line each, scrolling sideways (by a trackpad, a
/// wheel with Shift or a finger) when wider than the room it has; up and down go on to what's
/// around it. The host's overlay (`beside`), if any, is at the box's right at its own width, and
/// the TeX scrolls in what's left, so the overlay never covers it.
fn math_overlay(code: &CodeBlock, m: &MathLook, beside: Option<AnyElement>) -> AnyElement {
    let [pt, pr, pb, pl] = m.pad;
    let (box_, tex) = (
        format!("math:{}:{}", code.key, code.index),
        format!("math-tex:{}:{}", code.key, code.index),
    );
    let scroller = div()
        .id(ElementId::Name(
            format!("gpui-core-math:{}:{}", code.key, code.index).into(),
        ))
        .debug_selector(move || box_)
        .flex_1()
        .min_w_0()
        .h_full()
        .flex()
        .scroll_x()
        .font(m.font.clone())
        .text_size(m.size)
        .line_height(m.line_height)
        .text_color(m.color)
        .child(
            div()
                .debug_selector(move || tex)
                .flex_none()
                .flex()
                .flex_col()
                .pt(pt)
                .pr(pr)
                .pb(pb)
                .pl(pl)
                .map(|d| match math_drawing(m) {
                    // Typeset: the formula, at its size.
                    Some(drawn) => d.child(drawn),
                    // Its TeX, a line each.
                    None => d.children(m.lines.iter().map(|l| {
                        div()
                            .flex_none()
                            .h(m.line_height)
                            .whitespace_nowrap()
                            .child(l.clone())
                            .into_any_element()
                    })),
                }),
        );
    // Inside the box's border, so what scrolls out of view goes under it, not over it.
    div()
        .size_full()
        .p(m.border)
        .flex()
        .child(scroller)
        .children(beside.map(|el| div().flex_none().h_full().child(el)))
        .into_any_element()
}

/// A display equation typeset, as an element of its size that draws it.
#[cfg(feature = "math")]
fn math_drawing(m: &MathLook) -> Option<AnyElement> {
    let (f, em, color) = m.formula.clone()?;
    let (w, a, d) = (f.width(em), f.ascent(em), f.descent(em));
    Some(
        gpui::canvas(
            |_, _, _| {},
            move |b, _, window, cx| f.paint(point(b.left(), b.top() + a), em, color, window, cx),
        )
        .flex_none()
        .w(w)
        .h(a + d)
        .into_any_element(),
    )
}

#[cfg(not(feature = "math"))]
fn math_drawing(_: &MathLook) -> Option<AnyElement> {
    None
}

/// A cached line or item looked at, counted in tests: always true.
fn probe() -> bool {
    #[cfg(test)]
    tests::PROBES.with(|k| k.set(k.get() + 1));
    true
}

/// `p` cut at its line ends: a paragraph a line, each with its share of the spans.
fn split_lines(p: &Paragraph) -> Vec<Paragraph> {
    let mut out = Vec::new();
    let mut start = 0;
    for line in p.text.split('\n') {
        let end = start + line.len();
        let mut q = Paragraph::default();
        for (r, m) in &p.spans {
            let (a, b) = (r.start.max(start), r.end.min(end));
            if a < b {
                q.push(&p.text[a..b], m.clone());
            }
        }
        if q.text.len() < line.len() {
            // Spans don't cover it all (they always do; never mind).
            q = Paragraph::plain(line.to_string());
        }
        out.push(q);
        start = end + 1;
    }
    out
}

/// A list's items, one after another from `y`: each with its marker, its text set in. With
/// `cache` (a top-level list's), an item laid out before for the same place, width and style is
/// taken from it rather than laid out again.
#[allow(clippy::too_many_arguments)]
fn lay_list(
    start: Option<u32>,
    items: &[super::markdown::Item],
    x: Pixels,
    width: Pixels,
    mut y: Pixels,
    cx: &Ctx,
    out: &mut ViewLayout,
    window: &mut Window,
    mut cache: Option<&mut Vec<ItemLaid>>,
) -> Pixels {
    let width = width.max(px(8.));
    let indent = cx
        .style
        .list_indent
        .unwrap_or(cx.rem * if start.is_some() { 1.6 } else { 1.2 })
        .min(width * 0.25);
    let gap = cx.style.list_marker_gap.unwrap_or(indent * 0.3);
    let mut kept = Vec::new();
    let mut old: Vec<Option<ItemLaid>> = cache
        .as_deref_mut()
        .map(std::mem::take)
        .unwrap_or_default()
        .into_iter()
        .map(Some)
        .collect();
    for (n, item) in items.iter().enumerate() {
        if n > 0 {
            y += cx.style.list_item_gap.unwrap_or(cx.gap * 0.4);
        }
        // One item on its own, at the list's top left: from the cache, or laid out now.
        // The item at its place: its number is part of what it was laid out for, so an item
        // that moved is laid out again anyway.
        let cached = old.get_mut(n).and_then(|slot| {
            slot.take_if(|l| {
                probe()
                    && l.n == n
                    && l.start == start
                    && l.width == width
                    && l.key == out.key
                    && l.item == *item
            })
        });
        let laid = match cached {
            Some(l) => l,
            None => {
                let mut one = ViewLayout {
                    width,
                    key: out.key.clone(),
                    height: px(0.),
                    leaves: Vec::new(),
                    decos: Vec::new(),
                };
                let marker = match (start, item.checked) {
                    (_, Some(true)) => "☑".to_string(),
                    (_, Some(false)) => "☐".to_string(),
                    (Some(s), None) => format!("{}.", s as usize + n),
                    (None, None) => "•".to_string(),
                };
                let mut ms = cx.base.clone();
                ms.color = cx
                    .style
                    .colors
                    .list_marker
                    .unwrap_or(cx.theme.muted_foreground);
                let len = marker.len();
                let m = ParagraphLayout::new(&Paragraph::plain(marker), &ms, indent, window);
                // Its end sits the gap before the text; a marker too wide for that starts at
                // the list's edge.
                let marker_w = m.position_for(len).x;
                let mx = (indent - gap - marker_w).max(px(0.));
                one.decos.push(Deco::Text {
                    origin: point(x + mx, px(0.)),
                    layout: Rc::new(m),
                });
                one.height = lay(
                    &item.blocks,
                    x + indent,
                    width - indent,
                    px(0.),
                    cx,
                    &mut one,
                    window,
                );
                #[cfg(test)]
                tests::ITEMS_LAID.with(|k| k.set(k.get() + 1));
                ItemLaid {
                    item: item.clone(),
                    n,
                    start,
                    width,
                    key: out.key.clone(),
                    laid: Rc::new(one),
                }
            }
        };
        let before = out.leaves.len();
        for (j, leaf) in laid.laid.leaves.iter().enumerate() {
            let mut leaf = leaf.clone();
            leaf.origin.y += y;
            // An item's first paragraph goes on a new line when copied after the one above.
            if j == 0 && before > 0 {
                leaf.separator = "\n";
            }
            out.leaves.push(leaf);
        }
        out.decos
            .extend(laid.laid.decos.iter().map(|d| d.moved_down(y)));
        y += laid.laid.height;
        kept.push(laid);
    }
    if let Some(c) = cache {
        *c = kept;
    }
    y
}

/// `text` with `spans`' colours, weights and italics: those that are in order, apart and on
/// character edges (a host's highlighter isn't trusted to be).
fn highlighted(text: &str, spans: &super::Highlights) -> Paragraph {
    let mut p = Paragraph::default();
    let mut at = 0;
    for (r, style) in spans {
        let fits = r.start >= at
            && r.start < r.end
            && r.end <= text.len()
            && text.is_char_boundary(r.start)
            && text.is_char_boundary(r.end);
        if !fits {
            continue;
        }
        p.push(&text[at..r.start], Marks::default());
        p.push(
            &text[r.clone()],
            Marks {
                color: style.color,
                bold: style
                    .font_weight
                    .is_some_and(|w| w.0 >= FontWeight::SEMIBOLD.0),
                italic: style.font_style == Some(gpui::FontStyle::Italic),
                ..Marks::default()
            },
        );
        at = r.end;
    }
    p.push(&text[at..], Marks::default());
    p
}

#[allow(clippy::too_many_arguments)]
fn leaf(
    p: &Paragraph,
    s: &ParagraphStyle,
    x: Pixels,
    width: Pixels,
    y: Pixels,
    sep: &'static str,
    out: &mut ViewLayout,
    window: &mut Window,
) -> Pixels {
    let (p, maths) = with_maths(p, s);
    let l = ParagraphLayout::new(&p, s, width, window);
    let h = l.height;
    out.leaves.push(Leaf {
        origin: point(x, y),
        layout: Rc::new(l),
        paragraph: Rc::new(p.into_owned()),
        separator: sep,
        maths: Rc::new(maths),
    });
    y + h
}

/// `p` with its inline math typeset: each formula that can be is an inline box of its size
/// (its baseline on the line's), drawn by the view; the others stay their TeX in the code font.
#[cfg(feature = "math")]
fn with_maths<'a>(
    p: &'a Paragraph,
    s: &ParagraphStyle,
) -> (std::borrow::Cow<'a, Paragraph>, Vec<InlineMath>) {
    use super::paragraph::InlineBox;
    if !p.boxes.is_empty() || !p.spans.iter().any(|(_, m)| m.math) {
        return (std::borrow::Cow::Borrowed(p), Vec::new());
    }
    let em = s.size * MATH_SCALE;
    let mut out = Paragraph::default();
    let mut maths = Vec::new();
    for (r, m) in &p.spans {
        let text = &p.text[r.clone()];
        match m.math.then(|| super::math::typeset(text, false)).flatten() {
            Some(formula) => {
                let (a, d) = (formula.ascent(em), formula.descent(em));
                maths.push(InlineMath {
                    offset: out.text.len(),
                    tex: text.to_string().into(),
                    em,
                    color: m.color.unwrap_or(s.color),
                    formula: formula.clone(),
                });
                out.push_box(InlineBox {
                    width: formula.width(em),
                    height: a + d,
                    baseline: a,
                });
            }
            None => out.push(text, m.clone()),
        }
    }
    (std::borrow::Cow::Owned(out), maths)
}

#[cfg(not(feature = "math"))]
fn with_maths<'a>(
    p: &'a Paragraph,
    _: &ParagraphStyle,
) -> (std::borrow::Cow<'a, Paragraph>, Vec<InlineMath>) {
    (std::borrow::Cow::Borrowed(p), Vec::new())
}

/// What a layout depends on besides its width: the text's size, colour and family, and the rem.
/// What a layout depends on besides its width: the text's size, colour and family, the rem,
/// and the view's style.
type StyleKey = (Pixels, Hsla, SharedString, Pixels, u64);

/// `fingerprint` is the style's ([`TextViewStyle::fingerprint`]), worked out once per draw.
fn style_key(text: &gpui::TextStyle, rem: Pixels, fingerprint: u64) -> StyleKey {
    (
        text.font_size.to_pixels(rem),
        text.color,
        text.font_family.clone(),
        rem,
        fingerprint,
    )
}

/// Feeds what's formatted into it straight to a hasher, with nothing allocated.
struct HashWriter<'a>(&'a mut std::collections::hash_map::DefaultHasher);

impl std::fmt::Write for HashWriter<'_> {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        std::hash::Hasher::write(self.0, s.as_bytes());
        Ok(())
    }
}

impl TextViewStyle {
    /// Changes when anything a layout reads in the style does, so a host's new style (a theme
    /// switch, a new highlighter) lays the view out again. Functions count by which they are.
    /// Worked out once each time a view is drawn, the values formatted straight into the hash.
    fn fingerprint(&self) -> u64 {
        use std::fmt::Write as _;
        use std::hash::{Hash as _, Hasher as _};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        let _ = write!(
            HashWriter(&mut h),
            "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}",
            self.paragraph_gap,
            self.heading_base_font_size,
            self.code_block,
            self.list_indent,
            self.list_item_gap,
            self.list_marker_gap,
            self.heading_margin_top,
            self.heading_margin_bottom,
            self.heading_line_height,
            self.table_radius,
            self.table_cell_padding,
            self.table_text_size,
            self.table_header_weight,
            self.inline_code,
            self.colors,
            (),
        );
        let ptr = |p: Option<*const ()>| p.map(|p| p as usize);
        (
            ptr(self
                .heading_font_size
                .as_ref()
                .map(|f| Arc::as_ptr(f) as *const ())),
            ptr(self
                .heading_margins
                .as_ref()
                .map(|f| Arc::as_ptr(f) as *const ())),
            ptr(self.block_gap.as_ref().map(|f| Arc::as_ptr(f) as *const ())),
            ptr(self.highlight.as_ref().map(|f| Arc::as_ptr(f) as *const ())),
        )
            .hash(&mut h);
        h.finish()
    }
}

/// Lays the view out at `width` in `text`, the text style around it: each source's blocks
/// that aren't laid out at this width and style, then all of them one after another. That style
/// is taken where GPUI applies its parents' (while laying out or painting), not inside a
/// measure, which runs outside them.
#[allow(clippy::too_many_arguments)]
fn layout_for(
    state: &mut ViewState,
    style: &TextViewStyle,
    key: StyleKey,
    text: &gpui::TextStyle,
    width: Pixels,
    window: &mut Window,
    cx: &App,
) -> Assembled {
    let theme = cx.theme();
    let rem = window.rem_size();
    let size = text.font_size.to_pixels(rem);
    let base = ParagraphStyle {
        font: text.font(),
        size,
        line_height: text.line_height_in_pixels(rem),
        color: text.color,
        muted: style.colors.muted.unwrap_or(theme.muted_foreground),
        link: style.colors.link.unwrap_or(theme.foreground),
        code_family: style
            .inline_code
            .family
            .clone()
            .unwrap_or_else(|| theme.mono_font_family.clone()),
        code_background: style.inline_code.background.unwrap_or(theme.muted),
        align: TextAlign::Left,
        code: CodeStyle {
            size: style.inline_code.size.map(|r| size * r),
            padding: style.inline_code.padding.unwrap_or(px(0.)),
            padding_y: style.inline_code.padding_y,
            border_width: match (
                style.inline_code.border_width,
                style.inline_code.border_color,
            ) {
                (Some(w), Some(_)) => w,
                _ => px(0.),
            },
            border_color: style.inline_code.border_color.unwrap_or_default(),
            radius: style.inline_code.radius.unwrap_or(px(0.)),
            color: style.inline_code.color,
        },
    };
    let c = Ctx {
        theme,
        style,
        base,
        rem,
        gap: style.paragraph_gap.to_pixels(rem),
    };
    for k in &mut state.keys {
        // A source put together at this width and style has every block laid out at it.
        if k.laid
            .as_ref()
            .is_some_and(|l| l.width == width && l.key == key)
        {
            continue;
        }
        for piece in &mut k.pieces {
            if piece
                .laid
                .as_ref()
                .is_some_and(|l| l.width == width && l.key == key)
            {
                continue;
            }
            // Its source's blocks are put together again, with this one.
            k.laid = None;
            let mut out = ViewLayout {
                width,
                key: key.clone(),
                height: px(0.),
                leaves: Vec::new(),
                decos: Vec::new(),
            };
            out.height = match &piece.block {
                // A code block keeps its lines' layouts.
                Block::Code { text, lang } => lay_code(
                    text,
                    lang.as_deref(),
                    px(0.),
                    width,
                    px(0.),
                    "",
                    &c,
                    &mut out,
                    window,
                    Some(&mut piece.lines),
                ),
                // A table keeps its rows' layouts.
                Block::Table { align, rows } => lay_table(
                    align,
                    rows,
                    px(0.),
                    width,
                    px(0.),
                    &c,
                    &mut out,
                    window,
                    Some(&mut piece.rows),
                ),
                // A list keeps its items' layouts.
                Block::List { start, items } => lay_list(
                    *start,
                    items,
                    px(0.),
                    width,
                    px(0.),
                    &c,
                    &mut out,
                    window,
                    Some(&mut piece.items),
                ),
                block => lay(
                    std::slice::from_ref(block),
                    px(0.),
                    width,
                    px(0.),
                    &c,
                    &mut out,
                    window,
                ),
            };
            piece.laid = Some(Rc::new(out));
            #[cfg(test)]
            tests::LAID.with(|n| n.set(n.get() + 1));
        }
        // Its blocks one after another, unless they all were already.
        if k.laid.is_none() && !k.pieces.is_empty() {
            k.laid = Some(Rc::new(SourceLaid::new(k, width, key.clone(), &c)));
        }
    }
    // Then the sources one after another: a source's are placed, not copied, so what this
    // costs doesn't grow with how much the sources before the last one hold.
    let mut chunks = Vec::with_capacity(state.keys.len());
    let mut y = px(0.);
    let mut above: Option<&Block> = None;
    let mut leaf_count = 0;
    let mut tail = None;
    let last = state.keys.len().checked_sub(1);
    for (i, k) in state.keys.iter().enumerate() {
        let first_leaf = leaf_count;
        let mut top = None;
        if let (Some(laid), Some(first), Some(end)) = (&k.laid, k.pieces.first(), k.pieces.last()) {
            if let Some(a) = above {
                y += gap(a, &first.block, &c);
            }
            top = Some(y);
            chunks.push(Chunk {
                y,
                first: leaf_count,
                laid: laid.clone(),
            });
            leaf_count += laid.leaves.len();
            y += laid.height;
            above = Some(&end.block);
        }
        if Some(i) == last {
            // An empty last source starts where a paragraph after the rest would.
            let at = top.unwrap_or(if above.is_some() { y + c.gap } else { px(0.) });
            tail = Some((
                first_leaf..leaf_count,
                point(px(0.), at),
                c.base.line_height,
            ));
        }
    }
    let reach = Reach::new(
        chunks
            .iter()
            .map(|ch| (ch.y + ch.laid.span.0, ch.y + ch.laid.span.1)),
    );
    Assembled {
        width,
        key,
        generation: state.generation,
        height: y,
        chunks,
        reach,
        leaf_count,
        tail,
    }
}

impl RenderOnce for TextView {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::init_soon(cx);
        let state = window.use_keyed_state(self.id.clone(), cx, |_, cx| ViewState {
            keys: Vec::new(),
            generation: 0,
            focus: cx.focus_handle(),
            anchor: None,
            head: None,
            selecting: false,
            press: None,
            layout: None,
            bounds: Bounds::default(),
            width: None,
        });
        state.update(cx, |s, _| s.sync(&self.sources));
        let focus = state.read(cx).focus.clone();
        // The whole text, which names the document to assistive technology: worked out only
        // while one is listening, as it grows with the text.
        let plain: Option<SharedString> =
            window
                .is_a11y_active()
                .then(|| match self.sources.as_slice() {
                    [(_, one)] => one.clone(),
                    many => many
                        .iter()
                        .map(|(_, s)| s.as_ref())
                        .collect::<Vec<_>>()
                        .join("\n\n")
                        .into(),
                });
        let st = state.clone();
        let st2 = state.clone();
        let st3 = state.clone();
        div()
            .id(self.id.clone())
            .key_context(CONTEXT)
            .track_focus(&focus)
            .role(Role::Document)
            .when_some(plain, |d, plain| d.aria_label(plain))
            .w_full()
            .min_w_0()
            .on_action(move |_: &CopySelection, _, cx| {
                if let Some(t) = st.read(cx).selected_text() {
                    cx.write_to_clipboard(ClipboardItem::new_string(t));
                }
            })
            .on_action(move |_: &SelectAllText, _, cx| {
                st2.update(cx, |s, cx| {
                    if let Some(l) = &s.layout
                        && let Some(last) = l.leaf_count.checked_sub(1)
                        && let Some(leaf) = l.leaf(last)
                    {
                        s.anchor = Some((0, 0));
                        s.head = Some((last, leaf.paragraph.text.len()));
                        cx.notify();
                    }
                })
            })
            .on_action(move |_: &ClearSelection, _, cx| {
                st3.update(cx, |s, cx| {
                    s.anchor = None;
                    s.head = None;
                    cx.notify();
                })
            })
            .child(ViewElement {
                state,
                caret: self.caret,
                fingerprint: self.style.fingerprint(),
                style: self.style,
                selectable: self.selectable,
                on_link: self.on_link,
                code_overlay: self.code_overlay,
            })
    }
}

/// A draw's layout, and its hitboxes: the whole view's (its presses, and the I-beam while it's
/// selectable) and its links' (the pointing hand).
struct Prepainted {
    layout: Rc<Assembled>,
    body: gpui::Hitbox,
    links: Vec<gpui::Hitbox>,
    overlays: Vec<AnyElement>,
}

struct ViewElement {
    state: gpui::Entity<ViewState>,
    /// The trailing caret's colour, if it's drawn.
    caret: Option<Hsla>,
    style: TextViewStyle,
    /// The style's fingerprint, worked out once for this draw's measures and prepaint.
    fingerprint: u64,
    selectable: bool,
    on_link: Option<OnLink>,
    code_overlay: Option<CodeOverlay>,
}

impl IntoElement for ViewElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Leaf {
    /// Its text from `s` to `e` as copied: an inline formula as `$` its TeX `$`.
    fn copy(&self, s: usize, e: usize, out: &mut String) {
        let t = &self.paragraph.text;
        let mut at = s;
        for m in self.maths.iter().filter(|m| m.offset >= s && m.offset < e) {
            out.push_str(&t[at..m.offset]);
            out.push('$');
            out.push_str(&m.tex);
            out.push('$');
            at = m.offset + super::paragraph::BOX.len_utf8();
        }
        out.push_str(&t[at.min(e)..e]);
    }

    /// The paragraph's laid-out size.
    fn layout_size(&self) -> gpui::Size<Pixels> {
        size(self.layout.width, self.layout.height)
    }
}

impl Assembled {
    /// The paragraph at `i` among all the view's, where it is in the view.
    fn leaf(&self, i: usize) -> Option<Leaf> {
        let at = self
            .chunks
            .partition_point(|c| c.first <= i)
            .checked_sub(1)?;
        let c = &self.chunks[at];
        let mut leaf = c.laid.leaves.get(i - c.first)?.clone();
        leaf.origin.y += c.y;
        Some(leaf)
    }

    /// The TeX of the display equations whose boxes are between `top` and `bottom`, from the
    /// view's top, in order.
    fn math_between(&self, top: Pixels, bottom: Pixels) -> Vec<SharedString> {
        let mut out = Vec::new();
        for c in self.chunks_in(top, bottom) {
            for (code, b, math) in &c.laid.codes {
                if math.is_some() && c.y + b.top() >= top && c.y + b.bottom() <= bottom {
                    out.push(code.code.clone());
                }
            }
        }
        out
    }

    /// The sources' blocks that could reach into `top..bottom`, from the view's top.
    fn chunks_in(&self, top: Pixels, bottom: Pixels) -> &[Chunk] {
        &self.chunks[self.reach.within(top, bottom)]
    }

    /// The place nearest `p`, from the view's top left.
    fn pos_at(&self, p: Point<Pixels>) -> Option<Pos> {
        let mut best: Option<(Pixels, usize)> = None;
        for c in &self.chunks {
            for (j, l) in c.laid.leaves.iter().enumerate() {
                let top = c.y + l.origin.y;
                let bottom = top + l.layout.height;
                let dy = if p.y < top {
                    top - p.y
                } else if p.y > bottom {
                    p.y - bottom
                } else {
                    px(0.)
                };
                let left = l.origin.x;
                let right = left + l.layout.width;
                let dx = if p.x < left {
                    left - p.x
                } else if p.x > right {
                    p.x - right
                } else {
                    px(0.)
                };
                let d = dy * 4. + dx;
                if best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, c.first + j));
                }
            }
        }
        let (_, i) = best?;
        let l = self.leaf(i)?;
        Some((i, l.layout.index_for(p - l.origin)))
    }

    fn word_at(&self, (leaf, at): Pos) -> (Pos, Pos) {
        let Some(l) = self.leaf(leaf) else {
            return ((leaf, at), (leaf, at));
        };
        let t = &l.paragraph.text;
        for (i, w) in t.split_word_bound_indices() {
            if at >= i && at < i + w.len() {
                return ((leaf, i), (leaf, i + w.len()));
            }
        }
        ((leaf, at), (leaf, at))
    }
}

impl Element for ViewElement {
    type RequestLayoutState = ();
    type PrepaintState = Prepainted;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        _: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.min_size.width = Length::Definite(px(0.).into());
        let state = self.state.clone();
        let tv_style = self.style.clone();
        let fingerprint = self.fingerprint;
        // An empty view with a caret is a line tall, so the caret is inside it.
        let caret = self.caret.is_some();
        let text_style = window.text_style();
        let id = window.request_measured_layout(style, move |known, available, window, cx| {
            let definite = known.width.or(match available.width {
                AvailableSpace::Definite(w) => Some(w),
                _ => None,
            });
            // Without a definite width (a flex item asked its content size), the width it was
            // last drawn at. Laying it out at another width (400) would put every block
            // together at that width and then again at the drawn one: twice a frame, as long as
            // the text.
            let width = match definite {
                Some(w) => w,
                None => state.read(cx).width.unwrap_or(px(400.)),
            };
            // A flex layout also asks at no width at all (a definite 0). Laid out there, every
            // word on a line of its own, it would be put together again for each frame too: the
            // drawn layout's height answers it instead.
            if width <= px(1.)
                && let Some(l) = state.read(cx).layout.clone()
            {
                return size(width, l.height);
            }
            let key = style_key(&text_style, window.rem_size(), fingerprint);
            let cached = state.read(cx).layout.clone().filter(|l| {
                l.width == width && l.key == key && l.generation == state.read(cx).generation
            });
            let l = match cached {
                Some(l) => l,
                None => state.update(cx, |s, cx| {
                    let l = Rc::new(layout_for(
                        s,
                        &tv_style,
                        key,
                        &text_style,
                        width,
                        window,
                        cx,
                    ));
                    s.layout = Some(l.clone());
                    l
                }),
            };
            let height = if caret && l.leaf_count == 0 {
                l.height
                    .max(text_style.line_height_in_pixels(window.rem_size()))
            } else {
                l.height
            };
            size(width, height)
        });
        (id, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Prepainted {
        let width = bounds.size.width;
        self.state.update(cx, |s, _| s.width = Some(width));
        let text_style = window.text_style();
        let key = style_key(&text_style, window.rem_size(), self.fingerprint);
        let generation = self.state.read(cx).generation;
        let cached = self
            .state
            .read(cx)
            .layout
            .clone()
            .filter(|l| l.width == width && l.key == key && l.generation == generation);
        let l = match cached {
            Some(l) => l,
            None => self.state.update(cx, |s, cx| {
                Rc::new(layout_for(
                    s,
                    &self.style,
                    key,
                    &text_style,
                    width,
                    window,
                    cx,
                ))
            }),
        };
        self.state.update(cx, |s, _| {
            s.layout = Some(l.clone());
            s.bounds = bounds;
        });
        // The whole view: a press counts where it's hit, so not on something drawn over it (a
        // popover, a sheet) nor on a part scrolled out of its clip. Links go above it.
        let body = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        let seen = window.content_mask().bounds;
        // What's in view, from the view's top: only that is looked at.
        let (top, bottom) = (seen.top() - bounds.top(), seen.bottom() - bounds.top());
        let mut links = Vec::new();
        let mut overlays = Vec::new();
        for c in l.chunks_in(top, bottom) {
            let o = bounds.origin + point(px(0.), c.y);
            for leaf in &c.laid.leaves[c.laid.leaf_reach.within(top - c.y, bottom - c.y)] {
                #[cfg(test)]
                tests::VISITED.with(|n| n.set(n.get() + 1));
                let at = Bounds::new(o + leaf.origin, leaf.layout_size());
                if !at.intersects(&seen) {
                    continue;
                }
                for (r, m) in &leaf.paragraph.spans {
                    if m.link.is_some() {
                        for b in leaf.layout.bands(r.clone()) {
                            links.push(window.insert_hitbox(
                                Bounds::new(o + leaf.origin + b.origin, b.size),
                                HitboxBehavior::Normal,
                            ));
                        }
                    }
                }
            }
            for (code, b, math) in &c.laid.codes[c.laid.code_reach.within(top - c.y, bottom - c.y)]
            {
                let at = Bounds::new(o + b.origin, b.size);
                if !at.intersects(&seen) {
                    continue;
                }
                // A display equation's TeX, with the host's element beside it (never over it);
                // over any other code block, the host's element.
                let host = self.code_overlay.as_ref().map(|f| f(code, window, cx));
                let els: Vec<AnyElement> = match math {
                    Some(m) => vec![math_overlay(code, m, host)],
                    None => host.into_iter().collect(),
                };
                for mut el in els {
                    el.prepaint_as_root(
                        at.origin,
                        at.size.map(AvailableSpace::Definite),
                        window,
                        cx,
                    );
                    overlays.push(el);
                }
            }
        }
        Prepainted {
            layout: l,
            body,
            links,
            overlays,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        prepainted: &mut Prepainted,
        window: &mut Window,
        cx: &mut App,
    ) {
        let l = &prepainted.layout;
        let theme = cx.theme().clone();
        let o = bounds.origin;
        // Only what's in view is painted: a long message scrolled mostly away costs what shows.
        let seen = window.content_mask().bounds;
        let (focused, selection) = {
            let s = self.state.read(cx);
            (s.focus.is_focused(window), s.selection())
        };
        // What's in view, from the view's top, with room for ink past a paragraph's box: only
        // that is looked at.
        let ink_room = window.rem_size() * 0.2;
        let (top, bottom) = (seen.top() - o.y - ink_room, seen.bottom() - o.y + ink_room);
        let chunks = l.chunks_in(top, bottom);
        for c in chunks {
            let o = o + point(px(0.), c.y);
            for d in c.decos_in(top, bottom) {
                #[cfg(test)]
                tests::VISITED.with(|n| n.set(n.get() + 1));
                if let Deco::Box {
                    bounds: b,
                    fill: f,
                    border,
                    border_width,
                    radius,
                } = d
                    && Bounds::new(o + b.origin, b.size).intersects(&seen)
                {
                    window.paint_quad(quad(
                        Bounds::new(o + b.origin, b.size),
                        *radius,
                        *f,
                        *border_width,
                        *border,
                        Default::default(),
                    ));
                }
            }
        }
        if focused && let Some(((l0, o0), (l1, o1))) = selection {
            for c in chunks {
                let o = o + point(px(0.), c.y);
                let r = c.laid.leaf_reach.within(top - c.y, bottom - c.y);
                for (j, leaf) in c.laid.leaves.iter().enumerate().take(r.end).skip(r.start) {
                    let i = c.first + j;
                    if i < l0
                        || i > l1
                        || !Bounds::new(o + leaf.origin, leaf.layout_size()).intersects(&seen)
                    {
                        continue;
                    }
                    let len = leaf.paragraph.text.len();
                    let r = (if i == l0 { o0 } else { 0 })..(if i == l1 { o1 } else { len });
                    for b in leaf.layout.bands(r) {
                        window.paint_quad(fill(
                            Bounds::new(o + leaf.origin + b.origin, b.size),
                            self.style.colors.selection.unwrap_or(theme.selection()),
                        ));
                    }
                }
            }
        }
        for c in chunks {
            let o = o + point(px(0.), c.y);
            for d in c.decos_in(top, bottom) {
                if let Deco::Text { origin, layout } = d
                    && Bounds::new(o + *origin, size(layout.width, layout.height)).intersects(&seen)
                {
                    layout.paint(o + *origin, window, cx);
                }
            }
        }
        // Each paragraph is clipped to the width it was laid out in, with room all round for
        // ink past a glyph's advance and its line: a character wider than a narrow table cell
        // is cut there rather than drawn over its neighbour, while an inline code box taller
        // than its line (padded, bordered) keeps its top and bottom on the first and last lines.
        for c in chunks {
            let o = o + point(px(0.), c.y);
            let r = c.laid.leaf_reach.within(top - c.y, bottom - c.y);
            for leaf in &c.laid.leaves[r] {
                #[cfg(test)]
                tests::VISITED.with(|n| n.set(n.get() + 1));
                // Ink reaches further past the advance the bigger the text (a large emoji's
                // most), so the room is a share of the paragraph's line.
                let ink = ink_room.max(leaf.layout.line_height * 0.3);
                let clip = Bounds::new(
                    o + leaf.origin - point(ink, ink),
                    size(leaf.layout.width + ink * 2., leaf.layout.height + ink * 2.),
                );
                if !clip.intersects(&seen) {
                    continue;
                }
                #[cfg(test)]
                tests::PAINTED.with(|n| n.set(n.get() + 1));
                window.with_content_mask(Some(gpui::ContentMask { bounds: clip }), |window| {
                    leaf.layout.paint(o + leaf.origin, window, cx);
                    #[cfg(feature = "math")]
                    if !leaf.maths.is_empty() {
                        let boxes = leaf.layout.boxes();
                        for m in leaf.maths.iter() {
                            if let Some((_, b)) = boxes.iter().find(|(at, _)| *at == m.offset) {
                                let base = o + leaf.origin + b.origin;
                                m.formula.paint(
                                    point(base.x, base.y + m.formula.ascent(m.em)),
                                    m.em,
                                    m.color,
                                    window,
                                    cx,
                                );
                            }
                        }
                    }
                });
            }
        }

        // The trailing caret: after the last source's last character; with that source empty,
        // after the last character before it; with no text at all, at the start. Drawn over the
        // text, never laid out (an empty view is a line tall while it has one).
        if let (Some(color), Some((leaves, _, line))) = (self.caret, &l.tail) {
            let last = (!leaves.is_empty())
                .then(|| l.leaf(leaves.end - 1))
                .flatten()
                .or_else(|| l.leaf(l.leaf_count.checked_sub(1)?));
            let caret = match last {
                Some(leaf) => {
                    let c = leaf
                        .layout
                        .caret(leaf.paragraph.text.len(), Affinity::Downstream);
                    Bounds::new(o + leaf.origin + c.origin, c.size)
                }
                None => Bounds::new(o, size(px(2.), *line)),
            };
            if caret.intersects(&seen) {
                window.paint_quad(fill(caret, color));
            }
        }

        if self.selectable {
            window.set_cursor_style(CursorStyle::IBeam, &prepainted.body);
        }
        for hb in &prepainted.links {
            window.set_cursor_style(CursorStyle::PointingHand, hb);
        }

        let layout = l.clone();
        let state = self.state.clone();
        let selectable = self.selectable;
        let on_link = self.on_link.clone();
        window.on_mouse_event({
            let layout = layout.clone();
            let state = state.clone();
            let body = prepainted.body.clone();
            move |e: &MouseDownEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble
                    || e.button != MouseButton::Left
                    || !body.is_hovered(window)
                {
                    return;
                }
                let Some(pos) = layout.pos_at(e.position - bounds.origin) else {
                    return;
                };
                let focus = state.read(cx).focus.clone();
                if selectable {
                    window.focus(&focus, cx);
                }
                state.update(cx, |s, cx| {
                    s.press = Some((e.position, pos));
                    if selectable {
                        let (a, h) = match e.click_count {
                            2 => layout.word_at(pos),
                            n if n >= 3 => (
                                (pos.0, 0),
                                (
                                    pos.0,
                                    layout.leaf(pos.0).map_or(0, |l| l.paragraph.text.len()),
                                ),
                            ),
                            _ if e.modifiers.shift && s.anchor.is_some() => {
                                (s.anchor.unwrap_or(pos), pos)
                            }
                            _ => (pos, pos),
                        };
                        s.anchor = Some(a);
                        s.head = Some(h);
                        s.selecting = true;
                    }
                    cx.notify();
                });
            }
        });
        if state.read(cx).selecting || state.read(cx).press.is_some() {
            window.on_mouse_event({
                let layout = layout.clone();
                let state = state.clone();
                move |e: &MouseMoveEvent, phase, _, cx| {
                    if phase != DispatchPhase::Bubble || e.pressed_button != Some(MouseButton::Left)
                    {
                        return;
                    }
                    state.update(cx, |s, cx| {
                        if let Some((at, _)) = s.press
                            && (e.position - at).magnitude() > 3.
                        {
                            s.press = None;
                        }
                        if s.selecting
                            && let Some(pos) = layout.pos_at(e.position - bounds.origin)
                        {
                            s.head = Some(pos);
                            cx.notify();
                        }
                    });
                }
            });
            window.on_mouse_event(move |e: &MouseUpEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble {
                    return;
                }
                let press = state.update(cx, |s, cx| {
                    s.selecting = false;
                    cx.notify();
                    s.press.take()
                });
                // A press that didn't move, on a link: the host's to open.
                if let (Some((_, (leaf, at))), Some(f)) = (press, &on_link)
                    && let Some(url) = layout
                        .leaf(leaf)
                        .and_then(|l| l.paragraph.link_at(at).cloned())
                {
                    let collapsed = state.read(cx).selection().is_none();
                    if collapsed || !selectable {
                        f(&url, &e.modifiers, window, cx);
                    }
                }
            });
        }
        // Over the text, and after the view's listeners, so theirs run first.
        for el in &mut prepainted.overlays {
            el.paint(window, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use gpui::{
        AppContext as _, Context, InteractiveElement as _, IntoElement, ParentElement as _, Render,
        StatefulInteractiveElement as _, Styled as _, TestAppContext, Window, div, px,
    };

    use crate::{Root, TextView};

    thread_local! {
        /// How many paragraphs have been painted.
        pub(super) static PAINTED: Cell<usize> = const { Cell::new(0) };
        /// How many paragraphs and decorations a draw has looked at to find those in view.
        pub(super) static VISITED: Cell<usize> = const { Cell::new(0) };
        /// How many paragraphs have been placed in their source's blocks put together.
        pub(super) static ASSEMBLED: Cell<usize> = const { Cell::new(0) };
        /// How many top-level blocks have been laid out.
        pub(super) static LAID: Cell<usize> = const { Cell::new(0) };
        /// How many list items have been laid out.
        pub(super) static ITEMS_LAID: Cell<usize> = const { Cell::new(0) };
        /// How many code lines have been laid out.
        pub(super) static LINES_LAID: Cell<usize> = const { Cell::new(0) };
        /// How many table cells have been laid out.
        pub(super) static CELLS_LAID: Cell<usize> = const { Cell::new(0) };
        /// How many cached lines or items have been looked at.
        pub(super) static PROBES: Cell<usize> = const { Cell::new(0) };
    }

    struct Streamed {
        sources: Vec<(u64, gpui::SharedString)>,
        width: f32,
    }

    impl Render for Streamed {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .w(px(self.width))
                .child(TextView::blocks("streamed", self.sources.clone()))
        }
    }

    #[gpui::test]
    fn a_growing_code_block_lays_out_only_its_new_or_changed_lines(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let fence = |n: usize, last: &str| -> gpui::SharedString {
            let mut s = String::from("```\n");
            for i in 0..n {
                s.push_str(&format!("let line_{i} = {i};\n"));
            }
            s.push_str(last);
            s.into()
        };
        let sources = vec![(0u64, "Code:".into()), (1u64, fence(300, "let la"))];
        let made = std::rc::Rc::new(std::cell::RefCell::new(None));
        let m = made.clone();
        let (_, vcx) = cx.add_window_view(move |window, cx| {
            let v = cx.new(|_| Streamed {
                sources,
                width: 400.,
            });
            *m.borrow_mut() = Some(v.clone());
            Root::new(v, window, cx)
        });
        vcx.run_until_parked();
        let v = made.borrow().clone().unwrap();
        let laid = |vcx: &mut gpui::VisualTestContext, f: &dyn Fn(&mut Streamed)| {
            LINES_LAID.with(|n| n.set(0));
            PROBES.with(|n| n.set(0));
            vcx.update(|_, cx| {
                v.update(cx, |s, cx| {
                    f(s);
                    cx.notify();
                })
            });
            vcx.run_until_parked();
            (LINES_LAID.with(|n| n.get()), PROBES.with(|n| n.get()))
        };
        // Each delta looks at about one cached line a line, not at every line for each one.
        let few = |(laid, probes): (usize, usize)| {
            assert!(probes <= 3 * 302, "{probes} cached lines looked at");
            laid
        };
        // The last line grows: it alone.
        assert_eq!(
            few(laid(vcx, &|s| s.sources[1].1 = fence(300, "let last"))),
            1
        );
        // A new line: it (the line before it is unchanged).
        assert_eq!(
            few(laid(vcx, &|s| s.sources[1].1 =
                format!("{}\nlet more", fence(300, "let last")).into())),
            1
        );
        // A new width: every line.
        assert_eq!(few(laid(vcx, &|s| s.width = 380.)), 302);
    }

    /// A display equation wider than the view (a phone's 402 points) stays on its line, in a
    /// box no wider than the view, and scrolls sideways under a trackpad's swipe.
    #[gpui::test]
    fn a_wide_display_equation_scrolls_sideways_in_its_box(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let long = "f(x) = a_0 + a_1 x + a_2 x^2 + a_3 x^3 + a_4 x^4 + a_5 x^5 + a_6 x^6 + a_7 x^7 + a_8 x^8 + a_9 x^9 + a_{10} x^{10}";
        let sources = vec![(0u64, format!("Before.\n\n$${long}$$\n\nAfter.").into())];
        let (_, vcx) = cx.add_window_view(move |window, cx| {
            let v = cx.new(|_| Streamed {
                sources,
                width: 402.,
            });
            Root::new(v, window, cx)
        });
        vcx.run_until_parked();
        vcx.update(|window, cx| window.draw(cx).clear(cx));
        let b = vcx.debug_bounds("math:0:0").expect("the equation's box");
        let tex = vcx.debug_bounds("math-tex:0:0").expect("its TeX");
        assert!(b.size.width <= px(402.), "{b:?}");
        assert!(
            tex.size.width > b.size.width,
            "one line, wider than the box: {tex:?}"
        );
        assert!(b.size.height < px(60.), "a line tall, not wrapped: {b:?}");
        // A swipe to the left over it scrolls it, and only it.
        vcx.simulate_event(gpui::ScrollEvent {
            position: b.center(),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(-120.), px(0.))),
            ..Default::default()
        });
        vcx.update(|window, cx| window.draw(cx).clear(cx));
        let moved = vcx.debug_bounds("math-tex:0:0").unwrap();
        assert!(moved.left() < tex.left(), "{tex:?} then {moved:?}");
        assert_eq!(vcx.debug_bounds("math:0:0").unwrap(), b);
    }

    /// The host's overlay on a display equation (a Copy button) is beside its TeX, not over it:
    /// the TeX's room ends where the button starts.
    #[gpui::test]
    fn a_display_equations_overlay_is_beside_its_tex(cx: &mut TestAppContext) {
        cx.update(crate::init);
        struct WithCopy;
        impl Render for WithCopy {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div().w(px(402.)).child(
                    TextView::markdown(
                        "with-copy",
                        "$$f(x) = a_0 + a_1 x + a_2 x^2 + a_3 x^3 + a_4 x^4 + a_5 x^5 + a_6 x^6$$",
                    )
                    .code_block_overlay(|_, _, _| {
                        div()
                            .debug_selector(|| "copy".into())
                            .w(px(70.))
                            .h(px(26.))
                            .into_any_element()
                    }),
                )
            }
        }
        let (_, vcx) = cx.add_window_view(|window, cx| {
            let v = cx.new(|_| WithCopy);
            Root::new(v, window, cx)
        });
        vcx.run_until_parked();
        vcx.update(|window, cx| window.draw(cx).clear(cx));
        let tex = vcx.debug_bounds("math:0:0").expect("the TeX's room");
        let copy = vcx.debug_bounds("copy").expect("the button");
        assert!(tex.right() <= copy.left(), "{tex:?} and {copy:?}");
        assert!(copy.right() <= px(402.), "{copy:?}");
    }

    /// Inline math typeset (the `math` feature): its formula is an inline box in its paragraph,
    /// drawn by the view, and a selection across it copies its TeX between dollars.
    #[cfg(feature = "math")]
    #[gpui::test]
    fn inline_math_is_typeset_in_its_line_and_copies_as_its_tex(cx: &mut TestAppContext) {
        cx.update(crate::init);
        struct Doc;
        impl Render for Doc {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div().w(px(402.)).child(
                    TextView::markdown("inline", "A Jacobian $J_{ij}$ here.").selectable(true),
                )
            }
        }
        let (_, vcx) = cx.add_window_view(|window, cx| {
            let v = cx.new(|_| Doc);
            Root::new(v, window, cx)
        });
        vcx.run_until_parked();
        vcx.update(|window, cx| window.draw(cx).clear(cx));
        // A press in the text gives it the keyboard; then Select All and Copy.
        vcx.simulate_click(gpui::point(px(4.), px(6.)), gpui::Modifiers::none());
        vcx.dispatch_action(super::SelectAllText);
        vcx.dispatch_action(super::CopySelection);
        let copied = vcx.update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()));
        assert_eq!(copied.as_deref(), Some("A Jacobian $J_{ij}$ here."));
    }

    /// A display equation typeset is as tall as its formula: a fraction's two levels, not a line
    /// of its TeX.
    #[cfg(feature = "math")]
    #[gpui::test]
    fn a_typeset_display_equation_is_as_tall_as_its_formula(cx: &mut TestAppContext) {
        cx.update(crate::init);
        struct Doc(&'static str);
        impl Render for Doc {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div()
                    .w(px(402.))
                    .child(TextView::markdown("display", self.0))
            }
        }
        let height = |cx: &mut TestAppContext, source: &'static str| {
            let (_, vcx) = cx.add_window_view(move |window, cx| {
                let v = cx.new(|_| Doc(source));
                Root::new(v, window, cx)
            });
            vcx.run_until_parked();
            vcx.update(|window, cx| window.draw(cx).clear(cx));
            vcx.debug_bounds("math:0:0").expect("the box").size.height
        };
        let fraction = height(cx, r"$$\frac{\partial f_i}{\partial x_j}$$");
        let flat = height(cx, r"$$a + b$$");
        let source = height(
            cx,
            "```math-source\n\\frac{\\partial f_i}{\\partial x_j}\n```",
        );
        assert!(fraction > flat, "{fraction:?} {flat:?}");
        assert!(
            fraction != source,
            "typeset, not its TeX: {fraction:?} {source:?}"
        );
    }

    /// Inline math in a table's cell is typeset as in a paragraph: its formula an inline box
    /// in the cell, copied as its TeX between dollars.
    #[cfg(feature = "math")]
    #[gpui::test]
    fn inline_math_in_a_table_cell_is_typeset(cx: &mut TestAppContext) {
        cx.update(crate::init);
        struct Doc;
        impl Render for Doc {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div().w(px(402.)).child(
                    TextView::markdown(
                        "cells",
                        "| Rate | Formula |\n|---|---|\n| Decay | $e^{-kt}$ |",
                    )
                    .selectable(true),
                )
            }
        }
        let (_, vcx) = cx.add_window_view(|window, cx| {
            let v = cx.new(|_| Doc);
            Root::new(v, window, cx)
        });
        vcx.run_until_parked();
        vcx.update(|window, cx| window.draw(cx).clear(cx));
        vcx.simulate_click(gpui::point(px(4.), px(6.)), gpui::Modifiers::none());
        vcx.dispatch_action(super::SelectAllText);
        vcx.dispatch_action(super::CopySelection);
        let copied = vcx.update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()));
        let copied = copied.unwrap_or_default();
        assert!(
            copied.contains("$e^{-kt}$"),
            "typeset, copied as its TeX: {copied:?}"
        );
    }

    /// A selection dragged across a paragraph, a display equation and a paragraph copies the
    /// equation's TeX between them.
    #[gpui::test]
    fn a_selection_across_a_display_equation_copies_its_tex(cx: &mut TestAppContext) {
        cx.update(crate::init);
        struct Doc;
        impl Render for Doc {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
                div().w(px(402.)).child(
                    TextView::markdown(
                        "copied",
                        "The first words.\n\n$$\\frac{a}{b} \\\\ c$$\n\nThe last words.",
                    )
                    .selectable(true),
                )
            }
        }
        let (_, vcx) = cx.add_window_view(|window, cx| {
            let v = cx.new(|_| Doc);
            Root::new(v, window, cx)
        });
        vcx.run_until_parked();
        vcx.update(|window, cx| window.draw(cx).clear(cx));
        let b = vcx.debug_bounds("math:0:0").expect("the equation");
        let (from, to) = (
            gpui::point(px(1.), b.top() - px(12.)),
            gpui::point(px(390.), b.bottom() + px(12.)),
        );
        vcx.simulate_mouse_down(from, gpui::MouseButton::Left, gpui::Modifiers::none());
        vcx.simulate_mouse_move(to, gpui::MouseButton::Left, gpui::Modifiers::none());
        vcx.simulate_mouse_up(to, gpui::MouseButton::Left, gpui::Modifiers::none());
        vcx.dispatch_action(super::CopySelection);
        let copied = vcx.update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()));
        assert_eq!(
            copied.as_deref(),
            Some("The first words.\n\n$$\\frac{a}{b} \\\\ c$$\n\nThe last words.")
        );
    }

    #[gpui::test]
    fn a_growing_table_lays_out_only_its_new_or_changed_rows(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let table = |n: usize, last: &str| -> gpui::SharedString {
            let mut s = String::from("| Name | Value |\n| --- | ---: |\n");
            for i in 0..n {
                s.push_str(&format!("| row {i} | {i} |\n"));
            }
            s.push_str(last);
            s.into()
        };
        let sources = vec![(0u64, "A table:".into()), (1u64, table(300, "| last | 1"))];
        let made = std::rc::Rc::new(std::cell::RefCell::new(None));
        let m = made.clone();
        let (_, vcx) = cx.add_window_view(move |window, cx| {
            let v = cx.new(|_| Streamed {
                sources,
                width: 400.,
            });
            *m.borrow_mut() = Some(v.clone());
            Root::new(v, window, cx)
        });
        vcx.run_until_parked();
        let v = made.borrow().clone().unwrap();
        let laid = |vcx: &mut gpui::VisualTestContext, f: &dyn Fn(&mut Streamed)| {
            CELLS_LAID.with(|n| n.set(0));
            vcx.update(|_, cx| {
                v.update(cx, |s, cx| {
                    f(s);
                    cx.notify();
                })
            });
            vcx.run_until_parked();
            CELLS_LAID.with(|n| n.get())
        };
        // The last row grows: its two cells.
        assert_eq!(
            laid(vcx, &|s| s.sources[1].1 = table(300, "| last | 12")),
            2
        );
        // A new row: its cells (the row before it is unchanged).
        assert_eq!(
            laid(vcx, &|s| s.sources[1].1 =
                format!("{}\n| more | 3 |", table(300, "| last | 12 |"))
                    .into()),
            2
        );
        // A new width: every cell, the header's too.
        assert_eq!(laid(vcx, &|s| s.width = 380.), 2 * 303);
    }

    #[gpui::test]
    fn a_growing_list_lays_out_only_its_new_or_changed_items(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let list = |n: usize, last: &str| -> gpui::SharedString {
            let mut s: String = (0..n).map(|i| format!("- item number {i}\n")).collect();
            s.push_str(&format!("- {last}"));
            s.into()
        };
        let sources = vec![(0u64, "A reply:".into()), (1u64, list(300, "the last"))];
        let made = std::rc::Rc::new(std::cell::RefCell::new(None));
        let m = made.clone();
        let (_, vcx) = cx.add_window_view(move |window, cx| {
            let v = cx.new(|_| Streamed {
                sources,
                width: 300.,
            });
            *m.borrow_mut() = Some(v.clone());
            Root::new(v, window, cx)
        });
        vcx.run_until_parked();
        let v = made.borrow().clone().unwrap();
        let laid = |vcx: &mut gpui::VisualTestContext, f: &dyn Fn(&mut Streamed)| {
            ITEMS_LAID.with(|n| n.set(0));
            PROBES.with(|n| n.set(0));
            vcx.update(|_, cx| {
                v.update(cx, |s, cx| {
                    f(s);
                    cx.notify();
                })
            });
            vcx.run_until_parked();
            (ITEMS_LAID.with(|n| n.get()), PROBES.with(|n| n.get()))
        };
        // Each delta looks at about one cached item an item, not at every item for each one.
        let few = |(laid, probes): (usize, usize)| {
            assert!(probes <= 3 * 302, "{probes} cached items looked at");
            laid
        };
        // The last item grows: it alone.
        assert_eq!(
            few(laid(vcx, &|s| s.sources[1].1 = list(300, "the last one"))),
            1
        );
        // A new item: it alone.
        assert_eq!(
            few(laid(vcx, &|s| {
                s.sources[1].1 = format!("{}\n- and another", list(300, "the last one")).into()
            })),
            1
        );
        // A new width: every item.
        assert_eq!(few(laid(vcx, &|s| s.width = 280.)), 302);
    }

    #[gpui::test]
    fn a_change_to_the_last_block_lays_out_only_that_block(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let mut sources: Vec<(u64, gpui::SharedString)> = (0..30)
            .map(|i| (i, format!("Settled paragraph {i}.").into()))
            .collect();
        sources.push((
            30,
            "The tail's first.\n\nIts second.\n\nIts third, grow".into(),
        ));
        let made = std::rc::Rc::new(std::cell::RefCell::new(None));
        let m = made.clone();
        let (_, vcx) = cx.add_window_view(move |window, cx| {
            let v = cx.new(|_| Streamed {
                sources,
                width: 300.,
            });
            *m.borrow_mut() = Some(v.clone());
            Root::new(v, window, cx)
        });
        vcx.run_until_parked();
        let v = made.borrow().clone().unwrap();
        let laid = |vcx: &mut gpui::VisualTestContext, f: &dyn Fn(&mut Streamed)| {
            LAID.with(|n| n.set(0));
            vcx.update(|_, cx| {
                v.update(cx, |s, cx| {
                    f(s);
                    cx.notify();
                })
            });
            vcx.run_until_parked();
            LAID.with(|n| n.get())
        };
        // The tail's last paragraph grows: that block alone.
        let n = laid(vcx, &|s| {
            s.sources[30].1 = "The tail's first.\n\nIts second.\n\nIts third, growing".into()
        });
        assert_eq!(n, 1);
        // A new block in the tail: that one.
        let n = laid(vcx, &|s| {
            s.sources[30].1 =
                "The tail's first.\n\nIts second.\n\nIts third, growing\n\nA fourth".into()
        });
        assert_eq!(n, 1);
        // A new source after it: its block.
        let n = laid(vcx, &|s| s.sources.push((31, "Another.".into())));
        assert_eq!(n, 1);
        // Nothing changed: nothing.
        assert_eq!(laid(vcx, &|_| {}), 0);
        // A new width: every block.
        assert_eq!(laid(vcx, &|s| s.width = 250.), 35);
    }

    /// A reply's Markdown of about `lines` lines (headings, paragraphs with links, lists and
    /// code fences), a block a source, as gpui-agents' `StreamingMarkdown` hands settled
    /// blocks over.
    fn reply(lines: usize) -> Vec<(u64, gpui::SharedString)> {
        let mut out = Vec::new();
        let mut n = 0;
        while n < lines {
            let i = out.len();
            let block = match i % 4 {
                0 => format!("## Section {i}"),
                1 => format!(
                    "Paragraph {i} says what it's about, with a [link](https://x.test/{i}) in \
                     it,\nand a second line after that one."
                ),
                2 => (0..3)
                    .map(|j| format!("- item {j} of list {i}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
                _ => format!(
                    "```rust\n{}\n```",
                    (0..5)
                        .map(|j| format!("let v{j} = {i} + {j};"))
                        .collect::<Vec<_>>()
                        .join("\n")
                ),
            };
            // Its lines and the blank line after it.
            n += block.lines().count() + 1;
            out.push((i as u64, block.into()));
        }
        out
    }

    /// A window drawing a reply about `lines` long with an open block after it: its view, and
    /// a context for it.
    fn open_reply(
        lines: usize,
        cx: &mut TestAppContext,
    ) -> (gpui::Entity<Streamed>, gpui::VisualTestContext) {
        let mut sources = reply(lines);
        sources.push((u64::MAX, "The open block, growing".into()));
        let made = std::rc::Rc::new(std::cell::RefCell::new(None));
        let m = made.clone();
        let (_, vcx) = cx.add_window_view(move |window, cx| {
            let v = cx.new(|_| Streamed {
                sources,
                width: 600.,
            });
            *m.borrow_mut() = Some(v.clone());
            Root::new(v, window, cx)
        });
        vcx.run_until_parked();
        let window = gpui::VisualContext::window_handle(vcx);
        let v = made.borrow().clone().unwrap();
        (v, gpui::VisualTestContext::from_window(window, cx))
    }

    /// The time of a delta: the open block grows by a word, and the frame it causes is laid out
    /// and painted.
    fn delta(v: &gpui::Entity<Streamed>, vcx: &mut gpui::VisualTestContext) -> std::time::Duration {
        let start = std::time::Instant::now();
        vcx.update(|_, cx| {
            v.update(cx, |s, cx| {
                // A word on, or off again: the open block stays short, so what's timed is
                // mostly what the blocks before it cost.
                let open = &mut s.sources.last_mut().unwrap().1;
                *open = match open.strip_suffix(" word") {
                    Some(shorter) => shorter.to_string().into(),
                    None => format!("{open} word").into(),
                };
                cx.notify();
            })
        });
        vcx.run_until_parked();
        start.elapsed()
    }

    struct Clipped(Vec<(u64, gpui::SharedString)>);

    impl Render for Clipped {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .w(px(600.))
                .h(px(300.))
                .overflow_hidden()
                .child(TextView::blocks("clipped", self.0.clone()))
        }
    }

    /// A reply as a flex item, as a chat draws one: in a column whose width is its content's,
    /// in a row. Each frame it's measured for its content size, with no definite width, and
    /// laid out at the row's width.
    struct InARow(Vec<(u64, gpui::SharedString)>);

    impl Render for InARow {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .w(px(640.))
                .h(px(300.))
                .overflow_hidden()
                .flex()
                .flex_row()
                .child(
                    div().flex().flex_col().child(
                        div()
                            .w_full()
                            .min_w_0()
                            .child(TextView::blocks("in-a-row", self.0.clone())),
                    ),
                )
        }
    }

    /// As a flex item, a delta still puts together only the open block: the measure for its
    /// content size is answered at the width it's drawn at, not laid out again at another.
    #[gpui::test]
    fn as_a_flex_item_a_delta_puts_together_only_the_open_block(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let mut sources = reply(300);
        sources.push((u64::MAX, "The open block".into()));
        let made = std::rc::Rc::new(std::cell::RefCell::new(None));
        let m = made.clone();
        let (_, vcx) = cx.add_window_view(move |window, cx| {
            let v = cx.new(|_| InARow(sources));
            *m.borrow_mut() = Some(v.clone());
            Root::new(v, window, cx)
        });
        vcx.run_until_parked();
        let v = made.borrow().clone().unwrap();
        for grown in ["The open block, grown", "The open block, grown again"] {
            ASSEMBLED.with(|n| n.set(0));
            vcx.update(|_, cx| {
                v.update(cx, |s, cx| {
                    s.0.last_mut().unwrap().1 = grown.into();
                    cx.notify();
                })
            });
            vcx.run_until_parked();
            // Not every block again, at the content size's width and then at the row's.
            assert_eq!(ASSEMBLED.with(|n| n.get()), 1, "{grown}");
        }
    }

    /// A delta at the end of a reply puts together only the open block's paragraphs, and looks
    /// at only those in view (and their decorations) to draw: as many after 1,500 lines as
    /// after 100, the same few at the top in view.
    #[gpui::test]
    fn a_delta_looks_at_as_much_after_1500_lines_as_after_100(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let mut counts = Vec::new();
        for lines in [100, 1_500] {
            let mut sources = reply(lines);
            sources.push((u64::MAX, "The open block".into()));
            let made = std::rc::Rc::new(std::cell::RefCell::new(None));
            let m = made.clone();
            let (_, vcx) = cx.add_window_view(move |window, cx| {
                let v = cx.new(|_| Clipped(sources));
                *m.borrow_mut() = Some(v.clone());
                Root::new(v, window, cx)
            });
            vcx.run_until_parked();
            let v = made.borrow().clone().unwrap();
            ASSEMBLED.with(|n| n.set(0));
            VISITED.with(|n| n.set(0));
            PAINTED.with(|n| n.set(0));
            vcx.update(|_, cx| {
                v.update(cx, |s, cx| {
                    s.0.last_mut().unwrap().1 = "The open block, grown".into();
                    cx.notify();
                })
            });
            vcx.run_until_parked();
            let (assembled, visited, painted) = (
                ASSEMBLED.with(|n| n.get()),
                VISITED.with(|n| n.get()),
                PAINTED.with(|n| n.get()),
            );
            // The open block's one paragraph is put together again; some are painted.
            assert_eq!(assembled, 1, "after {lines} lines");
            assert!(painted > 0, "after {lines} lines");
            counts.push(visited);
        }
        assert_eq!(
            counts[0], counts[1],
            "looked at after 100 lines and after 1,500"
        );
    }

    /// A delta costs about the same at the end of a long reply as at the end of a short one:
    /// the blocks settled before the open one are neither assembled again nor walked to find
    /// those in view. Timed in a release build only; `cargo test --release -p gpui-core --lib
    /// a_delta_costs -- --ignored --nocapture` prints the numbers.
    #[gpui::test]
    #[cfg_attr(debug_assertions, ignore = "timed in a release build")]
    fn a_delta_costs_about_the_same_after_150_lines_as_after_1400(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (short_view, mut short_cx) = open_reply(150, cx);
        let (long_view, mut long_cx) = open_reply(1_400, cx);
        // In turn, a delta to each, so a busy machine slows both alike; the medians.
        let (mut short, mut long) = (Vec::new(), Vec::new());
        for d in 0..400 {
            let (s, l) = (
                delta(&short_view, &mut short_cx),
                delta(&long_view, &mut long_cx),
            );
            // The first few warm the caches up.
            if d >= 20 {
                short.push(s);
                long.push(l);
            }
        }
        short.sort();
        long.sort();
        let (short, long) = (short[short.len() / 2], long[long.len() / 2]);
        let ms = |d: std::time::Duration| d.as_secs_f64() * 1e3;
        eprintln!(
            "a delta after 150 lines: {:.3} ms; after 1,400: {:.3} ms ({:.2}x)",
            ms(short),
            ms(long),
            long.as_secs_f64() / short.as_secs_f64()
        );
        // 1.2 times, and 20 µs for the timer's and the machine's noise.
        assert!(
            long.as_secs_f64() <= short.as_secs_f64() * 1.2 + 20e-6,
            "{long:?} after 1,400 lines, {short:?} after 150"
        );
    }

    struct Long(String);

    impl Render for Long {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(
                div()
                    .id("scroller")
                    .h(px(120.))
                    .w(px(300.))
                    .overflow_y_scroll()
                    .child(TextView::markdown("long", self.0.clone())),
            )
        }
    }

    #[gpui::test]
    fn only_the_paragraphs_in_view_are_painted(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let source = (0..200)
            .map(|i| format!("Paragraph number {i}, a [link](https://x.test/{i})."))
            .collect::<Vec<_>>()
            .join("\n\n");
        let (_, vcx) = cx.add_window_view(move |window, cx| {
            let v = cx.new(|_| Long(source));
            Root::new(v, window, cx)
        });
        vcx.run_until_parked();
        PAINTED.with(|n| n.set(0));
        vcx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        let painted = PAINTED.with(|n| n.get());
        assert!(painted > 0 && painted < 20, "{painted} of 200 painted");
    }

    /// A press counts where the view is what's hit: one on something drawn over it (a popover, a
    /// sheet) or on a part of it scrolled out of its clip isn't the view's, while one anywhere
    /// it shows, out to its last point, is.
    struct Pressed {
        before: gpui::FocusHandle,
        source: String,
        opened: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
    }

    impl Render for Pressed {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let opened = self.opened.clone();
            div()
                .size_full()
                .relative()
                .child(div().track_focus(&self.before).h(px(20.)).w(px(300.)))
                .child(div().debug_selector(|| "covered".into()).w(px(300.)).child(
                    TextView::markdown("covered", "One.\n\nTwo.\n\nThree.").selectable(true),
                ))
                .child(
                    div()
                        .id("clip")
                        .debug_selector(|| "clip".into())
                        .h(px(120.))
                        .w(px(300.))
                        .overflow_y_scroll()
                        .child(
                            TextView::markdown("clipped", self.source.clone())
                                .selectable(true)
                                .on_link_click(move |url, _, _, _| {
                                    opened.borrow_mut().push(url.to_string())
                                }),
                        ),
                )
                // Over the first view's top, as a popover would be.
                .child(
                    div()
                        .absolute()
                        .top(px(20.))
                        .left(px(0.))
                        .w(px(300.))
                        .h(px(16.))
                        .occlude(),
                )
        }
    }

    /// A window with the views, the focus on the element before them, and the links opened.
    fn pressed(
        cx: &mut TestAppContext,
    ) -> (
        gpui::FocusHandle,
        std::rc::Rc<std::cell::RefCell<Vec<String>>>,
        gpui::VisualTestContext,
    ) {
        cx.update(crate::init);
        let before = cx.update(|cx| cx.focus_handle());
        let opened = std::rc::Rc::<std::cell::RefCell<Vec<String>>>::default();
        let (b, o) = (before.clone(), opened.clone());
        // Every paragraph a link, so a press that counts on one opens it.
        let source = (0..60)
            .map(|i| format!("[Paragraph number {i}](https://x.test/{i})"))
            .collect::<Vec<_>>()
            .join("\n\n");
        let (_, vcx) = cx.add_window_view(move |window, cx| {
            let v = cx.new(|_| Pressed {
                before: b,
                source,
                opened: o,
            });
            Root::new(v, window, cx)
        });
        vcx.update(|window, _| window.activate_window());
        vcx.run_until_parked();
        let window = gpui::VisualContext::window_handle(vcx);
        let mut vcx = gpui::VisualTestContext::from_window(window, cx);
        vcx.update(|window, cx| window.focus(&before, cx));
        vcx.run_until_parked();
        (before, opened, vcx)
    }

    /// Whether a press at `at` took the keyboard from the element before the views.
    fn takes_focus(at: gpui::Point<gpui::Pixels>, cx: &mut TestAppContext) -> bool {
        let (before, _, mut vcx) = pressed(cx);
        vcx.simulate_click(at, gpui::Modifiers::none());
        vcx.run_until_parked();
        !vcx.update(|window, _| before.is_focused(window))
    }

    /// The links a press at `at` opened.
    fn opens(at: gpui::Point<gpui::Pixels>, cx: &mut TestAppContext) -> Vec<String> {
        let (_, opened, mut vcx) = pressed(cx);
        vcx.simulate_mouse_down(at, gpui::MouseButton::Left, gpui::Modifiers::none());
        vcx.run_until_parked();
        vcx.simulate_mouse_up(at, gpui::MouseButton::Left, gpui::Modifiers::none());
        vcx.run_until_parked();
        opened.take()
    }

    #[gpui::test]
    fn a_press_on_what_is_drawn_over_the_view_is_not_the_views(cx: &mut TestAppContext) {
        let covered = {
            let (_, _, mut vcx) = pressed(cx);
            vcx.debug_bounds("covered").expect("the covered view")
        };
        assert!(covered.size.height > px(36.), "{covered:?}");
        // Under the cover: the cover's.
        assert!(!takes_focus(
            gpui::point(px(10.), covered.top() + px(8.)),
            cx
        ));
        // Below it, on the view's last point: the view's.
        assert!(takes_focus(
            gpui::point(covered.right() - px(1.), covered.bottom() - px(1.)),
            cx
        ));
    }

    #[gpui::test]
    fn a_press_on_a_part_scrolled_out_of_view_is_not_the_views(cx: &mut TestAppContext) {
        let clip = {
            let (_, _, mut vcx) = pressed(cx);
            vcx.debug_bounds("clip").expect("the clip")
        };
        // On a link in the clip: the view's, and it opens.
        assert_eq!(
            opens(gpui::point(px(10.), clip.top() + px(4.)), cx).len(),
            1
        );
        // Below the clip, over the rest of the view's links, hidden: nothing's.
        assert_eq!(
            opens(gpui::point(px(10.), clip.bottom() + px(60.)), cx),
            Vec::<String>::new()
        );
    }
}
