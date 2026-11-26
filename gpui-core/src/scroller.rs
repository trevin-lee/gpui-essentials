//! Scrolling boxes that take a wheel, a trackpad and a finger the way a browser's do.
//!
//! GPUI's own scrolling box (`div().overflow_x_scroll()`) hands every scroll to every scrolling
//! box under the pointer, from the innermost out, and a box that scrolls one way only takes the
//! other way's scroll as its own. A row that scrolls sideways inside a list that scrolls down
//! (a wide table in a chat) so slides sideways while the list scrolls down, and a trackpad's
//! swipe moves both a little at once.
//!
//! A [`Scroller`] takes a scroll as Safari and Chrome do:
//!
//! - it takes only the scroll along a way it scrolls: an up-and-down scroll over a box that
//!   scrolls sideways goes on, whole, to the box around it;
//! - a trackpad's or a finger's gesture keeps to the way it started in (whichever way it moved
//!   more) until it and its glide end, so a swipe down never moves anything sideways;
//! - at its end the rest goes to the box around it, the same way only;
//! - a mouse's wheel scrolls up and down, and sideways with Shift held.
//!
//! Every [`Scroller`] in the app shares the gesture, so a box that scrolls into place under
//! the pointer midway through a swipe goes the way the swipe started in.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;
// GPUI's clock: std's Instant, but in a browser (which has none) web-time's.
#[cfg(not(target_family = "wasm"))]
use std::time::Instant;
#[cfg(target_family = "wasm")]
use web_time::Instant;

use gpui::{
    AnyElement, App, Bounds, DispatchPhase, Div, Element, ElementId, Global, GlobalElementId,
    Hitbox, HitboxBehavior, InspectorElementId, InteractiveElement, Interactivity, IntoElement,
    IsZero as _, LayoutId, ParentElement, Pixels, Point, ScrollEvent, ScrollHandle, Stateful,
    StatefulInteractiveElement, StyleRefinement, Styled, TouchPhase, Window, point, px,
};

/// Which ways a [`Scroller`] or a [`ScrollArea`](crate::ScrollArea) scrolls.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScrollAxes {
    #[default]
    Vertical,
    Horizontal,
    Both,
}

impl ScrollAxes {
    /// Whether it scrolls up and down.
    pub fn vertical(self) -> bool {
        self != Self::Horizontal
    }

    /// Whether it scrolls sideways.
    pub fn horizontal(self) -> bool {
        self != Self::Vertical
    }
}

/// How long a pause between a trackpad's scrolls ends its gesture, where the platform doesn't
/// say when one starts (macOS and iOS do; their glide after the fingers lift comes within a
/// frame of it).
const GAP: Duration = Duration::from_millis(150);

/// The way a gesture keeps to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Way {
    Across,
    Down,
}

/// The way `d` moves more, if it moves.
fn more(d: Point<Pixels>) -> Option<Way> {
    if d.x.is_zero() && d.y.is_zero() {
        None
    } else if d.x.abs() > d.y.abs() {
        Some(Way::Across)
    } else {
        Some(Way::Down)
    }
}

/// The trackpad's or finger's gesture now, for every scroller in the app.
#[derive(Default)]
struct Gesture {
    way: Option<Way>,
    last: Option<Instant>,
}

impl Global for Gesture {}

impl Gesture {
    /// Takes one scroll of a gesture and says which way the gesture goes. The same scroll
    /// taken again (each scroller takes it) changes nothing.
    fn take(&mut self, phase: TouchPhase, d: Point<Pixels>, now: Instant) -> Option<Way> {
        let paused = self
            .last
            .is_none_or(|last| now.saturating_duration_since(last) >= GAP);
        match phase {
            TouchPhase::Started => self.way = more(d),
            TouchPhase::Cancelled => self.way = None,
            // The fingers lifting keeps the way: the glide after goes on with it, and the
            // next gesture says it starts.
            TouchPhase::Moved | TouchPhase::Ended => {
                if paused || self.way.is_none() {
                    self.way = more(d);
                }
            }
        }
        self.last = Some(now);
        self.way
    }
}

/// The scroll `event` asks of a scroller, in pixels (a wheel's lines `line_height` each,
/// `window.line_height()` unless the scroller has its own) and along one way only: a trackpad's or a
/// finger's (precise) along the way its gesture started in, kept until the gesture and its
/// glide end; a mouse wheel's (in lines) along the way it turned, sideways with Shift. Positive
/// values move the content right and down, as GPUI's deltas do, so a scroller's position goes
/// down by them.
///
/// [`Scroller`] uses it; a host's own scrolling (a canvas, a list GPUI doesn't scroll) can too,
/// with [`cx.stop_propagation()`](App::stop_propagation) when it moved, so the boxes around it
/// keep still:
///
/// ```no_run
/// # use gpui::*;
/// # fn f(el: Stateful<Div>, scroll_x: std::rc::Rc<std::cell::Cell<Pixels>>, max: Pixels) -> Stateful<Div> {
/// el.on_scroll(move |e, window, cx| {
///     let d = gpui_core::scroll_delta(e, window.line_height(), cx);
///     let to = (scroll_x.get() - d.x).clamp(px(0.), max);
///     if to != scroll_x.get() {
///         scroll_x.set(to);
///         cx.stop_propagation();
///     }
/// })
/// # }
/// ```
pub fn scroll_delta(event: &ScrollEvent, line_height: Pixels, cx: &mut App) -> Point<Pixels> {
    let mut d = event.delta.pixel_delta(line_height);
    let way = if event.delta.precise() {
        let now = cx.background_executor().now();
        cx.default_global::<Gesture>()
            .take(event.touch_phase, d, now)
    } else {
        // A wheel with Shift scrolls sideways (macOS has turned it already).
        if event.modifiers.shift && d.x.is_zero() {
            d = point(d.y, px(0.));
        }
        more(d)
    };
    match way {
        Some(Way::Across) => point(d.x, px(0.)),
        Some(Way::Down) => point(px(0.), d.y),
        None => Point::default(),
    }
}

/// Where `d` takes a scroller along `axes` from where `handle` has it, or none when it can't
/// move that way (it doesn't scroll that way, or it's at its end).
fn moved(handle: &ScrollHandle, axes: ScrollAxes, d: Point<Pixels>) -> Option<Point<Pixels>> {
    let max = handle.max_offset();
    let at = (-handle.offset()).max(&Point::default()).min(&max);
    let mut to = at;
    if axes.horizontal() && !d.x.is_zero() {
        to.x = (at.x - d.x).clamp(px(0.), max.x);
    }
    if axes.vertical() && !d.y.is_zero() {
        to.y = (at.y - d.y).clamp(px(0.), max.y);
    }
    (to != at).then_some(to)
}

/// Makes a `div` with an id a [`Scroller`]: content that scrolls along `axes` and takes a
/// wheel, a trackpad and a finger as a browser's scrolling box does. Use it where GPUI has
/// `overflow_x_scroll`, `overflow_y_scroll` and `overflow_scroll`:
///
/// ```no_run
/// # use gpui::*;
/// use gpui_core::ScrollExt as _;
/// # fn row(cells: Vec<AnyElement>) -> impl IntoElement {
/// // was: div().id("row").flex().overflow_x_scroll().children(cells)
/// div().id("row").flex().children(cells).scroll_x()
/// # }
/// ```
///
/// A [`ScrollHandle`] to read or set its scroll goes on the scroller with
/// [`Scroller::track_scroll`], after `scroll_x()`, not on the `div` before it; without one it
/// keeps its own.
pub trait ScrollExt: Sized {
    /// Scrolls along `axes`.
    fn scroll_axes(self, axes: ScrollAxes) -> Scroller;

    /// Scrolls sideways only: an up-and-down scroll over it goes to the box around it.
    fn scroll_x(self) -> Scroller {
        self.scroll_axes(ScrollAxes::Horizontal)
    }

    /// Scrolls up and down only: a sideways scroll over it goes to the box around it.
    fn scroll_y(self) -> Scroller {
        self.scroll_axes(ScrollAxes::Vertical)
    }

    /// Scrolls both ways (a gesture still keeps to the way it started in).
    fn scroll_both(self) -> Scroller {
        self.scroll_axes(ScrollAxes::Both)
    }
}

impl ScrollExt for Stateful<Div> {
    fn scroll_axes(self, axes: ScrollAxes) -> Scroller {
        Scroller {
            div: Some(self),
            axes,
            handle: None,
        }
    }
}

/// A `div` whose content scrolls and that takes a wheel, a trackpad and a finger as a
/// browser's scrolling box does (see [`ScrollExt`]). It styles, takes children and listens
/// as the `div` does.
pub struct Scroller {
    div: Option<Stateful<Div>>,
    axes: ScrollAxes,
    handle: Option<ScrollHandle>,
}

impl Scroller {
    /// Reads and sets its scroll through `handle` (GPUI's: its offset is the negated
    /// position, and its `scroll_to_item` and `max_offset` work as on a `div`).
    pub fn track_scroll(mut self, handle: &ScrollHandle) -> Self {
        self.handle = Some(handle.clone());
        self
    }

    fn div(&mut self) -> &mut Stateful<Div> {
        self.div
            .as_mut()
            .expect("a scroller is built before it's laid out")
    }
}

impl Styled for Scroller {
    fn style(&mut self) -> &mut StyleRefinement {
        self.div().style()
    }
}

impl InteractiveElement for Scroller {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.div().interactivity()
    }
}

impl StatefulInteractiveElement for Scroller {}

impl ParentElement for Scroller {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.div().extend(elements)
    }
}

impl IntoElement for Scroller {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

/// A scroll as a scroller heard it on the way in: its offset then, and its part.
type Seen = (Point<Pixels>, Point<Pixels>);

#[doc(hidden)]
pub struct Laid {
    child: AnyElement,
    handle: ScrollHandle,
}

impl Element for Scroller {
    type RequestLayoutState = Laid;
    type PrepaintState = Hitbox;

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
    ) -> (LayoutId, Laid) {
        let div = self.div.take().expect("laid out once");
        // Its own handle, kept beside the div's state under the div's id.
        let handle = self.handle.clone().unwrap_or_else(|| {
            let id = Element::id(&div).expect("a stateful div has an id");
            window.with_global_id(id, |id, window| {
                window.with_element_state(id, |handle: Option<ScrollHandle>, _| {
                    let handle = handle.unwrap_or_default();
                    (handle.clone(), handle)
                })
            })
        });
        // The div lays out, clips and offsets its content as GPUI's scrolling box does, and
        // keeps the handle's sizes; this takes the scrolls.
        let div = match self.axes {
            ScrollAxes::Horizontal => div.overflow_x_scroll(),
            ScrollAxes::Vertical => div.overflow_y_scroll(),
            ScrollAxes::Both => div.overflow_scroll(),
        }
        .track_scroll(&handle);
        let mut child = div.into_any_element();
        (child.request_layout(window, cx), Laid { child, handle })
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        laid: &mut Laid,
        window: &mut Window,
        cx: &mut App,
    ) -> Hitbox {
        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        laid.child.prepaint(window, cx);
        hitbox
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        laid: &mut Laid,
        hitbox: &mut Hitbox,
        window: &mut Window,
        cx: &mut App,
    ) {
        let handle = laid.handle.clone();
        let hitbox = hitbox.clone();
        let axes = self.axes;
        let view = window.current_view();
        let line_height = window.line_height();
        // The scroll as it was before the div's own listener had it, and this scroll's part
        // for a scroller.
        let seen: Rc<Cell<Option<Seen>>> = Rc::default();
        // Registered before the div's, so it hears a scroll before everything inside on the
        // way in, and after them (the boxes inside it, then the div's own GPUI listener) on
        // the way out.
        window.on_mouse_event(move |e: &ScrollEvent, phase, window, cx| match phase {
            DispatchPhase::Capture => {
                let d = scroll_delta(e, line_height, cx);
                seen.set(Some((handle.offset(), d)));
            }
            DispatchPhase::Bubble => {
                let Some((before, d)) = seen.take() else {
                    return;
                };
                // Undo GPUI's take on it (which moves a one-way box with the other way's
                // scroll, and each box under the pointer at once).
                if handle.offset() != before {
                    handle.set_offset(before);
                }
                if !hitbox.should_handle_scroll(window) {
                    return;
                }
                // Moved: the boxes around keep still. At its end (or the other way): on to
                // them, whole.
                if let Some(to) = moved(&handle, axes, d) {
                    handle.set_offset(-to);
                    cx.notify(view);
                    cx.stop_propagation();
                }
            }
        });
        laid.child.paint(window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f32, y: f32) -> Point<Pixels> {
        point(px(x), px(y))
    }

    #[test]
    fn a_gesture_keeps_the_way_it_started_through_its_glide() {
        let mut g = Gesture::default();
        let t = Instant::now();
        assert_eq!(g.take(TouchPhase::Started, p(2., -10.), t), Some(Way::Down));
        // Sideways noise larger than the scroll down, midway, doesn't turn it.
        assert_eq!(
            g.take(TouchPhase::Moved, p(9., -1.), t + Duration::from_millis(16)),
            Some(Way::Down)
        );
        assert_eq!(
            g.take(TouchPhase::Ended, p(0., 0.), t + Duration::from_millis(32)),
            Some(Way::Down)
        );
        // The glide after the fingers lift (macOS: no phase, so Moved).
        assert_eq!(
            g.take(TouchPhase::Moved, p(6., -2.), t + Duration::from_millis(48)),
            Some(Way::Down)
        );
        // The next gesture says it starts.
        assert_eq!(
            g.take(
                TouchPhase::Started,
                p(-8., 1.),
                t + Duration::from_millis(64)
            ),
            Some(Way::Across)
        );
    }

    #[test]
    fn without_phases_a_pause_starts_a_gesture() {
        let mut g = Gesture::default();
        let t = Instant::now();
        assert_eq!(g.take(TouchPhase::Moved, p(0., 5.), t), Some(Way::Down));
        assert_eq!(
            g.take(TouchPhase::Moved, p(5., 0.), t + Duration::from_millis(20)),
            Some(Way::Down)
        );
        assert_eq!(
            g.take(
                TouchPhase::Moved,
                p(5., 0.),
                t + GAP + Duration::from_millis(20)
            ),
            Some(Way::Across)
        );
    }

    #[test]
    fn taking_a_scroll_twice_changes_nothing() {
        let mut g = Gesture::default();
        let t = Instant::now();
        for _ in 0..2 {
            assert_eq!(
                g.take(TouchPhase::Started, p(-8., 1.), t),
                Some(Way::Across)
            );
        }
        for _ in 0..2 {
            assert_eq!(g.take(TouchPhase::Moved, p(0., 9.), t), Some(Way::Across));
        }
    }
}
