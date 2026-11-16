//! The outlines a whiteboard node can take, worked out as plain numbers in the node's own box:
//! the outline itself, a line drawn over it (a cylinder's rim), where edges meet it on each
//! side, the area its text may use, and where its badge sits. The canvas draws from here, and
//! so can anything else that draws a flow (an image export), so they always agree.

use serde::{Deserialize, Serialize};

use crate::types::{Rect, Side, XY, xy};

/// A whiteboard node's outline.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeShape {
    #[default]
    Rectangle,
    /// Ends rounded right off: a pill.
    Rounded,
    Ellipse,
    Diamond,
    Hexagon,
    Cylinder,
    Parallelogram,
    /// A page whose bottom edge waves.
    Document,
    Cloud,
}

impl NodeShape {
    pub const ALL: [NodeShape; 9] = [
        NodeShape::Rectangle,
        NodeShape::Rounded,
        NodeShape::Ellipse,
        NodeShape::Diamond,
        NodeShape::Hexagon,
        NodeShape::Cylinder,
        NodeShape::Parallelogram,
        NodeShape::Document,
        NodeShape::Cloud,
    ];

    /// The size the shape is comfortable at, in flow units: room for two lines of text inside
    /// it. A diamond only has half its box to write in, so it starts larger.
    pub fn default_size(self) -> (f32, f32) {
        match self {
            NodeShape::Rectangle | NodeShape::Rounded => (160., 64.),
            NodeShape::Ellipse => (168., 84.),
            NodeShape::Diamond => (184., 112.),
            NodeShape::Hexagon => (184., 72.),
            NodeShape::Cylinder => (144., 100.),
            NodeShape::Parallelogram => (184., 64.),
            NodeShape::Document => (160., 84.),
            NodeShape::Cloud => (192., 116.),
        }
    }
}

/// One step of an outline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathOp {
    Move(XY),
    Line(XY),
    /// A cubic curve to `to`.
    Cubic {
        c1: XY,
        c2: XY,
        to: XY,
    },
    Close,
}

/// A shape worked out at one size.
#[derive(Clone, Debug, PartialEq)]
pub struct ShapeGeometry {
    /// The closed outline.
    pub outline: Vec<PathOp>,
    /// A line drawn over the outline that isn't part of it: a cylinder's near rim.
    pub detail: Vec<PathOp>,
    /// Where an edge meets the shape on each side: on its visible edge, which for a
    /// parallelogram isn't the middle of the box's side.
    pub anchors: Anchors,
    /// The area the text may use without touching the outline.
    pub text: Rect,
    /// The middle of the badge (an icon or an emoji), inside the box near its top right.
    pub badge: XY,
}

/// Where edges meet a shape, one point per side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Anchors {
    pub top: XY,
    pub right: XY,
    pub bottom: XY,
    pub left: XY,
}

impl Anchors {
    pub fn on(&self, side: Side) -> XY {
        match side {
            Side::Top => self.top,
            Side::Right => self.right,
            Side::Bottom => self.bottom,
            Side::Left => self.left,
        }
    }

    fn of_box(w: f32, h: f32) -> Self {
        Self {
            top: xy(w / 2., 0.),
            right: xy(w, h / 2.),
            bottom: xy(w / 2., h),
            left: xy(0., h / 2.),
        }
    }
}

/// How far text keeps from the outline, in flow units.
const PAD_X: f32 = 12.;
const PAD_Y: f32 = 8.;
/// How round a plain rectangle's corners are.
const RADIUS: f32 = 6.;
/// How much of a quarter circle a cubic's control points stand out by.
const KAPPA: f32 = 0.552_284_8;

/// `shape` at `w` × `h` flow units. Sizes below 1 are taken as 1.
pub fn geometry(shape: NodeShape, w: f32, h: f32) -> ShapeGeometry {
    let (w, h) = (w.max(1.), h.max(1.));
    let inner = |x0: f32, y0: f32, x1: f32, y1: f32| {
        let (x0, x1) = (x0 + PAD_X, x1 - PAD_X);
        let (y0, y1) = (y0 + PAD_Y, y1 - PAD_Y);
        Rect::new(x0, y0, (x1 - x0).max(0.), (y1 - y0).max(0.))
    };
    let badge = |x: f32, y: f32| xy(x.min(w - 10.).max(10.), y.max(10.).min(h - 10.));
    match shape {
        NodeShape::Rectangle | NodeShape::Rounded => {
            let r = if shape == NodeShape::Rounded {
                h / 2.
            } else {
                RADIUS
            };
            let r = r.min(w / 2.).min(h / 2.);
            // A pill's text keeps clear of its round ends.
            let side = if shape == NodeShape::Rounded {
                r * 0.5
            } else {
                0.
            };
            ShapeGeometry {
                outline: rounded_rect(w, h, r),
                detail: Vec::new(),
                anchors: Anchors::of_box(w, h),
                text: inner(side, 0., w - side, h),
                badge: badge(w - 12. - side, 12.),
            }
        }
        NodeShape::Ellipse => ShapeGeometry {
            outline: ellipse(w / 2., h / 2., w / 2., h / 2.),
            detail: Vec::new(),
            anchors: Anchors::of_box(w, h),
            // The largest box inside an ellipse is 1/√2 of it each way.
            text: inner(
                w * 0.146 - PAD_X / 2.,
                h * 0.146 - PAD_Y / 2.,
                w * 0.854 + PAD_X / 2.,
                h * 0.854 + PAD_Y / 2.,
            ),
            badge: badge(w * 0.82, h * 0.2),
        },
        NodeShape::Diamond => ShapeGeometry {
            outline: polygon(&[xy(w / 2., 0.), xy(w, h / 2.), xy(w / 2., h), xy(0., h / 2.)]),
            detail: Vec::new(),
            anchors: Anchors::of_box(w, h),
            // The box inside a diamond touching its four sides at their middles.
            text: inner(
                w / 4. - PAD_X / 2.,
                h / 4. - PAD_Y / 2.,
                w * 0.75 + PAD_X / 2.,
                h * 0.75 + PAD_Y / 2.,
            ),
            badge: badge(w * 0.72, h * 0.26),
        },
        NodeShape::Hexagon => {
            let dx = (h / 2. * 0.577).min(w / 4.);
            ShapeGeometry {
                outline: polygon(&[
                    xy(dx, 0.),
                    xy(w - dx, 0.),
                    xy(w, h / 2.),
                    xy(w - dx, h),
                    xy(dx, h),
                    xy(0., h / 2.),
                ]),
                detail: Vec::new(),
                anchors: Anchors::of_box(w, h),
                text: inner(dx * 0.6, 0., w - dx * 0.6, h),
                badge: badge(w - dx - 4., 12.),
            }
        }
        NodeShape::Cylinder => {
            let ry = (h * 0.12).clamp(2., 12.);
            let rx = w / 2.;
            let mut outline = vec![PathOp::Move(xy(0., ry))];
            // The far half of the lid, left to right over the top.
            outline.extend(half_ellipse(rx, ry, rx, ry, true));
            outline.push(PathOp::Line(xy(w, h - ry)));
            // The bottom, right to left round the front.
            outline.extend(half_ellipse(rx, h - ry, rx, ry, false));
            outline.push(PathOp::Close);
            // The near half of the lid, drawn over the body.
            let mut detail = vec![PathOp::Move(xy(0., ry))];
            detail.extend(half_ellipse_front(rx, ry, rx, ry));
            ShapeGeometry {
                outline,
                detail,
                anchors: Anchors::of_box(w, h),
                text: inner(0., ry * 2., w, h - ry),
                badge: badge(w - 12., ry * 2. + 10.),
            }
        }
        NodeShape::Parallelogram => {
            let s = (h * 0.35).min(w * 0.2);
            ShapeGeometry {
                outline: polygon(&[xy(s, 0.), xy(w, 0.), xy(w - s, h), xy(0., h)]),
                detail: Vec::new(),
                anchors: Anchors {
                    left: xy(s / 2., h / 2.),
                    right: xy(w - s / 2., h / 2.),
                    ..Anchors::of_box(w, h)
                },
                text: inner(s, 0., w - s, h),
                badge: badge(w - 14., 12.),
            }
        }
        NodeShape::Document => {
            let a = (h * 0.1).min(9.);
            let outline = vec![
                PathOp::Move(xy(0., 0.)),
                PathOp::Line(xy(w, 0.)),
                PathOp::Line(xy(w, h - a)),
                // One wave along the bottom, right to left.
                PathOp::Cubic {
                    c1: xy(w * 0.75, h - a * 3.),
                    c2: xy(w * 0.5, h + a),
                    to: xy(w * 0.25, h - a),
                },
                PathOp::Cubic {
                    c1: xy(w * 0.12, h - a * 2.),
                    c2: xy(w * 0.05, h - a * 1.5),
                    to: xy(0., h - a),
                },
                PathOp::Close,
            ];
            ShapeGeometry {
                outline,
                detail: Vec::new(),
                anchors: Anchors {
                    bottom: xy(w / 2., h - a * 0.3),
                    ..Anchors::of_box(w, h)
                },
                text: inner(0., 0., w, h - a * 2.),
                badge: badge(w - 12., 12.),
            }
        }
        NodeShape::Cloud => ShapeGeometry {
            outline: cloud(w, h),
            detail: Vec::new(),
            anchors: Anchors::of_box(w, h),
            text: inner(w * 0.16, h * 0.2, w * 0.84, h * 0.8),
            badge: badge(w * 0.8, h * 0.22),
        },
    }
}

fn polygon(points: &[XY]) -> Vec<PathOp> {
    let mut ops: Vec<PathOp> = points
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if i == 0 {
                PathOp::Move(*p)
            } else {
                PathOp::Line(*p)
            }
        })
        .collect();
    ops.push(PathOp::Close);
    ops
}

/// A rectangle from (0, 0) with corners rounded by `r`.
fn rounded_rect(w: f32, h: f32, r: f32) -> Vec<PathOp> {
    let k = r * (1. - KAPPA);
    vec![
        PathOp::Move(xy(r, 0.)),
        PathOp::Line(xy(w - r, 0.)),
        PathOp::Cubic {
            c1: xy(w - k, 0.),
            c2: xy(w, k),
            to: xy(w, r),
        },
        PathOp::Line(xy(w, h - r)),
        PathOp::Cubic {
            c1: xy(w, h - k),
            c2: xy(w - k, h),
            to: xy(w - r, h),
        },
        PathOp::Line(xy(r, h)),
        PathOp::Cubic {
            c1: xy(k, h),
            c2: xy(0., h - k),
            to: xy(0., h - r),
        },
        PathOp::Line(xy(0., r)),
        PathOp::Cubic {
            c1: xy(0., k),
            c2: xy(k, 0.),
            to: xy(r, 0.),
        },
        PathOp::Close,
    ]
}

/// A whole ellipse around (`cx`, `cy`), as four cubic quarters.
fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32) -> Vec<PathOp> {
    let (kx, ky) = (rx * KAPPA, ry * KAPPA);
    vec![
        PathOp::Move(xy(cx, cy - ry)),
        PathOp::Cubic {
            c1: xy(cx + kx, cy - ry),
            c2: xy(cx + rx, cy - ky),
            to: xy(cx + rx, cy),
        },
        PathOp::Cubic {
            c1: xy(cx + rx, cy + ky),
            c2: xy(cx + kx, cy + ry),
            to: xy(cx, cy + ry),
        },
        PathOp::Cubic {
            c1: xy(cx - kx, cy + ry),
            c2: xy(cx - rx, cy + ky),
            to: xy(cx - rx, cy),
        },
        PathOp::Cubic {
            c1: xy(cx - rx, cy - ky),
            c2: xy(cx - kx, cy - ry),
            to: xy(cx, cy - ry),
        },
        PathOp::Close,
    ]
}

/// Half an ellipse: over the top from its left end to its right (`top`), or round the bottom
/// from its right end to its left.
fn half_ellipse(cx: f32, cy: f32, rx: f32, ry: f32, top: bool) -> Vec<PathOp> {
    let (kx, ky) = (rx * KAPPA, ry * KAPPA);
    if top {
        vec![
            PathOp::Cubic {
                c1: xy(cx - rx, cy - ky),
                c2: xy(cx - kx, cy - ry),
                to: xy(cx, cy - ry),
            },
            PathOp::Cubic {
                c1: xy(cx + kx, cy - ry),
                c2: xy(cx + rx, cy - ky),
                to: xy(cx + rx, cy),
            },
        ]
    } else {
        vec![
            PathOp::Cubic {
                c1: xy(cx + rx, cy + ky),
                c2: xy(cx + kx, cy + ry),
                to: xy(cx, cy + ry),
            },
            PathOp::Cubic {
                c1: xy(cx - kx, cy + ry),
                c2: xy(cx - rx, cy + ky),
                to: xy(cx - rx, cy),
            },
        ]
    }
}

/// The front half of an ellipse, left to right round its bottom.
fn half_ellipse_front(cx: f32, cy: f32, rx: f32, ry: f32) -> Vec<PathOp> {
    let (kx, ky) = (rx * KAPPA, ry * KAPPA);
    vec![
        PathOp::Cubic {
            c1: xy(cx - rx, cy + ky),
            c2: xy(cx - kx, cy + ry),
            to: xy(cx, cy + ry),
        },
        PathOp::Cubic {
            c1: xy(cx + kx, cy + ry),
            c2: xy(cx + rx, cy + ky),
            to: xy(cx + rx, cy),
        },
    ]
}

/// A cloud: points round an ellipse inside the box, each pair joined by a curve that bulges
/// out to the box's edge, so the outline scallops without leaving the box.
fn cloud(w: f32, h: f32) -> Vec<PathOp> {
    const LOBES: usize = 9;
    let (cx, cy) = (w / 2., h / 2.);
    let (rx, ry) = (w / 2., h / 2.);
    let at = |angle: f32, k: f32| xy(cx + rx * k * angle.cos(), cy + ry * k * angle.sin());
    let step = std::f32::consts::TAU / LOBES as f32;
    // Start at the bottom so the flat-ish base sits on the box's bottom.
    let start = std::f32::consts::FRAC_PI_2;
    let mut ops = vec![PathOp::Move(at(start, 0.82))];
    for i in 0..LOBES {
        let (a0, a1) = (start + step * i as f32, start + step * (i + 1) as f32);
        // Control points out past the ellipse, so the bulge peaks near its edge.
        ops.push(PathOp::Cubic {
            c1: at(a0 + step * 0.15, 1.1),
            c2: at(a1 - step * 0.15, 1.1),
            to: at(a1, 0.82),
        });
    }
    ops.push(PathOp::Close);
    ops
}

/// The points an outline passes through, its curves flattened, for checks and hit tests.
pub fn flatten(ops: &[PathOp], steps: usize) -> Vec<XY> {
    let mut out = Vec::new();
    let mut at = xy(0., 0.);
    for op in ops {
        match *op {
            PathOp::Move(p) | PathOp::Line(p) => {
                out.push(p);
                at = p;
            }
            PathOp::Cubic { c1, c2, to } => {
                for i in 1..=steps {
                    let t = i as f32 / steps as f32;
                    let u = 1. - t;
                    out.push(
                        at * (u * u * u)
                            + c1 * (3. * u * u * t)
                            + c2 * (3. * u * t * t)
                            + to * (t * t * t),
                    );
                }
                at = to;
            }
            PathOp::Close => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_outline_stays_in_its_box_and_its_text_inside_the_outline() {
        for shape in NodeShape::ALL {
            let (w, h) = shape.default_size();
            let g = geometry(shape, w, h);
            for p in flatten(&g.outline, 12)
                .into_iter()
                .chain(flatten(&g.detail, 12))
            {
                assert!(
                    p.x >= -0.01 && p.x <= w + 0.01 && p.y >= -0.01 && p.y <= h + 0.01,
                    "{shape:?}: {p:?} outside {w}×{h}"
                );
            }
            let t = g.text;
            assert!(
                t.w > 40. && t.h > 10.,
                "{shape:?}: text area {t:?} too small"
            );
            assert!(
                t.x >= 0. && t.y >= 0. && t.right() <= w && t.bottom() <= h,
                "{shape:?}"
            );
            assert!(g.badge.x > 0. && g.badge.x < w && g.badge.y > 0. && g.badge.y < h);
        }
    }

    #[test]
    fn anchors_sit_on_the_visible_edge() {
        let g = geometry(NodeShape::Parallelogram, 184., 64.);
        // The slanted sides cross the middle height inside the box.
        assert!(g.anchors.left.x > 0. && g.anchors.right.x < 184.);
        let g = geometry(NodeShape::Diamond, 100., 60.);
        assert_eq!(g.anchors.on(Side::Top), xy(50., 0.));
        assert_eq!(g.anchors.on(Side::Left), xy(0., 30.));
    }

    #[test]
    fn tiny_shapes_dont_turn_inside_out() {
        for shape in NodeShape::ALL {
            let g = geometry(shape, 0., -5.);
            assert!(g.text.w >= 0. && g.text.h >= 0.);
            assert!(
                flatten(&g.outline, 4)
                    .iter()
                    .all(|p| p.x.is_finite() && p.y.is_finite())
            );
        }
    }

    #[test]
    fn shapes_save_by_their_names() {
        assert_eq!(
            serde_json::to_value(NodeShape::Parallelogram).unwrap(),
            "parallelogram"
        );
        let s: NodeShape = serde_json::from_value("cloud".into()).unwrap();
        assert_eq!(s, NodeShape::Cloud);
    }
}
