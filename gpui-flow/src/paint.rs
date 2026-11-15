//! What the flow paints itself rather than building from elements: the pane's pattern, edges
//! with their arrowheads and labels, the minimap, and the controls' icons.

use gpui_kit::*;

use crate::geometry::clip_segment;
use crate::node::{MinimapMap, Shared};
use crate::theme::FlowColors;
use crate::types::{Marker, MarkerKind, Rect, Viewport, XY, xy};

/// The pattern drawn on the pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackgroundVariant {
    Dots,
    Lines,
    Cross,
}

/// The pane's pattern: what it is, how far apart it repeats, and how heavy it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlowBackground {
    pub variant: BackgroundVariant,
    /// The distance between repeats, in flow units.
    pub gap: f32,
    /// A dot's width, a line's thickness or a cross's arm span, in flow units.
    pub size: Option<f32>,
    pub color: Option<Hsla>,
}

impl FlowBackground {
    pub fn dots() -> Self {
        Self {
            variant: BackgroundVariant::Dots,
            gap: 20.,
            size: None,
            color: None,
        }
    }

    pub fn lines() -> Self {
        Self {
            variant: BackgroundVariant::Lines,
            gap: 20.,
            size: None,
            color: None,
        }
    }

    pub fn cross() -> Self {
        Self {
            variant: BackgroundVariant::Cross,
            gap: 20.,
            size: None,
            color: None,
        }
    }

    pub fn gap(mut self, gap: f32) -> Self {
        self.gap = gap;
        self
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = Some(size);
        self
    }

    pub fn color(mut self, color: impl Into<Hsla>) -> Self {
        self.color = Some(color.into());
        self
    }
}

fn pt(origin: Point<Pixels>, p: XY) -> Point<Pixels> {
    point(origin.x + px(p.x), origin.y + px(p.y))
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
    Bounds::new(point(px(x), px(y)), size(px(w), px(h)))
}

/// Solid shapes gathered into one path, so that however many there are the window gets a
/// single primitive to order and the GPU a single pass to draw. (GPUI gives every batch of
/// paths a render pass of its own, and every primitive a place in its ordering tree; thousands
/// of separate dots or hundreds of separate edges cost milliseconds a frame in each.)
struct Solid {
    path: Option<Path<Pixels>>,
}

impl Solid {
    fn new() -> Self {
        Self { path: None }
    }

    fn triangle(&mut self, a: Point<Pixels>, b: Point<Pixels>, c: Point<Pixels>) {
        // The texture coordinates GPUI gives the inside of a filled shape.
        let st = (point(0., 1.), point(0., 1.), point(0., 1.));
        self.path
            .get_or_insert_with(|| Path::new(a))
            .push_triangle((a, b, c), st);
    }

    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let (a, b) = (point(px(x), px(y)), point(px(x + w), px(y)));
        let (c, d) = (point(px(x + w), px(y + h)), point(px(x), px(y + h)));
        self.triangle(a, b, c);
        self.triangle(a, c, d);
    }

    /// A dot: a square when it's too small to tell, an octagon otherwise.
    fn dot(&mut self, x: f32, y: f32, d: f32) {
        if d < 3. {
            return self.rect(x - d / 2., y - d / 2., d, d);
        }
        let r = d / 2.;
        let corner = |i: usize| {
            let a = std::f32::consts::TAU * i as f32 / 8.;
            point(px(x + r * a.cos()), px(y + r * a.sin()))
        };
        for i in 1..7 {
            self.triangle(corner(0), corner(i), corner(i + 1));
        }
    }

    fn paint(self, color: Hsla, window: &mut Window) {
        if let Some(path) = self.path {
            window.paint_path(path, color);
        }
    }
}

/// The pane's pattern, moving and scaling with the viewport.
pub(crate) fn paint_background(
    background: &FlowBackground,
    bounds: Bounds<Pixels>,
    viewport: Viewport,
    colors: &FlowColors,
    window: &mut Window,
) {
    let color = background.color.unwrap_or(colors.pattern);
    let zoom = viewport.zoom;
    let mut gap = (background.gap * zoom).max(0.5);
    // Zoomed far out, every other repeat is dropped (and again) rather than painting a haze
    // of tens of thousands of marks.
    while gap < 12. {
        gap *= 2.;
    }
    let (left, top) = (f32::from(bounds.origin.x), f32::from(bounds.origin.y));
    let (width, height) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    let x0 = left + viewport.x.rem_euclid(gap);
    let y0 = top + viewport.y.rem_euclid(gap);
    let columns = (width / gap).ceil() as usize + 1;
    let rows = (height / gap).ceil() as usize + 1;
    let mut solid = Solid::new();
    match background.variant {
        BackgroundVariant::Dots => {
            let d = (background.size.unwrap_or(1.5) * zoom).max(1.);
            for row in 0..rows {
                for column in 0..columns {
                    solid.dot(x0 + column as f32 * gap, y0 + row as f32 * gap, d);
                }
            }
        }
        BackgroundVariant::Lines => {
            let w = background.size.unwrap_or(1.);
            // Whole lines carry far more ink than dots do, so they're drawn fainter. They're
            // few, and as quads they land on whole pixels.
            let color = if background.color.is_some() {
                color
            } else {
                color.opacity(0.3)
            };
            for column in 0..columns {
                window.paint_quad(fill(rect(x0 + column as f32 * gap, top, w, height), color));
            }
            for row in 0..rows {
                window.paint_quad(fill(rect(left, y0 + row as f32 * gap, width, w), color));
            }
        }
        BackgroundVariant::Cross => {
            let arm = (background.size.unwrap_or(6.) * zoom).max(3.);
            for row in 0..rows {
                for column in 0..columns {
                    let (x, y) = (x0 + column as f32 * gap, y0 + row as f32 * gap);
                    solid.rect(x - arm / 2., y - 0.5, arm, 1.);
                    solid.rect(x - 0.5, y - arm / 2., 1., arm);
                }
            }
        }
    }
    solid.paint(color, window);
}

/// Lines and arrowheads gathered by color and width, and painted as one path for each: all
/// the edges of a flow are usually two or three paths rather than one apiece.
pub(crate) struct Ink {
    origin: Point<Pixels>,
    /// What's drawn, in pane coordinates; lines are cut to it.
    clip: Rect,
    strokes: Vec<(Hsla, f32, PathBuilder)>,
    fills: Vec<(Hsla, Solid)>,
}

/// The farthest from the pane a point of a line may be and still be handed on whole.
const FAR: f32 = 1e7;

fn finite(p: XY) -> bool {
    p.x.is_finite() && p.y.is_finite()
}

impl Ink {
    /// Points given to it are in pane coordinates, from `origin` in the window.
    pub fn new(origin: Point<Pixels>) -> Self {
        Self::clipped(origin, Rect::new(-FAR, -FAR, FAR * 2., FAR * 2.))
    }

    /// Draws only what falls within `clip`, a rectangle in pane coordinates: a line that runs
    /// far off screen costs only what's on it.
    pub fn clipped(origin: Point<Pixels>, clip: Rect) -> Self {
        Self {
            origin,
            clip,
            strokes: Vec::new(),
            fills: Vec::new(),
        }
    }

    fn builder(&mut self, color: Hsla, width: f32) -> &mut PathBuilder {
        let found = self
            .strokes
            .iter()
            .position(|(c, w, _)| *c == color && *w == width);
        let i = found.unwrap_or_else(|| {
            self.strokes
                .push((color, width, PathBuilder::stroke(px(width))));
            self.strokes.len() - 1
        });
        &mut self.strokes[i].2
    }

    /// A solid line through `points`, cut to the clip.
    pub fn line(&mut self, points: &[XY], width: f32, color: Hsla) {
        if points.len() < 2 {
            return;
        }
        let (origin, clip) = (self.origin, self.clip);
        let builder = self.builder(color, width);
        // Whether the path is at the end of the last piece drawn.
        let mut open = false;
        for pair in points.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let Some((t0, t1)) = clip_segment(a, b, &clip) else {
                open = false;
                continue;
            };
            if !open || t0 > 0. {
                builder.move_to(pt(origin, a + (b - a) * t0));
            }
            builder.line_to(pt(origin, a + (b - a) * t1));
            open = t1 >= 1.;
        }
    }

    /// A line through `points` in dashes of `dash` with gaps as long, shifted `phase` along
    /// the line: raising the phase over time makes the dashes run toward the end.
    pub fn dashed(&mut self, points: &[XY], width: f32, dash: f32, phase: f32, color: Hsla) {
        if points.len() < 2 || dash <= 0. {
            return;
        }
        let (origin, clip) = (self.origin, self.clip);
        let builder = self.builder(color, width);
        // In f64: an f32 running total stops growing on a long enough line.
        let (dash, period) = (dash as f64, dash as f64 * 2.);
        // How far into the pattern the line's start is.
        let mut at = (period - (phase as f64).rem_euclid(period)).rem_euclid(period);
        let mut drawing = false;
        let point = |a: XY, b: XY, t: f64| {
            let t = t as f32;
            pt(origin, a + (b - a) * t)
        };
        for pair in points.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if !finite(a) || !finite(b) {
                drawing = false;
                continue;
            }
            let length = (a.distance(b)) as f64;
            if length <= 0. {
                continue;
            }
            // Only the part on screen is walked; the pattern skips ahead over the rest, so the
            // dashes stay where they'd be, and the work is bounded by the screen, not the line.
            let Some((t0, t1)) = clip_segment(a, b, &clip) else {
                at = (at + length) % period;
                drawing = false;
                continue;
            };
            let (start, end) = (t0 as f64 * length, t1 as f64 * length);
            if start > 0. {
                at = (at + start) % period;
                drawing = false;
            }
            let mut done = start;
            while end - done > 1e-6 {
                let on = at < dash;
                let left = if on { dash - at } else { period - at };
                let step = left.min(end - done).max(1e-6);
                if on {
                    if !drawing {
                        builder.move_to(point(a, b, done / length));
                        drawing = true;
                    }
                    builder.line_to(point(a, b, (done + step) / length));
                } else {
                    drawing = false;
                }
                done += step;
                at = (at + step) % period;
            }
            if end < length {
                at = (at + (length - end)) % period;
                drawing = false;
            }
        }
    }

    /// An arrowhead with its tip at `tip`, pointing the way the line arrives from `from`.
    pub fn marker(
        &mut self,
        marker: &Marker,
        tip: XY,
        from: XY,
        zoom: f32,
        width: f32,
        color: Hsla,
    ) {
        let length = from.distance(tip);
        if length <= 0. || !finite(tip) || !finite(from) || !self.clip.contains(tip) {
            return;
        }
        let dir = (tip - from) * (1. / length);
        let normal = xy(-dir.y, dir.x);
        let long = marker.size * zoom;
        let base = tip - dir * long;
        let a = base + normal * (long * 0.5);
        let b = base - normal * (long * 0.5);
        let color = marker.color.map(|c| rgb(c).into()).unwrap_or(color);
        match marker.kind {
            MarkerKind::Arrow => self.line(&[a, tip, b], width, color),
            MarkerKind::ArrowClosed => {
                let origin = self.origin;
                let found = self.fills.iter().position(|(c, _)| *c == color);
                let i = found.unwrap_or_else(|| {
                    self.fills.push((color, Solid::new()));
                    self.fills.len() - 1
                });
                self.fills[i]
                    .1
                    .triangle(pt(origin, a), pt(origin, tip), pt(origin, b));
            }
        }
    }

    pub fn paint(self, window: &mut Window) {
        for (color, _, builder) in self.strokes {
            if let Ok(path) = builder.build() {
                window.paint_path(path, color);
            }
        }
        for (color, solid) in self.fills {
            solid.paint(color, window);
        }
    }
}

/// One solid line through `points` (pane coordinates), painted at once.
pub(crate) fn stroke(
    origin: Point<Pixels>,
    points: &[XY],
    width: f32,
    color: Hsla,
    window: &mut Window,
) {
    let mut ink = Ink::new(origin);
    ink.line(points, width, color);
    ink.paint(window);
}

/// An edge's label, centered on `center`, on a patch of the pane's color so the line doesn't
/// run through the text.
pub(crate) fn paint_label(
    origin: Point<Pixels>,
    text: &SharedString,
    center: XY,
    zoom: f32,
    colors: &FlowColors,
    window: &mut Window,
    cx: &mut App,
) {
    let font_size = f32::from(crate::node::font_size(10., zoom));
    // Too small to read: not worth the shaping.
    if font_size < 3. || text.is_empty() {
        return;
    }
    let line: SharedString = text.replace('\n', " ").into();
    let run = TextRun {
        len: line.len(),
        font: window.text_style().font(),
        color: colors.edge_label,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let shaped = window
        .text_system()
        .shape_line(line, px(font_size), &[run], None);
    let width = f32::from(shaped.width);
    let line_height = font_size * 1.4;
    let (pad_x, pad_y) = (4. * zoom, 2. * zoom);
    let at = pt(origin, center);
    let (x, y) = (
        f32::from(at.x) - width / 2.,
        f32::from(at.y) - line_height / 2.,
    );
    window.paint_quad(quad(
        rect(
            x - pad_x,
            y - pad_y,
            width + pad_x * 2.,
            line_height + pad_y * 2.,
        ),
        px(2. * zoom),
        colors.edge_label_background,
        px(0.),
        transparent_black(),
        BorderStyle::default(),
    ));
    let _ = shaped.paint(
        point(px(x), px(y)),
        px(line_height),
        TextAlign::Left,
        None,
        window,
        cx,
    );
}

/// The minimap: every node as a box, with what the pane doesn't show dimmed. Records how the
/// flow was mapped into it, for presses.
pub(crate) fn paint_minimap(
    bounds: Bounds<Pixels>,
    nodes: &[(Rect, bool)],
    visible: Rect,
    colors: &FlowColors,
    shared: &Shared,
    window: &mut Window,
) {
    let all = nodes.iter().fold(visible, |all, (r, _)| all.union(r));
    let (width, height) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    let margin = 6.;
    let scale = ((width - margin * 2.) / all.w.max(1.)).min((height - margin * 2.) / all.h.max(1.));
    // The flow point at the minimap's top left, with the whole centered.
    let origin = xy(
        all.x - (width / scale - all.w) / 2.,
        all.y - (height / scale - all.h) / 2.,
    );
    shared.minimap.set(Some(MinimapMap {
        bounds,
        origin,
        scale,
    }));
    let map = |r: &Rect| {
        rect(
            f32::from(bounds.origin.x) + (r.x - origin.x) * scale,
            f32::from(bounds.origin.y) + (r.y - origin.y) * scale,
            r.w * scale,
            r.h * scale,
        )
    };
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        for (node, selected) in nodes {
            let color = if *selected {
                colors.selected
            } else {
                colors.minimap_node
            };
            window.paint_quad(quad(
                map(node),
                px(1.5),
                color,
                px(0.),
                transparent_black(),
                BorderStyle::default(),
            ));
        }
        // The dimming is four pieces around the visible part.
        let view = map(&visible);
        let (l, t, r, b) = (bounds.left(), bounds.top(), bounds.right(), bounds.bottom());
        let mask = colors.minimap_mask;
        window.paint_quad(fill(
            Bounds::from_corners(point(l, t), point(r, view.top())),
            mask,
        ));
        window.paint_quad(fill(
            Bounds::from_corners(point(l, view.bottom()), point(r, b)),
            mask,
        ));
        window.paint_quad(fill(
            Bounds::from_corners(point(l, view.top()), point(view.left(), view.bottom())),
            mask,
        ));
        window.paint_quad(fill(
            Bounds::from_corners(point(view.right(), view.top()), point(r, view.bottom())),
            mask,
        ));
        window.paint_quad(outline(view, colors.minimap_view, BorderStyle::default()));
    });
}

/// The controls' icons, drawn rather than loaded so the crate needs no assets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Icon {
    Plus,
    Minus,
    Fit,
    Locked,
    Unlocked,
}

/// An icon in the middle of `bounds`, in a 12 point box.
pub(crate) fn paint_icon(icon: Icon, bounds: Bounds<Pixels>, color: Hsla, window: &mut Window) {
    let c = bounds.center();
    let o = point(c.x - px(6.), c.y - px(6.));
    let w = 1.25;
    let mut ink = Ink::new(o);
    let mut line = |points: &[XY]| ink.line(points, w, color);
    match icon {
        Icon::Plus => {
            line(&[xy(1.5, 6.), xy(10.5, 6.)]);
            line(&[xy(6., 1.5), xy(6., 10.5)]);
        }
        Icon::Minus => line(&[xy(1.5, 6.), xy(10.5, 6.)]),
        Icon::Fit => {
            line(&[xy(1., 4.), xy(1., 1.), xy(4., 1.)]);
            line(&[xy(8., 1.), xy(11., 1.), xy(11., 4.)]);
            line(&[xy(11., 8.), xy(11., 11.), xy(8., 11.)]);
            line(&[xy(4., 11.), xy(1., 11.), xy(1., 8.)]);
        }
        Icon::Locked | Icon::Unlocked => {
            // The shackle: half a circle on two short legs, lifted off one side when open.
            let lift = if icon == Icon::Unlocked { 1.5 } else { 0. };
            let mut shackle = vec![xy(3.5, 5.5 - lift)];
            for i in 0..=8 {
                let a = std::f32::consts::PI * (1. - i as f32 / 8.);
                shackle.push(xy(6. + 2.5 * a.cos(), 3.5 - lift - 2.5 * a.sin()));
            }
            if icon == Icon::Locked {
                shackle.push(xy(8.5, 5.5));
            }
            line(&shackle);
            window.paint_quad(quad(
                Bounds::new(point(o.x + px(2.), o.y + px(5.5)), size(px(8.), px(6.))),
                px(1.5),
                color,
                px(0.),
                transparent_black(),
                BorderStyle::default(),
            ));
        }
    }
    ink.paint(window);
}
