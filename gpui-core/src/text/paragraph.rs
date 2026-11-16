//! The paragraph layer: styled text and inline boxes wrapped to a width, and the geometry around
//! it: the rows, where a caret goes, which offset a point falls on, the bands a range covers,
//! where each box is. TextView lays its paragraphs out with it; an editor (the block editor's
//! carets, ↑ ↓, Home and End) can too.
//!
//! Offsets are byte offsets into [`Paragraph::text`]. A line break (`\n`) ends its row and
//! belongs to none; an inline box is one U+FFFC in the text.

use std::ops::Range;

use gpui::{
    App, Bounds, Font, FontStyle, FontWeight, Hsla, Pixels, Point, ShapedLine, SharedString,
    StrikethroughStyle, TextAlign, TextRun, UnderlineStyle, Window, fill, point, px, size,
};
use unicode_segmentation::UnicodeSegmentation as _;

/// The character an inline box stands for in the text.
pub const BOX: char = '\u{fffc}';
const BOX_LEN: usize = 3;

/// How a stretch of text is marked.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Marks {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub underline: bool,
    pub code: bool,
    /// The address a link goes to. The paragraph only draws it; what a press does is the host's.
    pub link: Option<SharedString>,
    /// Drawn in the muted colour (an image's description, say).
    pub muted: bool,
    /// A fill behind the text.
    pub highlight: Option<Hsla>,
    /// The text's own colour, over the style's.
    pub color: Option<Hsla>,
}

/// A box in the text that the host draws an element in: its size, and where its baseline is
/// from its top (the row's baseline goes through it there).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InlineBox {
    pub width: Pixels,
    pub height: Pixels,
    pub baseline: Pixels,
}

/// Text with marks and inline boxes: `spans` cover `text` from start to end, in order; each box
/// is a U+FFFC in the text at its offset.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Paragraph {
    pub text: String,
    pub spans: Vec<(Range<usize>, Marks)>,
    pub boxes: Vec<(usize, InlineBox)>,
}

impl Paragraph {
    pub fn plain(text: impl Into<String>) -> Self {
        let text = text.into();
        let len = text.len();
        Self {
            text,
            spans: vec![(0..len, Marks::default())],
            boxes: Vec::new(),
        }
    }

    /// Adds `text` with `marks` at the end.
    pub fn push(&mut self, text: &str, marks: Marks) {
        if text.is_empty() {
            return;
        }
        let start = self.text.len();
        self.text.push_str(text);
        match self.spans.last_mut() {
            Some((r, m)) if *m == marks && r.end == start => r.end = self.text.len(),
            _ => self.spans.push((start..self.text.len(), marks)),
        }
    }

    /// Adds an inline box at the end.
    pub fn push_box(&mut self, b: InlineBox) {
        let at = self.text.len();
        let mut s = [0u8; 4];
        self.push(BOX.encode_utf8(&mut s), Marks::default());
        self.boxes.push((at, b));
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// The link at `offset`, if any.
    pub fn link_at(&self, offset: usize) -> Option<&SharedString> {
        self.spans
            .iter()
            .find(|(r, _)| r.start <= offset && offset < r.end)
            .and_then(|(_, m)| m.link.as_ref())
    }

    fn box_at_offset(&self, offset: usize) -> Option<InlineBox> {
        self.boxes
            .iter()
            .find(|(o, _)| *o == offset)
            .map(|(_, b)| *b)
    }
}

/// How a paragraph draws: its font, size, colours, alignment, and the look of code and links.
#[derive(Clone, Debug)]
pub struct ParagraphStyle {
    pub font: Font,
    pub size: Pixels,
    pub line_height: Pixels,
    pub color: Hsla,
    pub muted: Hsla,
    pub link: Hsla,
    pub code_family: SharedString,
    pub code_background: Hsla,
    pub align: TextAlign,
    /// How inline code is set, beyond its family and fill.
    pub code: CodeStyle,
}

/// Inline code's own size, padding, corners and colour. The default is code at the text's size
/// with no padding, square corners, in the text's colour.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CodeStyle {
    /// Its font size (the text's unless given).
    pub size: Option<Pixels>,
    /// Space either side of it, inside its fill; half as much above and below.
    pub padding: Pixels,
    /// Its fill's corners.
    pub radius: Pixels,
    /// Its text's colour (the text's unless given).
    pub color: Option<Hsla>,
}

impl ParagraphStyle {
    fn run(&self, len: usize, m: &Marks) -> TextRun {
        let mut font = self.font.clone();
        if m.bold {
            font.weight = FontWeight(font.weight.0.max(FontWeight::BOLD.0));
        }
        if m.italic {
            font.style = FontStyle::Italic;
        }
        if m.code {
            font.family = self.code_family.clone();
        }
        let color = m.color.unwrap_or(
            if m.code
                && let Some(c) = self.code.color
            {
                c
            } else if m.link.is_some() {
                self.link
            } else if m.muted {
                self.muted
            } else {
                self.color
            },
        );
        let underline = m.link.is_some() || m.underline;
        TextRun {
            len,
            font,
            color,
            // Inline code's fill is painted by the layout, with its padding and corners.
            background_color: m.highlight,
            underline: underline.then(|| UnderlineStyle {
                color: Some(if m.link.is_some() {
                    self.link.opacity(0.5)
                } else {
                    color
                }),
                thickness: px(1.),
                wavy: false,
            }),
            strikethrough: m.strike.then(|| StrikethroughStyle {
                color: Some(color),
                thickness: px(1.),
            }),
        }
    }

    /// The runs for `range` of `p`.
    fn runs(&self, p: &Paragraph, range: Range<usize>) -> Vec<TextRun> {
        let mut out = Vec::new();
        for (r, m) in &p.spans {
            let (s, e) = (r.start.max(range.start), r.end.min(range.end));
            if s < e {
                out.push(self.run(e - s, m));
            }
        }
        if out.is_empty() && !range.is_empty() {
            out.push(self.run(range.len(), &Marks::default()));
        }
        out
    }
}

/// Which side of a soft wrap a caret at the wrap's offset is drawn on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Affinity {
    /// The end of the upper row.
    Upstream,
    /// The start of the lower row.
    Downstream,
}

/// A row of the laid-out paragraph.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// Its text: a line break ends a row and belongs to none.
    pub range: Range<usize>,
    pub top: Pixels,
    pub height: Pixels,
    /// From the row's top.
    pub baseline: Pixels,
}

enum Piece {
    Text {
        range: Range<usize>,
        x: Pixels,
        line: Box<ShapedLine>,
        /// Inline code's padding before and after it (none for other text).
        pad: (Pixels, Pixels),
        /// Its text's baseline from the top of a line of the paragraph's line height.
        baseline: Pixels,
        code: bool,
    },
    Box {
        offset: usize,
        x: Pixels,
        b: InlineBox,
    },
}

impl Piece {
    fn range(&self) -> Range<usize> {
        match self {
            Piece::Text { range, .. } => range.clone(),
            Piece::Box { offset, .. } => *offset..*offset + BOX_LEN,
        }
    }

    fn x(&self) -> Pixels {
        match self {
            Piece::Text { x, .. } | Piece::Box { x, .. } => *x,
        }
    }

    fn width(&self) -> Pixels {
        match self {
            Piece::Text { line, pad, .. } => pad.0 + line.width + pad.1,
            Piece::Box { b, .. } => b.width,
        }
    }

    /// The x of `offset` in this piece (which holds it).
    fn x_for(&self, offset: usize) -> Pixels {
        match self {
            Piece::Text {
                range,
                x,
                line,
                pad,
                ..
            } => *x + pad.0 + line.x_for_index(offset - range.start),
            Piece::Box { offset: o, x, b } => *x + if offset > *o { b.width } else { px(0.) },
        }
    }
}

/// A run of text or a box that's laid out whole on one row unless it alone is too wide.
struct Atom {
    range: Range<usize>,
    /// Its width, and its width without the spaces it ends with.
    width: Pixels,
    inked: Pixels,
    is_box: bool,
}

/// Whether `c` is written right to left (Hebrew, Arabic, Syriac, Thaana, N'Ko and their
/// presentation forms).
fn is_rtl(c: char) -> bool {
    matches!(c as u32, 0x0590..=0x08ff | 0xfb1d..=0xfdff | 0xfe70..=0xfeff)
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x11ff | 0x2e80..=0x9fff | 0xa960..=0xa97f | 0xac00..=0xd7ff
        | 0xf900..=0xfaff | 0xfe30..=0xfe4f | 0xff00..=0xffef | 0x20000..=0x3ffff)
}

/// Where a row may break inside `text`: after spaces, on either side of a CJK character, and
/// after a hyphen between letters or digits (`well-known`, not `-5` or `--`).
fn breaks(text: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut before: Option<char> = None;
    let mut prev: Option<char> = None;
    for (i, c) in text.char_indices() {
        if let Some(p) = prev {
            let hyphen =
                p == '-' && c.is_alphanumeric() && before.is_some_and(char::is_alphanumeric);
            if (p.is_whitespace() && !c.is_whitespace())
                || is_cjk(p)
                || (is_cjk(c) && !p.is_whitespace())
                || hyphen
            {
                out.push(i);
            }
        }
        before = prev;
        prev = Some(c);
    }
    out
}

/// A paragraph laid out to a width.
pub struct ParagraphLayout {
    rows: Vec<Row>,
    pieces: Vec<Vec<Piece>>,
    /// Inline code's fill, its corners, and how far it reaches above and below the code's ink.
    code_fill: Hsla,
    code_radius: Pixels,
    code_pad_y: Pixels,
    pub line_height: Pixels,
    pub width: Pixels,
    pub height: Pixels,
}

impl ParagraphLayout {
    /// Lays `p` out in `style`, wrapped to `width`. Code wraps like everything else, so nothing
    /// runs out of its box; a word wider than the row breaks between characters.
    pub fn new(p: &Paragraph, style: &ParagraphStyle, width: Pixels, window: &mut Window) -> Self {
        let lh = style.line_height;
        let ts = window.text_system().clone();
        // Inline code's spans: shaped at their own size, padded, with fills of their own.
        let mut code_spans: Vec<Range<usize>> = Vec::new();
        for (r, m) in &p.spans {
            if m.code {
                match code_spans.last_mut() {
                    Some(last) if last.end == r.start => last.end = r.end,
                    _ => code_spans.push(r.clone()),
                }
            }
        }
        let code_of = |offset: usize| code_spans.iter().position(|c| c.contains(&offset));
        let code_size = style.code.size.unwrap_or(style.size);
        let pad = style.code.padding;
        let size_at = |offset: usize| {
            if code_of(offset).is_some() {
                code_size
            } else {
                style.size
            }
        };
        let shape = |range: Range<usize>| -> ShapedLine {
            let runs = style.runs(p, range.clone());
            let size = size_at(range.start);
            ts.shape_line(
                SharedString::from(p.text[range].to_string()),
                size,
                &runs,
                None,
            )
        };
        let probe = ts.shape_line(
            " ".into(),
            style.size,
            &[style.run(1, &Marks::default())],
            None,
        );
        let text_baseline = (lh - probe.ascent - probe.descent) / 2. + probe.ascent;
        let avail = width.max(px(1.));

        let mut rows = Vec::new();
        let mut pieces_by_row = Vec::new();
        let mut top = px(0.);
        let mut start = 0;
        for hard in p.text.split('\n') {
            let line = start..start + hard.len();
            start = line.end + 1;

            // The hard line's atoms: text between break opportunities, and boxes.
            let mut segments = Vec::new();
            let mut seg_start = line.start;
            for (o, _) in p.boxes.iter().filter(|(o, _)| line.contains(o)) {
                if *o > seg_start {
                    segments.push((seg_start..*o, false));
                }
                segments.push((*o..*o + BOX_LEN, true));
                seg_start = o + BOX_LEN;
            }
            if seg_start < line.end {
                segments.push((seg_start..line.end, false));
            }
            let segments: Vec<(Range<usize>, bool)> = segments
                .into_iter()
                .flat_map(|(seg, is_box)| {
                    if is_box {
                        return vec![(seg, true)];
                    }
                    let mut cuts = vec![seg.start, seg.end];
                    for c in &code_spans {
                        for e in [c.start, c.end] {
                            if e > seg.start && e < seg.end {
                                cuts.push(e);
                            }
                        }
                    }
                    cuts.sort();
                    cuts.dedup();
                    cuts.windows(2).map(|w| (w[0]..w[1], false)).collect()
                })
                .collect();
            let mut atoms = Vec::new();
            for (seg, is_box) in segments {
                if is_box {
                    let b = p.box_at_offset(seg.start).unwrap_or_default();
                    atoms.push(Atom {
                        range: seg,
                        width: b.width,
                        inked: b.width,
                        is_box: true,
                    });
                    continue;
                }
                let shaped = shape(seg.clone());
                let text = &p.text[seg.clone()];
                // Right-to-left text runs leftwards in a shaped line, so offsets in it don't
                // measure a word: such words are shaped on their own.
                let rtl = text.chars().any(is_rtl);
                let mut cuts = breaks(text);
                cuts.push(text.len());
                let mut a = 0;
                for b in cuts {
                    let trimmed = a + text[a..b].trim_end().len();
                    let measure = |end: usize| {
                        if end == a {
                            px(0.)
                        } else if rtl {
                            shape(seg.start + a..seg.start + end).width
                        } else {
                            shaped.x_for_index(end) - shaped.x_for_index(a)
                        }
                    };
                    let mut w = measure(b);
                    let mut inked = measure(trimmed);
                    if let Some(c) = code_of(seg.start).map(|i| &code_spans[i]) {
                        let (first, last) = (seg.start + a == c.start, seg.start + b == c.end);
                        let padding = pad * (first as u8 + last as u8) as f32;
                        w += padding;
                        inked += padding;
                    }
                    atoms.push(Atom {
                        range: seg.start + a..seg.start + b,
                        width: w,
                        inked,
                        is_box: false,
                    });
                    a = b;
                }
            }

            // Greedy rows; a lone atom too wide for a row breaks between graphemes.
            let mut row_ranges: Vec<Vec<Range<usize>>> = vec![Vec::new()];
            let mut used = px(0.);
            for atom in atoms {
                let fits = used + atom.inked <= avail + px(0.01);
                if !fits && !row_ranges.last().is_some_and(Vec::is_empty) {
                    row_ranges.push(Vec::new());
                    used = px(0.);
                }
                if used + atom.inked <= avail + px(0.01) || atom.is_box {
                    used += atom.width;
                    row_ranges.last_mut().expect("a row").push(atom.range);
                    continue;
                }
                // Too wide alone: as many graphemes as fit (at least one) to a row.
                let shaped = shape(atom.range.clone());
                let text = &p.text[atom.range.clone()];
                let mut row_start = 0;
                let mut last_fit = 0;
                for (g, s) in text.grapheme_indices(true) {
                    let end = g + s.len();
                    let w = if text.chars().any(is_rtl) {
                        shape(atom.range.start + row_start..atom.range.start + end).width
                    } else {
                        shaped.x_for_index(end) - shaped.x_for_index(row_start)
                    };
                    if w > avail && last_fit > row_start {
                        let r = atom.range.start + row_start..atom.range.start + last_fit;
                        row_ranges.last_mut().expect("a row").push(r);
                        row_ranges.push(Vec::new());
                        row_start = last_fit;
                    }
                    last_fit = end;
                }
                used = (shaped.x_for_index(text.len()) - shaped.x_for_index(row_start)).abs();
                row_ranges
                    .last_mut()
                    .expect("a row")
                    .push(atom.range.start + row_start..atom.range.end);
            }

            // Each row's pieces, shaped as drawn; its height and baseline; its alignment.
            for ranges in &row_ranges {
                let mut pieces = Vec::new();
                let mut x = px(0.);
                let mut above = text_baseline;
                let mut below = lh - text_baseline;
                // Text between boxes and code spans' edges is shaped as one piece.
                let mut text_run: Option<Range<usize>> = None;
                let flush =
                    |run: &mut Option<Range<usize>>, x: &mut Pixels, pieces: &mut Vec<Piece>| {
                        if let Some(r) = run.take() {
                            let line = shape(r.clone());
                            let code = code_of(r.start).map(|i| code_spans[i].clone());
                            let pads = match &code {
                                Some(c) => (
                                    if r.start == c.start { pad } else { px(0.) },
                                    if r.end == c.end { pad } else { px(0.) },
                                ),
                                None => (px(0.), px(0.)),
                            };
                            let baseline = (lh - line.ascent - line.descent) / 2. + line.ascent;
                            let w = pads.0 + line.width + pads.1;
                            pieces.push(Piece::Text {
                                range: r,
                                x: *x,
                                line: Box::new(line),
                                pad: pads,
                                baseline,
                                code: code.is_some(),
                            });
                            *x += w;
                        }
                    };
                for r in ranges {
                    let boxed = (r.len() == BOX_LEN && p.text[r.clone()].starts_with(BOX))
                        .then(|| p.box_at_offset(r.start))
                        .flatten();
                    if let Some(b) = boxed {
                        flush(&mut text_run, &mut x, &mut pieces);
                        pieces.push(Piece::Box {
                            offset: r.start,
                            x,
                            b,
                        });
                        x += b.width;
                        above = above.max(b.baseline);
                        below = below.max(b.height - b.baseline);
                    } else {
                        // A new piece where code starts or ends.
                        if let Some(t) = &text_run
                            && code_of(t.start) != code_of(r.start)
                        {
                            flush(&mut text_run, &mut x, &mut pieces);
                        }
                        text_run = Some(match text_run {
                            Some(t) => t.start..r.end,
                            None => r.clone(),
                        });
                    }
                }
                flush(&mut text_run, &mut x, &mut pieces);
                let range = match (ranges.first(), ranges.last()) {
                    (Some(a), Some(b)) => a.start..b.end,
                    // An empty line (or an empty paragraph) is a row of its own.
                    _ => line.end..line.end,
                };
                // Trailing spaces hang past the edge, as in browsers: alignment ignores them.
                let inked = pieces.last().map_or(px(0.), |pc| match pc {
                    Piece::Text {
                        range,
                        x,
                        line,
                        pad,
                        ..
                    } => {
                        let x = &(*x + pad.0);
                        let t = &p.text[range.clone()];
                        if t.chars().any(is_rtl) {
                            // Offsets don't measure right-to-left text; its shaped width does.
                            *x + line.width
                        } else {
                            *x + line.x_for_index(t.trim_end().len())
                        }
                    }
                    Piece::Box { x, b, .. } => *x + b.width,
                });
                let shift = match style.align {
                    TextAlign::Center => ((avail - inked) / 2.).max(px(0.)),
                    TextAlign::Right => (avail - inked).max(px(0.)),
                    _ => px(0.),
                };
                for pc in &mut pieces {
                    match pc {
                        Piece::Text { x, .. } | Piece::Box { x, .. } => *x += shift,
                    }
                }
                let height = above + below;
                rows.push(Row {
                    range,
                    top,
                    height,
                    baseline: above,
                });
                pieces_by_row.push(pieces);
                top += height;
            }
        }
        Self {
            rows,
            pieces: pieces_by_row,
            code_fill: style.code_background,
            code_radius: style.code.radius,
            code_pad_y: style.code.padding / 2.,
            line_height: lh,
            width,
            height: top.max(lh),
        }
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// The row a caret at `offset` is drawn on.
    fn row_for(&self, offset: usize, affinity: Affinity) -> usize {
        let last = self.rows.len().saturating_sub(1);
        for (i, r) in self.rows.iter().enumerate() {
            if offset < r.range.start {
                // Between rows (on a line break): the end of the row before.
                return i.saturating_sub(1);
            }
            if offset < r.range.end {
                return i;
            }
            if offset == r.range.end {
                // At a soft wrap the next row starts here too: downstream belongs there.
                let wraps = self
                    .rows
                    .get(i + 1)
                    .is_some_and(|n| n.range.start == offset);
                if !(wraps && affinity == Affinity::Downstream) {
                    return i;
                }
            }
        }
        last
    }

    /// The x of `offset` on row `i`, kept inside the paragraph's width (a soft wrap's trailing
    /// space would put it past the edge).
    fn x_on_row(&self, i: usize, offset: usize) -> Pixels {
        let row = &self.rows[i];
        let pieces = &self.pieces[i];
        let o = offset.clamp(row.range.start, row.range.end);
        let x = pieces
            .iter()
            .find(|pc| {
                let r = pc.range();
                o >= r.start && o < r.end
            })
            .map(|pc| pc.x_for(o))
            .or_else(|| pieces.last().map(|pc| pc.x() + pc.width()))
            .unwrap_or(px(0.));
        // To a 256th of a pixel, so text split into more runs (marks) sits exactly where it did.
        let x = x.min(self.width.max(px(0.)));
        px((f32::from(x) * 256.).round() / 256.)
    }

    /// The caret at `offset`: its row's full height, two pixels wide.
    pub fn caret(&self, offset: usize, affinity: Affinity) -> Bounds<Pixels> {
        let i = self.row_for(offset, affinity);
        let row = &self.rows[i];
        Bounds::new(
            point(self.x_on_row(i, offset), row.top),
            size(px(2.), row.height),
        )
    }

    /// Where `offset` is drawn (its caret's top left, downstream).
    pub fn position_for(&self, offset: usize) -> Point<Pixels> {
        self.caret(offset, Affinity::Downstream).origin
    }

    /// The offset nearest `p`, from the paragraph's top left. Above the text is its start and
    /// below it its end; left or right of a row is that row's start or end.
    pub fn index_for(&self, p: Point<Pixels>) -> usize {
        let Some(first) = self.rows.first() else {
            return 0;
        };
        if p.y < first.top {
            return 0;
        }
        let last = self.rows.len() - 1;
        let i = self
            .rows
            .iter()
            .position(|r| p.y < r.top + r.height)
            .unwrap_or(last);
        if p.y >= self.height {
            return self.rows[last].range.end;
        }
        let row = &self.rows[i];
        let pieces = &self.pieces[i];
        let Some(left) = pieces.first() else {
            return row.range.start;
        };
        if p.x <= left.x() {
            return row.range.start;
        }
        for pc in pieces {
            if p.x < pc.x() + pc.width() {
                return match pc {
                    Piece::Text {
                        range,
                        x,
                        line,
                        pad,
                        ..
                    } => range.start + line.closest_index_for_x((p.x - *x - pad.0).max(px(0.))),
                    Piece::Box { offset, x, b } => {
                        if p.x - *x < b.width / 2. {
                            *offset
                        } else {
                            offset + BOX_LEN
                        }
                    }
                };
            }
        }
        row.range.end
    }

    /// The bands `range` covers, one per row it touches (an empty row inside it included), from
    /// the paragraph's top left. A row the range runs past ends a little beyond its text, so a
    /// selected line break shows.
    pub fn bands(&self, range: Range<usize>) -> Vec<Bounds<Pixels>> {
        if range.is_empty() {
            return Vec::new();
        }
        let mark = self.line_height * 0.3;
        let mut out = Vec::new();
        for (i, row) in self.rows.iter().enumerate() {
            let touches = if row.range.is_empty() {
                row.range.start >= range.start && row.range.start < range.end
            } else {
                row.range.start < range.end && row.range.end > range.start
            };
            if !touches {
                continue;
            }
            let x0 = self.x_on_row(i, range.start.max(row.range.start));
            let x1 = if range.end <= row.range.end {
                self.x_on_row(i, range.end)
            } else {
                self.x_on_row(i, row.range.end) + mark
            };
            let x1 = x1.max(x0 + if row.range.is_empty() { mark } else { px(0.) });
            if x1 > x0 {
                out.push(Bounds::from_corners(
                    point(x0, row.top),
                    point(x1, row.top + row.height),
                ));
            }
        }
        out
    }

    /// Each inline box's offset and where it's drawn, from the paragraph's top left.
    pub fn boxes(&self) -> Vec<(usize, Bounds<Pixels>)> {
        let mut out = Vec::new();
        for (row, pieces) in self.rows.iter().zip(&self.pieces) {
            for pc in pieces {
                if let Piece::Box { offset, x, b } = pc {
                    let top = row.top + row.baseline - b.baseline;
                    out.push((
                        *offset,
                        Bounds::new(point(*x, top), size(b.width, b.height)),
                    ));
                }
            }
        }
        out
    }

    /// The box under `p`, if any.
    pub fn box_at(&self, p: Point<Pixels>) -> Option<usize> {
        self.boxes()
            .into_iter()
            .find(|(_, b)| b.contains(&p))
            .map(|(o, _)| o)
    }

    /// Paints the text, its highlights and its decorations at `origin`. The boxes are the
    /// host's to draw, at [`boxes`](Self::boxes).
    pub fn paint(&self, origin: Point<Pixels>, window: &mut Window, cx: &mut App) {
        let lh = self.line_height;
        for (row, pieces) in self.rows.iter().zip(&self.pieces) {
            for pc in pieces {
                if let Piece::Text {
                    x,
                    line,
                    pad,
                    baseline,
                    code,
                    ..
                } = pc
                {
                    let base = row.top + row.baseline;
                    // Inline code's fill: round its ink, padded.
                    if *code {
                        let top = base - line.ascent - self.code_pad_y;
                        let bottom = base + line.descent + self.code_pad_y;
                        let b = Bounds::from_corners(
                            origin + point(*x, top),
                            origin + point(*x + pad.0 + line.width + pad.1, bottom),
                        );
                        window.paint_quad(gpui::quad(
                            b,
                            self.code_radius,
                            self.code_fill,
                            px(0.),
                            gpui::transparent_black(),
                            Default::default(),
                        ));
                    }
                    // The text's line, placed so its baseline is the row's.
                    let at = origin + point(*x + pad.0, base - *baseline);
                    let _ = line.paint_background(at, lh, TextAlign::Left, None, window, cx);
                    let _ = line.paint(at, lh, TextAlign::Left, None, window, cx);
                }
            }
        }
    }

    /// Fills the bands of `range` in `color` at `origin` (paint text after it).
    pub fn paint_bands(
        &self,
        range: Range<usize>,
        color: Hsla,
        origin: Point<Pixels>,
        window: &mut Window,
    ) {
        for b in self.bands(range) {
            window.paint_quad(fill(Bounds::new(origin + b.origin, b.size), color));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::breaks;

    #[test]
    fn rows_break_after_hyphens_between_words() {
        assert_eq!(breaks("well-known"), vec![5]);
        assert_eq!(breaks("a -5 --b"), vec![2, 5]);
        assert_eq!(breaks("x-y-z"), vec![2, 4]);
    }
}
