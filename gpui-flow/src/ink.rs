//! Ink: strokes drawn with a pen, stored as the iPad's Notes stores PencilKit strokes, so a
//! whiteboard drawn with the Pencil on iPad shows the same strokes on the Mac, and the other
//! way round.
//!
//! A stroke is a node of type `ink` whose `data.ink` is the stroke:
//!
//! ```json
//! { "id": "…", "type": "ink", "position": { "x": 70, "y": 112 }, "width": 238, "height": 16,
//!   "data": { "ink": { "tool": "ball", "color": "#000000FF", "width": 2.4,
//!                      "points": "AACQQgAA8EI…", "transform": null } } }
//! ```
//!
//! - `tool`: `fountain`, `ball`, `brush`, `pencil` or `highlighter` (PencilKit's fountain pen,
//!   monoline, pen, pencil and marker).
//! - `color`: `#RRGGBBAA` in sRGB (`#RRGGBB` reads as opaque).
//! - `width`: the tool's width in points, more than 0 and at most 200.
//! - `points`: base64 of little-endian 32-bit floats, seven a point: `x, y, t, force, azimuth,
//!   altitude, size` (flow units, seconds from the stroke's start, PencilKit's force, radians,
//!   and the tip's width).
//! - `transform`: `[a, b, c, d, tx, ty]` applied after the points (`x' = a·x + c·y + tx`,
//!   `y' = b·x + d·y + ty`), or null. A move changes this, never the points.
//!
//! The node's `position`, `width` and `height` are the stroke's bounds after the transform,
//! with its width: where it is on the canvas.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::types::{Node, Rect, XY, xy};

/// The most points in a stroke.
pub const MAX_POINTS: usize = 10_000;
/// The farthest any value of a point may be from 0.
pub const MAX_COORDINATE: f32 = 20_000.;
/// The widest a tool may be, in points.
pub const MAX_WIDTH: f64 = 200.;

/// What drew a stroke, by its name in the JSON.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InkTool {
    /// PencilKit's fountain pen: wider across the direction of the nib.
    Fountain,
    /// PencilKit's monoline: one width throughout.
    #[default]
    Ball,
    /// PencilKit's pen: wider where it's pressed harder.
    Brush,
    /// A pencil: a little lighter.
    Pencil,
    /// A marker: wide, see-through, and under other ink.
    Highlighter,
}

/// One point of a stroke, as PencilKit's `PKStrokePoint` has it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct InkPoint {
    pub x: f32,
    pub y: f32,
    /// Seconds from the stroke's start.
    pub t: f32,
    pub force: f32,
    /// Radians.
    pub azimuth: f32,
    pub altitude: f32,
    /// The tip's width, in points.
    pub size: f32,
}

impl InkPoint {
    /// A point drawn with a mouse or a trackpad: no pressure or tilt, the pen upright.
    pub fn at(x: f32, y: f32, t: f32, size: f32) -> Self {
        Self {
            x,
            y,
            t,
            force: 1.,
            azimuth: 0.,
            altitude: std::f32::consts::FRAC_PI_2,
            size,
        }
    }

    fn values(&self) -> [f32; 7] {
        [
            self.x,
            self.y,
            self.t,
            self.force,
            self.azimuth,
            self.altitude,
            self.size,
        ]
    }
}

/// Points as stored: base64 of seven little-endian `f32`s a point.
pub fn encode_points(points: &[InkPoint]) -> String {
    let mut bytes = Vec::with_capacity(points.len() * 28);
    for p in points {
        for v in p.values() {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
    }
    STANDARD.encode(bytes)
}

/// Points from their stored form; none when it isn't base64 of whole points.
pub fn decode_points(text: &str) -> Option<Vec<InkPoint>> {
    let bytes = STANDARD.decode(text.trim()).ok()?;
    if !bytes.len().is_multiple_of(28) {
        return None;
    }
    let f = |c: &[u8]| f32::from_le_bytes([c[0], c[1], c[2], c[3]]);
    Some(
        bytes
            .as_chunks::<28>()
            .0
            .iter()
            .map(|p| InkPoint {
                x: f(&p[0..4]),
                y: f(&p[4..8]),
                t: f(&p[8..12]),
                force: f(&p[12..16]),
                azimuth: f(&p[16..20]),
                altitude: f(&p[20..24]),
                size: f(&p[24..28]),
            })
            .collect(),
    )
}

/// A colour as `#RRGGBBAA`, uppercase, as the iPad writes it.
pub fn color_hex(rgba: u32) -> String {
    format!("#{rgba:08X}")
}

/// `#RRGGBBAA`, or `#RRGGBB` as opaque, as `0xRRGGBBAA`.
pub fn parse_color_hex(text: &str) -> Option<u32> {
    let hex = text.strip_prefix('#').unwrap_or(text);
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    match hex.len() {
        6 => u32::from_str_radix(hex, 16).ok().map(|v| v << 8 | 0xff),
        8 => u32::from_str_radix(hex, 16).ok(),
        _ => None,
    }
}

/// A stroke, as a node's `data.ink` holds it.
#[derive(Clone, Debug, PartialEq)]
pub struct InkStroke {
    pub tool: InkTool,
    /// `0xRRGGBBAA`.
    pub color: u32,
    pub width: f64,
    pub points: Vec<InkPoint>,
    /// `[a, b, c, d, tx, ty]`, applied after the points; none is the identity.
    pub transform: Option<[f64; 6]>,
    /// Fields of a newer writer's, kept.
    pub extra: Map<String, Value>,
}

#[derive(Serialize, Deserialize)]
struct Wire {
    tool: InkTool,
    color: String,
    width: f64,
    points: String,
    #[serde(default)]
    transform: Option<Vec<f64>>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

impl Serialize for InkStroke {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        Wire {
            tool: self.tool,
            color: color_hex(self.color),
            width: self.width,
            points: encode_points(&self.points),
            transform: self.transform.map(|t| t.to_vec()),
            extra: self.extra.clone(),
        }
        .serialize(s)
    }
}

impl<'de> Deserialize<'de> for InkStroke {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        let w = Wire::deserialize(d)?;
        let color = parse_color_hex(&w.color)
            .ok_or_else(|| D::Error::custom(format!("not a #RRGGBBAA colour: {}", w.color)))?;
        let points = decode_points(&w.points)
            .ok_or_else(|| D::Error::custom("ink points aren't base64 of 7 floats a point"))?;
        let transform = match w.transform {
            None => None,
            Some(t) => Some(
                <[f64; 6]>::try_from(t.as_slice())
                    .map_err(|_| D::Error::custom("a transform is 6 numbers"))?,
            ),
        };
        Ok(Self {
            tool: w.tool,
            color,
            width: w.width,
            points,
            transform,
            extra: w.extra,
        })
    }
}

impl InkStroke {
    pub fn new(tool: InkTool, color: u32, width: f64, points: Vec<InkPoint>) -> Self {
        Self {
            tool,
            color,
            width,
            points,
            transform: None,
            extra: Map::new(),
        }
    }

    /// The stroke of an `ink` node, if it has a readable one.
    pub fn of(node: &Node) -> Option<Self> {
        serde_json::from_value(node.data.get("ink")?.clone()).ok()
    }

    /// Where a point goes once transformed.
    pub fn place(&self, x: f32, y: f32) -> XY {
        match self.transform {
            None => xy(x, y),
            Some([a, b, c, d, tx, ty]) => {
                let (x, y) = (x as f64, y as f64);
                xy((a * x + c * y + tx) as f32, (b * x + d * y + ty) as f32)
            }
        }
    }

    /// How much the transform scales lengths.
    pub fn scale(&self) -> f32 {
        match self.transform {
            None => 1.,
            Some([a, b, c, d, ..]) => ((a * d - b * c).abs().sqrt() as f32).max(0.),
        }
    }

    /// The stroke as drawn: each point placed, with half the width the tool lays down there,
    /// and smoothed along a Catmull-Rom curve through the points, a point every flow unit or so.
    /// Its bounds are the node's box, so the ink never leaves it at any zoom.
    pub fn outline(&self) -> Vec<(XY, f32)> {
        curve(&self.placed())
    }

    /// Each point placed, with half the width the tool lays down there.
    fn placed(&self) -> Vec<(XY, f32)> {
        let scale = self.scale();
        let base = self.width as f32;
        self.points
            .iter()
            .filter(|p| p.x.is_finite() && p.y.is_finite())
            .map(|p| {
                let size = if p.size.is_finite() && p.size > 0. {
                    p.size
                } else {
                    base
                };
                let size = match self.tool {
                    // Pressure widens the pen, up to half again.
                    InkTool::Brush if p.force.is_finite() && p.force > 0. => {
                        size * (0.6 + 0.4 * p.force.min(1.5))
                    }
                    _ => size,
                };
                (self.place(p.x, p.y), (size * scale / 2.).max(0.2))
            })
            .collect()
    }

    /// The box the stroke covers once transformed, with its width.
    pub fn bounds(&self) -> Option<Rect> {
        bounds_of(&self.outline())
    }

    /// Moves the stroke: a change to its transform, never its points.
    pub fn translate(&mut self, dx: f64, dy: f64) {
        let mut t = self.transform.unwrap_or([1., 0., 0., 1., 0., 0.]);
        t[4] += dx;
        t[5] += dy;
        self.transform = Some(t);
    }
}

/// A centripetal Catmull-Rom curve through the points (which doesn't overshoot at corners, or
/// loop where points bunch up), sampled about a flow unit apart (at most 32 a span), the
/// half-width going linearly from each point to the next. The ends are continued straight.
fn curve(points: &[(XY, f32)]) -> Vec<(XY, f32)> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let n = points.len();
    let p = |i: isize| -> XY {
        if i < 0 {
            points[0].0 + (points[0].0 - points[1].0)
        } else if i as usize >= n {
            points[n - 1].0 + (points[n - 1].0 - points[n - 2].0)
        } else {
            points[i as usize].0
        }
    };
    let knot = |a: XY, b: XY| {
        let d = b - a;
        (d.x * d.x + d.y * d.y).sqrt().sqrt().max(1e-4)
    };
    let lerp = |a: XY, b: XY, ta: f32, tb: f32, t: f32| {
        let (u, v) = ((tb - t) / (tb - ta), (t - ta) / (tb - ta));
        xy(a.x * u + b.x * v, a.y * u + b.y * v)
    };
    let mut out = Vec::with_capacity(n * 2);
    for i in 0..n - 1 {
        let (p0, p1, p2, p3) = (
            p(i as isize - 1),
            p(i as isize),
            p(i as isize + 1),
            p(i as isize + 2),
        );
        let (r1, r2) = (points[i].1, points[i + 1].1);
        let t1 = knot(p0, p1);
        let t2 = t1 + knot(p1, p2);
        let t3 = t2 + knot(p2, p3);
        let d = p2 - p1;
        let steps = (d.x * d.x + d.y * d.y).sqrt().ceil().clamp(1., 32.) as usize;
        for s in 0..steps {
            let f = s as f32 / steps as f32;
            let t = t1 + (t2 - t1) * f;
            let a1 = lerp(p0, p1, 0., t1, t);
            let a2 = lerp(p1, p2, t1, t2, t);
            let a3 = lerp(p2, p3, t2, t3, t);
            let b1 = lerp(a1, a2, 0., t2, t);
            let b2 = lerp(a2, a3, t1, t3, t);
            out.push((lerp(b1, b2, t1, t2, t), r1 + (r2 - r1) * f));
        }
    }
    out.push(points[n - 1]);
    out
}

pub(crate) fn bounds_of(outline: &[(XY, f32)]) -> Option<Rect> {
    let mut it = outline.iter();
    let &(p, r) = it.next()?;
    let (mut x0, mut y0, mut x1, mut y1) = (p.x - r, p.y - r, p.x + r, p.y + r);
    for &(p, r) in it {
        x0 = x0.min(p.x - r);
        y0 = y0.min(p.y - r);
        x1 = x1.max(p.x + r);
        y1 = y1.max(p.y + r);
    }
    Some(Rect::new(x0, y0, x1 - x0, y1 - y0))
}

impl Node {
    /// An `ink` node of this stroke, where the stroke is: its position and size are the
    /// stroke's bounds. A highlighter's goes under other ink.
    pub fn ink(id: impl Into<crate::types::NodeId>, stroke: &InkStroke) -> Self {
        let b = stroke.bounds().unwrap_or_default();
        let mut node = Node::new(id, xy(b.x, b.y)).kind("ink").size(b.w, b.h);
        node.data = serde_json::json!({ "ink": stroke });
        if stroke.tool == InkTool::Highlighter {
            node.z_index = -1;
        }
        node
    }
}

/// A stroke ready to draw: its outline from its bounds' corner, and how it's coloured.
#[derive(Clone, Debug)]
pub(crate) struct Drawn {
    pub outline: Vec<(XY, f32)>,
    pub size: XY,
    pub tool: InkTool,
    pub color: u32,
    /// The widest half-width, for finding the stroke under a press.
    pub reach: f32,
}

impl Drawn {
    pub fn of(stroke: &InkStroke) -> Self {
        let outline = stroke.outline();
        let b = bounds_of(&outline).unwrap_or_default();
        let reach = outline.iter().map(|&(_, r)| r).fold(0., f32::max);
        Self {
            outline: outline
                .into_iter()
                .map(|(p, r)| (xy(p.x - b.x, p.y - b.y), r))
                .collect(),
            size: xy(b.w, b.h),
            tool: stroke.tool,
            color: stroke.color,
            reach,
        }
    }

    /// How far a point (from the bounds' corner) is from the stroke's edge: below 0 inside it.
    pub fn distance(&self, at: XY) -> f32 {
        let line: Vec<XY> = self.outline.iter().map(|&(p, _)| p).collect();
        if line.len() == 1 {
            let d = line[0] - at;
            return (d.x * d.x + d.y * d.y).sqrt() - self.reach;
        }
        crate::geometry::distance_to_polyline(at, &line) - self.reach
    }
}

/// A smoothed line through the points a mouse gave: each moved part of the way toward the one
/// before (taking out the shake of a hand), the first and last kept where they were, then
/// thinned to what changes the line by more than `tolerance`.
pub fn smooth(raw: &[InkPoint], tolerance: f32) -> Vec<InkPoint> {
    if raw.len() < 3 {
        return raw.to_vec();
    }
    let mut out = Vec::with_capacity(raw.len());
    out.push(raw[0]);
    let mut prev = raw[0];
    for p in &raw[1..raw.len() - 1] {
        let s = InkPoint {
            x: prev.x + (p.x - prev.x) * 0.55,
            y: prev.y + (p.y - prev.y) * 0.55,
            ..*p
        };
        out.push(s);
        prev = s;
    }
    out.push(raw[raw.len() - 1]);
    simplify(&out, tolerance)
}

/// The points that change the line by more than `tolerance` (Ramer–Douglas–Peucker).
fn simplify(points: &[InkPoint], tolerance: f32) -> Vec<InkPoint> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    let mut stack = vec![(0, points.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        let (pa, pb) = (xy(points[a].x, points[a].y), xy(points[b].x, points[b].y));
        let mut far = (0., a);
        for (i, p) in points.iter().enumerate().take(b).skip(a + 1) {
            let d = crate::geometry::distance_to_polyline(xy(p.x, p.y), &[pa, pb]);
            // A change of width counts as much as a change of place.
            let w = (p.size - points[a].size)
                .abs()
                .max((p.size - points[b].size).abs())
                / 2.;
            let d = d.max(w);
            if d > far.0 {
                far = (d, i);
            }
        }
        if far.0 > tolerance {
            keep[far.1] = true;
            stack.push((a, far.1));
            stack.push((far.1, b));
        }
    }
    points
        .iter()
        .zip(keep)
        .filter_map(|(p, k)| k.then_some(*p))
        .collect()
}
