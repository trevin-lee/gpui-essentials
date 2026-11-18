//! Drags a host starts itself: a press on something gpui-dnd doesn't know is a source (an
//! event drawn on a calendar's canvas, a row of a list it draws), recognised by the host and
//! handed over. From there the drag is gpui-dnd's like any other: the threshold, the targets,
//! the keys, the touch, the end.

use std::rc::Rc;

use gpui::{
    App, Bounds, CursorStyle, ElementId, Pixels, Point, SharedString, Size, Window, px, rems,
};

use crate::monitor::DragMonitor;
use crate::source::{Backend, DragSource};
use crate::state::{self, Pending, SourceRecord, TargetRecord};
use crate::target::DropTarget;

/// How a host-started drag was pressed: see [`press_at`].
#[derive(Clone, Debug)]
pub struct Press {
    /// What pressed: the mouse, a finger (whose touch the host claimed), or a key.
    pub backend: Backend,
    /// How far the pointer moves before the press is a drag: a quarter of a rem for the mouse
    /// and 8 points for a finger unless given.
    pub threshold: Option<Pixels>,
    /// Where in the item it was taken, from the item's top-left: the anchor of the preview and
    /// of [`source_client_offset`](DragMonitor::source_client_offset). Zero for a create.
    pub grab: Point<Pixels>,
    /// The item's size, for the preview and the monitor's
    /// [`source_size`](DragMonitor::source_size).
    pub size: Size<Pixels>,
    /// The item's name, for the keyboard's announcements.
    pub label: Option<SharedString>,
    /// A touch drag begun with [`begin_at`] that follows the next finger by how far it moves,
    /// not where it is: something lifted in place (a block sheet's Move), then carried by a
    /// finger landing anywhere. The offset from that finger to the drag's pointer is taken as
    /// it lands; a tap drops it where it is.
    pub relative: bool,
}

impl Default for Press {
    fn default() -> Self {
        Self {
            backend: Backend::Mouse,
            threshold: None,
            grab: Point::default(),
            size: Size::default(),
            label: None,
            relative: false,
        }
    }
}

impl Press {
    pub fn new(backend: Backend) -> Self {
        Self {
            backend,
            ..Self::default()
        }
    }
}

/// A press the host recognised as the start of a drag: it becomes one once the pointer moves
/// [`threshold`](Press::threshold) away, as a source's press does, and otherwise is forgotten
/// (the press, its click and its double click reach the host unchanged). Call it from the
/// press's own handler: a mouse down (the left button), or a touch drag the host claimed with
/// `prevent_default`. `id` names the drag's source, for
/// [`is_dragging_source`](DragMonitor::is_dragging_source). A [`Backend::Keyboard`] press
/// begins at once, as [`begin_at`] does.
///
/// The source is built here, once a press, not every frame. Ignored while a drag is on.
pub fn press_at<T: 'static>(
    window: &mut Window,
    cx: &mut App,
    id: impl Into<ElementId>,
    at: Point<Pixels>,
    source: DragSource<T>,
    press: Press,
) {
    if press.backend == Backend::Keyboard {
        begin_at(window, cx, id, at, source, press);
        return;
    }
    let threshold = press.threshold.unwrap_or_else(|| match press.backend {
        Backend::Touch => px(8.),
        _ => rems(0.25).to_pixels(window.rem_size()),
    });
    let record = record(id.into(), at, source, &press);
    let shared = state::shared(cx);
    let mut state = shared.borrow_mut();
    if state.drag.is_some() {
        return;
    }
    let pending = Some(Pending {
        source: record,
        at,
        threshold,
        window: state::window_id(window),
    });
    match press.backend {
        Backend::Touch => state.touch_pending = pending,
        _ => state.pending = pending,
    }
}

/// Begins a drag the host chose to start, at once: on a long press, or a key. With
/// [`Backend::Keyboard`], the keyboard holds the drag as for a source picked up with Space: the
/// arrows step through the target's [`keyboard_step`](crate::DropTarget::keyboard_step), Space
/// or Enter drops and Escape cancels. False if it didn't begin: a drag is on, the source can't
/// be dragged now or makes no item, or (for the keyboard) the provider's keyboard is off.
pub fn begin_at<T: 'static>(
    window: &mut Window,
    cx: &mut App,
    id: impl Into<ElementId>,
    at: Point<Pixels>,
    source: DragSource<T>,
    press: Press,
) -> bool {
    {
        let shared = state::shared(cx);
        let mut state = shared.borrow_mut();
        let window_id = state::window_id(window);
        let keyboard_off = !state.keyboard.get(&window_id).copied().unwrap_or(false);
        if state.drag.is_some() || (press.backend == Backend::Keyboard && keyboard_off) {
            return false;
        }
        state.pending = None;
        state.touch_pending = None;
    }
    let record = record(id.into(), at, source, &press);
    if press.backend == Backend::Keyboard {
        return crate::keyboard::start(record, at, window, cx);
    }
    let began = state::begin(&record, at, press.backend, window, cx);
    if began && press.relative && press.backend == Backend::Touch {
        let shared = state::shared(cx);
        if let Some(drag) = shared.borrow_mut().drag.as_mut() {
            drag.relative = Some(None);
        }
    }
    began
}

/// Ends the window's drag, or forgets its pending press, as Escape would: for a host whose own
/// key or question has to win.
pub fn cancel(window: &mut Window, cx: &mut App) {
    {
        let shared = state::shared(cx);
        let mut state = shared.borrow_mut();
        let window_id = state::window_id(window);
        if state
            .pending
            .as_ref()
            .is_some_and(|p| p.window == window_id)
        {
            state.pending = None;
        }
        if state
            .touch_pending
            .as_ref()
            .is_some_and(|p| p.window == window_id)
        {
            state.touch_pending = None;
        }
    }
    if state::drag_in(window, cx) == Some(false) {
        state::cancel(window, cx);
    }
}

/// The cursor the drag shows now: its source's [`cursor`](DragSource::cursor), or a closed
/// hand. None when no drag is on (or its preview is on its way back).
pub fn drag_cursor(cx: &App) -> Option<CursorStyle> {
    let monitor = DragMonitor::of(cx);
    let item = monitor.item()?;
    if monitor.is_returning() {
        return None;
    }
    let cursor = state::drag_cursor(cx);
    Some(
        cursor
            .and_then(|f| f(item, &monitor))
            .unwrap_or(CursorStyle::ClosedHand),
    )
}

/// The record a host-started drag holds in place of a drawn source's: no element, so it's in no
/// registry.
fn record<T: 'static>(
    id: ElementId,
    at: Point<Pixels>,
    source: DragSource<T>,
    press: &Press,
) -> Rc<SourceRecord> {
    Rc::new(SourceRecord {
        id,
        bounds: Bounds::new(at - press.grab, press.size),
        spec: Rc::new(source.erase()),
        key: None,
        label: press.label.clone(),
        hitbox: None,
        focus: None,
    })
}

/// A target an element registers from its own prepaint, for a view that draws itself in one
/// element and can't be wrapped in a [`dnd`](crate::dnd) div without changing its layout (an
/// editor whose padding is part of its own hitbox). `bounds` is where it takes drops this
/// frame, in window coordinates; it's cut by the content mask as a `dnd` target's is.
///
/// The target lives as long as the returned [`TargetHold`]: keep it until the element's next
/// prepaint replaces it, as a `dnd` element keeps its record in its element state. Register it
/// every frame the element is drawn.
pub fn register_target(
    window: &mut Window,
    cx: &mut App,
    id: impl Into<ElementId>,
    bounds: Bounds<Pixels>,
    target: DropTarget,
) -> TargetHold {
    let shared = state::shared(cx);
    let mut state = shared.borrow_mut();
    let parent = state.target_stack.last().map(Rc::downgrade);
    let record = Rc::new(TargetRecord {
        id: id.into(),
        bounds,
        visible: bounds.intersect(&window.content_mask().bounds),
        seq: state.next_seq(),
        parent,
        spec: Rc::new(target),
        label: None,
    });
    state.registry(state::window_id(window)).add_target(&record);
    TargetHold(record)
}

/// A target registered with [`register_target`]: it takes drops while this is kept.
pub struct TargetHold(#[allow(dead_code)] Rc<TargetRecord>);
