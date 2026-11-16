//! The lines edges follow, in flow coordinates: the same routes React Flow draws for its
//! bezier, simple bezier, straight, step and smooth step edges, and where each one's label goes.

use crate::types::{EdgeKind, Rect, Side, XY, xy};

/// One piece of an edge's line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Segment {
    Line(XY),
    /// A quadratic curve to `to`.
    Quad {
        ctrl: XY,
        to: XY,
    },
    /// A cubic curve to `to`.
    Cubic {
        c1: XY,
        c2: XY,
        to: XY,
    },
}

/// An edge's line: where it starts, the pieces that follow, and where its label sits.
#[derive(Clone, Debug, PartialEq)]
pub struct EdgePath {
    pub start: XY,
    pub segments: Vec<Segment>,
    pub label: XY,
}

impl EdgePath {
    pub fn end(&self) -> XY {
        match self.segments.last() {
            Some(Segment::Line(to) | Segment::Quad { to, .. } | Segment::Cubic { to, .. }) => *to,
            None => self.start,
        }
    }

    /// A box the whole line lies in: the box around its points and control points, which a
    /// curve never leaves.
    pub fn bounds(&self) -> Rect {
        let (mut lo, mut hi) = (self.start, self.start);
        let mut take = |p: XY| {
            lo = xy(lo.x.min(p.x), lo.y.min(p.y));
            hi = xy(hi.x.max(p.x), hi.y.max(p.y));
        };
        for segment in &self.segments {
            match *segment {
                Segment::Line(to) => take(to),
                Segment::Quad { ctrl, to } => {
                    take(ctrl);
                    take(to);
                }
                Segment::Cubic { c1, c2, to } => {
                    take(c1);
                    take(c2);
                    take(to);
                }
            }
        }
        Rect::from_corners(lo, hi)
    }

    /// The line as straight pieces no longer than about `step` each along its curves.
    pub fn flatten(&self, step: f32) -> Vec<XY> {
        let mut out = vec![self.start];
        let mut at = self.start;
        for segment in &self.segments {
            match *segment {
                Segment::Line(to) => out.push(to),
                Segment::Quad { ctrl, to } => {
                    let n = pieces(at.distance(ctrl) + ctrl.distance(to), step);
                    for i in 1..=n {
                        let t = i as f32 / n as f32;
                        let u = 1. - t;
                        out.push(at * (u * u) + ctrl * (2. * u * t) + to * (t * t));
                    }
                }
                Segment::Cubic { c1, c2, to } => {
                    let n = pieces(at.distance(c1) + c1.distance(c2) + c2.distance(to), step);
                    for i in 1..=n {
                        let t = i as f32 / n as f32;
                        let u = 1. - t;
                        out.push(
                            at * (u * u * u)
                                + c1 * (3. * u * u * t)
                                + c2 * (3. * u * t * t)
                                + to * (t * t * t),
                        );
                    }
                }
            }
            at = out[out.len() - 1];
        }
        out
    }
}

fn pieces(length: f32, step: f32) -> usize {
    ((length / step.max(0.1)).ceil() as usize).clamp(4, 96)
}

/// An edge's line of the given kind between two handles.
pub fn edge_path(
    kind: &EdgeKind,
    source: XY,
    source_position: Side,
    target: XY,
    target_position: Side,
) -> EdgePath {
    match kind {
        EdgeKind::Bezier | EdgeKind::Custom(_) => {
            bezier_path(source, source_position, target, target_position, 0.25)
        }
        EdgeKind::SimpleBezier => {
            simple_bezier_path(source, source_position, target, target_position)
        }
        EdgeKind::Straight => straight_path(source, target),
        EdgeKind::Step => {
            smooth_step_path(source, source_position, target, target_position, 0., 20.)
        }
        EdgeKind::SmoothStep => {
            smooth_step_path(source, source_position, target, target_position, 5., 20.)
        }
    }
}

pub fn straight_path(source: XY, target: XY) -> EdgePath {
    EdgePath {
        start: source,
        segments: vec![Segment::Line(target)],
        label: (source + target) * 0.5,
    }
}

fn control_offset(distance: f32, curvature: f32) -> f32 {
    if distance >= 0. {
        0.5 * distance
    } else {
        curvature * 25. * (-distance).sqrt()
    }
}

fn control_with_curvature(position: Side, a: XY, b: XY, curvature: f32) -> XY {
    match position {
        Side::Left => xy(a.x - control_offset(a.x - b.x, curvature), a.y),
        Side::Right => xy(a.x + control_offset(b.x - a.x, curvature), a.y),
        Side::Top => xy(a.x, a.y - control_offset(a.y - b.y, curvature)),
        Side::Bottom => xy(a.x, a.y + control_offset(b.y - a.y, curvature)),
    }
}

fn cubic(source: XY, c1: XY, c2: XY, target: XY) -> EdgePath {
    EdgePath {
        start: source,
        segments: vec![Segment::Cubic { c1, c2, to: target }],
        // The curve's own midpoint.
        label: source * 0.125 + c1 * 0.375 + c2 * 0.375 + target * 0.125,
    }
}

/// The default edge: a curve whose bend grows when the target sits behind the source, so the
/// line still leaves and arrives square to its handles.
pub fn bezier_path(
    source: XY,
    source_position: Side,
    target: XY,
    target_position: Side,
    curvature: f32,
) -> EdgePath {
    let c1 = control_with_curvature(source_position, source, target, curvature);
    let c2 = control_with_curvature(target_position, target, source, curvature);
    cubic(source, c1, c2, target)
}

fn simple_control(position: Side, a: XY, b: XY) -> XY {
    match position {
        Side::Left | Side::Right => xy(0.5 * (a.x + b.x), a.y),
        Side::Top | Side::Bottom => xy(a.x, 0.5 * (a.y + b.y)),
    }
}

pub fn simple_bezier_path(
    source: XY,
    source_position: Side,
    target: XY,
    target_position: Side,
) -> EdgePath {
    let c1 = simple_control(source_position, source, target);
    let c2 = simple_control(target_position, target, source);
    cubic(source, c1, c2, target)
}

/// Reads or writes the coordinate a route runs along.
fn along(p: XY, horizontal: bool) -> f32 {
    if horizontal { p.x } else { p.y }
}

fn across(p: XY, horizontal: bool) -> f32 {
    if horizontal { p.y } else { p.x }
}

/// The corners of a right-angled route between two handles, both ends included, and where its
/// label goes. `offset` is how far the line runs straight out of each handle before it may turn.
pub fn step_points(
    source: XY,
    source_position: Side,
    target: XY,
    target_position: Side,
    offset: f32,
) -> (Vec<XY>, XY) {
    let source_dir = source_position.direction();
    let target_dir = target_position.direction();
    let source_gapped = source + source_dir * offset;
    let target_gapped = target + target_dir * offset;

    // The way the route mostly runs, as seen from the source's side.
    let horizontal = matches!(source_position, Side::Left | Side::Right);
    let dir = if horizontal {
        if source_gapped.x < target_gapped.x {
            1.
        } else {
            -1.
        }
    } else if source_gapped.y < target_gapped.y {
        1.
    } else {
        -1.
    };

    let mut source_gap_offset = xy(0., 0.);
    let mut target_gap_offset = xy(0., 0.);
    let default_center = (source + target) * 0.5;
    let points: Vec<XY>;
    let center: XY;

    if along(source_dir, horizontal) * along(target_dir, horizontal) == -1. {
        // Handles that face each other: one jog in the middle.
        center = default_center;
        let vertical_split = vec![xy(center.x, source_gapped.y), xy(center.x, target_gapped.y)];
        let horizontal_split = vec![xy(source_gapped.x, center.y), xy(target_gapped.x, center.y)];
        points = if along(source_dir, horizontal) == dir {
            if horizontal {
                vertical_split
            } else {
                horizontal_split
            }
        } else if horizontal {
            horizontal_split
        } else {
            vertical_split
        };
    } else {
        // One corner: x from the source and y from the target, or the other way round.
        let source_target = vec![xy(source_gapped.x, target_gapped.y)];
        let target_source = vec![xy(target_gapped.x, source_gapped.y)];
        let mut corner = if horizontal {
            if source_dir.x == dir {
                target_source.clone()
            } else {
                source_target.clone()
            }
        } else if source_dir.y == dir {
            source_target.clone()
        } else {
            target_source.clone()
        };

        if source_position == target_position {
            // Two handles on the same side and nearly level: without this the corner would
            // land on the gapped ends and the line would double back on itself.
            let diff = (along(source, horizontal) - along(target, horizontal)).abs();
            if diff <= offset {
                let gap = (offset - 1.).min(offset - diff);
                let set = |p: &mut XY, v: f32| {
                    if horizontal { p.x = v } else { p.y = v }
                };
                if along(source_dir, horizontal) == dir {
                    let sign = if along(source_gapped, horizontal) > along(source, horizontal) {
                        -1.
                    } else {
                        1.
                    };
                    set(&mut source_gap_offset, sign * gap);
                } else {
                    let sign = if along(target_gapped, horizontal) > along(target, horizontal) {
                        -1.
                    } else {
                        1.
                    };
                    set(&mut target_gap_offset, sign * gap);
                }
            }
        } else {
            // Handles on adjoining sides, such as right to bottom.
            let same_dir = along(source_dir, horizontal) == across(target_dir, horizontal);
            let source_gt = across(source_gapped, horizontal) > across(target_gapped, horizontal);
            let source_lt = across(source_gapped, horizontal) < across(target_gapped, horizontal);
            let flip = if along(source_dir, horizontal) == 1. {
                (!same_dir && source_gt) || (same_dir && source_lt)
            } else {
                (!same_dir && source_lt) || (same_dir && source_gt)
            };
            if flip {
                corner = if horizontal {
                    source_target
                } else {
                    target_source
                };
            }
        }

        let source_gap_point = source_gapped + source_gap_offset;
        let target_gap_point = target_gapped + target_gap_offset;
        let max_x = (source_gap_point.x - corner[0].x)
            .abs()
            .max((target_gap_point.x - corner[0].x).abs());
        let max_y = (source_gap_point.y - corner[0].y)
            .abs()
            .max((target_gap_point.y - corner[0].y).abs());
        // The label goes on the longer leg.
        center = if max_x >= max_y {
            xy((source_gap_point.x + target_gap_point.x) / 2., corner[0].y)
        } else {
            xy(corner[0].x, (source_gap_point.y + target_gap_point.y) / 2.)
        };
        points = corner;
    }

    // A point the route runs straight through is dropped: left in, it would shrink the bend of
    // the corner after it (React Flow 12.12.1).
    let mut all: Vec<XY> = Vec::with_capacity(points.len() + 4);
    let ends = [source, source_gapped + source_gap_offset];
    let tail = [target_gapped + target_gap_offset, target];
    for p in ends.into_iter().chain(points).chain(tail) {
        if all.len() >= 2 && between(all[all.len() - 2], all[all.len() - 1], p) {
            all.pop();
        }
        all.push(p);
    }
    (all, center)
}

/// Whether `b` lies on the straight, axis-aligned piece from `a` to `c` (`b == c` included).
fn between(a: XY, b: XY, c: XY) -> bool {
    (a.x == b.x && b.x == c.x && (b.y - a.y) * (c.y - b.y) >= 0.)
        || (a.y == b.y && b.y == c.y && (b.x - a.x) * (c.x - b.x) >= 0.)
}

/// The part of the segment from `a` to `b` inside `area`, as the fractions along it where that
/// part starts and ends. None when it misses (or a point isn't finite).
pub fn clip_segment(a: XY, b: XY, area: &Rect) -> Option<(f32, f32)> {
    if !(a.x.is_finite() && a.y.is_finite() && b.x.is_finite() && b.y.is_finite()) {
        return None;
    }
    let d = b - a;
    let (mut t0, mut t1) = (0f32, 1f32);
    // Liang–Barsky: each side of the box cuts the segment's range of t.
    for (p, q) in [
        (-d.x, a.x - area.x),
        (d.x, area.right() - a.x),
        (-d.y, a.y - area.y),
        (d.y, area.bottom() - a.y),
    ] {
        if p == 0. {
            if q < 0. {
                return None;
            }
        } else {
            let r = q / p;
            if p < 0. {
                t0 = t0.max(r);
            } else {
                t1 = t1.min(r);
            }
            if t0 > t1 {
                return None;
            }
        }
    }
    Some((t0, t1))
}

/// A right-angled route with its corners rounded by `radius` (none for a plain step edge).
pub fn smooth_step_path(
    source: XY,
    source_position: Side,
    target: XY,
    target_position: Side,
    radius: f32,
    offset: f32,
) -> EdgePath {
    let (points, label) = step_points(source, source_position, target, target_position, offset);
    rounded_path(&points, radius, label)
}

/// A line through `points` (corners at right angles), its corners rounded by `radius`, with
/// its label at `label`.
pub fn rounded_path(points: &[XY], radius: f32, label: XY) -> EdgePath {
    let Some(&start) = points.first() else {
        return EdgePath {
            start: label,
            segments: Vec::new(),
            label,
        };
    };
    let mut segments = Vec::with_capacity(points.len() * 2);
    for i in 1..points.len() {
        let b = points[i];
        if i == points.len() - 1 {
            segments.push(Segment::Line(b));
            break;
        }
        let a = points[i - 1];
        let c = points[i + 1];
        let straight = (a.x == b.x && b.x == c.x) || (a.y == b.y && b.y == c.y);
        let bend = (a.distance(b) / 2.).min(b.distance(c) / 2.).min(radius);
        if straight || bend <= 0. {
            segments.push(Segment::Line(b));
            continue;
        }
        // Stop short of the corner, curve round it, and carry on from as far past it.
        let toward = |from: XY, to: XY| {
            let d = from.distance(to).max(f32::EPSILON);
            to + (from - to) * (bend / d)
        };
        segments.push(Segment::Line(toward(a, b)));
        segments.push(Segment::Quad {
            ctrl: b,
            to: toward(c, b),
        });
    }
    EdgePath {
        start,
        segments,
        label,
    }
}

/// The middle of the longest piece of a line through `points`: where a routed edge's label
/// reads best.
pub fn longest_middle(points: &[XY]) -> XY {
    points
        .windows(2)
        .max_by(|a, b| a[0].distance(a[1]).total_cmp(&b[0].distance(b[1])))
        .map_or(points.first().copied().unwrap_or_default(), |w| {
            (w[0] + w[1]) * 0.5
        })
}

/// How far `p` is from the nearest point of the line through `points`.
pub fn distance_to_polyline(p: XY, points: &[XY]) -> f32 {
    let mut best = f32::MAX;
    for pair in points.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let ab = b - a;
        let len2 = ab.x * ab.x + ab.y * ab.y;
        let t = if len2 == 0. {
            0.
        } else {
            (((p.x - a.x) * ab.x + (p.y - a.y) * ab.y) / len2).clamp(0., 1.)
        };
        best = best.min(p.distance(a + ab * t));
    }
    if points.len() == 1 {
        best = p.distance(points[0]);
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bezier_leaves_and_arrives_square_to_its_handles() {
        let path = bezier_path(xy(0., 0.), Side::Bottom, xy(100., 200.), Side::Top, 0.25);
        let [Segment::Cubic { c1, c2, to }] = path.segments[..] else {
            panic!("one curve");
        };
        assert_eq!((c1, c2, to), (xy(0., 100.), xy(100., 100.), xy(100., 200.)));
        assert_eq!(path.label, xy(50., 100.));
    }

    #[test]
    fn a_bezier_bends_out_when_the_target_is_behind_the_source() {
        // The target is above a handle that points down: the control points still go down from
        // the source and up from the target, by 0.25 * 25 * sqrt(distance).
        let path = bezier_path(xy(0., 0.), Side::Bottom, xy(0., -100.), Side::Top, 0.25);
        let [Segment::Cubic { c1, c2, .. }] = path.segments[..] else {
            panic!("one curve");
        };
        assert_eq!(c1, xy(0., 62.5));
        assert_eq!(c2, xy(0., -162.5));
    }

    #[test]
    fn a_step_between_facing_handles_jogs_half_way() {
        let (points, label) = step_points(xy(0., 0.), Side::Right, xy(200., 100.), Side::Left, 20.);
        assert_eq!(
            points,
            // The gapped ends lie on the straight legs, so they're dropped.
            [xy(0., 0.), xy(100., 0.), xy(100., 100.), xy(200., 100.)]
        );
        assert_eq!(label, xy(100., 50.));
    }

    #[test]
    fn a_step_goes_round_when_the_target_is_behind() {
        // Bottom to top with the target above: out by the gap, across half way, and back up.
        let (points, _) = step_points(xy(0., 0.), Side::Bottom, xy(100., -100.), Side::Top, 20.);
        assert_eq!(
            points,
            [
                xy(0., 0.),
                xy(0., 20.),
                xy(50., 20.),
                xy(50., -120.),
                xy(100., -120.),
                xy(100., -100.)
            ]
        );
    }

    #[test]
    fn a_step_between_adjoining_sides_turns_once() {
        let (points, _) = step_points(xy(0., 0.), Side::Right, xy(100., 100.), Side::Top, 20.);
        assert_eq!(points, [xy(0., 0.), xy(100., 0.), xy(100., 100.)]);
    }

    #[test]
    fn short_legs_keep_the_full_corner_radius() {
        // React Flow 12.12.1: without dropping (20, 0) and (30, 100) the corners would only get
        // half of the 5 they're due.
        let path = smooth_step_path(xy(0., 0.), Side::Right, xy(50., 100.), Side::Left, 5., 20.);
        let corner = path.segments.iter().find_map(|s| match s {
            Segment::Quad { ctrl, to } => Some((*ctrl, *to)),
            _ => None,
        });
        assert_eq!(corner, Some((xy(25., 0.), xy(25., 5.))));
    }

    #[test]
    fn clipping_keeps_the_part_inside_the_box() {
        let area = Rect::new(0., 0., 100., 100.);
        assert_eq!(
            clip_segment(xy(-100., 50.), xy(100., 50.), &area),
            Some((0.5, 1.))
        );
        assert_eq!(clip_segment(xy(-10., -10.), xy(-5., 200.), &area), None);
        assert_eq!(
            clip_segment(xy(10., 10.), xy(20., 20.), &area),
            Some((0., 1.))
        );
        assert_eq!(clip_segment(xy(f32::NAN, 0.), xy(20., 20.), &area), None);
        let far = clip_segment(xy(0., 50.), xy(1e9, 50.), &area).unwrap();
        assert!(far.1 > 0. && far.1 < 1e-6);
    }

    #[test]
    fn smooth_steps_round_each_corner_and_end_where_they_should() {
        let path = smooth_step_path(xy(0., 0.), Side::Right, xy(200., 100.), Side::Left, 5., 20.);
        assert_eq!(path.end(), xy(200., 100.));
        let quads = path
            .segments
            .iter()
            .filter(|s| matches!(s, Segment::Quad { .. }))
            .count();
        assert_eq!(quads, 2);
        // Every flattened point stays within the route's box.
        for p in path.flatten(2.) {
            assert!(
                (0. ..=200.).contains(&p.x) && (0. ..=100.).contains(&p.y),
                "{p:?}"
            );
        }
        let plain = smooth_step_path(xy(0., 0.), Side::Right, xy(200., 100.), Side::Left, 0., 20.);
        assert!(plain.segments.iter().all(|s| matches!(s, Segment::Line(_))));
    }

    #[test]
    fn distance_to_a_line_is_to_its_nearest_point() {
        let line = [xy(0., 0.), xy(100., 0.), xy(100., 100.)];
        assert_eq!(distance_to_polyline(xy(50., 7.), &line), 7.);
        assert_eq!(distance_to_polyline(xy(104., 50.), &line), 4.);
        assert_eq!(distance_to_polyline(xy(-3., -4.), &line), 5.);
    }
}
