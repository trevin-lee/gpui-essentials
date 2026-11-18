//! The calendar's drags on gpui-dnd. The calendar still decides what a press means
//! (`paint::target_at`) and works every proposal out itself, in wall minutes from its own
//! geometry; gpui-dnd runs the drag's life: when a press becomes a drag, the pointer and the
//! finger while it's on, Escape and a lost touch, the release (in the window or out of it), the
//! cursor, and the hours' auto-scroll on timed frames.
//!
//! A press that can become a drag is handed over with `press_at` (the mouse, or a finger on the
//! selected event) or begun with `begin_at` (a long press). The calendar's root is the drag's
//! target, catching it anywhere in the window, so a release over the header or outside the
//! calendar commits at the clamped spot as before.

use std::rc::{Rc, Weak};

use gpui::*;
use gpui_dnd::{
    AutoScroll, Backend, DragSource, DropOutcome, DropTarget, Preview, Scrollable, dnd,
};

use super::{Calendar, Drag};
use crate::paint::{self, Area, Geo, Shared, Target};
use crate::time;

/// What gpui-dnd carries for a calendar's drag: whose it is, and the cursor it shows. The drag
/// itself (what's moved, from where, to when) stays the calendar's own `Drag`.
#[derive(Clone, Debug)]
pub(crate) struct DraggedEvent {
    view: EntityId,
    cursor: CursorStyle,
}

impl Calendar {
    /// The press just made `Drag::Press`: if it can become a drag, gpui-dnd takes it from here
    /// and begins it once it moves `4 × scale` (`8 × scale` for a finger).
    pub(super) fn hand_over(
        &mut self,
        at: Point<Pixels>,
        backend: Backend,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(geo) = self.shared.geo.get() else {
            return;
        };
        let Some(Drag::Press { target, .. }) = &self.drag else {
            return;
        };
        let Some(cursor) = self.drag_cursor(target, &geo) else {
            return;
        };
        let slop = match backend {
            Backend::Touch => 8.,
            _ => 4.,
        };
        let source = self.drag_source(cursor, cx);
        let id = self.drag_id(cx);
        gpui_dnd::press_at(
            window,
            cx,
            id,
            at,
            source,
            gpui_dnd::Press {
                threshold: Some(px(slop * geo.s)),
                ..gpui_dnd::Press::new(backend)
            },
        );
    }

    /// The drag a long press made, for gpui-dnd to begin and follow: where, its source's id,
    /// and its source.
    pub(super) fn held(
        &mut self,
        at: Point<Pixels>,
        cursor: CursorStyle,
        cx: &mut Context<Self>,
    ) -> Held {
        Held {
            at,
            id: self.drag_id(cx),
            source: self.drag_source(cursor, cx),
        }
    }

    /// The cursor of the drag a press on `target` would become, or None if it can't become
    /// one: nothing in the agenda drags, a read-only event doesn't move, and empty time makes
    /// an event only where there's a calendar to put it in.
    pub(super) fn drag_cursor(&self, target: &Target, geo: &Geo) -> Option<CursorStyle> {
        if geo.kind == paint::Kind::Agenda {
            return None;
        }
        match target {
            Target::Event {
                id,
                area,
                resize,
                read_only,
            } => {
                if *read_only || self.editable(id).is_none() {
                    return None;
                }
                Some(match (resize, area) {
                    (false, _) => CursorStyle::ClosedHand,
                    (true, Area::Grid) => CursorStyle::ResizeUpDown,
                    (true, _) => CursorStyle::ResizeLeftRight,
                })
            }
            Target::Link { id, read_only, .. } => {
                (!read_only && self.editable(id).is_some()).then_some(CursorStyle::ClosedHand)
            }
            Target::Grid { .. } | Target::Day { .. } => self
                .new_event_calendar()
                .is_some()
                .then_some(CursorStyle::Arrow),
            _ => None,
        }
    }

    fn drag_id(&self, cx: &Context<Self>) -> ElementId {
        ElementId::NamedInteger("gpui-calendar-drag".into(), cx.entity_id().as_u64())
    }

    fn drag_source(&self, cursor: CursorStyle, cx: &mut Context<Self>) -> DragSource<DraggedEvent> {
        let (begin, end) = (cx.entity().downgrade(), cx.entity().downgrade());
        DragSource::item(DraggedEvent {
            view: cx.entity_id(),
            cursor,
        })
        // The calendar draws the ghost on its canvas, and the original faded.
        .preview(Preview::None)
        .keyboard(false)
        .cursor(|d, _| d.cursor)
        .on_begin(move |m, _, cx| {
            if let Some(at) = m.initial_client_offset() {
                begin.update(cx, |c, cx| c.begin_drag(at, cx)).ok();
            }
        })
        .on_end(move |_, _, cx| {
            // Dropped here, `finish` has run; cancelled, or dropped on someone else's target,
            // the drag is over for the calendar too.
            end.update(cx, |c, cx| c.drag_ended(cx)).ok();
        })
    }

    /// gpui-dnd began the drag: the press becomes the drag it is, from where it went down.
    fn begin_drag(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        if let Some(press @ Drag::Press { .. }) = self.drag.clone() {
            self.drag_to(press, at, true, cx);
        }
    }

    pub(super) fn drag_ended(&mut self, cx: &mut Context<Self>) {
        self.touch_from = None;
        self.shared.drag_scrolls.set(false);
        if self.drag.take().is_some() {
            cx.notify();
        }
    }

    /// The pointer of the calendar's drag is at `p`. A finger moves it only once it has moved
    /// off where it landed, so a long press's hour isn't pulled to the finger by a tremble.
    fn follow(&mut self, p: Point<Pixels>, backend: Option<Backend>, cx: &mut Context<Self>) {
        let Some(drag) = self.drag.clone() else {
            return;
        };
        if matches!(drag, Drag::Press { .. }) {
            return;
        }
        if backend == Some(Backend::Touch)
            && let Some(from) = self.touch_from
            && let Some(geo) = self.shared.geo.get()
        {
            self.touch_moved |= (p - from).magnitude() as f32 >= 8. * geo.s;
            if !self.touch_moved {
                return;
            }
        }
        self.drag_to(drag, p, false, cx);
    }

    /// The calendar's root as its drags' target, catching them anywhere in the window.
    pub(super) fn drop_target(&self, root: impl IntoElement, cx: &mut Context<Self>) -> Dnd {
        let view = cx.entity_id();
        let (hover, drop) = (cx.entity().downgrade(), cx.entity().downgrade());
        dnd(ElementId::NamedInteger(
            "gpui-calendar-target".into(),
            view.as_u64(),
        ))
        .size_full()
        .drop_indicator(false)
        .target(
            DropTarget::new()
                .accept::<DraggedEvent>()
                .catch_outside(true)
                // Its own drags only, so two calendars in a window keep theirs.
                .can_drop(move |item, _, _| {
                    item.get::<DraggedEvent>().is_some_and(|d| d.view == view)
                })
                .on_hover(move |_, m, _, cx| {
                    if let Some(at) = m.client_offset() {
                        let backend = m.backend();
                        hover.update(cx, |c, cx| c.follow(at, backend, cx)).ok();
                    }
                })
                .on_drop(move |_, m, _, cx| {
                    drop.update(cx, |c, cx| {
                        if let Some(at) = m.client_offset() {
                            c.follow(at, m.backend(), cx);
                        }
                        // A press that never became the calendar's drag drops nothing.
                        if matches!(c.drag, Some(Drag::Press { .. })) {
                            c.drag = None;
                        }
                        c.finish(cx);
                    })
                    .ok();
                    DropOutcome::Handled(None)
                }),
        )
        .child(root)
    }
}

use gpui_dnd::Dnd;

/// A long press's drag, begun once the calendar has made it (gpui-dnd calls back into the
/// calendar as it begins, so not from inside the calendar's update).
pub(crate) struct Held {
    at: Point<Pixels>,
    id: ElementId,
    source: DragSource<DraggedEvent>,
}

/// A long press on the calendar: the calendar makes its drag, then gpui-dnd begins it and
/// follows the finger. Whether it took the press.
pub(crate) fn long_press(
    view: &WeakEntity<Calendar>,
    event: &LongPressEvent,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let Some(Some(held)) = view.update(cx, |c, cx| c.long_press(event, cx)).ok() else {
        return false;
    };
    if gpui_dnd::begin_at(
        window,
        cx,
        held.id,
        held.at,
        held.source,
        gpui_dnd::Press::new(Backend::Touch),
    ) {
        return true;
    }
    view.update(cx, |c, cx| c.drag_ended(cx)).ok();
    false
}

/// The hours as gpui-dnd scrolls them: GPUI's offsets (negative down the day) over the
/// calendar's scroll in wall minutes.
pub(crate) struct Hours {
    pub shared: Weak<Shared>,
    pub view: WeakEntity<Calendar>,
    pub hour_h: Pixels,
    pub max_minutes: f32,
}

impl Scrollable for Hours {
    fn offset(&self) -> Point<Pixels> {
        let minute = self
            .shared
            .upgrade()
            .and_then(|s| s.scroll.get())
            .unwrap_or(0.);
        point(px(0.), -self.hour_h * (minute / 60.))
    }

    fn max_offset(&self) -> Point<Pixels> {
        point(px(0.), self.hour_h * (self.max_minutes / 60.))
    }

    fn set_offset(&self, offset: Point<Pixels>, _: &mut Window, cx: &mut App) {
        let Some(shared) = self.shared.upgrade() else {
            return;
        };
        let minute =
            (-f32::from(offset.y) / f32::from(self.hour_h) * 60.).clamp(0., self.max_minutes);
        shared.scroll.set(Some(minute));
        self.view.update(cx, |_, cx| cx.notify()).ok();
    }
}

/// Registers the hours' auto-scroll for this frame: near the grid's top or bottom, or past
/// them, a drag that creates, moves over the hours or resizes in them scrolls the day.
pub(crate) fn auto_scroll(
    geo: &Geo,
    shared: &Rc<Shared>,
    view: WeakEntity<Calendar>,
    window: &mut Window,
    cx: &mut App,
) {
    if geo.kind != paint::Kind::Grid || geo.grid_bottom <= geo.grid_top {
        return;
    }
    let Some(entity) = view.upgrade() else {
        return;
    };
    let id = entity.entity_id();
    let band = Bounds::from_corners(
        point(geo.bounds.left(), geo.grid_top),
        point(geo.bounds.right(), geo.grid_bottom),
    );
    let rem = f32::from(window.rem_size());
    let scrolls = shared.clone();
    let mut options = AutoScroll::vertical();
    // The calendar's 18 points from the edge, at its scale.
    options.edge = rems(18. * geo.s / rem);
    // About four hours a second at full speed.
    options.speed = f32::from(geo.hour_h) * 4. / rem;
    let options = options.beyond_edges(true).for_items(move |item| {
        item.get::<DraggedEvent>().is_some_and(|d| d.view == id) && scrolls.drag_scrolls.get()
    });
    gpui_dnd::auto_scroll_zone(
        window,
        cx,
        band,
        Rc::new(Hours {
            shared: Rc::downgrade(shared),
            view,
            hour_h: geo.hour_h,
            max_minutes: (time::DAY_MINUTES - geo.visible_minutes()).max(0.),
        }),
        options,
    );
}
