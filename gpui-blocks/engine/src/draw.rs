// Added in Trevin Lee's fork (2026): blocks drawn with gpui-core's paragraph layer.
//! Blocks the engine draws itself, on gpui-core's paragraph layer, and the geometry they report
//! for the caret and hit testing. See `docs/drawing.md` for what's drawn here and what still goes
//! through GPUI Kit.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use crate::kind::ColumnAlign;
use crate::text_mark::TextMark;
use gpui::StyledImage as _;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, AvailableSpace, Bounds, Element, ElementId, GlobalElementId, Hsla,
    ImageSource, InspectorElementId, IntoElement, LayoutId, ParentElement as _, Pixels, Point,
    Size, Style, Styled as _, Window, div, point, px, relative, size,
};
use gpui_core::text::paragraph::{
    Affinity, CodeStyle, InlineBox, Marks, Paragraph, ParagraphLayout, ParagraphStyle,
};

use super::extension::{CustomNode, Extensions, InlineContext};
use super::import::SOURCE_INLINE;
use super::inline::{ATOM_LEN, InlineAtom, InlineText};

/// A leaf's text as the engine laid it out, where it was drawn.
pub(crate) struct OwnLeaf {
    layout: Rc<ParagraphLayout>,
    len: usize,
    origin: Point<Pixels>,
    width: Pixels,
    caret_height: Pixels,
}

impl OwnLeaf {
    pub(crate) fn len(&self) -> usize {
        self.len
    }

    pub(crate) fn bounds(&self) -> Bounds<Pixels> {
        Bounds::new(self.origin, size(self.width, self.layout.height))
    }

    pub(crate) fn line_height(&self) -> Pixels {
        self.layout.line_height
    }

    pub(crate) fn caret_height(&self) -> Pixels {
        self.caret_height
    }

    /// The top of the text line box the caret at `index` stands in, window coordinates. A row
    /// made taller by an inline box keeps its text at the bottom, on the baseline.
    pub(crate) fn position_for_index(&self, index: usize) -> Point<Pixels> {
        let caret = self.layout.caret(index, Affinity::Downstream);
        let top = caret.bottom() - self.layout.line_height;
        self.origin + point(caret.left(), top.max(caret.top()))
    }

    pub(crate) fn index_for_position(&self, at: Point<Pixels>) -> usize {
        self.layout.index_for(at - self.origin)
    }

    pub(crate) fn atom_at(&self, at: Point<Pixels>) -> Option<Range<usize>> {
        self.layout
            .box_at(at - self.origin)
            .map(|start| start..start + ATOM_LEN)
    }
}

/// The leaves the engine drew this frame, by block key.
pub(crate) type OwnLeaves = Rc<RefCell<HashMap<u64, OwnLeaf>>>;

/// A piece of a leaf before its boxes are measured.
enum Run {
    Text(String, Marks),
    Box(AnyElement, Pixels),
    /// A picture, through the host's image loader.
    Picture(ImageSource),
}

/// A text leaf (a paragraph's or a heading's) drawn on the paragraph layer.
pub(crate) struct TextLeaf {
    key: u64,
    runs: Vec<Run>,
    style: ParagraphStyle,
    bands: Vec<(Range<usize>, Hsla)>,
    leaves: OwnLeaves,
    /// Its paragraph and inline boxes, once built (to measure it before it's laid out).
    built: Option<Built>,
}

/// A leaf's paragraph and what's drawn in its boxes.
struct Built {
    paragraph: Rc<Paragraph>,
    boxes: Vec<AnyElement>,
    /// For each box, the picture drawn in it, if it's one: it's made at the box's size once
    /// that's known.
    pictures: Vec<Option<ImageSource>>,
}

/// What a text leaf needs besides its text.
#[derive(Clone, Copy)]
pub(crate) struct LeafContext<'a> {
    pub(crate) extensions: &'a Extensions,
    pub(crate) leaves: &'a OwnLeaves,
    /// The host's image loader. Without one, pictures show as their descriptions.
    pub(crate) images: Option<&'a crate::ImageSourceFn>,
}

impl TextLeaf {
    pub(crate) fn new(
        key: u64,
        inline: &InlineText,
        style: ParagraphStyle,
        bands: Vec<(Range<usize>, Hsla)>,
        cx: &LeafContext<'_>,
        window: &mut Window,
        app: &mut App,
    ) -> Self {
        let line = InlineContext {
            font_size: style.size,
            line_height: style.line_height,
        };
        let mut runs = Vec::new();
        for (range, mark, atom) in inline.pieces() {
            match atom {
                None => runs.push(Run::Text(
                    inline.text()[range].to_string(),
                    marks(mark, &style),
                )),
                Some(InlineAtom::Custom(node)) => {
                    let custom = node.data::<CustomNode>();
                    let drawn = custom.and_then(|custom| {
                        cx.extensions
                            .iter()
                            .find(|e| e.name() == custom.name())
                            .and_then(|e| e.render_inline(custom, line, window, app))
                    });
                    let (element, baseline) = match drawn {
                        Some(drawn) => drawn,
                        None => {
                            // No drawing of its own (a kept source, an extension without one):
                            // its text, in the line's style.
                            let muted = node.name() == SOURCE_INLINE;
                            (
                                text_box(node.as_text().to_string(), &style, muted),
                                text_baseline(&style, window),
                            )
                        }
                    };
                    runs.push(Run::Box(element, baseline));
                }
                Some(InlineAtom::Image(image)) => match cx.images {
                    Some(load) => runs.push(Run::Picture(load(&image.url))),
                    // Never loaded without the host's loader: its description in its place.
                    None => {
                        let label = image
                            .alt
                            .clone()
                            .filter(|alt| !alt.is_empty())
                            .map_or_else(|| "Image".to_string(), |alt| alt.to_string());
                        runs.push(Run::Box(
                            text_box(label, &style, false),
                            text_baseline(&style, window),
                        ));
                    }
                },
            }
        }
        Self {
            key,
            runs,
            style,
            bands,
            leaves: cx.leaves.clone(),
            built: None,
        }
    }

    /// A code block's leaf: `code` in `style`, coloured by `highlights` (sorted byte ranges;
    /// where they overlap, the earlier one wins).
    pub(crate) fn code(
        key: u64,
        code: &str,
        highlights: &[(Range<usize>, gpui::HighlightStyle)],
        style: ParagraphStyle,
        bands: Vec<(Range<usize>, Hsla)>,
        cx: &LeafContext<'_>,
    ) -> Self {
        let mut runs = Vec::new();
        let mut at = 0;
        let push = |runs: &mut Vec<Run>, range: Range<usize>, marks: Marks| {
            if range.start < range.end {
                runs.push(Run::Text(code[range].to_string(), marks));
            }
        };
        for (range, highlight) in highlights {
            let start = range.start.max(at).min(code.len());
            let end = range.end.min(code.len());
            if start >= end || !code.is_char_boundary(start) || !code.is_char_boundary(end) {
                continue;
            }
            push(&mut runs, at..start, Marks::default());
            let marks = Marks {
                bold: highlight
                    .font_weight
                    .is_some_and(|w| w >= gpui::FontWeight::SEMIBOLD),
                italic: highlight.font_style == Some(gpui::FontStyle::Italic),
                color: highlight.color,
                ..Marks::default()
            };
            push(&mut runs, start..end, marks);
            at = end;
        }
        push(&mut runs, at..code.len(), Marks::default());
        Self {
            key,
            runs,
            style,
            bands,
            leaves: cx.leaves.clone(),
            built: None,
        }
    }

    /// Its paragraph and inline boxes, built from its runs once (the boxes are laid out then).
    fn build(&mut self, window: &mut Window, app: &mut App) -> Rc<Paragraph> {
        if let Some(built) = &self.built {
            return built.paragraph.clone();
        }
        let mut paragraph = Paragraph::default();
        let mut boxes = Vec::new();
        let mut pictures = Vec::new();
        let baseline_of_text = text_baseline(&self.style, window);
        for run in std::mem::take(&mut self.runs) {
            match run {
                Run::Text(text, marks) => paragraph.push(&text, marks),
                Run::Box(mut element, baseline) => {
                    let size = element.layout_as_root(
                        Size {
                            width: AvailableSpace::MinContent,
                            height: AvailableSpace::MinContent,
                        },
                        window,
                        app,
                    );
                    paragraph.push_box(InlineBox {
                        width: size.width,
                        height: size.height,
                        baseline: baseline.min(size.height),
                    });
                    boxes.push(element);
                    pictures.push(None);
                }
                Run::Picture(source) => {
                    // Its own size once it's loaded (laying it out starts the load), and a
                    // square three quarters of the line high until then.
                    let mut measure = gpui::img(source.clone())
                        .object_fit(gpui::ObjectFit::Contain)
                        .into_any_element();
                    let natural = measure.layout_as_root(AvailableSpace::min_size(), window, app);
                    let size = if natural.width > px(0.) && natural.height > px(0.) {
                        natural
                    } else {
                        let side = (self.style.line_height * 0.75).max(px(1.));
                        size(side, side)
                    };
                    paragraph.push_box(picture_box(size, baseline_of_text, self.style.line_height));
                    boxes.push(measure);
                    pictures.push(Some(source));
                }
            }
        }
        let paragraph = Rc::new(paragraph);
        self.built = Some(Built {
            paragraph: paragraph.clone(),
            boxes,
            pictures,
        });
        paragraph
    }

    /// The width it takes unwrapped: the end of its longest line.
    pub(crate) fn natural_width(&mut self, window: &mut Window, app: &mut App) -> Pixels {
        let paragraph = self.build(window, app);
        let layout = ParagraphLayout::new(&paragraph, &self.style, px(100_000.), window);
        layout
            .rows()
            .iter()
            .map(|row| layout.caret(row.range.end, Affinity::Upstream).left())
            .fold(px(0.), Pixels::max)
    }
}

/// A picture's box at `size`, centred on the text's middle, as GPUI Kit places pictures.
fn picture_box(size: Size<Pixels>, text_baseline: Pixels, line: Pixels) -> InlineBox {
    InlineBox {
        width: size.width,
        height: size.height,
        baseline: (size.height / 2. + text_baseline - line / 2.)
            .max(px(0.))
            .min(size.height),
    }
}

/// How a leaf's pictures fit the width it's laid out to.
#[derive(Clone)]
struct Fit {
    /// Which boxes are pictures.
    pictures: Rc<[bool]>,
    text_baseline: Pixels,
    line: Pixels,
}

impl Fit {
    fn new(pictures: &[Option<ImageSource>], style: &ParagraphStyle, window: &Window) -> Self {
        Self {
            pictures: pictures.iter().map(Option::is_some).collect(),
            text_baseline: text_baseline(style, window),
            line: style.line_height,
        }
    }

    /// `paragraph` with each picture wider than `width` narrowed to it, in its ratio.
    fn apply(&self, paragraph: &Rc<Paragraph>, width: Pixels) -> Rc<Paragraph> {
        let wide = |(i, (_, b)): (usize, &(usize, InlineBox))| {
            self.pictures.get(i).copied().unwrap_or(false) && b.width > width
        };
        if !paragraph.boxes.iter().enumerate().any(wide) || width <= px(0.) {
            return paragraph.clone();
        }
        let mut fitted = Paragraph::clone(paragraph);
        for (i, (_, b)) in fitted.boxes.iter_mut().enumerate() {
            if self.pictures.get(i).copied().unwrap_or(false) && b.width > width {
                let size = size(width, b.height * (width / b.width));
                *b = picture_box(size, self.text_baseline, self.line);
            }
        }
        Rc::new(fitted)
    }
}

fn text_box(text: String, style: &ParagraphStyle, muted: bool) -> AnyElement {
    div()
        .whitespace_nowrap()
        .text_size(style.size)
        .line_height(style.line_height)
        .text_color(if muted { style.muted } else { style.color })
        .child(text)
        .into_any_element()
}

/// Where text set in `style` has its baseline, from the top of its line box.
fn text_baseline(style: &ParagraphStyle, window: &Window) -> Pixels {
    let ts = window.text_system();
    let font = ts.resolve_font(&style.font);
    let (ascent, descent) = (ts.ascent(font, style.size), ts.descent(font, style.size));
    (style.line_height - (ascent + descent.abs())) / 2. + ascent
}

/// The paragraph layer's marks for an editor mark.
fn marks(mark: &TextMark, style: &ParagraphStyle) -> Marks {
    Marks {
        bold: mark.bold,
        italic: mark.italic,
        strike: mark.strikethrough,
        underline: mark.underline,
        code: mark.code,
        link: mark.link.as_ref().map(|l| l.url.clone()),
        muted: false,
        highlight: mark.highlight,
        // A highlight sets the text's colour with its fill, as HTML's `mark` does, so a light
        // fill stays readable in a dark theme.
        color: mark
            .highlight
            .filter(|c| c.a > 0.)
            .map(|c| readable_on(c, style.color)),
    }
}

fn readable_on(fill: Hsla, text: Hsla) -> Hsla {
    if fill.l > 0.6 {
        gpui::black()
    } else if fill.l < 0.35 {
        gpui::white()
    } else {
        text
    }
}

/// The layout measured for a width, kept for the paint that follows.
type LayoutCache = Rc<RefCell<Option<(Pixels, Rc<ParagraphLayout>)>>>;

pub(crate) struct TextLeafState {
    paragraph: Rc<Paragraph>,
    boxes: Vec<AnyElement>,
    pictures: Vec<Option<ImageSource>>,
    fit: Fit,
    cache: LayoutCache,
}

impl IntoElement for TextLeaf {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextLeaf {
    type RequestLayoutState = TextLeafState;
    type PrepaintState = Option<Rc<ParagraphLayout>>;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.build(window, cx);
        let Built {
            paragraph,
            boxes,
            pictures,
        } = self.built.take().expect("built above");
        let cache: LayoutCache = Rc::default();
        let fit = Fit::new(&pictures, &self.style, window);
        let (measured, style, written) = (paragraph.clone(), self.style.clone(), cache.clone());
        let fits = fit.clone();
        let mut layout_style = Style::default();
        layout_style.size.width = relative(1.).into();
        let id =
            window.request_measured_layout(layout_style, move |known, available, window, _| {
                let width = known.width.unwrap_or(match available.width {
                    AvailableSpace::Definite(w) => w,
                    _ => px(100_000.),
                });
                let fitted = fits.apply(&measured, width);
                let layout = Rc::new(ParagraphLayout::new(&fitted, &style, width, window));
                let height = layout.height;
                *written.borrow_mut() = Some((width, layout));
                size(width, height)
            });
        (
            id,
            TextLeafState {
                paragraph,
                boxes,
                pictures,
                fit,
                cache,
            },
        )
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let cached = state
            .cache
            .borrow()
            .as_ref()
            .filter(|(w, _)| (*w - bounds.size.width).abs() < px(0.5))
            .map(|(_, l)| l.clone());
        let layout = cached.unwrap_or_else(|| {
            Rc::new(ParagraphLayout::new(
                &state.fit.apply(&state.paragraph, bounds.size.width),
                &self.style,
                bounds.size.width,
                window,
            ))
        });
        let ts = window.text_system();
        let font = ts.resolve_font(&self.style.font);
        let caret_height =
            ts.ascent(font, self.style.size) + ts.descent(font, self.style.size).abs();
        self.leaves.borrow_mut().insert(
            self.key,
            OwnLeaf {
                layout: layout.clone(),
                len: state.paragraph.text.len(),
                origin: bounds.origin,
                width: bounds.size.width,
                caret_height,
            },
        );
        for (((_, b), element), picture) in layout
            .boxes()
            .into_iter()
            .zip(&mut state.boxes)
            .zip(&state.pictures)
        {
            // A picture fills its box, now that its size is known.
            if let Some(source) = picture {
                *element = gpui::img(source.clone())
                    .object_fit(gpui::ObjectFit::Contain)
                    .w(b.size.width)
                    .h(b.size.height)
                    .into_any_element();
            }
            element.prepaint_as_root(
                bounds.origin + b.origin,
                Size {
                    width: AvailableSpace::Definite(b.size.width),
                    height: AvailableSpace::Definite(b.size.height),
                },
                window,
                cx,
            );
        }
        Some(layout)
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        state: &mut Self::RequestLayoutState,
        layout: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(layout) = layout else {
            return;
        };
        for (range, color) in &self.bands {
            layout.paint_bands(range.clone(), *color, bounds.origin, window);
        }
        layout.paint(bounds.origin, window, cx);
        for element in &mut state.boxes {
            element.paint(window, cx);
        }
    }
}

/// The paragraph layer's style for the editor's text at `font_size` and `line_height`.
// The struct update below is for `CodeStyle` fields gpui-core adds after these.
#[allow(clippy::needless_update)]
pub(crate) fn paragraph_style(
    base: &gpui::TextStyle,
    font_size: Pixels,
    line_height: Pixels,
    colors: &Colors,
) -> ParagraphStyle {
    ParagraphStyle {
        font: base.font(),
        size: font_size,
        line_height,
        color: base.color,
        muted: colors.muted,
        link: colors.link,
        code_family: colors.mono.clone(),
        code_background: colors.code_background,
        align: gpui::TextAlign::Left,
        code: CodeStyle {
            size: Some(font_size * 0.875),
            padding: font_size * 0.25,
            radius: px(4.),
            color: None,
            // Fields gpui-core adds keep their defaults.
            ..Default::default()
        },
    }
}

/// The colours and families the text is drawn in, from the editor's style.
#[derive(Clone)]
pub(crate) struct Colors {
    pub(crate) muted: Hsla,
    pub(crate) link: Hsla,
    pub(crate) code_background: Hsla,
    pub(crate) mono: gpui::SharedString,
    pub(crate) border: Hsla,
    /// A table's fill.
    pub(crate) surface: Hsla,
}

/// A line height snapped to whole device pixels, as GPUI Kit lays its text out.
pub(crate) fn device_line(line: Pixels, window: &Window) -> Pixels {
    let scale = window.scale_factor();
    px((f32::from(line) * scale).round() / scale)
}

/// What came before a block among its siblings, for the gap above a heading or a rule.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Prev {
    Heading(u8),
    Rule,
    Other,
}

/// Where a block stands among its siblings, for the gaps around it.
#[derive(Clone, Copy)]
pub(crate) struct Place {
    pub(crate) first: bool,
    pub(crate) last: bool,
    pub(crate) prev: Option<Prev>,
    /// How many list items it's inside, as GPUI Kit counts depth: quotes, callouts and toggles
    /// don't add to it. A nested heading or rule sits flush; a list's markers follow it.
    pub(crate) depth: usize,
}

/// What drawing a block needs besides the block.
#[derive(Clone, Copy)]
pub(crate) struct DrawCx<'a> {
    pub(crate) base: &'a gpui::TextStyle,
    pub(crate) body_size: Pixels,
    pub(crate) body_line: Pixels,
    pub(crate) paragraph_gap: Pixels,
    pub(crate) colors: &'a Colors,
    pub(crate) dark: bool,
    pub(crate) bands: &'a HashMap<u64, Vec<(Range<usize>, Hsla)>>,
    pub(crate) leaves: LeafContext<'a>,
    pub(crate) code: &'a CodeLook,
}

/// How code blocks look: GPUI Kit's code block, in the editor's theme.
pub(crate) struct CodeLook {
    /// The code's size; its line is 1.6 times that.
    pub(crate) size: Pixels,
    /// The language label's family and size.
    pub(crate) label_family: gpui::SharedString,
    pub(crate) label_size: Pixels,
    /// The editor style's refinement of code blocks (their corners, say).
    pub(crate) refinement: gpui::StyleRefinement,
}

/// The host's highlighter and the highlights it made, for the code blocks the engine draws.
pub(crate) struct Highlighting {
    pub(crate) highlighter: crate::CodeHighlightFn,
    pub(crate) cache: crate::state::HighlightCache,
}

/// A block as the engine draws it, copied out of the document first, since drawing needs the
/// app.
pub(crate) enum Tree {
    /// A paragraph (no level) or a heading.
    Text {
        key: u64,
        level: Option<u8>,
        inline: InlineText,
    },
    List(ListTree),
    Table(TableTree),
    Quote(Vec<Tree>),
    Callout(gpui::SharedString, Vec<Tree>),
    /// A toggle: its summary, then its body when open.
    Toggle(bool, Vec<Tree>),
    Rule,
    /// A code block: its leaf's key, its language, its code and the code's highlights.
    Code {
        key: u64,
        lang: gpui::SharedString,
        code: String,
        highlights: Rc<[(Range<usize>, gpui::HighlightStyle)]>,
    },
    Custom(gpui_kit::base::text::MarkdownNode),
    /// A break, a link definition or an unknown block: nothing to see.
    Nothing,
}

pub(crate) struct ListTree {
    ordered: bool,
    start: Option<u32>,
    items: Vec<ItemTree>,
}

struct ItemTree {
    checked: Option<bool>,
    children: Vec<Tree>,
}

pub(crate) struct TableTree {
    aligns: Vec<ColumnAlign>,
    /// Each row's cells, the header first: a cell's key and its text.
    rows: Vec<Vec<(u64, InlineText)>>,
}

/// `block` as a tree the engine can draw: `None` when something in it is still drawn by GPUI
/// Kit (a code block, when the host gave no highlighter).
pub(crate) fn tree(
    doc: &super::document::EditorDocument,
    block: super::id::BlockId,
    code: Option<&Highlighting>,
) -> Option<Tree> {
    use super::document::{AtomData, BlockContent};
    use super::kind::BlockKindData as K;
    let children = |id| -> Option<Vec<Tree>> {
        doc.children(id)
            .iter()
            .map(|c| tree(doc, *c, code))
            .collect()
    };
    Some(
        match (doc.kind(block)?, doc.get(block).map(|e| &e.content)) {
            (K::Paragraph, Some(BlockContent::Inline(inline))) => Tree::Text {
                key: block.as_u64(),
                level: None,
                inline: inline.clone(),
            },
            (K::Heading { level }, Some(BlockContent::Inline(inline))) => Tree::Text {
                key: block.as_u64(),
                level: Some(*level),
                inline: inline.clone(),
            },
            (K::List { .. }, _) => Tree::List(list_tree(doc, block, None, code)?),
            (K::Table { aligns }, _) => Tree::Table(TableTree {
                aligns: aligns.clone(),
                rows: doc
                    .children(block)
                    .iter()
                    .map(|row| {
                        doc.children(*row)
                            .iter()
                            .map(|cell| match doc.get(*cell).map(|e| &e.content) {
                                Some(BlockContent::Inline(inline)) => {
                                    Some((cell.as_u64(), inline.clone()))
                                }
                                _ => None,
                            })
                            .collect::<Option<Vec<_>>>()
                    })
                    .collect::<Option<_>>()?,
            }),
            (K::Blockquote, _) => Tree::Quote(children(block)?),
            (K::Callout { name }, _) => Tree::Callout(name.clone(), children(block)?),
            (K::Toggle { open }, _) => {
                let ids = doc.children(block);
                // A closed toggle draws only its summary.
                let shown = if *open { ids } else { &ids[..ids.len().min(1)] };
                Tree::Toggle(
                    *open,
                    shown
                        .iter()
                        .map(|c| tree(doc, *c, code))
                        .collect::<Option<_>>()?,
                )
            }
            (K::HorizontalRule, _) => Tree::Rule,
            (K::CodeBlock { lang, .. }, Some(BlockContent::Plain(text))) => {
                let code = code?;
                let lang = lang.clone().unwrap_or_default();
                Tree::Code {
                    key: block.as_u64(),
                    highlights: crate::state::highlights(
                        &code.cache,
                        &code.highlighter,
                        block,
                        &lang,
                        text,
                    ),
                    lang,
                    code: text.clone(),
                }
            }
            (K::Custom, Some(BlockContent::Atom(AtomData::Custom(node)))) => {
                Tree::Custom(node.clone())
            }
            (K::Break { .. } | K::Definition { .. } | K::Unknown, _) => Tree::Nothing,
            _ => return None,
        },
    )
}

/// `list` (or `items` of it, for a chunk of a long list) as a tree, or `None` as for [`tree`].
pub(crate) fn list_tree(
    doc: &super::document::EditorDocument,
    list: super::id::BlockId,
    items: Option<Range<usize>>,
    code: Option<&Highlighting>,
) -> Option<ListTree> {
    use super::kind::BlockKindData as K;
    let Some(K::List { ordered, start }) = doc.kind(list) else {
        return None;
    };
    let all = doc.children(list);
    let range = items.unwrap_or(0..all.len());
    let mut out = Vec::with_capacity(range.len());
    for item in &all[range.clone()] {
        let Some(K::ListItem { checked, .. }) = doc.kind(*item) else {
            return None;
        };
        out.push(ItemTree {
            checked: *checked,
            children: doc
                .children(*item)
                .iter()
                .map(|c| tree(doc, *c, code))
                .collect::<Option<_>>()?,
        });
    }
    Some(ListTree {
        ordered: *ordered,
        // A chunk of a long list carries on its numbering.
        start: if *ordered && range.start > 0 {
            Some(start.unwrap_or(1) + range.start as u32)
        } else {
            *start
        },
        items: out,
    })
}

impl Tree {
    fn prev(&self) -> Prev {
        match self {
            Tree::Text {
                level: Some(level), ..
            } => Prev::Heading(*level),
            Tree::Rule => Prev::Rule,
            _ => Prev::Other,
        }
    }
}

/// A heading's size relative to the body text, its line height, and the gap above it relative
/// to its size: hierarchy from weight more than size.
fn heading_metrics(level: u8) -> (f32, f32, f32) {
    match level {
        1 => (1.8, 1.25, 1.1),
        2 => (4. / 3., 1.35, 1.6),
        3 => (17. / 15., 1.35, 1.4),
        4 => (1., 1.35, 1.4),
        5 => (14. / 15., 1.35, 1.4),
        _ => (13. / 15., 1.35, 1.4),
    }
}

/// The gap below a heading, relative to its size.
const HEADING_BOTTOM_GAP: f32 = 0.35;

/// The room above and below a rule.
fn rule_space(body_size: Pixels) -> Pixels {
    body_size * 1.6
}

/// A block, with the gaps around it. `color` is the text's, when a container sets it (a quote's
/// muted text, a done task's).
pub(crate) fn draw(
    block: &Tree,
    place: Place,
    color: Option<Hsla>,
    d: &DrawCx<'_>,
    window: &mut Window,
    app: &mut App,
) -> AnyElement {
    let gap = if place.last { px(0.) } else { d.paragraph_gap };
    match block {
        Tree::Text {
            key,
            level: None,
            inline,
        } => {
            let mut style = paragraph_style(d.base, d.body_size, d.body_line, d.colors);
            if let Some(color) = color {
                style.color = color;
            }
            let bands = d.bands.get(key).cloned().unwrap_or_default();
            let leaf = TextLeaf::new(*key, inline, style, bands, &d.leaves, window, app);
            div().pb(gap).child(leaf).into_any_element()
        }
        Tree::Text {
            key,
            level: Some(level),
            inline,
        } => {
            let (scale, line, section) = heading_metrics(*level);
            let size = d.body_size * scale;
            let mut style =
                paragraph_style(d.base, size, device_line(size * line, window), d.colors);
            style.font.weight = gpui::FontWeight::SEMIBOLD;
            if let Some(color) = color {
                style.color = color;
            }
            if *level >= 6 {
                style.color = d.colors.muted;
            }
            // The previous block already leaves a gap below itself, so only the rest of the
            // section gap goes above; a heading right after another stays close; nested, it
            // sits flush.
            let top = match (place.first || place.depth > 0, place.prev) {
                (true, _) | (_, None) => px(0.),
                (false, Some(Prev::Heading(_))) => size * 0.25,
                (false, Some(Prev::Rule)) => (size * section - rule_space(d.body_size)).max(px(0.)),
                (false, Some(Prev::Other)) => (size * section - d.paragraph_gap).max(px(0.)),
            };
            let bottom = if place.last {
                px(0.)
            } else {
                size * HEADING_BOTTOM_GAP
            };
            let bands = d.bands.get(key).cloned().unwrap_or_default();
            let leaf = TextLeaf::new(*key, inline, style, bands, &d.leaves, window, app);
            div().pt(top).pb(bottom).child(leaf).into_any_element()
        }
        Tree::Code {
            key,
            lang,
            code,
            highlights,
        } => div()
            .pb(gap)
            .w_full()
            .child(code_element(*key, lang, code, highlights, d, window))
            .into_any_element(),
        Tree::Table(table) => div()
            .pb(gap)
            .w_full()
            .child(table_element(table, color, d, window, app))
            .into_any_element(),
        Tree::Rule => {
            // A hairline with more room around it than a paragraph gap. The previous block's
            // own gap counts toward the space above.
            let space = rule_space(d.body_size);
            let top = if place.first || place.depth > 0 {
                px(0.)
            } else {
                let above = match place.prev {
                    None => px(0.),
                    Some(Prev::Rule) => space,
                    Some(Prev::Heading(level)) => {
                        d.body_size * heading_metrics(level).0 * HEADING_BOTTOM_GAP
                    }
                    Some(Prev::Other) => d.paragraph_gap,
                };
                (space - above).max(px(0.))
            };
            let bottom = if place.last { px(0.) } else { space };
            div()
                .pt(top)
                .pb(bottom)
                .child(div().bg(d.colors.border).h(px(1.)))
                .into_any_element()
        }
        Tree::Custom(node) => {
            let drawn = node.data::<CustomNode>().and_then(|custom| {
                d.leaves
                    .extensions
                    .iter()
                    .find(|e| e.name() == custom.name())
                    .map(|e| e.render(custom, window, app))
            });
            let content =
                drawn.unwrap_or_else(|| div().child(node.as_text().to_string()).into_any_element());
            div().pb(gap).child(content).into_any_element()
        }
        Tree::Nothing => div().into_any_element(),
        Tree::List(list) => list_element(list, place.depth, color, d, window, app)
            .pb(gap)
            .into_any_element(),
        Tree::Quote(children) => {
            let body = children_of(children, place.depth, Some(d.colors.muted), d, window, app);
            div()
                .w_full()
                .min_w_0()
                .border_l_2()
                .border_color(d.colors.border)
                .pl(d.body_size * 0.9)
                .children(body)
                .pb(gap)
                .into_any_element()
        }
        Tree::Callout(name, children) => {
            let body = children_of(children, place.depth, color, d, window, app);
            let container = super::surfaces::callout_container(name, app);
            div()
                .pb(gap)
                .child(container.w_full().min_w_0().children(body))
                .into_any_element()
        }
        Tree::Toggle(open, children) => {
            let body = children_of(children, place.depth, color, d, window, app);
            let container = super::surfaces::toggle_container(*open, window, app);
            div()
                .pb(gap)
                .child(container.w_full().min_w_0().children(body))
                .into_any_element()
        }
    }
}

/// A container's blocks, each placed among the others.
fn children_of(
    children: &[Tree],
    depth: usize,
    color: Option<Hsla>,
    d: &DrawCx<'_>,
    window: &mut Window,
    app: &mut App,
) -> Vec<AnyElement> {
    let n = children.len();
    let mut out = Vec::with_capacity(n);
    for (ix, child) in children.iter().enumerate() {
        let place = Place {
            first: ix == 0,
            last: ix + 1 == n,
            prev: ix.checked_sub(1).map(|p| children[p].prev()),
            depth,
        };
        out.push(draw(child, place, color, d, window, app));
    }
    out
}

// Lists.

const BULLETS: [&str; 5] = ["•", "◦", "▪", "‣", "⁃"];

/// An item's marker: a bullet by depth, or its number (a letter in a nested ordered list).
fn list_marker(ix: usize, start: Option<u32>, ordered: bool, depth: usize) -> String {
    if !ordered {
        return BULLETS[depth.min(BULLETS.len() - 1)].to_string();
    }
    let ordinal = start
        .unwrap_or(1)
        .saturating_add(u32::try_from(ix).unwrap_or(u32::MAX));
    if depth == 0 || start == Some(0) {
        return format!("{ordinal}.");
    }
    let first = if depth == 1 { b'A' } else { b'a' };
    let mut remaining = ordinal;
    let mut letters = Vec::new();
    while remaining > 0 {
        remaining -= 1;
        letters.push((first + (remaining % 26) as u8) as char);
        remaining /= 26;
    }
    format!("{}.", letters.into_iter().rev().collect::<String>())
}

/// The white or black check drawn in a done task's box.
const CHECK_SVG_LIGHT: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none"><path d="m3.25 8.25 3 3 6.5-7" stroke="white" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;
const CHECK_SVG_DARK: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16" fill="none"><path d="m3.25 8.25 3 3 6.5-7" stroke="black" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;

/// A list at `depth` (how many lists it's nested in).
fn list_element(
    list: &ListTree,
    depth: usize,
    color: Option<Hsla>,
    d: &DrawCx<'_>,
    window: &mut Window,
    app: &mut App,
) -> gpui::Div {
    let mut items = Vec::with_capacity(list.items.len());
    for (ix, item) in list.items.iter().enumerate() {
        items.push(item_element(list, item, ix, depth, color, d, window, app));
    }
    div().w_full().min_w_0().children(items)
}

#[allow(clippy::too_many_arguments)]
fn item_element(
    list: &ListTree,
    item: &ItemTree,
    ix: usize,
    depth: usize,
    color: Option<Hsla>,
    d: &DrawCx<'_>,
    window: &mut Window,
    app: &mut App,
) -> AnyElement {
    let indent = d.body_size * 1.4;
    // A done task steps back so the open ones stand out.
    let color = if item.checked == Some(true) {
        Some(d.colors.muted)
    } else {
        color
    };
    let mut rows: Vec<gpui::Div> = Vec::new();
    for (child_ix, child) in item.children.iter().enumerate() {
        let place = Place {
            first: child_ix == 0,
            // Blocks in an item sit close: no gap below them.
            last: true,
            prev: None,
            depth: depth + 1,
        };
        match child {
            Tree::List(nested) => {
                let element = list_element(nested, depth + 1, color, d, window, app);
                rows.push(div().ml(indent).child(element));
            }
            _ => {
                let element = draw(child, place, color, d, window, app);
                let after_list =
                    child_ix > 0 && matches!(item.children[child_ix - 1], Tree::List(_));
                if child_ix == 0 {
                    rows.push(item_row(element, list, item, ix, depth, d));
                } else if matches!(child, Tree::Text { level: None, .. }) && !after_list {
                    // A later paragraph lines up with the item's text.
                    let continued = div().w_full().pl(indent).overflow_hidden().child(element);
                    match rows.pop() {
                        Some(row) => rows.push(div().child(row).child(continued)),
                        None => rows.push(continued),
                    }
                } else {
                    rows.push(
                        div()
                            .w_full()
                            .min_w_0()
                            .pl(indent)
                            .overflow_hidden()
                            .child(element),
                    );
                }
            }
        }
    }
    div()
        .w_full()
        .min_w_0()
        .when(ix > 0, |el| el.pt(d.body_size * 0.25))
        .children(rows)
        .into_any_element()
}

/// An item's first row: its marker or task box, then its first block.
fn item_row(
    content: AnyElement,
    list: &ListTree,
    item: &ItemTree,
    ix: usize,
    depth: usize,
    d: &DrawCx<'_>,
) -> gpui::Div {
    let indent = d.body_size * 1.4;
    let colors = d.colors;
    let row = div()
        .flex()
        .flex_row()
        .w_full()
        .min_w_0()
        .relative()
        .items_start();
    let row = match item.checked {
        None => row.child(
            // The marker hangs in an indent column, right-aligned against the text, and steps
            // back in colour so the text leads.
            div()
                .flex_none()
                .min_w(indent)
                .pr(indent * 0.3)
                .text_right()
                .text_size(d.body_size)
                .line_height(d.body_line)
                .text_color(colors.muted)
                .child(list_marker(ix, list.start, list.ordered, depth)),
        ),
        Some(checked) => {
            let check = if d.dark {
                CHECK_SVG_DARK
            } else {
                CHECK_SVG_LIGHT
            };
            row.child(
                div()
                    .flex()
                    .mr_2()
                    .h(d.body_line)
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .flex()
                            .size(gpui::rems(0.9375))
                            .items_center()
                            .justify_center()
                            .rounded(px(4.))
                            .border(px(1.5))
                            .border_color(colors.muted)
                            .when(checked, |el| {
                                el.bg(colors.link).border_color(colors.link).child(
                                    gpui::img(std::sync::Arc::new(gpui::Image::from_bytes(
                                        gpui::ImageFormat::Svg,
                                        check.to_vec(),
                                    )))
                                    .size(gpui::rems(0.75)),
                                )
                            }),
                    ),
            )
        }
    };
    row.child(div().flex_auto().min_w_0().overflow_hidden().child(content))
}

/// A table cell's padding (GPUI Kit's `px_3` and `py(7)`), the narrowest a column starts from,
/// and the rules between rows and columns and around the table.
const CELL_PAD_X: f32 = 12.;
const CELL_PAD_Y: f32 = 7.;
const CELL_MIN: f32 = 48.;
const RULE: f32 = 1.;

/// A table as GPUI Kit lays it out, so the grips and the caret find its cells where they were:
/// each column is a cell and slack. A cell grows to fit its widest line, never narrower than its
/// floor; flex layout grows every cell alike and stops each at its width, so narrow columns fit
/// and the widest share what's left. Room left after every cell fits goes to the slack, in
/// proportion to the columns, after, around or before the cell as the column is aligned.
fn table_element(
    table: &TableTree,
    color: Option<Hsla>,
    d: &DrawCx<'_>,
    window: &mut Window,
    app: &mut App,
) -> AnyElement {
    // The cells' share of the free room while any is still growing; the slack's is about a
    // millionth of it.
    const CELL_GROW: f32 = 1e6;
    let cols = table.rows.iter().map(Vec::len).max().unwrap_or(0);
    let style = paragraph_style(d.base, d.body_size, d.body_line, d.colors);
    let mut rows: Vec<Vec<TextLeaf>> = table
        .rows
        .iter()
        .enumerate()
        .map(|(r, row)| {
            row.iter()
                .enumerate()
                .map(|(c, (key, inline))| {
                    let mut style = style.clone();
                    style.align = match table.aligns.get(c) {
                        Some(ColumnAlign::Center) => gpui::TextAlign::Center,
                        Some(ColumnAlign::Right) => gpui::TextAlign::Right,
                        _ => gpui::TextAlign::Left,
                    };
                    // The header keeps the body colour in a quote, as GPUI Kit's does.
                    if let Some(color) = color.filter(|_| r > 0) {
                        style.color = color;
                    }
                    let bands = d.bands.get(key).cloned().unwrap_or_default();
                    TextLeaf::new(*key, inline, style, bands, &d.leaves, window, app)
                })
                .collect()
        })
        .collect();
    let divider = |c: usize| if c + 1 < cols { RULE } else { 0. };
    // Each column's width to fit its widest line, with the cells' padding and divider.
    let mut widths = vec![CELL_MIN; cols];
    for row in &mut rows {
        for (c, leaf) in row.iter_mut().enumerate() {
            let w = f32::from(leaf.natural_width(window, app)) + CELL_PAD_X * 2. + divider(c);
            widths[c] = widths[c].max(w);
        }
    }
    // Rounded up to device pixels, or a column can lose the fraction that keeps a line whole.
    let scale = window.scale_factor();
    let ceil = |w: f32| (w * scale).ceil() / scale;
    let floor = f32::from(window.rem_size() * 4.);
    // Each cell's (narrowest, widest), its divider left out.
    let columns: Vec<(f32, f32)> = widths
        .iter()
        .enumerate()
        .map(|(c, &w)| {
            let max = ceil(w - divider(c));
            (ceil(w.min(floor) - divider(c)).min(max), max)
        })
        .collect();
    let total = columns.iter().map(|(_, max)| max).sum::<f32>().max(1.);
    let count = rows.len();
    let border = d.colors.border;
    let slack = |grow: f32| {
        div()
            .flex_basis(px(0.))
            .flex_grow(grow)
            .flex_shrink(0.)
            .border_color(border)
    };
    let rows = rows.into_iter().enumerate().map(|(r, row)| {
        let mut cells = row.into_iter();
        let mut children = Vec::with_capacity(cols * 2);
        for (c, &(min, max)) in columns.iter().enumerate() {
            let last = c + 1 == cols;
            let grow = max / total;
            let (lead, trail) = match table.aligns.get(c) {
                Some(ColumnAlign::Center) => (grow / 2., grow / 2.),
                Some(ColumnAlign::Right) => (grow, 0.),
                _ => (0., grow),
            };
            if lead > 0. {
                children.push(slack(lead).into_any_element());
            }
            children.push(
                div()
                    .flex_basis(px(0.))
                    .flex_grow(CELL_GROW)
                    .min_w(px(min))
                    .max_w(px(max))
                    .overflow_hidden()
                    .px(px(CELL_PAD_X))
                    .py(px(CELL_PAD_Y))
                    // A short row keeps its missing cells' places.
                    .children(cells.next())
                    .into_any_element(),
            );
            if trail > 0. || !last {
                children.push(
                    slack(trail)
                        .when(!last, |s| s.border_r_1())
                        .into_any_element(),
                );
            }
        }
        div()
            .w_full()
            .flex()
            .flex_row()
            .when(r + 1 < count, |row| row.border_b_1())
            .border_color(border)
            .when(r == 0, |row| row.bg(d.colors.code_background))
            .children(children)
            .into_any_element()
    });
    div()
        .w_full()
        .overflow_hidden()
        .bg(d.colors.surface)
        .border_1()
        .border_color(border)
        .children(rows)
        .into_any_element()
}

/// A code block as GPUI Kit draws one: monospaced on the code background, in a border, with its
/// language in the top right corner.
fn code_element(
    key: u64,
    lang: &gpui::SharedString,
    code: &str,
    highlights: &[(Range<usize>, gpui::HighlightStyle)],
    d: &DrawCx<'_>,
    window: &Window,
) -> AnyElement {
    let size = d.code.size;
    let mut style = paragraph_style(d.base, size, device_line(size * 1.6, window), d.colors);
    style.font.family = d.colors.mono.clone();
    let bands = d.bands.get(&key).cloned().unwrap_or_default();
    let leaf = TextLeaf::code(key, code, highlights, style, bands, &d.leaves);
    let label = (!lang.is_empty()).then(|| {
        div()
            .absolute()
            .top(px(7.))
            .right(px(12.))
            .font_family(d.code.label_family.clone())
            .text_size(d.code.label_size)
            .line_height(px(16.))
            .text_color(d.colors.muted)
            .child(lang.clone())
    });
    let mut block = div()
        .w_full()
        .min_w_0()
        .px(px(16.))
        .pt(px(if label.is_some() { 28. } else { 14. }))
        .pb(px(14.))
        .bg(d.colors.code_background)
        .border_1()
        .border_color(d.colors.border)
        .relative();
    gpui::Refineable::refine(block.style(), &d.code.refinement);
    block.child(leaf).children(label).into_any_element()
}
