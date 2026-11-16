// Added in Trevin Lee's fork (2026): blocks drawn with gpui-core's paragraph layer.
//! Blocks the engine draws itself, on gpui-core's paragraph layer, and the geometry they report
//! for the caret and hit testing. See `docs/drawing.md` for what's drawn here and what still goes
//! through GPUI Kit.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use gpui::{
    AnyElement, App, AvailableSpace, Bounds, Element, ElementId, GlobalElementId, Hsla,
    InspectorElementId, IntoElement, LayoutId, ParentElement as _, Pixels, Point, Size, Style,
    Styled as _, Window, div, point, px, relative, size,
};
use gpui_core::text::paragraph::{
    Affinity, CodeStyle, InlineBox, Marks, Paragraph, ParagraphLayout, ParagraphStyle,
};
use gpui_kit::base::text::editor_host::TextMark;

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
}

/// A text leaf (a paragraph's or a heading's) drawn on the paragraph layer.
pub(crate) struct TextLeaf {
    key: u64,
    runs: Vec<Run>,
    style: ParagraphStyle,
    bands: Vec<(Range<usize>, Hsla)>,
    leaves: OwnLeaves,
}

/// What a text leaf needs besides its text.
pub(crate) struct LeafContext<'a> {
    pub(crate) extensions: &'a Extensions,
    pub(crate) leaves: &'a OwnLeaves,
}

/// Whether the engine draws this text itself: everything but pictures, for now.
pub(crate) fn draws(inline: &InlineText) -> bool {
    !inline
        .pieces()
        .any(|(_, _, atom)| matches!(atom, Some(InlineAtom::Image(_))))
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
                Some(InlineAtom::Image(image)) => {
                    // Not drawn here yet (see `draws`): its description.
                    let alt = image.alt.clone().unwrap_or_default().to_string();
                    runs.push(Run::Box(
                        text_box(alt, &style, true),
                        text_baseline(&style, window),
                    ));
                }
            }
        }
        Self {
            key,
            runs,
            style,
            bands,
            leaves: cx.leaves.clone(),
        }
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
        let mut paragraph = Paragraph::default();
        let mut boxes = Vec::new();
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
                        cx,
                    );
                    paragraph.push_box(InlineBox {
                        width: size.width,
                        height: size.height,
                        baseline: baseline.min(size.height),
                    });
                    boxes.push(element);
                }
            }
        }
        let paragraph = Rc::new(paragraph);
        let cache: LayoutCache = Rc::default();
        let (measured, style, written) = (paragraph.clone(), self.style.clone(), cache.clone());
        let mut layout_style = Style::default();
        layout_style.size.width = relative(1.).into();
        let id =
            window.request_measured_layout(layout_style, move |known, available, window, _| {
                let width = known.width.unwrap_or(match available.width {
                    AvailableSpace::Definite(w) => w,
                    _ => px(100_000.),
                });
                let layout = Rc::new(ParagraphLayout::new(&measured, &style, width, window));
                let height = layout.height;
                *written.borrow_mut() = Some((width, layout));
                size(width, height)
            });
        (
            id,
            TextLeafState {
                paragraph,
                boxes,
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
                &state.paragraph,
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
        for ((_, b), element) in layout.boxes().into_iter().zip(&mut state.boxes) {
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
}

/// A line height snapped to whole device pixels, as GPUI Kit lays its text out.
pub(crate) fn device_line(line: Pixels, window: &Window) -> Pixels {
    let scale = window.scale_factor();
    px((f32::from(line) * scale).round() / scale)
}

/// What came before a top-level block, for the gap above a heading.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Prev {
    Heading,
    Rule,
    Other,
}

/// A top-level paragraph or heading.
pub(crate) struct TopLevel<'a> {
    pub(crate) key: u64,
    /// A heading's level; `None` for a paragraph.
    pub(crate) level: Option<u8>,
    pub(crate) inline: &'a InlineText,
    pub(crate) first: bool,
    pub(crate) last: bool,
    pub(crate) prev: Option<Prev>,
    pub(crate) bands: Vec<(Range<usize>, Hsla)>,
    pub(crate) base: &'a gpui::TextStyle,
    pub(crate) body_size: Pixels,
    pub(crate) body_line: Pixels,
    pub(crate) paragraph_gap: Pixels,
    pub(crate) colors: &'a Colors,
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

/// A top-level paragraph or heading, with the gaps around it.
pub(crate) fn top_level_text(
    block: TopLevel<'_>,
    cx: &LeafContext<'_>,
    window: &mut Window,
    app: &mut App,
) -> AnyElement {
    match block.level {
        None => {
            let style = paragraph_style(block.base, block.body_size, block.body_line, block.colors);
            let leaf = TextLeaf::new(block.key, block.inline, style, block.bands, cx, window, app);
            let gap = if block.last {
                px(0.)
            } else {
                block.paragraph_gap
            };
            div().pb(gap).child(leaf).into_any_element()
        }
        Some(level) => {
            let (scale, line, section) = heading_metrics(level);
            let size = block.body_size * scale;
            let mut style = paragraph_style(
                block.base,
                size,
                device_line(size * line, window),
                block.colors,
            );
            style.font.weight = gpui::FontWeight::SEMIBOLD;
            if level >= 6 {
                style.color = block.colors.muted;
            }
            // The previous block already leaves a gap below itself, so only the rest of the
            // section gap goes above; a heading right after another stays close.
            let top = match (block.first, block.prev) {
                (true, _) | (_, None) => px(0.),
                (false, Some(Prev::Heading)) => size * 0.25,
                (false, Some(Prev::Rule)) => (size * section - block.body_size * 1.6).max(px(0.)),
                (false, Some(Prev::Other)) => (size * section - block.paragraph_gap).max(px(0.)),
            };
            let bottom = if block.last {
                px(0.)
            } else {
                size * HEADING_BOTTOM_GAP
            };
            let leaf = TextLeaf::new(block.key, block.inline, style, block.bands, cx, window, app);
            div().pt(top).pb(bottom).child(leaf).into_any_element()
        }
    }
}
