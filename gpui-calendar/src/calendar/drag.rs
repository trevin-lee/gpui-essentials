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

use chrono::{Datelike as _, Duration, NaiveDate};
use gpui::*;
use gpui_dnd::{
    AutoScroll, Backend, DragSource, DropOutcome, DropTarget, Preview, Scrollable, dnd,
};

use super::{Calendar, Drag};
use crate::model::EventTime;
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
    /// and begins it once it moves `slop × scale` (4 for the mouse, 8 for a finger).
    pub(super) fn hand_over(
        &mut self,
        at: Point<Pixels>,
        backend: Backend,
        slop: f32,
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
        // A keyboard drag moves by its steps (`key_step`), not by where its pointer is.
        if matches!(drag, Drag::Press { .. }) || backend == Some(Backend::Keyboard) {
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
        let (step, say) = (cx.entity().downgrade(), cx.entity().downgrade());
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
                .keyboard_step(move |_, m, dir, _, cx| {
                    step.update(cx, |c, cx| c.key_step(dir, cx))
                        .ok()
                        .flatten()
                        .or(m.client_offset())
                })
                .say(move |_, _, said, cx| {
                    say.upgrade()
                        .map(|c| c.read(cx).spoken(said))
                        .unwrap_or_default()
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

use gpui_dnd::{Dnd, Said};

impl Calendar {
    /// Space (Shift+Space for its end): the selected event picked up by the keyboard. The arrows
    /// then move it a snap step (up and down) or a day (left and right) on the grid, a day or a
    /// week in the month; Space or Enter puts it down as a mouse drop would (a repeating event's
    /// question after), Escape puts it back. Each step is said in the calendar's own words.
    pub(super) fn keyboard_drag(
        &mut self,
        resize: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.drag.is_some() || self.asking.is_some() {
            return;
        }
        let Some(geo) = self.shared.geo.get() else {
            return;
        };
        if geo.kind == paint::Kind::Agenda {
            return;
        }
        let Some(id) = self.selected_event().cloned() else {
            return;
        };
        // Only what a mouse could move: shown, and not read-only.
        let Some(e) = self.editable(&id) else {
            return;
        };
        let tz = self.options.time_zone;
        let original = e.time;
        let title = e.title.clone();
        let month = geo.kind == paint::Kind::Month;
        let (area, start, end) = match original {
            EventTime::Timed { start, end } if !month => {
                (Area::Grid, time::local(start, tz), time::local(end, tz))
            }
            EventTime::Timed { start, end } => (
                Area::Month,
                (time::local(start, tz).0, 0.),
                (time::local(end, tz).0, 0.),
            ),
            EventTime::AllDay { start, end } => (
                if month { Area::Month } else { Area::AllDay },
                (start, 0.),
                (end - Duration::days(1), 0.),
            ),
        };
        let drag = if resize {
            // A timed event's end moves on the grid; an all-day event's by days anywhere.
            if area == Area::Month && !original.is_all_day() {
                return;
            }
            Drag::Resize {
                id,
                original,
                area,
                now: end,
                shift: 0.,
            }
        } else {
            Drag::Move {
                id,
                original,
                area,
                over: area,
                grab: start,
                now: start,
            }
        };
        self.drag = Some(drag);
        // An all-day event on the grid is in the all-day row (minute -1).
        let row = |m: f32| if area == Area::AllDay { -1. } else { m };
        let at = self
            .point_at(start.0, row(start.1))
            .unwrap_or_else(|| geo.bounds.center());
        let held = self.held(at, CursorStyle::Arrow, cx);
        let view = cx.entity().downgrade();
        // gpui-dnd calls back into the calendar as the drag begins: not from inside its update.
        window.defer(cx, move |window, cx| {
            let press = gpui_dnd::Press {
                label: Some(if title.trim().is_empty() {
                    "Untitled event".into()
                } else {
                    title
                }),
                ..gpui_dnd::Press::new(Backend::Keyboard)
            };
            if !gpui_dnd::begin_at(window, cx, held.id, held.at, held.source, press) {
                view.update(cx, |c, cx| c.drag_ended(cx)).ok();
            }
        });
        cx.notify();
    }

    /// An arrow key during a keyboard drag: the event (or its end) a snap step or a day on the
    /// grid, a day or a week in the month, brought into view. Where it is now on screen, if it
    /// is.
    fn key_step(&mut self, dir: Point<f32>, cx: &mut Context<Self>) -> Option<Point<Pixels>> {
        let step = self.options.snap();
        let (dx, dy) = (dir.x.round() as i64, dir.y.round() as i64);
        let moved = |p: (NaiveDate, f32), area: Area| match area {
            Area::Grid => (
                p.0 + Duration::days(dx),
                (p.1 + dy as f32 * step).clamp(0., time::DAY_MINUTES),
            ),
            Area::AllDay => (p.0 + Duration::days(dx), p.1),
            Area::Month => (p.0 + Duration::days(dx + 7 * dy), p.1),
        };
        let (area, now) = match self.drag.as_mut()? {
            Drag::Move { area, now, .. } | Drag::Resize { area, now, .. } => {
                *now = moved(*now, *area);
                (*area, *now)
            }
            _ => return None,
        };
        let minute = if area == Area::Grid { now.1 } else { -1. };
        self.reveal(now.0, minute, cx);
        cx.notify();
        self.point_at(
            now.0,
            if area == Area::AllDay {
                -1.
            } else {
                now.1.max(0.)
            },
        )
    }

    /// What a keyboard drag says, in the calendar's formats (12- or 24-hour, the display zone,
    /// full day names, the day named when it changes): over the calendar, "Standup, Tuesday 9 to
    /// 9:30 AM, moving to Wednesday 10 AM" ("…, ending at 10 AM" for its end); dropped,
    /// "Standup, moved to Wednesday 10 AM" ("…, now ending at 10 AM").
    pub(crate) fn spoken(&self, said: Said) -> SharedString {
        let (id, original, resize) = match self.drag.as_ref() {
            Some(Drag::Move { id, original, .. }) => (id, *original, false),
            Some(Drag::Resize { id, original, .. }) => (id, *original, true),
            _ => return SharedString::default(),
        };
        let Some((_, proposed)) = self.drag_proposal() else {
            return SharedString::default();
        };
        let tz = self.options.time_zone;
        let hour12 = self.options.hour12;
        let title = self
            .event(id)
            .map(|e| e.title.trim().to_string())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| "Untitled event".into());
        // A day as said beside where it was: its name within a week, its date further.
        let day = |d: NaiveDate, from: NaiveDate| {
            if (d - from).num_days().abs() < 7 {
                time::weekday_name(d.weekday(), false).to_string()
            } else {
                crate::a11y::day_label(d)
            }
        };
        // A time, with its day when that changed.
        let at = |t: chrono::DateTime<chrono::Utc>, was: chrono::DateTime<chrono::Utc>| {
            let (l, w) = (t.with_timezone(&tz), was.with_timezone(&tz));
            let clock = time::format_time(l.time(), hour12);
            if l.date_naive() == w.date_naive() {
                clock
            } else {
                format!("{} {clock}", day(l.date_naive(), w.date_naive()))
            }
        };
        let to = match (resize, original, proposed) {
            (false, EventTime::Timed { start: was, .. }, EventTime::Timed { start, .. }) => {
                at(start, was)
            }
            (true, EventTime::Timed { end: was, .. }, EventTime::Timed { end, .. }) => at(end, was),
            (false, EventTime::AllDay { start: was, .. }, EventTime::AllDay { start, .. }) => {
                day(start, was)
            }
            (true, EventTime::AllDay { end: was, .. }, EventTime::AllDay { end, .. }) => {
                day(end - Duration::days(1), was - Duration::days(1))
            }
            // Into or out of the hours (only a pointer does that): where it lands.
            (_, _, time) => crate::a11y::when(&time, tz, hour12),
        };
        let all_day = proposed.is_all_day();
        let text = match (said, resize) {
            (Said::Over, false) => format!(
                "{title}, {}, moving to {to}",
                crate::a11y::when(&original, tz, hour12)
            ),
            (Said::Over, true) => format!(
                "{title}, {}, ending {}{to}",
                crate::a11y::when(&original, tz, hour12),
                if all_day { "on " } else { "at " }
            ),
            (Said::Dropped, false) => format!("{title}, moved to {to}"),
            (Said::Dropped, true) => {
                format!(
                    "{title}, now ending {}{to}",
                    if all_day { "on " } else { "at " }
                )
            }
        };
        text.into()
    }
}

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
