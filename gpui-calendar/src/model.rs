//! What the host hands the calendar: events, the view to show, and how to show it.

use std::collections::HashMap;

use chrono::{DateTime, Duration, NaiveDate, NaiveTime, Utc, Weekday};
use chrono_tz::Tz;
use gpui_kit::SharedString;

/// An event's id: whatever the host uses to find it again.
pub type EventId = SharedString;

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

/// One event, as the host supplies it for the range on screen.
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub id: EventId,
    pub title: SharedString,
    pub time: EventTime,
    /// The calendar it belongs to, which picks its colour unless it has its own.
    pub calendar: SharedString,
    /// Its own colour, as 0xRRGGBB, over its calendar's.
    pub color: Option<u32>,
    pub location: Option<SharedString>,
    /// Can't be moved or resized by dragging (it can still be selected and opened).
    pub read_only: bool,
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
            title: title.into(),
            time,
            calendar: SharedString::default(),
            color: None,
            location: None,
            read_only: false,
        }
    }

    pub fn calendar(mut self, calendar: impl Into<SharedString>) -> Self {
        self.calendar = calendar.into();
        self
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
}

impl CalendarView {
    /// Whether the view is the time grid (day, week, N days) rather than the month.
    pub fn is_time_grid(self) -> bool {
        !matches!(self, CalendarView::Month)
    }

    pub fn label(self) -> &'static str {
        match self {
            CalendarView::Day => "Day",
            CalendarView::Week => "Week",
            CalendarView::Days(_) => "Days",
            CalendarView::Month => "Month",
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
    /// Colours per calendar id, as 0xRRGGBB. A calendar without one gets one of the palette's,
    /// chosen by its id, so it keeps it from one launch to the next.
    pub calendar_colors: HashMap<SharedString, u32>,
    /// Draws events that have ended a little quieter.
    pub dim_past: bool,
    /// The most rows of all-day events the time grid shows before it says "+N".
    pub all_day_rows: usize,
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
            calendar_colors: HashMap::new(),
            dim_past: true,
            all_day_rows: 3,
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
