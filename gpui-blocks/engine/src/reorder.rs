//! Motion when top-level blocks change places.
//!
//! While a block is dragged it follows the pointer exactly, and the blocks it passes slide out
//! of the way to open the slot it would drop into. When the order changes (a drop, a keyboard
//! move, an undo of either), every block that moved starts from where it was last drawn and
//! springs to its new place; a dropped block starts at the pointer's speed and lands with a
//! little give, the rest settle without overshoot. Springs, not timed curves, so a block caught
//! mid-flight by the next move carries on from where and how fast it was going.
//!
//! Positions are the layout's own (`Slot`: a block's top and the distance to the next block's
//! top, in editor-local y), not what was drawn, so the slot under the pointer can't feed back
//! on the motion. Nothing here asks for a frame once everything has settled.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use super::id::BlockId;

/// A top-level block's place in the layout.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Slot {
    pub(crate) top: f32,
    /// From this block's top to the next one's: its height and the gap after it.
    pub(crate) pitch: f32,
}

/// Stiff and fast: a block is most of the way there (about 95 percent of its travel) in about
/// 150 ms. Coming to rest, within `REST` and slower than `REST_SPEED`, takes about 160 to 390 ms
/// at 120 Hz frames, the longer the farther it travels.
const STIFFNESS: f32 = 1100.;
/// Critically damped: blocks making room, or moved by a key, arrive without overshoot.
const SETTLE: f32 = 2. * 1.0 * 33.2;
/// A drop lands with a slight give, since the pointer threw it.
const RELEASE: f32 = 2. * 0.8 * 33.2;
/// Within this many points, and slower than `REST_SPEED` points a second, a spring is at rest.
const REST: f32 = 0.25;
const REST_SPEED: f32 = 4.;

#[derive(Clone, Copy, Debug)]
struct Spring {
    x: f32,
    v: f32,
    damping: f32,
}

impl Spring {
    fn step(&mut self, target: f32, dt: f32) {
        // Semi-implicit Euler in steps of at most 4 ms, stable at these constants.
        let mut left = dt;
        while left > 0. {
            let h = left.min(0.004);
            let a = -STIFFNESS * (self.x - target) - self.damping * self.v;
            self.v += a * h;
            self.x += self.v * h;
            left -= h;
        }
    }

    fn at_rest(&self, target: f32) -> bool {
        (self.x - target).abs() < REST && self.v.abs() < REST_SPEED
    }
}

/// The block being dragged, this frame.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Dragged {
    pub(crate) block: BlockId,
    /// How far the pointer has moved since the drag began, in points.
    pub(crate) dy: f32,
    /// Where the pointer is, in content y: the slot is the one it's over.
    pub(crate) pointer: f32,
    /// The pointer's speed, in points a second.
    pub(crate) velocity: f32,
}

#[derive(Default)]
pub(crate) struct Reorder {
    springs: HashMap<BlockId, Spring>,
    /// The order and slots of the last frame.
    last: Vec<(BlockId, Slot)>,
    /// The slots when the current drag began.
    drag_slots: Option<Vec<(BlockId, Slot)>>,
    /// The middle of each block as drawn when the drag began, in content y.
    middles: HashMap<BlockId, f32>,
    /// The drag just let go of, for the frame that follows.
    released: Option<Dragged>,
    last_step: Option<Instant>,
    /// Whether anything was moving after the last frame. Frames are drawn while nothing moves
    /// too (a caret blink), so the time since the last one is no step for a motion just begun.
    moving: bool,
}

impl Reorder {
    /// A drag became active: the slots it measures against are this layout's, and `middles`
    /// are the blocks' middles as drawn (in content y, which scrolling leaves alone), which the
    /// pointer passes to move them.
    pub(crate) fn begin_drag(&mut self, mut middles: HashMap<BlockId, f32>) {
        // A block that wasn't drawn (outside the laid-out range) gets a middle from its slot,
        // moved by how far the measured ones are from theirs.
        let centre = |s: &Slot| s.top + s.pitch / 2.;
        let shifts: Vec<f32> = self
            .last
            .iter()
            .filter_map(|(id, s)| Some(middles.get(id)? - centre(s)))
            .collect();
        let shift = shifts.iter().sum::<f32>() / shifts.len().max(1) as f32;
        for (id, slot) in &self.last {
            middles.entry(*id).or_insert(centre(slot) + shift);
        }
        self.drag_slots = Some(self.last.clone());
        self.middles = middles;
        self.released = None;
    }

    /// The drag was abandoned without a drop (the document changed under it): every block,
    /// the dragged one too, springs back to its place.
    pub(crate) fn cancel_drag(&mut self) {
        self.drag_slots = None;
        self.released = None;
    }

    /// The drag ended (dropped or not); the next frame lands the block.
    pub(crate) fn release(&mut self, dragged: Dragged) {
        self.drag_slots = None;
        self.released = Some(dragged);
    }

    /// The gap between top-level blocks (0 is before the first) the dragged block would drop
    /// into: past every block whose middle the pointer has passed. In the terms of
    /// `BlockEditorModel::move_block_to`.
    pub(crate) fn drop_gap(&self, dragged: &Dragged) -> Option<usize> {
        let slots = self.drag_slots.as_ref()?;
        let (from, at) = new_index(slots, &self.middles, dragged)?;
        Some(if at > from { at + 1 } else { at })
    }

    /// Advances the motion to `now` for a layout of `slots`, and returns each moving block's
    /// offset from its slot, and whether anything is still moving (so another frame is needed).
    pub(crate) fn frame(
        &mut self,
        slots: Vec<(BlockId, Slot)>,
        dragged: Option<Dragged>,
        now: Instant,
        reduce_motion: bool,
    ) -> (HashMap<BlockId, f32>, bool) {
        // A motion's first frame draws where it starts; it steps from the next one on.
        let dt = match self.last_step {
            Some(t) if self.moving => now.duration_since(t).as_secs_f32().min(1. / 30.),
            _ => 0.,
        };
        self.last_step = Some(now);

        // A block that left the layout (merged into another by a drop, or deleted) has nothing
        // left to move.
        let present: HashSet<BlockId> = slots.iter().map(|(id, _)| *id).collect();
        self.springs.retain(|id, _| present.contains(id));
        // The order changed: each block that is still there starts from where it was drawn,
        // even when the drop merged or split blocks around it.
        if reordered(&self.last, &slots) {
            let before: HashMap<BlockId, f32> =
                self.last.iter().map(|(id, s)| (*id, s.top)).collect();
            for (id, slot) in &slots {
                let Some(top) = before.get(id) else {
                    continue;
                };
                let spring = self.springs.entry(*id).or_insert(Spring {
                    x: 0.,
                    v: 0.,
                    damping: SETTLE,
                });
                spring.x += top - slot.top;
                spring.damping = SETTLE;
            }
        }
        // A released block lands from the pointer, at its speed.
        if let Some(released) = self.released.take()
            && let Some(spring) = self.springs.get_mut(&released.block)
        {
            spring.v = released.velocity;
            spring.damping = RELEASE;
        }

        let targets = match (&self.drag_slots, dragged) {
            (Some(start), Some(dragged)) => room_for(start, &self.middles, &dragged),
            _ => HashMap::new(),
        };
        // Every block that has somewhere to go, and the one under the pointer, has a spring.
        let held = dragged
            .filter(|_| self.drag_slots.is_some())
            .map(|d| d.block);
        for id in targets.keys().copied().chain(held) {
            self.springs.entry(id).or_insert(Spring {
                x: 0.,
                v: 0.,
                damping: SETTLE,
            });
        }
        let mut moving = false;
        self.springs.retain(|id, spring| {
            if let Some(d) = dragged.filter(|d| d.block == *id && self.drag_slots.is_some()) {
                // Under the pointer: exactly where it is, no spring.
                spring.x = d.dy;
                spring.v = d.velocity;
                spring.damping = RELEASE;
                return true;
            }
            let target = targets.get(id).copied().unwrap_or(0.);
            if reduce_motion {
                spring.x = target;
                spring.v = 0.;
            } else {
                spring.step(target, dt);
            }
            if spring.at_rest(target) {
                spring.x = target;
                spring.v = 0.;
                return target != 0.;
            }
            moving = true;
            true
        });
        self.last = slots;
        self.moving = moving;
        let offsets = self
            .springs
            .iter()
            .filter(|(_, s)| s.x != 0.)
            .map(|(id, s)| (*id, s.x))
            .collect();
        (offsets, moving)
    }
}

/// Whether the blocks in both layouts come in a different order. Blocks in only one of them
/// (merged, split, added or deleted) don't count: an edit that keeps the order of the rest
/// moves nothing.
fn reordered(before: &[(BlockId, Slot)], after: &[(BlockId, Slot)]) -> bool {
    let in_before: HashSet<BlockId> = before.iter().map(|(id, _)| *id).collect();
    let in_after: HashSet<BlockId> = after.iter().map(|(id, _)| *id).collect();
    let a = before
        .iter()
        .map(|(id, _)| *id)
        .filter(|id| in_after.contains(id));
    let b = after
        .iter()
        .map(|(id, _)| *id)
        .filter(|id| in_before.contains(id));
    !a.eq(b)
}

/// The dragged block's index in `slots`, and its index once dropped where the pointer is: after
/// every other block whose middle is above the pointer. A block that wasn't drawn (scrolled far
/// away) stays on its side.
fn new_index(
    slots: &[(BlockId, Slot)],
    middles: &HashMap<BlockId, f32>,
    dragged: &Dragged,
) -> Option<(usize, usize)> {
    let from = slots.iter().position(|(id, _)| *id == dragged.block)?;
    let at = slots
        .iter()
        .enumerate()
        .filter(|(ix, (id, _))| {
            *ix != from
                && middles
                    .get(id)
                    .map_or(*ix < from, |middle| *middle < dragged.pointer)
        })
        .count();
    Some((from, at))
}

/// Where the other blocks go to make room: those between the dragged block's slot and the one
/// it would drop into move by its pitch, toward the slot it left.
fn room_for(
    slots: &[(BlockId, Slot)],
    middles: &HashMap<BlockId, f32>,
    dragged: &Dragged,
) -> HashMap<BlockId, f32> {
    let Some((from, at)) = new_index(slots, middles, dragged) else {
        return HashMap::new();
    };
    let pitch = slots[from].1.pitch;
    slots
        .iter()
        .enumerate()
        .filter_map(|(ix, (id, _))| {
            let target = if from < ix && ix <= at {
                -pitch
            } else if at <= ix && ix < from {
                pitch
            } else {
                0.
            };
            (ix != from && target != 0.).then_some((*id, target))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn slots(ids: &[BlockId]) -> Vec<(BlockId, Slot)> {
        ids.iter()
            .enumerate()
            .map(|(ix, id)| {
                (
                    *id,
                    Slot {
                        top: ix as f32 * 40.,
                        pitch: 40.,
                    },
                )
            })
            .collect()
    }

    /// Each block's middle, drawn where it lays out.
    fn middles(layout: &[(BlockId, Slot)]) -> HashMap<BlockId, f32> {
        layout.iter().map(|(id, s)| (*id, s.top + 20.)).collect()
    }

    fn ids(n: usize) -> Vec<BlockId> {
        (0..n).map(|_| BlockId::next()).collect()
    }

    /// Runs frames 8 ms apart until nothing moves; returns how many it took.
    fn settle(r: &mut Reorder, layout: &[(BlockId, Slot)], start: Instant) -> usize {
        for n in 1..200 {
            let (_, moving) = r.frame(
                layout.to_vec(),
                None,
                start + Duration::from_millis(8 * n as u64),
                false,
            );
            if !moving {
                return n;
            }
        }
        panic!("never settled");
    }

    #[test]
    fn dragging_down_opens_the_slot_below_and_drops_into_it() {
        let b = ids(4);
        let layout = slots(&b);
        let mut r = Reorder::default();
        let t = Instant::now();
        r.frame(layout.clone(), None, t, false);
        r.begin_drag(middles(&slots(&b)));
        // Picked up at its middle (20) and moved 75 down, to 95: past block 1's middle (60), not 2's (100).
        let d = Dragged {
            block: b[0],
            dy: 75.,
            pointer: 95.,
            velocity: 300.,
        };
        assert_eq!(r.drop_gap(&d), Some(2));
        let (offsets, moving) =
            r.frame(layout.clone(), Some(d), t + Duration::from_millis(8), false);
        assert!(moving);
        assert_eq!(
            offsets[&b[0]], 75.,
            "the dragged block is exactly under the pointer"
        );
        assert!(
            !offsets.contains_key(&b[1]),
            "the block passed starts where it is"
        );
        let (offsets, _) = r.frame(
            layout.clone(),
            Some(d),
            t + Duration::from_millis(16),
            false,
        );
        assert!(offsets[&b[1]] < 0., "the block passed is moving up");
        assert!(!offsets.contains_key(&b[2]), "the block not passed stays");
        // Held there, block 1 ends a whole slot up and stops asking for frames.
        let mut moving = true;
        let mut n = 3;
        while moving && n < 100 {
            let (o, m) = r.frame(
                layout.clone(),
                Some(d),
                t + Duration::from_millis(8 * n),
                false,
            );
            moving = m;
            n += 1;
            if !moving {
                assert_eq!(o[&b[1]], -40.);
            }
        }
        assert!(!moving, "at rest while held still");
    }

    #[test]
    fn a_drop_lands_from_the_pointer_and_settles_with_nothing_left_moving() {
        let b = ids(3);
        let mut r = Reorder::default();
        let t = Instant::now();
        r.frame(slots(&b), None, t, false);
        r.begin_drag(middles(&slots(&b)));
        let d = Dragged {
            block: b[0],
            dy: 50.,
            pointer: 70.,
            velocity: 600.,
        };
        r.frame(slots(&b), Some(d), t, false);
        r.release(d);
        // The model moved block 0 after block 1.
        let after = slots(&[b[1], b[0], b[2]]);
        let (offsets, moving) = r.frame(after.clone(), None, t + Duration::from_millis(8), false);
        assert!(moving);
        // It was drawn at 0 + 50 and now lays out at 40: it starts about 10 below its slot.
        assert!((offsets[&b[0]] - 10.).abs() < 6., "{offsets:?}");
        let frames = settle(&mut r, &after, t + Duration::from_millis(8));
        assert!(
            frames < 40,
            "settles within about 300 ms, took {frames} frames"
        );
        let (offsets, moving) = r.frame(after, None, t + Duration::from_secs(2), false);
        assert!(offsets.is_empty() && !moving);
    }

    #[test]
    fn a_keyboard_move_slides_both_blocks_without_overshoot() {
        let b = ids(2);
        let mut r = Reorder::default();
        let t = Instant::now();
        r.frame(slots(&b), None, t, false);
        let after = slots(&[b[1], b[0]]);
        let mut low = f32::MAX;
        let mut x = r.frame(after.clone(), None, t, false).0[&b[0]];
        assert_eq!(x, -40., "block 0 starts where it was, a slot above");
        for n in 1..60 {
            let (o, _) = r.frame(after.clone(), None, t + Duration::from_millis(8 * n), false);
            x = o.get(&b[0]).copied().unwrap_or(0.);
            low = low.min(-x.abs());
            assert!(x <= 0.01, "never passes its slot: {x}");
        }
        assert_eq!(x, 0.);
        let _ = low;
    }

    #[test]
    fn reduced_motion_jumps_but_the_dragged_block_still_follows() {
        let b = ids(3);
        let mut r = Reorder::default();
        let t = Instant::now();
        r.frame(slots(&b), None, t, true);
        r.begin_drag(middles(&slots(&b)));
        let d = Dragged {
            block: b[0],
            dy: 50.,
            pointer: 70.,
            velocity: 0.,
        };
        let (o, moving) = r.frame(slots(&b), Some(d), t, true);
        assert_eq!(o[&b[0]], 50.);
        assert_eq!(o[&b[1]], -40.);
        assert!(!moving);
    }

    #[test]
    fn a_drop_that_merges_blocks_still_springs_the_rest_from_where_they_were() {
        // Moving the paragraph between two lists to the end merges the lists: the second list's
        // id is gone, and the blocks that stay start from where they were drawn.
        let b = ids(4);
        let mut r = Reorder::default();
        let t = Instant::now();
        r.frame(slots(&b), None, t, false);
        let (list, para, gone, last) = (b[0], b[1], b[2], b[3]);
        let mut after = slots(&[list, last, para]);
        after[0].1.pitch = 80.;
        after[1].1.top = 80.;
        after[2].1.top = 120.;
        let (o, moving) = r.frame(after, None, t, false);
        assert!(moving);
        assert_eq!(
            o[&para],
            40. - 120.,
            "the paragraph starts where it was drawn"
        );
        assert_eq!(o[&last], 120. - 80., "so does the block it passed");
        assert!(!o.contains_key(&list) && !o.contains_key(&gone));
    }

    #[test]
    fn a_spring_of_a_block_that_left_is_dropped() {
        let b = ids(3);
        let mut r = Reorder::default();
        let t = Instant::now();
        r.frame(slots(&b), None, t, false);
        r.frame(slots(&[b[1], b[0], b[2]]), None, t, false);
        let (o, _) = r.frame(
            slots(&[b[1], b[2]]),
            None,
            t + Duration::from_millis(8),
            false,
        );
        assert!(!o.contains_key(&b[0]));
    }

    #[test]
    fn a_motion_after_idle_frames_starts_where_the_blocks_were() {
        let b = ids(2);
        let mut r = Reorder::default();
        let t = Instant::now();
        // Frames while nothing moves, as a caret blink draws them.
        r.frame(slots(&b), None, t, false);
        r.frame(slots(&b), None, t + Duration::from_millis(500), false);
        let after = slots(&[b[1], b[0]]);
        let (o, moving) = r.frame(after.clone(), None, t + Duration::from_millis(1000), false);
        assert!(moving);
        assert_eq!(o[&b[0]], -40., "no travel skipped on the first frame");
        let (o, _) = r.frame(after, None, t + Duration::from_millis(1008), false);
        let x = o[&b[0]];
        assert!(x > -40. && x < -30., "one 8 ms step: {x}");
    }

    #[test]
    fn edits_that_keep_the_order_dont_animate() {
        let b = ids(3);
        let mut r = Reorder::default();
        let t = Instant::now();
        r.frame(slots(&b), None, t, false);
        // Block 0 grew a line: the others move down in the layout, at once.
        let mut grown = slots(&b);
        grown[0].1.pitch = 64.;
        grown[1].1.top = 64.;
        grown[2].1.top = 104.;
        let (o, moving) = r.frame(grown, None, t, false);
        assert!(o.is_empty() && !moving);
    }
}
