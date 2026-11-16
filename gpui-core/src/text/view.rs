//! TextView: Markdown drawn natively, selectable, with links as events for the host.

use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    AbsoluteLength, App, AvailableSpace, Bounds, ClipboardItem, Corners, CursorStyle,
    DefiniteLength, DispatchPhase, Element, ElementId, Fill, FocusHandle, FontWeight,
    GlobalElementId, HitboxBehavior, Hsla, InspectorElementId, InteractiveElement as _,
    IntoElement, KeyBinding, LayoutId, Length, Modifiers, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, ParentElement as _, Pixels, Point, Rems, RenderOnce, Role,
    SharedString, StatefulInteractiveElement as _, Style, StyleRefinement, Styled as _, Window,
    actions, div, fill, point, px, quad, relative, rems, size, transparent_black,
};
use unicode_segmentation::UnicodeSegmentation as _;

use gpui::TextAlign;

use super::markdown::{Align, Block, parse};
use super::paragraph::{CodeStyle, Paragraph, ParagraphLayout, ParagraphStyle};
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
    /// A table's header row's weight (semibold unless given).
    pub table_header_weight: Option<FontWeight>,
    /// The space between two blocks by their kinds, over every rule above: the web's margins
    /// depend on the pair (a heading has less below it before a paragraph than before a list).
    /// `None` for a pair keeps the view's own rule. It applies between siblings: a list's items
    /// keep [`list_item_gap`](Self::list_item_gap), and the first block anywhere has no space
    /// above it.
    pub block_gap: Option<BlockGap>,
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
            table_header_weight: None,
            block_gap: None,
            inline_code: InlineCodeStyle::default(),
            colors: TextViewColors::default(),
        }
    }
}

type OnLink = Rc<dyn Fn(&str, &Modifiers, &mut Window, &mut App)>;

/// Markdown, drawn. Nothing in it fetches or opens anything: a link press goes to
/// [`on_link_click`](TextView::on_link_click), and an image shows as its description.
#[derive(IntoElement)]
pub struct TextView {
    id: ElementId,
    source: SharedString,
    style: TextViewStyle,
    selectable: bool,
    on_link: Option<OnLink>,
}

impl TextView {
    pub fn markdown(id: impl Into<ElementId>, source: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            source: source.into(),
            style: TextViewStyle::default(),
            selectable: false,
            on_link: None,
        }
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

/// What a view keeps between draws.
struct ViewState {
    source: SharedString,
    blocks: Rc<Vec<Block>>,
    focus: FocusHandle,
    anchor: Option<Pos>,
    head: Option<Pos>,
    selecting: bool,
    /// A press that hasn't moved yet: a link press if it lets go here.
    press: Option<(Point<Pixels>, Pos)>,
    layout: Option<Rc<ViewLayout>>,
    bounds: Bounds<Pixels>,
}

impl ViewState {
    fn selection(&self) -> Option<(Pos, Pos)> {
        let (a, h) = (self.anchor?, self.head?);
        (a != h).then(|| if a <= h { (a, h) } else { (h, a) })
    }

    fn selected_text(&self) -> Option<String> {
        let ((l0, o0), (l1, o1)) = self.selection()?;
        let layout = self.layout.as_ref()?;
        let mut out = String::new();
        for i in l0..=l1 {
            let leaf = layout.leaves.get(i)?;
            let t = &leaf.paragraph.text;
            let (s, e) = (
                if i == l0 { o0 } else { 0 },
                if i == l1 { o1 } else { t.len() },
            );
            if i > l0 {
                out.push_str(leaf.separator);
            }
            out.push_str(&t[s.min(t.len())..e.min(t.len())]);
        }
        Some(out)
    }
}

/// A paragraph placed in the view.
struct Leaf {
    origin: Point<Pixels>,
    layout: ParagraphLayout,
    paragraph: Rc<Paragraph>,
    /// What goes before it when copied with the one above.
    separator: &'static str,
}

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
        layout: ParagraphLayout,
    },
}

struct ViewLayout {
    width: Pixels,
    key: StyleKey,
    height: Pixels,
    leaves: Vec<Leaf>,
    decos: Vec<Deco>,
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
            let margins = |b: &Block| match b {
                Block::Heading(level, _) => Some(cx.heading_margins(*level)),
                _ => None,
            };
            let host = cx
                .style
                .block_gap
                .as_ref()
                .and_then(|f| f(kind(&blocks[i - 1]), kind(b)));
            y += host.unwrap_or_else(|| match (margins(&blocks[i - 1]), margins(b)) {
                (Some((_, below)), Some((above, _))) => below.max(above),
                (Some((_, below)), None) => below,
                (None, Some((above, _))) => above,
                (None, None) => cx.gap,
            });
        }
        let sep = if out.leaves.is_empty() { "" } else { "\n\n" };
        match b {
            Block::Paragraph(p) => y = leaf(p, &cx.base, x, width, y, sep, out, window),
            Block::Heading(level, p) => {
                let s = cx.heading(*level);
                y = leaf(p, &s, x, width, y, sep, out, window);
            }
            Block::Code { text, .. } => {
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
                y = leaf(
                    &Paragraph::plain(text.clone()),
                    &s,
                    x + pl + bw,
                    inner,
                    y + pt + bw,
                    sep,
                    out,
                    window,
                );
                y += pb + bw;
                out.decos[deco_ix] = Deco::Box {
                    bounds: Bounds::new(point(x, top), size(width, y - top)),
                    fill: fill_,
                    border,
                    border_width: bw,
                    radius: Corners::all(radius),
                };
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
                let indent = cx
                    .style
                    .list_indent
                    .unwrap_or(cx.rem * if start.is_some() { 1.6 } else { 1.2 })
                    .min(width * 0.25);
                let gap = cx.style.list_marker_gap.unwrap_or(indent * 0.3);
                for (n, item) in items.iter().enumerate() {
                    if n > 0 {
                        y += cx.style.list_item_gap.unwrap_or(cx.gap * 0.4);
                    }
                    let marker = match (start, item.checked) {
                        (_, Some(true)) => "☑".to_string(),
                        (_, Some(false)) => "☐".to_string(),
                        (Some(s), None) => format!("{}.", *s as usize + n),
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
                    // Its end sits the gap before the text; a marker too wide for that starts
                    // at the list's edge.
                    let marker_w = m.position_for(len).x;
                    let mx = (indent - gap - marker_w).max(px(0.));
                    out.decos.push(Deco::Text {
                        origin: point(x + mx, y),
                        layout: m,
                    });
                    let before = out.leaves.len();
                    y = lay(&item.blocks, x + indent, width - indent, y, cx, out, window);
                    if let Some(l) = out.leaves.get_mut(before)
                        && before > 0
                    {
                        l.separator = "\n";
                    }
                }
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
                let cols = rows.iter().map(|r| r.len()).max().unwrap_or(0).max(1);
                let pad = cx.style.table_cell_padding.unwrap_or(cx.rem * 0.5);
                let col_w = width / cols as f32;
                let top = y;
                let radius = cx
                    .style
                    .table_radius
                    .unwrap_or(cx.theme.radius_md().to_pixels(cx.rem));
                for (ri, row) in rows.iter().enumerate() {
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
                    let s = if ri == 0 {
                        let mut s = cx.with_size(cx.base.size, true);
                        if let Some(w) = cx.style.table_header_weight {
                            s.font.weight = w;
                        }
                        if let Some(c) = cx.style.colors.table_header_text {
                            s.color = c;
                        }
                        s
                    } else {
                        cx.base.clone()
                    };
                    for c in 0..cols {
                        let p = row.get(c).cloned().unwrap_or_default();
                        let cx0 = x + col_w * c as f32 + pad;
                        let inner = (col_w - pad * 2.).max(px(8.));
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
                        let l = ParagraphLayout::new(&p, &cs, inner, window);
                        let h = l.height;
                        out.leaves.push(Leaf {
                            origin: point(cx0, row_top + pad),
                            layout: l,
                            paragraph: Rc::new(p),
                            separator: sep,
                        });
                        row_bottom = row_bottom.max(row_top + pad * 2. + h);
                    }
                    y = row_bottom;
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
            }
        }
    }
    y
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
    let l = ParagraphLayout::new(p, s, width, window);
    let h = l.height;
    out.leaves.push(Leaf {
        origin: point(x, y),
        layout: l,
        paragraph: Rc::new(p.clone()),
        separator: sep,
    });
    y + h
}

/// What a layout depends on besides its width: the text's size, colour and family, and the rem.
type StyleKey = (Pixels, Hsla, SharedString, Pixels);

fn style_key(text: &gpui::TextStyle, rem: Pixels) -> StyleKey {
    (
        text.font_size.to_pixels(rem),
        text.color,
        text.font_family.clone(),
        rem,
    )
}

/// Lays the view out at `width` in `text`, the text style around it. That style is taken where
/// GPUI applies its parents' (while laying out or painting), not inside a measure, which runs
/// outside them.
fn layout_for(
    state: &ViewState,
    style: &TextViewStyle,
    text: &gpui::TextStyle,
    width: Pixels,
    window: &mut Window,
    cx: &App,
) -> ViewLayout {
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
    let mut out = ViewLayout {
        width,
        key: style_key(text, rem),
        height: px(0.),
        leaves: Vec::new(),
        decos: Vec::new(),
    };
    out.height = lay(&state.blocks, px(0.), width, px(0.), &c, &mut out, window);
    out
}

impl RenderOnce for TextView {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::init_soon(cx);
        let state = window.use_keyed_state(self.id.clone(), cx, |_, cx| ViewState {
            source: SharedString::default(),
            blocks: Rc::new(Vec::new()),
            focus: cx.focus_handle(),
            anchor: None,
            head: None,
            selecting: false,
            press: None,
            layout: None,
            bounds: Bounds::default(),
        });
        state.update(cx, |s, _| {
            if s.source != self.source {
                s.blocks = Rc::new(parse(&self.source));
                s.source = self.source.clone();
                s.layout = None;
                s.anchor = None;
                s.head = None;
            }
        });
        let focus = state.read(cx).focus.clone();
        let plain: SharedString = self.source.clone();
        let st = state.clone();
        let st2 = state.clone();
        let st3 = state.clone();
        div()
            .id(self.id.clone())
            .key_context(CONTEXT)
            .track_focus(&focus)
            .role(Role::Document)
            .aria_label(plain)
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
                        && let Some(last) = l.leaves.len().checked_sub(1)
                    {
                        s.anchor = Some((0, 0));
                        s.head = Some((last, l.leaves[last].paragraph.text.len()));
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
                style: self.style,
                selectable: self.selectable,
                on_link: self.on_link,
            })
    }
}

/// A draw's layout, and the hitboxes its cursors go on.
struct Prepainted {
    layout: Rc<ViewLayout>,
    body: Option<gpui::Hitbox>,
    links: Vec<gpui::Hitbox>,
}

struct ViewElement {
    state: gpui::Entity<ViewState>,
    style: TextViewStyle,
    selectable: bool,
    on_link: Option<OnLink>,
}

impl IntoElement for ViewElement {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl ViewLayout {
    /// The place nearest `p`, from the view's top left.
    fn pos_at(&self, p: Point<Pixels>) -> Option<Pos> {
        let mut best: Option<(Pixels, usize)> = None;
        for (i, l) in self.leaves.iter().enumerate() {
            let top = l.origin.y;
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
                best = Some((d, i));
            }
        }
        let (_, i) = best?;
        let l = &self.leaves[i];
        Some((i, l.layout.index_for(p - l.origin)))
    }

    fn word_at(&self, (leaf, at): Pos) -> (Pos, Pos) {
        let t = &self.leaves[leaf].paragraph.text;
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
        let text_style = window.text_style();
        let id = window.request_measured_layout(style, move |known, available, window, cx| {
            let width = known.width.or(match available.width {
                AvailableSpace::Definite(w) => Some(w),
                _ => None,
            });
            let width = width.unwrap_or(px(400.));
            let key = style_key(&text_style, window.rem_size());
            let cached = state
                .read(cx)
                .layout
                .clone()
                .filter(|l| l.width == width && l.key == key);
            let l = match cached {
                Some(l) => l,
                None => {
                    let l = Rc::new(layout_for(
                        state.read(cx),
                        &tv_style,
                        &text_style,
                        width,
                        window,
                        cx,
                    ));
                    state.update(cx, |s, _| s.layout = Some(l.clone()));
                    l
                }
            };
            size(width, l.height)
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
        let text_style = window.text_style();
        let key = style_key(&text_style, window.rem_size());
        let cached = self
            .state
            .read(cx)
            .layout
            .clone()
            .filter(|l| l.width == width && l.key == key);
        let l = match cached {
            Some(l) => l,
            None => Rc::new(layout_for(
                self.state.read(cx),
                &self.style,
                &text_style,
                width,
                window,
                cx,
            )),
        };
        self.state.update(cx, |s, _| {
            s.layout = Some(l.clone());
            s.bounds = bounds;
        });
        // The text takes an I-beam; links, above it, a pointing hand.
        let body = self
            .selectable
            .then(|| window.insert_hitbox(bounds, HitboxBehavior::Normal));
        let mut links = Vec::new();
        for leaf in &l.leaves {
            for (r, m) in &leaf.paragraph.spans {
                if m.link.is_some() {
                    for b in leaf.layout.bands(r.clone()) {
                        links.push(window.insert_hitbox(
                            Bounds::new(bounds.origin + leaf.origin + b.origin, b.size),
                            HitboxBehavior::Normal,
                        ));
                    }
                }
            }
        }
        Prepainted {
            layout: l,
            body,
            links,
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
        let (focused, selection) = {
            let s = self.state.read(cx);
            (s.focus.is_focused(window), s.selection())
        };
        for d in &l.decos {
            if let Deco::Box {
                bounds: b,
                fill: f,
                border,
                border_width,
                radius,
            } = d
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
        if focused && let Some(((l0, o0), (l1, o1))) = selection {
            for (i, leaf) in l.leaves.iter().enumerate().take(l1 + 1).skip(l0) {
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
        for d in &l.decos {
            if let Deco::Text { origin, layout } = d {
                layout.paint(o + *origin, window, cx);
            }
        }
        // Each paragraph is clipped to the width it was laid out in, with three pixels' room
        // either side for ink past a glyph's advance: a character wider than a narrow table
        // cell is cut there rather than drawn over its neighbour.
        for leaf in &l.leaves {
            // Ink reaches further past the advance the bigger the text (a large emoji's most),
            // so the room is a share of the paragraph's line.
            let ink = (window.rem_size() * 0.2).max(leaf.layout.line_height * 0.3);
            let clip = Bounds::new(
                o + leaf.origin - point(ink, px(0.)),
                size(leaf.layout.width + ink * 2., leaf.layout.height),
            );
            window.with_content_mask(Some(gpui::ContentMask { bounds: clip }), |window| {
                leaf.layout.paint(o + leaf.origin, window, cx)
            });
        }

        if let Some(hb) = &prepainted.body {
            window.set_cursor_style(CursorStyle::IBeam, hb);
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
            move |e: &MouseDownEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble
                    || e.button != MouseButton::Left
                    || !bounds.contains(&e.position)
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
                                (pos.0, layout.leaves[pos.0].paragraph.text.len()),
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
                        .leaves
                        .get(leaf)
                        .and_then(|l| l.paragraph.link_at(at))
                        .cloned()
                {
                    let collapsed = state.read(cx).selection().is_none();
                    if collapsed || !selectable {
                        f(&url, &e.modifiers, window, cx);
                    }
                }
            });
        }
    }
}
