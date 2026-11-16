//! Day, week, N-day, month and agenda calendar views with a time grid, in GPUI.
//!
//! A [`Calendar`] is an entity that shows the host's [`Event`]s. The day and week views share a
//! time grid (an all-day row, hour lines, a line at the current time, and a scroll position that
//! opens near it); the month view shows each day's events with "+N more" for what doesn't fit.
//! The agenda lists the days' events, with their links, for narrow places such as a phone.
//! Overlapping timed events sit side by side in columns, and events across days are spans.
//!
//! People drag empty time to make an event, drag an event to move it (to other days too), drag
//! its bottom edge to change its end, click to select and double-click to open. Drags snap to 15
//! minutes (configurable), show a ghost of where the event would land, and Escape drops them.
//! The calendar never changes an event itself: each of these is a [`CalendarEvent`] the host
//! answers by updating the events it supplies.
//!
//! The README has a complete example, compiled as a doctest.
//!
//! Recurring events are the host's to expand: it supplies each occurrence in the range asked
//! for as an event of its own.

mod calendar;
mod find;
pub mod layout;
mod model;
#[cfg(test)]
mod overflow;
mod paint;
mod theme;
pub mod time;

pub use calendar::{
    Calendar, CalendarEvent, Cancel, DayView, DeleteSelected, MonthView, NextPeriod, OpenSelected,
    PreviousPeriod, SelectDown, SelectLeft, SelectRight, SelectUp, Today, WeekView, bind_keys,
};
pub use model::*;
pub use paint::Area;
pub use theme::{CalendarColors, PALETTE, palette_color};

// The README's examples, compiled as doctests.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

/// Sets the calendar up: its key bindings. Call once, after `gpui_core::init`.
pub fn init(cx: &mut gpui::App) {
    bind_keys(cx);
}
