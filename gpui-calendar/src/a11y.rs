//! What the calendar tells assistive technology (VoiceOver on the Mac and the iPhone).
//!
//! Events, days and hours are painted on one canvas rather than built from elements, so GPUI
//! can't describe them on its own. The canvas is wrapped in [`Accessible`], an element with a
//! role and a label of its own. Its children are synthetic accessibility nodes, one for each
//! event, day and hour on screen, each with a label, a role, its bounds and what activating it
//! does.
//!
//! With a screen reader on, the canvas lays its frame out in prepaint without painting it, and
//! the nodes are built from that same frame. Without one, nothing is built and nothing extra
//! runs.
//!
//! The nodes are plain [`A11yItem`]s first, so tests can check them; the platform's test
//! double never turns accessibility on. Only [`Accessible`] turns them into AccessKit nodes.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use chrono::{Datelike, Duration, NaiveDate};
use gpui::{
    A11ySubtreeBuilder, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId,
    IntoElement, LayoutId, Pixels, Role, WeakEntity, Window, accesskit, point, size,
};

use crate::calendar::Calendar;
use crate::model::*;
use crate::paint::Kind;
use crate::time;

/// What activating a node does, and the key that keeps its node the same from frame to frame
/// (so VoiceOver's cursor stays on it as events move and the view scrolls).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum A11yTarget {
    /// An event: selects it and opens it, as a click does.
    Event(EventId),
    /// A day's name: shows that day.
    Day(NaiveDate),
    /// "+N more" on a day: shows that day.
    More(NaiveDate),
    /// "+N" beside side-by-side events, for those that didn't fit: opens their list.
    Crowd(Vec<EventId>),
    /// An hour of a day on the grid, or a day of the month: selects it (Enter then makes an
    /// event there).
    Slot { date: NaiveDate, minute: u32 },
}

/// One node for assistive technology: something the calendar drew.
#[derive(Clone, Debug, PartialEq)]
pub struct A11yItem {
    pub role: Role,
    pub label: String,
    /// Where it was drawn, in the window.
    pub bounds: Bounds<Pixels>,
    pub selected: bool,
    pub read_only: bool,
    pub target: A11yTarget,
}

/// "Tuesday 3 to 4 PM", "Tuesday, all day", "Monday to Wednesday, all day".
pub(crate) fn when(time: &EventTime, tz: chrono_tz::Tz, hour12: bool) -> String {
    let day = |d: NaiveDate| time::weekday_name(d.weekday(), false).to_string();
    match *time {
        EventTime::AllDay { start, end } => {
            let last = end - Duration::days(1);
            if last <= start {
                format!("{}, all day", day(start))
            } else if (last - start).num_days() < 7 {
                format!("{} to {}, all day", day(start), day(last))
            } else {
                format!("{} to {}, all day", date_label(start), date_label(last))
            }
        }
        EventTime::Timed { start, end } => {
            let (s, f) = (start.with_timezone(&tz), end.with_timezone(&tz));
            if s.date_naive() == f.date_naive() {
                let range = time::format_range(s.time(), f.time(), hour12).replace(" – ", " to ");
                format!("{} {range}", day(s.date_naive()))
            } else {
                format!(
                    "{} {} to {} {}",
                    day(s.date_naive()),
                    time::format_time(s.time(), hour12),
                    day(f.date_naive()),
                    time::format_time(f.time(), hour12)
                )
            }
        }
    }
}

/// "November 18".
fn date_label(d: NaiveDate) -> String {
    format!("{} {}", time::month_name(d.month()), d.day())
}

/// "Wednesday, November 18".
pub(crate) fn day_label(d: NaiveDate) -> String {
    format!(
        "{}, {}",
        time::weekday_name(d.weekday(), false),
        date_label(d)
    )
}

/// The order events are read in: by day, all-day ones first, then by when they start.
fn reading_order(e: &Event, tz: chrono_tz::Tz) -> (NaiveDate, bool, i64) {
    let start = match e.time {
        EventTime::Timed { start, .. } => start.timestamp(),
        EventTime::AllDay { .. } => 0,
    };
    (time::event_days(&e.time, tz).0, !e.time.is_all_day(), start)
}

impl Calendar {
    /// What a screen reader says for an event: "Team sync, Tuesday 3 to 4 PM, Work calendar",
    /// with ", repeating", ", read-only", ", not answered" (or ", declined") and ", 2 links"
    /// where they apply.
    pub fn describe_event(&self, id: &str) -> Option<String> {
        let e = self.event(id)?;
        let title = if e.title.trim().is_empty() {
            "Untitled event"
        } else {
            e.title.as_ref()
        };
        let mut out = format!(
            "{title}, {}",
            when(&e.time, self.options.time_zone, self.options.hour12)
        );
        if let Some(c) = self.calendar(&e.calendar) {
            out.push_str(&format!(", {} calendar", c.name));
        }
        if e.series.is_some() {
            out.push_str(", repeating");
        }
        if self.is_read_only(e) {
            out.push_str(", read-only");
        }
        // An invitation the user didn't send: not answered yet, or declined, as its chip shows.
        match e.my_answer() {
            Some(Response::NeedsAction) => out.push_str(", not answered"),
            Some(Response::Declined) => out.push_str(", declined"),
            _ => {}
        }
        match e.links.len() {
            0 => {}
            1 => out.push_str(", 1 link"),
            n => out.push_str(&format!(", {n} links")),
        }
        Some(out)
    }

    /// What a screen reader says for a "+N": how many events it stands for and when they are,
    /// "12 more events, 2 to 5 PM".
    pub(crate) fn describe_crowd(&self, ids: &[EventId]) -> String {
        let tz = self.options.time_zone;
        let mut span: Option<(chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)> = None;
        for e in ids.iter().filter_map(|id| self.event(id)) {
            if let EventTime::Timed { start, end } = e.time {
                span = Some(span.map_or((start, end), |(a, z)| (a.min(start), z.max(end))));
            }
        }
        let count = if ids.len() == 1 {
            "1 more event".to_string()
        } else {
            format!("{} more events", ids.len())
        };
        match span {
            Some((a, z)) => format!(
                "{count}, {}",
                time::format_range(
                    a.with_timezone(&tz).time(),
                    z.with_timezone(&tz).time(),
                    self.options.hour12
                )
                .replace(" – ", " to ")
            ),
            None => count,
        }
    }

    /// What the canvas is called as a whole: "Calendar, November 2026".
    pub fn accessibility_label(&self) -> String {
        format!("Calendar, {}", self.title())
    }

    /// A node for each day, event, "+N more" and hour the frame drew, in reading order: the
    /// days' names, the events by when they start, the "more"s, then the hours (or the
    /// month's days). Nothing scrolled out of view is in it.
    pub fn accessibility_items(&self) -> Vec<A11yItem> {
        let shared = &self.shared;
        let Some(geo) = shared.geo.get() else {
            return Vec::new();
        };
        let today = self.today();
        let selected_slot = match self.selection() {
            Some(Selection::Slot { date, minute }) => Some((*date, *minute)),
            _ => None,
        };
        let mut out = Vec::new();
        for (b, date) in shared.dates.borrow().iter() {
            let mut label = day_label(*date);
            if *date == today {
                label.push_str(", today");
            }
            out.push(A11yItem {
                role: Role::Button,
                label,
                bounds: *b,
                selected: false,
                read_only: false,
                target: A11yTarget::Day(*date),
            });
        }
        // One node an event, over the first piece of it drawn: a span's weeks and an overnight
        // event's two days are one event.
        let mut events: Vec<(&Event, Bounds<Pixels>, bool)> = Vec::new();
        for hit in shared.hits.borrow().iter() {
            if events.iter().any(|(e, ..)| e.id == hit.id) {
                continue;
            }
            if let Some(e) = self.event(&hit.id) {
                events.push((e, hit.bounds, hit.read_only));
            }
        }
        let tz = self.options.time_zone;
        events.sort_by_key(|(e, b, _)| (reading_order(e, tz), b.left()));
        for (e, bounds, read_only) in events {
            let Some(label) = self.describe_event(&e.id) else {
                continue;
            };
            out.push(A11yItem {
                role: Role::Button,
                label,
                bounds,
                selected: self.selected_event() == Some(&e.id),
                read_only,
                target: A11yTarget::Event(e.id.clone()),
            });
        }
        for (b, date) in shared.mores.borrow().iter() {
            out.push(A11yItem {
                role: Role::Button,
                label: format!("More events on {}", day_label(*date)),
                bounds: *b,
                selected: false,
                read_only: false,
                target: A11yTarget::More(*date),
            });
        }
        // "+N" for side-by-side events that didn't fit: "12 more events, 2 to 5 PM".
        for (b, ids) in shared.crowds.borrow().iter() {
            out.push(A11yItem {
                role: Role::Button,
                label: self.describe_crowd(ids),
                bounds: *b,
                selected: false,
                read_only: false,
                target: A11yTarget::Crowd(ids.clone()),
            });
        }
        match geo.kind {
            Kind::Grid => {
                for i in 0..geo.days as i32 {
                    let date = geo.date(i);
                    let x = geo.col_x(i);
                    for h in 0..24u32 {
                        let top = geo.y_of(h as f32 * 60.).max(geo.grid_top);
                        let bottom = geo.y_of(h as f32 * 60. + 60.).min(geo.grid_bottom);
                        if bottom <= top {
                            continue;
                        }
                        out.push(A11yItem {
                            role: Role::Cell,
                            label: format!(
                                "{}, {}",
                                day_label(date),
                                time::format_hour(h, self.options.hour12)
                            ),
                            bounds: Bounds::from_corners(
                                point(x, top),
                                point(x + geo.col_w, bottom),
                            ),
                            selected: selected_slot.is_some_and(|(d, m)| d == date && m / 60 == h),
                            read_only: false,
                            target: A11yTarget::Slot {
                                date,
                                minute: h * 60,
                            },
                        });
                    }
                }
            }
            Kind::Month => {
                for row in 0..geo.rows {
                    for col in 0..7 {
                        let date = geo.date(row as i32 * 7 + col);
                        let mut label = day_label(date);
                        if date == today {
                            label.push_str(", today");
                        }
                        out.push(A11yItem {
                            role: Role::Cell,
                            label,
                            bounds: Bounds::new(
                                point(geo.col_x(col), geo.row_top(row)),
                                size(geo.col_w, geo.row_h),
                            ),
                            selected: selected_slot.is_some_and(|(d, _)| d == date),
                            read_only: false,
                            target: A11yTarget::Slot { date, minute: 0 },
                        });
                    }
                }
            }
            Kind::Agenda => {}
            // The year: every day, which shows that day.
            Kind::Year => {
                let laid = self.laid.clone();
                for (i, date) in geo.first.iter_days().take(geo.days as usize).enumerate() {
                    let Some(bounds) = geo.year_cell(date) else {
                        continue;
                    };
                    let mut label = day_label(date);
                    if date == today {
                        label.push_str(", today");
                    }
                    match laid.as_ref().and_then(|l| l.year.get(i)).copied() {
                        Some(1) => label.push_str(", 1 event"),
                        Some(n) if n > 1 => label.push_str(&format!(", {n} events")),
                        _ => {}
                    }
                    out.push(A11yItem {
                        role: Role::Button,
                        label,
                        bounds,
                        selected: selected_slot.is_some_and(|(d, _)| d == date),
                        read_only: false,
                        target: A11yTarget::Day(date),
                    });
                }
            }
        }
        out
    }

    /// Does what activating a node asks, as a click on it would.
    pub fn activate(&mut self, target: &A11yTarget, cx: &mut gpui::Context<Self>) {
        match target {
            A11yTarget::Event(id) => {
                self.select(Some(Selection::Event(id.clone())), cx);
                self.open_event(id.clone(), cx);
            }
            A11yTarget::Day(date) => {
                self.set_view(CalendarView::Day, cx);
                self.go_to(*date, cx);
            }
            A11yTarget::More(date) => {
                if !self.open_more(*date, cx) {
                    self.set_view(CalendarView::Day, cx);
                    self.go_to(*date, cx);
                }
            }
            A11yTarget::Crowd(ids) => {
                if let Some(id) = ids.first() {
                    self.open_hidden(id, cx);
                }
            }
            A11yTarget::Slot { date, minute } => self.select(
                Some(Selection::Slot {
                    date: *date,
                    minute: *minute,
                }),
                cx,
            ),
        }
    }
}

#[cfg(test)]
thread_local! {
    /// Builds the nodes as if a screen reader were on: the test platform never turns one on.
    pub(crate) static FORCE: Cell<bool> = const { Cell::new(false) };
}

/// Whether to build the nodes without a screen reader on (only ever in tests).
pub(crate) fn forced() -> bool {
    #[cfg(test)]
    return FORCE.with(Cell::get);
    #[cfg(not(test))]
    false
}

/// What the canvas laid out for assistive technology this frame, shared between the canvas
/// (which fills it in prepaint) and [`Accessible`] (which hands it to GPUI).
#[derive(Default)]
pub(crate) struct A11yFrame {
    items: RefCell<Vec<A11yItem>>,
    scale: Cell<f32>,
    /// The nodes made from the items, and what each does: for the actions, in paint.
    nodes: RefCell<Vec<(accesskit::NodeId, A11yTarget)>>,
}

impl A11yFrame {
    pub(crate) fn set(&self, items: Vec<A11yItem>, scale: f32) {
        *self.items.borrow_mut() = items;
        self.scale.set(scale);
    }
}

/// An item as an AccessKit node, in the window's device pixels as GPUI's own nodes are.
pub(crate) fn node(item: &A11yItem, scale: f32) -> accesskit::Node {
    let mut node = accesskit::Node::new(item.role);
    node.set_label(item.label.clone());
    let b = item.bounds;
    node.set_bounds(accesskit::Rect {
        x0: (f32::from(b.left()) * scale) as f64,
        y0: (f32::from(b.top()) * scale) as f64,
        x1: (f32::from(b.right()) * scale) as f64,
        y1: (f32::from(b.bottom()) * scale) as f64,
    });
    if item.selected {
        node.set_selected(true);
    }
    if item.read_only {
        node.set_read_only();
    }
    node.add_action(accesskit::Action::Click);
    node
}

/// The canvas, with what it drew told to assistive technology as synthetic children. Layout,
/// prepaint and paint are the canvas's own.
pub(crate) struct Accessible<E> {
    inner: E,
    this: WeakEntity<Calendar>,
    label: String,
    frame: Rc<A11yFrame>,
}

impl<E: Element> Accessible<E> {
    pub(crate) fn new(
        inner: E,
        this: WeakEntity<Calendar>,
        label: String,
        frame: Rc<A11yFrame>,
    ) -> Self {
        Self {
            inner,
            this,
            label,
            frame,
        }
    }
}

impl<E: Element> IntoElement for Accessible<E> {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl<E: Element> Element for Accessible<E> {
    type RequestLayoutState = E::RequestLayoutState;
    type PrepaintState = E::PrepaintState;

    fn id(&self) -> Option<ElementId> {
        Some("calendar-canvas".into())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        self.inner.source_location()
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.inner.request_layout(id, inspector, window, cx)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.inner
            .prepaint(id, inspector, bounds, layout, window, cx)
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.inner
            .paint(id, inspector, bounds, layout, prepaint, window, cx);
        if !window.is_a11y_active() {
            return;
        }
        // Activating a node does what a click on it would.
        for (node, target) in self.frame.nodes.borrow().iter() {
            let (this, target) = (self.this.clone(), target.clone());
            window.on_a11y_action(*node, accesskit::Action::Click, move |_, _, cx| {
                this.update(cx, |c, cx| c.activate(&target, cx)).ok();
            });
        }
    }

    fn a11y_role(&self) -> Option<Role> {
        Some(Role::Group)
    }

    fn write_a11y_info(&self, node: &mut accesskit::Node) {
        node.set_label(self.label.clone());
    }

    fn a11y_synthetic_children(
        &mut self,
        _: &mut Self::PrepaintState,
        builder: &mut A11ySubtreeBuilder,
    ) {
        let scale = self.frame.scale.get();
        let mut nodes = self.frame.nodes.borrow_mut();
        nodes.clear();
        for item in self.frame.items.borrow().iter() {
            let id = builder.synthetic_node_id(&item.target);
            if builder.push_child(id, node(item, scale)) {
                nodes.push((id, item.target.clone()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{NaiveDate, TimeZone, Utc};
    use gpui::{
        Bounds, Entity, Focusable as _, Role, TestAppContext, VisualTestContext, accesskit, point,
        px, size,
    };

    use super::*;

    #[test]
    fn an_item_becomes_a_node_in_device_pixels() {
        let item = A11yItem {
            role: Role::Button,
            label: "Team sync, Tuesday 3 to 4 PM, Work calendar".into(),
            bounds: Bounds::new(point(px(10.), px(20.)), size(px(100.), px(40.))),
            selected: true,
            read_only: true,
            target: A11yTarget::Event("sync".into()),
        };
        let n = node(&item, 2.);
        assert_eq!(n.role(), Role::Button);
        assert_eq!(
            n.label(),
            Some("Team sync, Tuesday 3 to 4 PM, Work calendar")
        );
        assert_eq!(
            n.bounds(),
            Some(accesskit::Rect {
                x0: 20.,
                y0: 40.,
                x1: 220.,
                y1: 120.
            })
        );
        assert_eq!(n.is_selected(), Some(true));
        assert!(n.is_read_only());
        assert!(n.supports_action(accesskit::Action::Click));
        // Not selected, not read-only: neither said.
        let plain = node(
            &A11yItem {
                selected: false,
                read_only: false,
                ..item
            },
            1.,
        );
        assert_eq!(plain.is_selected(), None);
        assert!(!plain.is_read_only());
    }

    fn draw(vcx: &mut VisualTestContext) {
        vcx.run_until_parked();
        vcx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
    }

    #[gpui::test]
    fn with_a_screen_reader_the_nodes_are_this_frames(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_core::init(cx);
            crate::init(cx);
        });
        let tz = chrono_tz::America::Los_Angeles;
        let wednesday = NaiveDate::from_ymd_opt(2026, 11, 18).unwrap();
        let at = |h: f32| crate::time::at_minutes(wednesday, h * 60., tz);
        let (c, vcx) = cx.add_window_view(|_, cx| {
            Calendar::new(cx)
                .with_options(|o| o.time_zone = tz)
                .with_now(Utc.with_ymd_and_hms(2026, 11, 18, 18, 40, 0).unwrap())
                .with_date(wednesday)
                .with_events(vec![
                    Event::timed("a", "Planning", at(11.), at(12.)),
                    Event::all_day(
                        "b",
                        "Trip",
                        wednesday,
                        wednesday + chrono::Duration::days(3),
                    ),
                ])
        });
        vcx.simulate_resize(size(px(1024.), px(800.)));
        draw(vcx);
        // Off: nothing is built.
        assert!(c.read_with(vcx, |c, _| c.a11y.items.borrow().is_empty()));
        FORCE.with(|f| f.set(true));
        draw(vcx);
        // On: laid out in prepaint, before this frame painted, and the same as what it painted.
        let (built, painted) = c.read_with(vcx, |c, _| {
            (c.a11y.items.borrow().clone(), c.accessibility_items())
        });
        assert!(!built.is_empty());
        assert_eq!(built, painted);
        // Moved, the next frame's nodes are where it's drawn now, with no frame in between.
        let before = built;
        c.update(vcx, |c, _| c.scroll_to_minute(13. * 60.));
        draw(vcx);
        let (built, painted) = c.read_with(vcx, |c, _| {
            (c.a11y.items.borrow().clone(), c.accessibility_items())
        });
        assert_ne!(built, before);
        assert_eq!(built, painted);
        FORCE.with(|f| f.set(false));
    }

    #[derive(Debug, PartialEq)]
    enum Stop {
        Days,
        Mini,
        Other,
    }

    fn stop(c: &Entity<Calendar>, vcx: &mut VisualTestContext) -> Stop {
        vcx.update(|window, cx| {
            let focused = window.focused(cx);
            let c = c.read(cx);
            if focused.as_ref() == Some(&c.focus_handle(cx)) {
                Stop::Days
            } else if c.mini.as_ref().map(|m| m.focus_handle(cx)) == focused {
                Stop::Mini
            } else {
                Stop::Other
            }
        })
    }

    /// Space, down and up: what clicks a focused control.
    fn press_space(vcx: &mut VisualTestContext) {
        let keystroke = gpui::Keystroke::parse("space").unwrap();
        vcx.simulate_event(gpui::KeyDownEvent {
            keystroke: keystroke.clone(),
            is_held: false,
            prefer_character_input: false,
        });
        vcx.simulate_event(gpui::KeyUpEvent { keystroke });
        draw(vcx);
    }

    fn tab(vcx: &mut VisualTestContext) {
        vcx.update(|window, cx| window.focus_next(cx));
        draw(vcx);
    }

    #[gpui::test]
    fn tab_goes_through_the_header_the_mini_month_the_days_then_the_panel(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_core::init(cx);
            crate::init(cx);
        });
        let tz = chrono_tz::America::Los_Angeles;
        let wednesday = NaiveDate::from_ymd_opt(2026, 11, 18).unwrap();
        let (c, vcx) = cx.add_window_view(|_, cx| {
            Calendar::new(cx)
                .with_options(|o| {
                    o.time_zone = tz;
                    o.mini_month = true;
                })
                .with_now(Utc.with_ymd_and_hms(2026, 11, 18, 18, 40, 0).unwrap())
                .with_date(wednesday)
                .with_events(vec![Event::timed(
                    "a",
                    "Planning",
                    crate::time::at_minutes(wednesday, 11. * 60., tz),
                    crate::time::at_minutes(wednesday, 12. * 60., tz),
                )])
        });
        vcx.simulate_resize(size(px(1280.), px(820.)));
        draw(vcx);
        draw(vcx);
        // A week on, so Today has something to do; the keyboard on the days, and a panel open.
        c.update(vcx, |c, cx| c.next(cx));
        vcx.update(|window, cx| c.read(cx).focus_handle(cx).focus(window, cx));
        c.update(vcx, |c, cx| c.open_panel("a".into(), cx));
        draw(vcx);
        assert_eq!(stop(&c, vcx), Stop::Days);

        // After the days, the panel: its first stop is its close button.
        tab(vcx);
        assert_eq!(stop(&c, vcx), Stop::Other);
        press_space(vcx);
        assert!(
            c.read_with(vcx, |c, _| c.panel_event().is_none()),
            "space closed the panel"
        );

        // Around again from the days, with no panel: the header's first control, Today.
        vcx.update(|window, cx| c.read(cx).focus_handle(cx).focus(window, cx));
        draw(vcx);
        tab(vcx);
        assert_eq!(stop(&c, vcx), Stop::Other);
        press_space(vcx);
        draw(vcx);
        assert_eq!(c.read_with(vcx, |c, _| c.date()), wednesday);

        // Then the rest of the header, the mini month, and the days.
        let mut seen = vec![Stop::Other];
        for _ in 0..20 {
            tab(vcx);
            let s = stop(&c, vcx);
            let days = s == Stop::Days;
            seen.push(s);
            if days {
                break;
            }
        }
        let n = seen.len();
        assert!(n >= 4, "{seen:?}");
        assert_eq!(seen[n - 2..], [Stop::Mini, Stop::Days], "{seen:?}");
        assert!(seen[..n - 2].iter().all(|s| *s == Stop::Other), "{seen:?}");
    }
}
