//! Auto-scroll: while a drag's pointer is near the edge of a scroll container, the container
//! scrolls, faster the nearer, on animation frames, so it goes on while the pointer holds still.
//! It stops asking for frames as soon as nothing under the pointer can scroll.

use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, IntoElement,
    LayoutId, Pixels, Point, Rems, ScrollHandle, UniformListScrollHandle, Window, point, px, rems,
};

use crate::source::Backend;
use crate::state;

/// Something that scrolls: GPUI's [`ScrollHandle`] and [`UniformListScrollHandle`], or your own
/// (a canvas that pans). Offsets are GPUI's: zero at the start, negative as the content moves
/// up or left.
pub trait Scrollable: 'static {
    fn offset(&self) -> Point<Pixels>;
    /// How far it can scroll each way, as positive lengths.
    fn max_offset(&self) -> Point<Pixels>;
    fn set_offset(&self, offset: Point<Pixels>, window: &mut Window, cx: &mut App);
}

impl Scrollable for ScrollHandle {
    fn offset(&self) -> Point<Pixels> {
        ScrollHandle::offset(self)
    }

    fn max_offset(&self) -> Point<Pixels> {
        ScrollHandle::max_offset(self)
    }

    fn set_offset(&self, offset: Point<Pixels>, _: &mut Window, _: &mut App) {
        ScrollHandle::set_offset(self, offset)
    }
}

impl Scrollable for UniformListScrollHandle {
    fn offset(&self) -> Point<Pixels> {
        self.0.borrow().base_handle.offset()
    }

    fn max_offset(&self) -> Point<Pixels> {
        self.0.borrow().base_handle.max_offset()
    }

    fn set_offset(&self, offset: Point<Pixels>, _: &mut Window, _: &mut App) {
        self.0.borrow().base_handle.set_offset(offset)
    }
}

/// How a container scrolls while a drag is near its edge.
#[derive(Clone, Copy, Debug)]
pub struct AutoScroll {
    /// How near its edge the pointer has to be.
    pub edge: Rems,
    /// How fast it scrolls with the pointer at the edge itself, in rems per second. It eases
    /// in from nothing at `edge` away.
    pub speed: f32,
    pub vertical: bool,
    pub horizontal: bool,
}

impl Default for AutoScroll {
    fn default() -> Self {
        Self {
            edge: rems(3.),
            speed: 75.,
            vertical: true,
            horizontal: true,
        }
    }
}

impl AutoScroll {
    pub fn vertical() -> Self {
        Self {
            horizontal: false,
            ..Self::default()
        }
    }

    pub fn horizontal() -> Self {
        Self {
            vertical: false,
            ..Self::default()
        }
    }
}

/// [`auto_scroll`](AutoScrollExt::auto_scroll) on any element.
pub trait AutoScrollExt: IntoElement + Sized {
    /// Scrolls `scroller` while a drag's pointer is near this element's edges. Call it last on
    /// the scroll container, with the handle it tracks:
    ///
    /// ```ignore
    /// div().id("list").overflow_y_scroll().track_scroll(&handle).children(rows)
    ///     .auto_scroll(&handle, AutoScroll::vertical())
    /// ```
    ///
    /// Nested containers each scroll on their own: the innermost one that can scroll that way
    /// goes first. Keyboard drags don't auto-scroll.
    fn auto_scroll(
        self,
        scroller: &(impl Scrollable + Clone),
        options: AutoScroll,
    ) -> AutoScrollArea {
        AutoScrollArea {
            child: Some(self.into_any_element()),
            scroller: Rc::new(scroller.clone()),
            options,
        }
    }
}

impl<E: IntoElement> AutoScrollExt for E {}

/// See [`AutoScrollExt::auto_scroll`].
pub struct AutoScrollArea {
    child: Option<AnyElement>,
    scroller: Rc<dyn Scrollable>,
    options: AutoScroll,
}

pub(crate) struct Zone {
    visible: Bounds<Pixels>,
    scroller: Rc<dyn Scrollable>,
    options: AutoScroll,
    rem: Pixels,
    seq: u64,
    pub(crate) stamp: u64,
}

impl IntoElement for AutoScrollArea {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for AutoScrollArea {
    type RequestLayoutState = AnyElement;
    type PrepaintState = ();

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
    ) -> (LayoutId, AnyElement) {
        let mut child = self.child.take().expect("laid out once");
        (child.request_layout(window, cx), child)
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        child: &mut AnyElement,
        window: &mut Window,
        cx: &mut App,
    ) {
        let shared = state::shared(cx);
        {
            let mut state = shared.borrow_mut();
            // Gathered afresh each frame: a zone keeps its frame's stamp, and the active
            // provider drops the older ones as it paints.
            let window_id = state::window_id(window);
            if state::has_provider(&state, window_id) {
                let seq = state.next_seq();
                let zone = Zone {
                    visible: bounds.intersect(&window.content_mask().bounds),
                    scroller: self.scroller.clone(),
                    options: self.options,
                    rem: window.rem_size(),
                    seq,
                    stamp: state.zone_stamp,
                };
                state.zones.entry(window_id).or_default().push(zone);
            }
        }
        child.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        child: &mut AnyElement,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        child.paint(window, cx);
    }
}

/// A run of auto-scrolling: when its last step was.
pub(crate) struct Run {
    last: Instant,
}

/// The longest step taken at once, so a late frame doesn't jump.
const MAX_STEP: Duration = Duration::from_millis(50);

/// Starts scrolling if the drag's pointer is at an edge that can scroll.
pub(crate) fn kick(window: &mut Window, cx: &mut App) {
    let now = cx.background_executor().now();
    let Some(at) = pointer(window, cx) else {
        return;
    };
    let shared = state::shared(cx);
    {
        let mut state = shared.borrow_mut();
        if state.auto_run.is_some() {
            return;
        }
        let zones = state.zones.get(&state::window_id(window));
        if zones.is_none_or(|z| velocities(z, at).is_empty()) {
            return;
        }
        state.auto_run = Some(Run { last: now });
    }
    window.on_next_frame(tick);
}

/// The drag's pointer, if it's a drag that auto-scrolls in this window.
fn pointer(window: &Window, cx: &App) -> Option<Point<Pixels>> {
    if state::drag_in(window, cx) != Some(false) {
        return None;
    }
    let snap = state::snapshot(cx)?;
    (snap.backend != Backend::Keyboard).then_some(snap.current)
}

fn tick(window: &mut Window, cx: &mut App) {
    let now = cx.background_executor().now();
    let shared = state::shared(cx);
    let Some(run) = shared.borrow_mut().auto_run.take() else {
        return;
    };
    let Some(at) = pointer(window, cx) else {
        return;
    };
    let dt = now
        .saturating_duration_since(run.last)
        .min(MAX_STEP)
        .as_secs_f32();
    let steps = {
        let state = shared.borrow();
        match state.zones.get(&state::window_id(window)) {
            Some(zones) => velocities(zones, at),
            None => Vec::new(),
        }
    };
    if steps.is_empty() {
        return;
    }
    for (scroller, velocity) in steps {
        let offset = scroller.offset();
        let max = scroller.max_offset();
        let next = point(
            (offset.x - velocity.x * dt).clamp(-max.x, px(0.)),
            (offset.y - velocity.y * dt).clamp(-max.y, px(0.)),
        );
        scroller.set_offset(next, window, cx);
    }
    shared.borrow_mut().auto_run = Some(Run { last: now });
    window.on_next_frame(tick);
    // The content moved under the pointer: hover what's there now.
    state::move_to(at, window.modifiers(), window, cx);
}

/// For each axis, the innermost zone under `at` near whose edge it is and that can still scroll
/// that way, with its velocity in pixels per second (positive: towards the content's end).
fn velocities(zones: &[Zone], at: Point<Pixels>) -> Vec<(Rc<dyn Scrollable>, Point<Pixels>)> {
    let mut under: Vec<&Zone> = zones.iter().filter(|z| z.visible.contains(&at)).collect();
    under.sort_by_key(|z| std::cmp::Reverse(z.seq));
    let mut found: Vec<(Rc<dyn Scrollable>, Point<Pixels>)> = Vec::new();
    let (mut x_done, mut y_done) = (false, false);
    for zone in under {
        let edge = zone.options.edge.to_pixels(zone.rem);
        let speed = zone.options.speed * f32::from(zone.rem);
        let offset = zone.scroller.offset();
        let max = zone.scroller.max_offset();
        let b = zone.visible;
        let mut v = Point::<Pixels>::default();
        if zone.options.vertical && !y_done {
            v.y = pull(
                at.y - b.top(),
                b.bottom() - at.y,
                edge,
                speed,
                offset.y,
                max.y,
            );
            y_done = v.y != px(0.);
        }
        if zone.options.horizontal && !x_done {
            v.x = pull(
                at.x - b.left(),
                b.right() - at.x,
                edge,
                speed,
                offset.x,
                max.x,
            );
            x_done = v.x != px(0.);
        }
        if v != Point::default() {
            found.push((zone.scroller.clone(), v));
        }
        if x_done && y_done {
            break;
        }
    }
    found
}

/// The velocity along one axis: towards the start when `to_start` is within `edge`, towards
/// the end when `to_end` is, if it can still scroll that way; eased in quadratically.
fn pull(
    to_start: Pixels,
    to_end: Pixels,
    edge: Pixels,
    speed: f32,
    offset: Pixels,
    max: Pixels,
) -> Pixels {
    let closeness = |d: Pixels| {
        let t = 1. - f32::from(d) / f32::from(edge);
        t.clamp(0., 1.).powi(2)
    };
    if to_end < edge && offset > -max {
        px(speed * closeness(to_end))
    } else if to_start < edge && offset < px(0.) {
        px(-speed * closeness(to_start))
    } else {
        px(0.)
    }
}
