//! The touch backend, through GPUI's gesture recognizer. A source picks up on a long press by
//! default, so a touch that moves at once still scrolls the list it's in; a handle can pick up
//! as soon as the finger moves ([`TouchActivation::Immediate`](crate::TouchActivation)). The
//! preview and the targets follow the touch's own position: GPUI leaves its mouse position
//! where a touch drag started.

use std::rc::Rc;

use gpui::{
    App, DispatchPhase, LongPressEvent, Pixels, Point, TouchDragEvent, TouchPhase, Window, px,
};

use crate::source::{Backend, TouchActivation};
use crate::state::{self, Pending, SourceRecord};

/// How far a finger moves before an at-once source's touch becomes a drag: GPUI's touch slop.
const TOUCH_SLOP: Pixels = px(8.);

fn enabled(window: &Window, cx: &mut App) -> bool {
    state::shared(cx)
        .borrow()
        .touch
        .get(&state::window_id(window))
        .copied()
        .unwrap_or(false)
}

fn touch_drag_on(window: &Window, cx: &App) -> bool {
    state::drag_in(window, cx) == Some(false)
        && state::snapshot(cx).is_some_and(|s| s.backend == Backend::Touch)
}

/// A long press began on `source`. True if it picked it up (the press is then claimed).
pub(crate) fn long_press(
    source: &Rc<SourceRecord>,
    event: &LongPressEvent,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    if source.spec.touch != TouchActivation::LongPress
        || !enabled(window, cx)
        || state::drag_in(window, cx).is_some()
    {
        return false;
    }
    if !state::begin(source, event.start_position, Backend::Touch, window, cx) {
        return false;
    }
    if event.position != event.start_position {
        state::move_to(event.position, window.modifiers(), window, cx);
    }
    true
}

/// GPUI offers a touch drag as a finger goes down on `source`. True if it takes it: an at-once
/// source does, and picks up once the finger moves past the slop.
pub(crate) fn offer(
    source: &Rc<SourceRecord>,
    event: &TouchDragEvent,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    if source.spec.touch != TouchActivation::Immediate
        || !enabled(window, cx)
        || state::drag_in(window, cx).is_some()
    {
        return false;
    }
    let shared = state::shared(cx);
    let mut state = shared.borrow_mut();
    if state.touch_pending.is_some() {
        return false;
    }
    state.touch_pending = Some(Pending {
        source: source.clone(),
        at: event.start_position,
        threshold: TOUCH_SLOP,
        window: state::window_id(window),
    });
    true
}

/// Follows the touches of a drag in this window.
pub(crate) fn listen(window: &mut Window) {
    window.on_mouse_event(|event: &LongPressEvent, phase, window, cx| {
        // A relative drag follows its finger's touch drag only, by how far it moves.
        if phase != DispatchPhase::Capture || !touch_drag_on(window, cx) || relative(cx).is_some() {
            return;
        }
        follow(event.phase, event.position, window, cx);
    });
    window.on_mouse_event(|event: &TouchDragEvent, phase, window, cx| {
        if phase != DispatchPhase::Capture {
            return;
        }
        if touch_drag_on(window, cx)
            && let Some(offset) = relative(cx)
        {
            follow_relative(event, offset, window, cx);
            return;
        }
        if event.phase == TouchPhase::Started {
            // A new touch forgets an old one, before the sources under it are offered it.
            state::shared(cx).borrow_mut().touch_pending = None;
            return;
        }
        if touch_drag_on(window, cx) {
            follow(event.phase, event.position, window, cx);
            return;
        }
        let shared = state::shared(cx);
        let start = {
            let mut state = shared.borrow_mut();
            let window_id = state::window_id(window);
            let Some(pending) = state
                .touch_pending
                .as_ref()
                .filter(|p| p.window == window_id)
            else {
                return;
            };
            if event.phase != TouchPhase::Moved {
                state.touch_pending = None;
                return;
            }
            if distance(event.position, pending.at) <= pending.threshold {
                return;
            }
            state.touch_pending.take()
        };
        if let Some(pending) = start
            && state::begin(&pending.source, pending.at, Backend::Touch, window, cx)
        {
            state::move_to(event.position, window.modifiers(), window, cx);
        }
    });
}

/// A relative drag's offset from its finger to its pointer: `Some(None)` before the finger
/// lands.
fn relative(cx: &App) -> Option<Option<Point<Pixels>>> {
    state::relative(cx)
}

/// A finger carrying a relative drag ([`Press::relative`](crate::Press::relative)): claimed as
/// it lands, the drag's pointer then moves as far as it does, and its lift drops the drag where
/// that leaves it (where it is, for a tap).
fn follow_relative(
    event: &TouchDragEvent,
    offset: Option<Point<Pixels>>,
    window: &mut Window,
    cx: &mut App,
) {
    match (event.phase, offset) {
        (TouchPhase::Started, _) => {
            let Some(at) = state::snapshot(cx).map(|s| s.current) else {
                return;
            };
            state::set_relative(Some(at - event.position), cx);
            window.prevent_default();
            cx.stop_propagation();
        }
        (TouchPhase::Moved, Some(offset)) => {
            state::move_to(event.position + offset, window.modifiers(), window, cx);
            cx.stop_propagation();
        }
        (TouchPhase::Ended, Some(offset)) => {
            state::release(event.position + offset, window, cx);
            cx.stop_propagation();
        }
        (TouchPhase::Cancelled, Some(_)) => state::cancel(window, cx),
        _ => {}
    }
}

fn follow(phase: TouchPhase, at: Point<Pixels>, window: &mut Window, cx: &mut App) {
    match phase {
        TouchPhase::Started => {}
        TouchPhase::Moved => state::move_to(at, window.modifiers(), window, cx),
        TouchPhase::Ended => {
            state::release(at, window, cx);
        }
        TouchPhase::Cancelled => state::cancel(window, cx),
    }
}

fn distance(a: Point<Pixels>, b: Point<Pixels>) -> Pixels {
    let d = a - b;
    px(f32::from(d.x).hypot(f32::from(d.y)))
}
