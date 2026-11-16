//! What the host hands the calendar: its calendars, their events, the view to show, and how
//! to show it.

use chrono::{DateTime, Duration, NaiveDate, NaiveTime, Utc, Weekday};
use chrono_tz::Tz;
use gpui::SharedString;

/// An event's id: whatever the host uses to find it again. For a recurring event it names one
/// occurrence (an instance id), and [`Event::series`] names the series.
pub type EventId = SharedString;

/// A calendar's id, as the host knows it.
pub type CalendarId = SharedString;

/// One of the host's calendars: a list of them comes with the events, and each event names the
/// one it belongs to. The calendar's colour is its events' colour.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CalendarSource {
    pub id: CalendarId,
    pub name: SharedString,
    /// As 0xRRGGBB.
    pub color: u32,
    /// Hidden calendars' events aren't drawn, selected or counted.
    pub visible: bool,
    /// Its events can be selected and opened, but not moved, resized or deleted, and nothing is
    /// made in it.
    pub read_only: bool,
}

impl CalendarSource {
    /// A visible calendar that can be edited.
    pub fn new(id: impl Into<CalendarId>, name: impl Into<SharedString>, rgb: u32) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            color: rgb,
            visible: true,
            read_only: false,
        }
    }

    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }
}

/// Which event an edit is about: the occurrence's id, its series if it's one of a recurring
/// event's, and its calendar. The host decides whether a change to an occurrence changes that
/// one only or the whole series.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EventRef {
    pub id: EventId,
    pub series: Option<EventId>,
    pub calendar: CalendarId,
}

/// When an event happens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EventTime {
    /// From one instant to another. The end is exclusive: an event that ends at midnight
    /// doesn't show on the next day.
    Timed {
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    },
    /// Whole days, the same dates in every time zone. The end is exclusive, as an iCalendar
    /// DTEND: a one-day event on the 14th is `start: 14th, end: 15th`.
    AllDay { start: NaiveDate, end: NaiveDate },
}

impl EventTime {
    pub fn timed(start: DateTime<Utc>, end: DateTime<Utc>) -> Self {
        Self::Timed {
            start,
            end: end.max(start),
        }
    }

    /// Whole days from `start` up to but not including `end`.
    pub fn all_day(start: NaiveDate, end: NaiveDate) -> Self {
        Self::AllDay {
            start,
            end: end.max(start + Duration::days(1)),
        }
    }

    pub fn is_all_day(&self) -> bool {
        matches!(self, Self::AllDay { .. })
    }
}

/// What an event's link points at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LinkKind {
    /// A page or other doc in the host app.
    Doc,
    /// One of the host's agents.
    Agent,
}

/// A link on an event: to a doc or an agent. The calendar shows it and reports a click on it
/// ([`crate::CalendarEvent::OpenLink`]); it never fetches or opens anything itself.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct EventLink {
    pub kind: LinkKind,
    /// The host's id for what it points at.
    pub target: SharedString,
    /// What it's called on screen.
    pub label: SharedString,
}

impl EventLink {
    pub fn new(
        kind: LinkKind,
        target: impl Into<SharedString>,
        label: impl Into<SharedString>,
    ) -> Self {
        Self {
            kind,
            target: target.into(),
            label: label.into(),
        }
    }

    /// A link to a doc.
    pub fn doc(target: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self::new(LinkKind::Doc, target, label)
    }

    /// A link to an agent.
    pub fn agent(target: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self::new(LinkKind::Agent, target, label)
    }
}

/// One event, as the host supplies it for the range on screen. A recurring event comes
/// expanded: one `Event` per occurrence, each with an id of its own and the series' id.
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    /// The occurrence's own id, unique among the events.
    pub id: EventId,
    /// The recurring series it's an occurrence of, if it is one.
    pub series: Option<EventId>,
    pub title: SharedString,
    pub time: EventTime,
    /// The id of the [`CalendarSource`] it belongs to, which gives its colour, whether it's
    /// shown and whether it can be edited.
    pub calendar: CalendarId,
    /// A colour of its own, as 0xRRGGBB, over its calendar's (Google Calendar lets one event
    /// have its own). Usually none.
    pub color: Option<u32>,
    pub location: Option<SharedString>,
    /// This one event can't be moved, resized or deleted, whatever its calendar allows (it can
    /// still be selected and opened).
    pub read_only: bool,
    /// Links to docs and agents, shown on the event in the order given.
    pub links: Vec<EventLink>,
}

impl Event {
    /// A timed event from `start` up to `end`.
    pub fn timed(
        id: impl Into<EventId>,
        title: impl Into<SharedString>,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Self {
        Self::new(id, title, EventTime::timed(start, end))
    }

    /// An all-day event from `start` up to but not including `end`.
    pub fn all_day(
        id: impl Into<EventId>,
        title: impl Into<SharedString>,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Self {
        Self::new(id, title, EventTime::all_day(start, end))
    }

    pub fn new(id: impl Into<EventId>, title: impl Into<SharedString>, time: EventTime) -> Self {
        Self {
            id: id.into(),
            series: None,
            title: title.into(),
            time,
            calendar: CalendarId::default(),
            color: None,
            location: None,
            read_only: false,
            links: Vec::new(),
        }
    }

    pub fn calendar(mut self, calendar: impl Into<CalendarId>) -> Self {
        self.calendar = calendar.into();
        self
    }

    /// Marks it an occurrence of a recurring series.
    pub fn series(mut self, series: impl Into<EventId>) -> Self {
        self.series = Some(series.into());
        self
    }

    /// The reference edits to it carry.
    pub fn to_ref(&self) -> EventRef {
        EventRef {
            id: self.id.clone(),
            series: self.series.clone(),
            calendar: self.calendar.clone(),
        }
    }

    pub fn color(mut self, rgb: u32) -> Self {
        self.color = Some(rgb);
        self
    }

    pub fn location(mut self, location: impl Into<SharedString>) -> Self {
        self.location = Some(location.into());
        self
    }

    pub fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Adds a link to a doc or an agent.
    pub fn link(mut self, link: EventLink) -> Self {
        self.links.push(link);
        self
    }
}

/// Which view the calendar shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CalendarView {
    Day,
    /// Seven days from the week's first day (see [`CalendarOptions::week_start`]).
    Week,
    /// This many days from the anchor date, on the time grid (2 to 14 is sensible).
    Days(u32),
    Month,
    /// A list of this many days' events from the anchor date, grouped by day: the compact view
    /// for narrow places such as a phone.
    Agenda(u32),
}

impl CalendarView {
    /// The agenda the header's Agenda button shows: two weeks.
    pub const AGENDA: CalendarView = CalendarView::Agenda(14);

    /// Whether the view is the time grid (day, week, N days) rather than the month or the
    /// agenda.
    pub fn is_time_grid(self) -> bool {
        !matches!(self, CalendarView::Month | CalendarView::Agenda(_))
    }

    pub fn label(self) -> &'static str {
        match self {
            CalendarView::Day => "Day",
            CalendarView::Week => "Week",
            CalendarView::Days(_) => "Days",
            CalendarView::Month => "Month",
            CalendarView::Agenda(_) => "Agenda",
        }
    }
}

/// How the calendar behaves and reads. The host can change any of it at any time.
#[derive(Clone, Debug)]
pub struct CalendarOptions {
    /// The zone times are shown in. Event times are instants, so changing it moves events on
    /// the grid; all-day events stay on their dates.
    pub time_zone: Tz,
    /// The first day of a week, for the week and month views.
    pub week_start: Weekday,
    /// 9 AM rather than 09:00.
    pub hour12: bool,
    /// Drags snap to this many minutes (15 by default). 1 turns snapping off.
    pub snap_minutes: u32,
    /// How far the arrow keys move the selected slot on the time grid.
    pub slot_minutes: u32,
    /// How long an event made by a double click or Enter on a slot lasts.
    pub default_duration: Duration,
    /// The height of an hour on the time grid, in CSS pixels (it scales with the window's rem).
    pub hour_height: f32,
    /// Where the time grid opens when today isn't on screen. With today on screen it opens
    /// with the current time about a third of the way down.
    pub scroll_time: NaiveTime,
    /// The calendar new events are proposed in. Without one, or if it's hidden or can't be
    /// edited, it's the first visible calendar that can be.
    pub default_calendar: Option<CalendarId>,
    /// Draws events that have ended a little quieter.
    pub dim_past: bool,
    /// The most rows of all-day events the time grid shows before it says "+N".
    pub all_day_rows: usize,
    /// Taken by touch, as on a phone or a tablet. A long press picks an event up or makes one,
    /// a touch on the selected event drags it, a swipe sideways changes the period, small
    /// things take taps in a 44-point square, and the header's arrows give way to the swipe.
    /// On by default on iOS and Android.
    pub touch: bool,
}

impl Default for CalendarOptions {
    fn default() -> Self {
        Self {
            time_zone: chrono_tz::UTC,
            week_start: Weekday::Mon,
            hour12: true,
            snap_minutes: 15,
            slot_minutes: 30,
            default_duration: Duration::hours(1),
            hour_height: 48.,
            scroll_time: NaiveTime::from_hms_opt(8, 0, 0).expect("8:00"),
            default_calendar: None,
            dim_past: true,
            all_day_rows: 3,
            touch: cfg!(any(target_os = "ios", target_os = "android")),
        }
    }
}

impl CalendarOptions {
    /// The snap step, never less than a minute.
    pub fn snap(&self) -> f32 {
        self.snap_minutes.max(1) as f32
    }
}

/// What's selected: an event, or a slot of time with nothing in it yet.
#[derive(Clone, Debug, PartialEq)]
pub enum Selection {
    Event(EventId),
    /// A slot on a day: `minute` is minutes after the day's midnight on the wall clock. In the
    /// month view only the date counts.
    Slot {
        date: NaiveDate,
        minute: u32,
    },
}
