//! Right-angled routes that go round nodes rather than through them, for step and smooth step
//! edges when the flow asks for it.
//!
//! It's the classic orthogonal connector: every node is grown by a gap into an obstacle; the
//! lines through the obstacles' sides and the edge's two ends make a grid; and the shortest
//! way across that grid that enters no obstacle, with each turn costing extra, is the route.
//! Only the obstacles near the edge are counted, and past a limit it gives up and the edge
//! keeps its usual route: avoiding nodes is worth a little time, never a dropped frame.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use crate::types::{Rect, Side, XY, xy};

/// The most obstacles a route takes account of; past it, the edge isn't routed.
pub const MAX_OBSTACLES: usize = 40;
/// What a turn costs, in flow units of length: a longer straight route beats a shorter one
/// with more corners, up to this much.
const BEND: f32 = 40.;

/// A route from `from` (leaving by `from_side`) to `to` (arriving by `to_side`) that keeps
/// `gap` clear of every obstacle, as its corners, both ends included. None when there's none
/// or the problem's too large to be worth it.
pub fn route(
    from: XY,
    from_side: Side,
    to: XY,
    to_side: Side,
    obstacles: &[Rect],
    gap: f32,
) -> Option<Vec<XY>> {
    if obstacles.len() > MAX_OBSTACLES
        || ![from.x, from.y, to.x, to.y, gap]
            .iter()
            .all(|v| v.is_finite())
    {
        return None;
    }
    let grown: Vec<Rect> = obstacles
        .iter()
        .map(|r| Rect::new(r.x - gap, r.y - gap, r.w + gap * 2., r.h + gap * 2.))
        .collect();
    // Out from each end by a little more than the gap, so the route starts clear of its own
    // node and leaves square to it.
    let out = gap + 1.;
    let start = escape(from + from_side.direction() * out, from_side, &grown);
    let end = escape(to + to_side.direction() * out, to_side, &grown);

    let mut xs = vec![start.x, end.x];
    let mut ys = vec![start.y, end.y];
    for r in &grown {
        xs.extend([r.x, r.right()]);
        ys.extend([r.y, r.bottom()]);
    }
    // And half way between the ends, where a single jog usually goes.
    xs.push((start.x + end.x) / 2.);
    ys.push((start.y + end.y) / 2.);
    for v in [&mut xs, &mut ys] {
        v.sort_by(f32::total_cmp);
        v.dedup_by(|a, b| (*a - *b).abs() < 0.01);
    }
    let (nx, ny) = (xs.len(), ys.len());
    let inside = |p: XY| grown.iter().any(|r| strictly_inside(r, p));
    let index = |i: usize, j: usize| j * nx + i;
    let find = |v: &[f32], t: f32| v.iter().position(|x| (x - t).abs() < 0.01);
    let (si, sj) = (find(&xs, start.x)?, find(&ys, start.y)?);
    let (ei, ej) = (find(&xs, end.x)?, find(&ys, end.y)?);
    let open: Vec<bool> = (0..nx * ny)
        .map(|k| !inside(xy(xs[k % nx], ys[k / nx])))
        .collect();
    if !open[index(si, sj)] || !open[index(ei, ej)] {
        return None;
    }
    // A segment between grid neighbours is open if its middle is outside every obstacle (the
    // grid lines are the obstacles' sides, so a segment is wholly in or wholly out).
    let clear = |a: XY, b: XY| !inside((a + b) * 0.5);

    // Dijkstra over (point, heading): headings 0..4 = right, left, down, up; 4 = none yet.
    let states = nx * ny * 5;
    let mut cost = vec![f32::INFINITY; states];
    let mut back: Vec<usize> = vec![usize::MAX; states];
    let first = index(si, sj) * 5 + heading_of(from_side);
    cost[first] = 0.;
    let mut queue = BinaryHeap::new();
    queue.push(Next(0., first));
    let mut done = None;
    while let Some(Next(c, state)) = queue.pop() {
        if c > cost[state] {
            continue;
        }
        let (k, h) = (state / 5, state % 5);
        let (i, j) = (k % nx, k / nx);
        if (i, j) == (ei, ej) {
            done = Some(state);
            break;
        }
        let here = xy(xs[i], ys[j]);
        let steps: [(isize, isize); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
        for (d, (di, dj)) in steps.iter().enumerate() {
            let (ni, nj) = (i as isize + di, j as isize + dj);
            if ni < 0 || nj < 0 || ni >= nx as isize || nj >= ny as isize {
                continue;
            }
            let (ni, nj) = (ni as usize, nj as usize);
            let nk = index(ni, nj);
            let there = xy(xs[ni], ys[nj]);
            if !open[nk] || !clear(here, there) {
                continue;
            }
            // No doubling straight back.
            if h < 4 && is_reverse(h, d) {
                continue;
            }
            let turn = if h == 4 || h == d { 0. } else { BEND };
            let next = nk * 5 + d;
            let nc = c + here.distance(there) + turn;
            if nc < cost[next] {
                cost[next] = nc;
                back[next] = state;
                queue.push(Next(nc, next));
            }
        }
    }
    let mut state = done?;
    let mut points = Vec::new();
    loop {
        let k = state / 5;
        points.push(xy(xs[k % nx], ys[k / nx]));
        if back[state] == usize::MAX {
            break;
        }
        state = back[state];
    }
    points.reverse();
    let mut all = vec![from];
    all.extend(points);
    all.push(to);
    Some(simplify(all))
}

/// `p` moved on the way `side` faces until no obstacle holds it: an end on an inset anchor
/// (a parallelogram's slanted side) is still inside its own node's box a gap away from it.
fn escape(mut p: XY, side: Side, grown: &[Rect]) -> XY {
    for _ in 0..grown.len() + 1 {
        let Some(r) = grown.iter().find(|r| strictly_inside(r, p)) else {
            break;
        };
        p = match side {
            Side::Left => xy(r.x - 1., p.y),
            Side::Right => xy(r.right() + 1., p.y),
            Side::Top => xy(p.x, r.y - 1.),
            Side::Bottom => xy(p.x, r.bottom() + 1.),
        };
    }
    p
}

/// The heading leaving a side: right, left, down, up.
fn heading_of(side: Side) -> usize {
    match side {
        Side::Right => 0,
        Side::Left => 1,
        Side::Bottom => 2,
        Side::Top => 3,
    }
}

fn is_reverse(a: usize, b: usize) -> bool {
    matches!((a, b), (0, 1) | (1, 0) | (2, 3) | (3, 2))
}

fn strictly_inside(r: &Rect, p: XY) -> bool {
    p.x > r.x + 0.01 && p.x < r.right() - 0.01 && p.y > r.y + 0.01 && p.y < r.bottom() - 0.01
}

/// Without the points a straight run passes through, or that repeat.
fn simplify(points: Vec<XY>) -> Vec<XY> {
    let mut out: Vec<XY> = Vec::with_capacity(points.len());
    for p in points {
        if out.last().is_some_and(|l: &XY| l.distance(p) < 0.01) {
            continue;
        }
        if out.len() >= 2 {
            let (a, b) = (out[out.len() - 2], out[out.len() - 1]);
            let straight = (a.x - b.x).abs() < 0.01 && (b.x - p.x).abs() < 0.01
                || (a.y - b.y).abs() < 0.01 && (b.y - p.y).abs() < 0.01;
            if straight {
                out.pop();
            }
        }
        out.push(p);
    }
    out
}

#[derive(PartialEq)]
struct Next(f32, usize);

impl Eq for Next {}

impl PartialOrd for Next {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Next {
    // The cheapest first, for a max-heap.
    fn cmp(&self, other: &Self) -> Ordering {
        other.0.total_cmp(&self.0).then(other.1.cmp(&self.1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crosses(points: &[XY], r: &Rect) -> bool {
        points.windows(2).any(|w| {
            (0..=20).any(|i| {
                let p = w[0] + (w[1] - w[0]) * (i as f32 / 20.);
                strictly_inside(r, p)
            })
        })
    }

    #[test]
    fn a_route_goes_round_a_node_in_the_way() {
        // From the right of a node at the left to the left of one at the right, with a third
        // node square between them.
        let source = Rect::new(0., 0., 100., 40.);
        let target = Rect::new(400., 0., 100., 40.);
        let wall = Rect::new(200., -60., 100., 160.);
        let points = route(
            xy(100., 20.),
            Side::Right,
            xy(400., 20.),
            Side::Left,
            &[source, target, wall],
            12.,
        )
        .expect("a route");
        assert_eq!(points.first(), Some(&xy(100., 20.)));
        assert_eq!(points.last(), Some(&xy(400., 20.)));
        assert!(!crosses(&points, &wall), "{points:?}");
        // Every piece runs straight across or down.
        for w in points.windows(2) {
            assert!((w[0].x - w[1].x).abs() < 0.01 || (w[0].y - w[1].y).abs() < 0.01);
        }
        // Round the end of the wall nearer the line: above or below, with few corners.
        assert!(points.len() <= 8, "{points:?}");
    }

    #[test]
    fn nothing_in_the_way_is_a_straight_line() {
        let points = route(
            xy(100., 20.),
            Side::Right,
            xy(400., 20.),
            Side::Left,
            &[Rect::new(0., 0., 100., 40.), Rect::new(400., 0., 100., 40.)],
            12.,
        )
        .unwrap();
        assert_eq!(points, [xy(100., 20.), xy(400., 20.)]);
    }

    #[test]
    fn an_end_inside_its_own_box_still_routes() {
        // Into a parallelogram's slanted left side, inset from its box, past a node between.
        let target = Rect::new(230., -32., 184., 64.);
        let wall = Rect::new(484., -56., 184., 112.);
        let source = Rect::new(738., 38., 160., 64.);
        let points = route(
            xy(898., 70.),
            Side::Right,
            xy(241., 0.),
            Side::Left,
            &[target, wall, source],
            12.,
        )
        .expect("a route");
        assert!(!crosses(&points, &wall), "{points:?}");
        assert_eq!(points.last(), Some(&xy(241., 0.)));
    }

    #[test]
    fn too_much_to_route_is_left_alone() {
        let many: Vec<Rect> = (0..=MAX_OBSTACLES)
            .map(|i| Rect::new(i as f32 * 50., 300., 10., 10.))
            .collect();
        assert!(
            route(
                xy(0., 0.),
                Side::Right,
                xy(100., 0.),
                Side::Left,
                &many,
                10.
            )
            .is_none()
        );
        assert!(
            route(
                xy(f32::NAN, 0.),
                Side::Right,
                xy(100., 0.),
                Side::Left,
                &[],
                10.
            )
            .is_none()
        );
    }
}
