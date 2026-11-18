//! The date and time maths: days in a zone (23 and 25 hours long on DST days), where an instant
//! falls on a day's wall clock and back, snapping, the dates each view shows, and the words for
//! times and ranges. Nothing here draws.
//!
//! The time grid is the wall clock. An hour is the same height on every day, so on the day the
//! clocks go forward the skipped hour has no instants in it, and on the day they go back the
//! repeated hour's two passes share the same rows. Turning a place on the grid back into an
//! instant follows RFC 5545 and JavaScript's Temporal: a time in the repeated hour is its first
//! pass, and a time in the skipped hour moves forward by the gap (2:30 becomes 3:30).

use chrono::{
    DateTime, Datelike, Duration, LocalResult, Months, NaiveDate, NaiveDateTime, NaiveTime, Offset,
    TimeZone, Timelike, Utc, Weekday,
};
use chrono_tz::{OffsetName, Tz};

use crate::model::{CalendarView, EventTime};

/// Minutes in a day on the wall clock, whatever the day's real length.
pub const DAY_MINUTES: f32 = 1440.;

/// The instant a wall-clock time names in `tz`: the first pass through a repeated hour, and
/// the same time after the gap in a skipped one.
pub fn resolve(local: NaiveDateTime, tz: Tz) -> DateTime<Utc> {
    match tz.from_local_datetime(&local) {
        LocalResult::Single(t) => t.with_timezone(&Utc),
        LocalResult::Ambiguous(first, _) => first.with_timezone(&Utc),
        LocalResult::None => {
            // Read with the offset from before the gap: 2:30 at UTC−5 is 3:30 at UTC−4.
            let before = tz
                .offset_from_local_datetime(&(local - Duration::days(1)))
                .earliest()
                .map(|o| o.fix().local_minus_utc())
                .unwrap_or(0);
            DateTime::from_naive_utc_and_offset(local - Duration::seconds(before.into()), Utc)
        }
    }
}

/// The first instant of `date` in `tz`: midnight, or the first time that exists where the
/// zone's clocks skip midnight itself.
pub fn day_start(date: NaiveDate, tz: Tz) -> DateTime<Utc> {
    resolve(date.and_time(NaiveTime::MIN), tz)
}

/// How long `date` lasts in `tz`: 24 hours, or 23 and 25 on the days the clocks change.
pub fn day_length(date: NaiveDate, tz: Tz) -> Duration {
    day_start(date + Duration::days(1), tz) - day_start(date, tz)
}

/// The instant `minutes` after midnight on `date`'s wall clock. Minutes past the end of the
/// day, or before its start, land on the days either side.
pub fn at_minutes(date: NaiveDate, minutes: f32, tz: Tz) -> DateTime<Utc> {
    let whole = minutes.round() as i64;
    resolve(date.and_time(NaiveTime::MIN) + Duration::minutes(whole), tz)
}

/// Where `t` falls on `date`'s wall clock, in minutes after midnight: 0 if it's before the
/// day and 1440 if it's after.
pub fn wall_minutes(t: DateTime<Utc>, date: NaiveDate, tz: Tz) -> f32 {
    let local = t.with_timezone(&tz).naive_local();
    match local.date().cmp(&date) {
        std::cmp::Ordering::Less => 0.,
        std::cmp::Ordering::Greater => DAY_MINUTES,
        std::cmp::Ordering::Equal => minutes_of(local.time()),
    }
}

/// A wall-clock time as minutes after midnight.
pub fn minutes_of(t: NaiveTime) -> f32 {
    t.hour() as f32 * 60. + t.minute() as f32 + t.second() as f32 / 60.
}

/// The local date and minutes of an instant.
pub fn local(t: DateTime<Utc>, tz: Tz) -> (NaiveDate, f32) {
    let l = t.with_timezone(&tz).naive_local();
    (l.date(), minutes_of(l.time()))
}

/// Rounds to the nearest step.
pub fn snap(minutes: f32, step: f32) -> f32 {
    if step <= 1. {
        return minutes.round();
    }
    (minutes / step).round() * step
}

/// Rounds down to a step.
pub fn snap_floor(minutes: f32, step: f32) -> f32 {
    let step = step.max(1.);
    (minutes / step).floor() * step
}

/// Rounds up to a step.
pub fn snap_ceil(minutes: f32, step: f32) -> f32 {
    let step = step.max(1.);
    (minutes / step).ceil() * step
}

/// The day the clocks change on, if `date` is one: which wall-clock minutes are skipped or
/// happen twice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DstChange {
    /// The clocks go forward: these minutes don't exist.
    Skipped { from: u32, to: u32 },
    /// The clocks go back: these minutes happen twice.
    Repeated { from: u32, to: u32 },
}

/// Finds the change on `date`, looking every 15 minutes (every zone in use changes on a
/// quarter hour).
pub fn dst_change(date: NaiveDate, tz: Tz) -> Option<DstChange> {
    let length = day_length(date, tz);
    if length == Duration::hours(24) {
        return None;
    }
    let skipped = length < Duration::hours(24);
    let mut range: Option<(u32, u32)> = None;
    for m in (0..1440u32).step_by(15) {
        let at = date.and_time(NaiveTime::MIN) + Duration::minutes(m.into());
        let hit = match tz.from_local_datetime(&at) {
            LocalResult::None => skipped,
            LocalResult::Ambiguous(..) => !skipped,
            LocalResult::Single(_) => false,
        };
        if hit {
            range = Some(match range {
                Some((from, _)) => (from, m + 15),
                None => (m, m + 15),
            });
        }
    }
    let (from, to) = range?;
    Some(if skipped {
        DstChange::Skipped { from, to }
    } else {
        DstChange::Repeated { from, to }
    })
}

/// The first day of the week `date` is in.
pub fn week_start(date: NaiveDate, first: Weekday) -> NaiveDate {
    let back = (date.weekday().num_days_from_monday() + 7 - first.num_days_from_monday()) % 7;
    date - Duration::days(back.into())
}

/// The first day of `date`'s month.
pub fn month_start(date: NaiveDate) -> NaiveDate {
    date.with_day(1).expect("every month has a first")
}

/// The dates a view shows around `anchor`: the first, and how many. The month view shows whole
/// weeks, as many as the month touches (four to six).
pub fn visible_days(view: CalendarView, anchor: NaiveDate, first_day: Weekday) -> (NaiveDate, u32) {
    match view {
        CalendarView::Day => (anchor, 1),
        CalendarView::Week => (week_start(anchor, first_day), 7),
        CalendarView::Days(n) => (anchor, n.clamp(1, 31)),
        CalendarView::Agenda(n) => (anchor, n.clamp(1, 366)),
        CalendarView::Month => {
            let first = month_start(anchor);
            let last = first + Months::new(1) - Duration::days(1);
            let start = week_start(first, first_day);
            let weeks = (last - start).num_days() / 7 + 1;
            (start, weeks as u32 * 7)
        }
        CalendarView::Year => {
            let first = year_start(anchor);
            let next = first + Months::new(12);
            (first, (next - first).num_days() as u32)
        }
    }
}

/// The first day of `date`'s year.
pub fn year_start(date: NaiveDate) -> NaiveDate {
    NaiveDate::from_ymd_opt(date.year(), 1, 1).expect("every year has a first")
}

/// The instants a view covers, from the first day's start to the day after the last's: what
/// the host should supply events for.
pub fn visible_range(
    view: CalendarView,
    anchor: NaiveDate,
    first_day: Weekday,
    tz: Tz,
) -> (DateTime<Utc>, DateTime<Utc>) {
    let (first, days) = visible_days(view, anchor, first_day);
    (
        day_start(first, tz),
        day_start(first + Duration::days(days.into()), tz),
    )
}

/// The anchor one period before (`dir` −1) or after (+1) this one.
pub fn step(view: CalendarView, anchor: NaiveDate, dir: i32) -> NaiveDate {
    let days = |n: i64| anchor + Duration::days(n * dir as i64);
    match view {
        CalendarView::Day => days(1),
        CalendarView::Week => days(7),
        CalendarView::Days(n) => days(n.clamp(1, 31).into()),
        CalendarView::Agenda(n) => days(n.clamp(1, 366).into()),
        CalendarView::Month | CalendarView::Year => {
            let per = if view == CalendarView::Year { 12 } else { 1 };
            let m = Months::new(dir.unsigned_abs() * per);
            if dir >= 0 { anchor + m } else { anchor - m }
        }
    }
}

/// The local dates a timed event touches, the last inclusive. An event that ends at midnight
/// doesn't touch the next day.
pub fn timed_days(start: DateTime<Utc>, end: DateTime<Utc>, tz: Tz) -> (NaiveDate, NaiveDate) {
    let first = start.with_timezone(&tz).date_naive();
    let last = if end > start {
        (end - Duration::nanoseconds(1))
            .with_timezone(&tz)
            .date_naive()
    } else {
        first
    };
    (first, last.max(first))
}

/// The dates an event touches, the last inclusive.
pub fn event_days(time: &EventTime, tz: Tz) -> (NaiveDate, NaiveDate) {
    match *time {
        EventTime::Timed { start, end } => timed_days(start, end, tz),
        EventTime::AllDay { start, end } => (start, (end - Duration::days(1)).max(start)),
    }
}

/// Whether an event goes in the all-day row rather than on the grid: all-day events, and timed
/// ones a day long or longer. A shorter one that crosses midnight is drawn in a piece on each
/// day.
pub fn in_all_day_row(time: &EventTime) -> bool {
    match *time {
        EventTime::AllDay { .. } => true,
        EventTime::Timed { start, end } => end - start >= Duration::hours(24),
    }
}

/// Moves an event by what the pointer moved, on the wall clock: `from` is where it was grabbed
/// and `to` where it is, each a date and minutes after that date's midnight. On the time grid
/// the start snaps to `step` and the event keeps its length in real time; in the month view and
/// the all-day row (`by_days`) it moves by whole days and keeps its time of day.
pub fn moved(
    time: EventTime,
    from: (NaiveDate, f32),
    to: (NaiveDate, f32),
    by_days: bool,
    step: f32,
    tz: Tz,
) -> EventTime {
    let days = (to.0 - from.0).num_days();
    match time {
        EventTime::AllDay { start, end } => EventTime::AllDay {
            start: start + Duration::days(days),
            end: end + Duration::days(days),
        },
        EventTime::Timed { start, end } => {
            let length = end - start;
            let (date, minute) = local(start, tz);
            let new_start = if by_days {
                at_minutes(date + Duration::days(days), minute, tz)
            } else {
                // Everything measured in wall-clock minutes from the grabbed day's midnight.
                let start_at = (date - from.0).num_days() as f32 * DAY_MINUTES + minute;
                let delta = days as f32 * DAY_MINUTES + (to.1 - from.1);
                at_minutes(from.0, snap(start_at + delta, step), tz)
            };
            EventTime::Timed {
                start: new_start,
                end: new_start + length,
            }
        }
    }
}

/// Moves an event's end to where the pointer is, snapped, never before one step after its
/// start. An all-day event's end moves by whole days.
pub fn resized(time: EventTime, to: (NaiveDate, f32), step: f32, tz: Tz) -> EventTime {
    match time {
        EventTime::AllDay { start, .. } => EventTime::AllDay {
            start,
            end: (to.0 + Duration::days(1)).max(start + Duration::days(1)),
        },
        EventTime::Timed { start, .. } => {
            let end = at_minutes(to.0, snap(to.1, step), tz);
            let shortest = start + Duration::minutes(step.max(1.) as i64);
            EventTime::Timed {
                start,
                end: end.max(shortest),
            }
        }
    }
}

/// The event a drag on empty grid makes, on `date` from one place to the other in either
/// order: the earlier rounded down and the later up, at least one step long.
pub fn created(date: NaiveDate, a: f32, b: f32, step: f32, tz: Tz) -> EventTime {
    let step = step.max(1.);
    let lo = snap_floor(a.min(b), step).clamp(0., DAY_MINUTES - step);
    let hi = snap_ceil(a.max(b), step).clamp(lo + step, DAY_MINUTES);
    EventTime::Timed {
        start: at_minutes(date, lo, tz),
        end: at_minutes(date, hi, tz),
    }
}

/// The all-day event a drag across days makes, the two dates in either order.
pub fn created_days(a: NaiveDate, b: NaiveDate) -> EventTime {
    EventTime::AllDay {
        start: a.min(b),
        end: a.max(b) + Duration::days(1),
    }
}

/// An hour on a 12-hour clock: 12, then 1 to 11.
fn twelve(hour: u32) -> u32 {
    match hour % 12 {
        0 => 12,
        h => h,
    }
}

/// An hour as the gutter shows it: "9 AM" or "09:00".
pub fn format_hour(hour: u32, hour12: bool) -> String {
    let hour = hour % 24;
    if hour12 {
        let h = twelve(hour);
        format!("{h} {}", if hour < 12 { "AM" } else { "PM" })
    } else {
        format!("{hour:02}:00")
    }
}

/// A time: "9 AM", "9:30 AM", or "09:30".
pub fn format_time(t: NaiveTime, hour12: bool) -> String {
    if hour12 {
        let h = twelve(t.hour());
        let half = if t.hour() < 12 { "AM" } else { "PM" };
        if t.minute() == 0 {
            format!("{h} {half}")
        } else {
            format!("{h}:{:02} {half}", t.minute())
        }
    } else {
        format!("{:02}:{:02}", t.hour(), t.minute())
    }
}

/// A clock reading without AM or PM, for the line at the current time: "10:40", or "10:40" on
/// a 24-hour clock too ("22:40" in the afternoon).
pub fn format_clock(t: NaiveTime, hour12: bool) -> String {
    if hour12 {
        let h = twelve(t.hour());
        format!("{h}:{:02}", t.minute())
    } else {
        format!("{:02}:{:02}", t.hour(), t.minute())
    }
}

/// A time without its half of the day, for the month view's rows: "9", "9:30", "09:30".
pub fn format_time_short(t: NaiveTime, hour12: bool) -> String {
    if hour12 {
        let h = twelve(t.hour());
        let half = if t.hour() < 12 { "a" } else { "p" };
        if t.minute() == 0 {
            format!("{h}{half}")
        } else {
            format!("{h}:{:02}{half}", t.minute())
        }
    } else {
        format!("{:02}:{:02}", t.hour(), t.minute())
    }
}

/// A range of times, the half of the day said once when both share it: "9 – 10:30 AM",
/// "11 AM – 1 PM", "09:00 – 10:30".
pub fn format_range(start: NaiveTime, end: NaiveTime, hour12: bool) -> String {
    if hour12 && (start.hour() < 12) == (end.hour() < 12) {
        let h = twelve(start.hour());
        let first = if start.minute() == 0 {
            format!("{h}")
        } else {
            format!("{h}:{:02}", start.minute())
        };
        format!("{first} – {}", format_time(end, true))
    } else {
        format!(
            "{} – {}",
            format_time(start, hour12),
            format_time(end, hour12)
        )
    }
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// A month's name.
pub fn month_name(month: u32) -> &'static str {
    MONTHS[(month as usize + 11) % 12]
}

/// A month's first three letters.
pub fn month_short(month: u32) -> &'static str {
    &month_name(month)[..3]
}

/// A weekday's name, or its first three letters.
pub fn weekday_name(day: Weekday, short: bool) -> &'static str {
    let names = [
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
    ];
    let name = names[day.num_days_from_monday() as usize];
    if short { &name[..3] } else { name }
}

/// What the header calls the period on screen: "November 2026", "Nov – Dec 2026",
/// "Dec 2026 – Jan 2027", or for a day, "Saturday, November 14, 2026".
/// A shorter title for a narrow header: "Wed, Nov 18", "Nov 2026", "Nov – Dec 2026" or
/// "Nov 17 – 30".
pub fn short_title(view: CalendarView, anchor: NaiveDate, first_day: Weekday) -> String {
    let short = |d: NaiveDate| format!("{} {}", month_short(d.month()), d.day());
    let (first, days) = visible_days(view, anchor, first_day);
    let last = first + Duration::days(days as i64 - 1);
    match view {
        CalendarView::Day => format!(
            "{}, {}",
            weekday_name(anchor.weekday(), true),
            short(anchor)
        ),
        CalendarView::Month => format!("{} {}", month_short(anchor.month()), anchor.year()),
        CalendarView::Year => anchor.year().to_string(),
        CalendarView::Agenda(_) if first == last => short(first),
        CalendarView::Agenda(_) if first.month() == last.month() => {
            format!("{} – {}", short(first), last.day())
        }
        CalendarView::Agenda(_) => format!("{} – {}", short(first), short(last)),
        CalendarView::Week | CalendarView::Days(_) => {
            if first.month() == last.month() {
                format!("{} {}", month_short(first.month()), first.year())
            } else if first.year() == last.year() {
                format!(
                    "{} – {} {}",
                    month_short(first.month()),
                    month_short(last.month()),
                    last.year()
                )
            } else {
                format!(
                    "{} {} – {} {}",
                    month_short(first.month()),
                    first.year() % 100,
                    month_short(last.month()),
                    last.year() % 100
                )
            }
        }
    }
}

pub fn title(view: CalendarView, anchor: NaiveDate, first_day: Weekday) -> String {
    match view {
        CalendarView::Month => format!("{} {}", month_name(anchor.month()), anchor.year()),
        CalendarView::Year => anchor.year().to_string(),
        CalendarView::Day => format!(
            "{}, {} {}, {}",
            weekday_name(anchor.weekday(), false),
            month_name(anchor.month()),
            anchor.day(),
            anchor.year()
        ),
        CalendarView::Agenda(_) => {
            let (first, days) = visible_days(view, anchor, first_day);
            let last = first + Duration::days(days as i64 - 1);
            let short = |d: NaiveDate| format!("{} {}", month_short(d.month()), d.day());
            if first == last {
                format!("{}, {}", short(first), first.year())
            } else if first.year() == last.year() && first.month() == last.month() {
                format!("{} – {}, {}", short(first), last.day(), last.year())
            } else if first.year() == last.year() {
                format!("{} – {}, {}", short(first), short(last), last.year())
            } else {
                format!(
                    "{}, {} – {}, {}",
                    short(first),
                    first.year(),
                    short(last),
                    last.year()
                )
            }
        }
        CalendarView::Week | CalendarView::Days(_) => {
            let (first, days) = visible_days(view, anchor, first_day);
            let last = first + Duration::days(days as i64 - 1);
            if first.month() == last.month() && first.year() == last.year() {
                format!("{} {}", month_name(first.month()), first.year())
            } else if first.year() == last.year() {
                format!(
                    "{} – {} {}",
                    month_short(first.month()),
                    month_short(last.month()),
                    first.year()
                )
            } else {
                format!(
                    "{} {} – {} {}",
                    month_short(first.month()),
                    first.year(),
                    month_short(last.month()),
                    last.year()
                )
            }
        }
    }
}

/// A zone's offset from GMT at an instant, as Google Calendar writes it: "GMT-5", "GMT+5:30",
/// or "GMT" itself.
pub fn gmt_offset(at: DateTime<Utc>, tz: Tz) -> String {
    let secs = tz
        .offset_from_utc_datetime(&at.naive_utc())
        .fix()
        .local_minus_utc();
    let sign = if secs < 0 { '-' } else { '+' };
    let (h, m) = (secs.abs() / 3600, secs.abs() % 3600 / 60);
    match (h, m) {
        (0, 0) => "GMT".into(),
        (h, 0) => format!("GMT{sign}{h}"),
        (h, m) => format!("GMT{sign}{h}:{m:02}"),
    }
}

/// A zone's place, from its IANA name: "New York" for America/New_York, "Buenos Aires" for
/// America/Argentina/Buenos_Aires, "UTC" for UTC.
pub fn zone_place(tz: Tz) -> String {
    tz.name()
        .rsplit('/')
        .next()
        .unwrap_or(tz.name())
        .replace('_', " ")
}

/// The zone's short name on a date, for the corner of the grid: "PST", or "GMT+5:30" where the
/// database has no letters for it.
pub fn zone_label(date: NaiveDate, tz: Tz) -> String {
    let offset = tz
        .offset_from_utc_datetime(&date.and_time(NaiveTime::from_hms_opt(12, 0, 0).expect("noon")));
    if let Some(abbr) = offset.abbreviation()
        && abbr.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
    {
        return abbr.to_string();
    }
    let secs = offset.fix().local_minus_utc();
    let sign = if secs < 0 { '-' } else { '+' };
    let (h, m) = (secs.abs() / 3600, secs.abs() % 3600 / 60);
    if m == 0 {
        format!("GMT{sign}{h}")
    } else {
        format!("GMT{sign}{h}:{m:02}")
    }
}

/// What a second zone's column says at each of the grid's 24 hour lines on `date` (the hour
/// lines are `primary`'s wall clock): "9 PM" where the zones are whole hours apart, "9:30p" where
/// they aren't. Each line is turned into an instant and read in `secondary`, so a DST change in
/// either zone on that day shows as a jump in the labels.
pub fn secondary_hours(date: NaiveDate, primary: Tz, secondary: Tz, hour12: bool) -> Vec<String> {
    (0..24)
        .map(|h| {
            let t = at_minutes(date, (h * 60) as f32, primary)
                .with_timezone(&secondary)
                .time();
            if t.minute() == 0 {
                format_hour(t.hour(), hour12)
            } else {
                format_time_short(t, hour12)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono_tz::America::{Los_Angeles, New_York, Santiago};

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    fn utc(y: i32, m: u32, day: u32, h: u32, min: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, day, h, min, 0).unwrap()
    }

    #[test]
    fn dst_days_are_23_and_25_hours() {
        assert_eq!(day_length(d(2026, 3, 8), New_York), Duration::hours(23));
        assert_eq!(day_length(d(2026, 11, 1), New_York), Duration::hours(25));
        assert_eq!(day_length(d(2026, 11, 2), New_York), Duration::hours(24));
        assert_eq!(
            day_length(d(2026, 3, 8), chrono_tz::UTC),
            Duration::hours(24)
        );
    }

    #[test]
    fn dst_changes_are_found() {
        assert_eq!(
            dst_change(d(2026, 3, 8), New_York),
            Some(DstChange::Skipped { from: 120, to: 180 })
        );
        assert_eq!(
            dst_change(d(2026, 11, 1), Los_Angeles),
            Some(DstChange::Repeated { from: 60, to: 120 })
        );
        assert_eq!(dst_change(d(2026, 11, 2), Los_Angeles), None);
    }

    #[test]
    fn a_skipped_time_moves_forward_and_a_repeated_one_is_its_first_pass() {
        // 2:30 doesn't exist on 8 March in New York: it's 3:30 EDT, 07:30 UTC.
        let skipped = d(2026, 3, 8).and_hms_opt(2, 30, 0).unwrap();
        assert_eq!(resolve(skipped, New_York), utc(2026, 3, 8, 7, 30));
        // 1:30 happens twice on 1 November: the first is EDT, 05:30 UTC.
        let repeated = d(2026, 11, 1).and_hms_opt(1, 30, 0).unwrap();
        assert_eq!(resolve(repeated, New_York), utc(2026, 11, 1, 5, 30));
    }

    #[test]
    fn a_zone_that_skips_midnight_starts_its_day_at_one() {
        // Chile goes forward at midnight: 6 September 2026 starts at 1:00.
        let start = day_start(d(2026, 9, 6), Santiago);
        let local = start.with_timezone(&Santiago).naive_local();
        assert_eq!(local, d(2026, 9, 6).and_hms_opt(1, 0, 0).unwrap());
        assert_eq!(day_length(d(2026, 9, 6), Santiago), Duration::hours(23));
    }

    #[test]
    fn wall_minutes_on_dst_days() {
        let day = d(2026, 11, 1);
        // The second 1:30 (EST, 06:30 UTC) is at the same place as the first.
        assert_eq!(wall_minutes(utc(2026, 11, 1, 5, 30), day, New_York), 90.);
        assert_eq!(wall_minutes(utc(2026, 11, 1, 6, 30), day, New_York), 90.);
        // Before and after the day clamp to its ends.
        assert_eq!(wall_minutes(utc(2026, 10, 31, 12, 0), day, New_York), 0.);
        assert_eq!(wall_minutes(utc(2026, 11, 2, 12, 0), day, New_York), 1440.);
        // 3:00 EDT on the spring day is 180 minutes on the wall clock, two hours after 1:00.
        assert_eq!(
            wall_minutes(utc(2026, 3, 8, 7, 0), d(2026, 3, 8), New_York),
            180.
        );
    }

    #[test]
    fn at_minutes_round_trips_and_runs_over_midnight() {
        let day = d(2026, 11, 18);
        let t = at_minutes(day, 600., Los_Angeles);
        assert_eq!(wall_minutes(t, day, Los_Angeles), 600.);
        assert_eq!(
            at_minutes(day, 1440., Los_Angeles),
            day_start(d(2026, 11, 19), Los_Angeles)
        );
        assert_eq!(
            at_minutes(day, -60., Los_Angeles),
            resolve(d(2026, 11, 17).and_hms_opt(23, 0, 0).unwrap(), Los_Angeles)
        );
    }

    #[test]
    fn snapping() {
        assert_eq!(snap(7., 15.), 0.);
        assert_eq!(snap(8., 15.), 15.);
        assert_eq!(snap(52.4, 15.), 45.);
        assert_eq!(snap(53., 15.), 60.);
        assert_eq!(snap(13.4, 1.), 13.);
        assert_eq!(snap(13.4, 0.), 13.);
        assert_eq!(snap_floor(29.9, 15.), 15.);
        assert_eq!(snap_ceil(15.1, 15.), 30.);
        assert_eq!(snap_ceil(15., 15.), 15.);
        assert_eq!(snap(100., 30.), 90.);
    }

    #[test]
    fn weeks_start_on_the_chosen_day() {
        // 18 November 2026 is a Wednesday.
        assert_eq!(week_start(d(2026, 11, 18), Weekday::Mon), d(2026, 11, 16));
        assert_eq!(week_start(d(2026, 11, 18), Weekday::Sun), d(2026, 11, 15));
        assert_eq!(week_start(d(2026, 11, 15), Weekday::Sun), d(2026, 11, 15));
        assert_eq!(week_start(d(2026, 11, 15), Weekday::Mon), d(2026, 11, 9));
        assert_eq!(week_start(d(2026, 11, 18), Weekday::Sat), d(2026, 11, 14));
    }

    #[test]
    fn visible_days_per_view() {
        let a = d(2026, 11, 18);
        assert_eq!(visible_days(CalendarView::Day, a, Weekday::Mon), (a, 1));
        assert_eq!(
            visible_days(CalendarView::Week, a, Weekday::Mon),
            (d(2026, 11, 16), 7)
        );
        assert_eq!(visible_days(CalendarView::Days(3), a, Weekday::Mon), (a, 3));
        // November 2026 starts on a Sunday and ends on a Monday.
        assert_eq!(
            visible_days(CalendarView::Month, a, Weekday::Mon),
            (d(2026, 10, 26), 42)
        );
        assert_eq!(
            visible_days(CalendarView::Month, a, Weekday::Sun),
            (d(2026, 11, 1), 35)
        );
        // February 2026 is exactly four weeks from Sunday the 1st.
        assert_eq!(
            visible_days(CalendarView::Month, d(2026, 2, 10), Weekday::Sun),
            (d(2026, 2, 1), 28)
        );
    }

    #[test]
    fn a_week_with_a_dst_change_is_167_or_169_hours() {
        let (a, b) = visible_range(CalendarView::Week, d(2026, 3, 9), Weekday::Sun, New_York);
        assert_eq!(b - a, Duration::hours(167));
        let (a, b) = visible_range(CalendarView::Week, d(2026, 11, 3), Weekday::Sun, New_York);
        assert_eq!(b - a, Duration::hours(169));
        let (a, _) = visible_range(
            CalendarView::Day,
            d(2026, 11, 18),
            Weekday::Sun,
            Los_Angeles,
        );
        assert_eq!(a, utc(2026, 11, 18, 8, 0));
    }

    #[test]
    fn stepping_periods() {
        let a = d(2026, 1, 31);
        assert_eq!(step(CalendarView::Month, a, 1), d(2026, 2, 28));
        assert_eq!(step(CalendarView::Month, a, -1), d(2025, 12, 31));
        assert_eq!(step(CalendarView::Week, a, 1), d(2026, 2, 7));
        assert_eq!(step(CalendarView::Day, a, -1), d(2026, 1, 30));
        assert_eq!(step(CalendarView::Days(4), a, 1), d(2026, 2, 4));
        assert_eq!(step(CalendarView::Agenda(14), a, 1), d(2026, 2, 14));
    }

    #[test]
    fn timed_events_touch_the_days_they_cover() {
        let tz = Los_Angeles;
        // 22:00 to midnight the same day touches one day.
        let (a, b) = timed_days(utc(2026, 11, 19, 6, 0), utc(2026, 11, 19, 8, 0), tz);
        assert_eq!((a, b), (d(2026, 11, 18), d(2026, 11, 18)));
        // 22:00 to 01:00 touches two.
        let (a, b) = timed_days(utc(2026, 11, 19, 6, 0), utc(2026, 11, 19, 9, 0), tz);
        assert_eq!((a, b), (d(2026, 11, 18), d(2026, 11, 19)));
        let all_day = EventTime::all_day(d(2026, 11, 18), d(2026, 11, 21));
        assert_eq!(event_days(&all_day, tz), (d(2026, 11, 18), d(2026, 11, 20)));
        assert!(in_all_day_row(&all_day));
        assert!(!in_all_day_row(&EventTime::timed(
            utc(2026, 11, 19, 6, 0),
            utc(2026, 11, 19, 9, 0)
        )));
        assert!(in_all_day_row(&EventTime::timed(
            utc(2026, 11, 19, 6, 0),
            utc(2026, 11, 20, 6, 0)
        )));
    }

    #[test]
    fn moving_on_the_grid_snaps_the_start_and_keeps_the_length() {
        let tz = Los_Angeles;
        let day = d(2026, 11, 18);
        // 9:07 to 10:07, grabbed at 9:30 and dropped at 11:20 on the next day.
        let t = EventTime::timed(at_minutes(day, 547., tz), at_minutes(day, 607., tz));
        let moved = moved(t, (day, 570.), (d(2026, 11, 19), 680.), false, 15., tz);
        let EventTime::Timed { start, end } = moved else {
            panic!()
        };
        assert_eq!(local(start, tz), (d(2026, 11, 19), 660.)); // 9:07 + 1:50 = 10:57 → 11:00
        assert_eq!(end - start, Duration::hours(1));
        // In the month view it moves by days and keeps 9:07.
        let EventTime::Timed { start, .. } = moved_by_days(t, day, d(2026, 11, 21), tz) else {
            panic!()
        };
        assert_eq!(local(start, tz), (d(2026, 11, 21), 547.));
    }

    fn moved_by_days(t: EventTime, from: NaiveDate, to: NaiveDate, tz: Tz) -> EventTime {
        moved(t, (from, 0.), (to, 0.), true, 15., tz)
    }

    #[test]
    fn moving_across_the_dst_change_keeps_real_length() {
        let tz = New_York;
        // A two-hour event on Saturday 0:00–2:00 moved to Sunday 1:00 (the clocks skip 2:00).
        let sat = d(2026, 3, 7);
        let t = EventTime::timed(at_minutes(sat, 0., tz), at_minutes(sat, 120., tz));
        let EventTime::Timed { start, end } =
            moved(t, (sat, 0.), (d(2026, 3, 8), 60.), false, 15., tz)
        else {
            panic!()
        };
        assert_eq!(end - start, Duration::hours(2));
        // It ends at 4:00 on the wall clock, three wall hours after it starts.
        assert_eq!(local(end, tz), (d(2026, 3, 8), 240.));
    }

    #[test]
    fn all_day_events_move_and_resize_by_days() {
        let t = EventTime::all_day(d(2026, 11, 16), d(2026, 11, 18));
        assert_eq!(
            moved(
                t,
                (d(2026, 11, 16), 0.),
                (d(2026, 11, 19), 0.),
                true,
                15.,
                Los_Angeles
            ),
            EventTime::all_day(d(2026, 11, 19), d(2026, 11, 21))
        );
        assert_eq!(
            resized(t, (d(2026, 11, 20), 0.), 15., Los_Angeles),
            EventTime::all_day(d(2026, 11, 16), d(2026, 11, 21))
        );
        // Never shorter than its first day.
        assert_eq!(
            resized(t, (d(2026, 11, 10), 0.), 15., Los_Angeles),
            EventTime::all_day(d(2026, 11, 16), d(2026, 11, 17))
        );
    }

    #[test]
    fn resizing_snaps_and_keeps_one_step() {
        let tz = Los_Angeles;
        let day = d(2026, 11, 18);
        let t = EventTime::timed(at_minutes(day, 540., tz), at_minutes(day, 600., tz));
        let EventTime::Timed { end, .. } = resized(t, (day, 652.), 15., tz) else {
            panic!()
        };
        assert_eq!(local(end, tz), (day, 645.));
        let EventTime::Timed { start, end } = resized(t, (day, 500.), 15., tz) else {
            panic!()
        };
        assert_eq!(end - start, Duration::minutes(15));
    }

    #[test]
    fn creating_rounds_outward_in_either_direction() {
        let tz = Los_Angeles;
        let day = d(2026, 11, 18);
        let span = |t: EventTime| match t {
            EventTime::Timed { start, end } => (local(start, tz).1, local(end, tz).1),
            _ => panic!(),
        };
        assert_eq!(span(created(day, 547., 610., 15., tz)), (540., 615.));
        assert_eq!(span(created(day, 610., 547., 15., tz)), (540., 615.));
        // A press without a drag makes one step.
        assert_eq!(span(created(day, 547., 547., 15., tz)), (540., 555.));
        // Never past the end of the day.
        let end = created(day, 1430., 1440., 15., tz);
        let EventTime::Timed { end, .. } = end else {
            panic!()
        };
        assert_eq!(end, day_start(d(2026, 11, 19), tz));
        assert_eq!(
            created_days(d(2026, 11, 20), d(2026, 11, 18)),
            EventTime::all_day(d(2026, 11, 18), d(2026, 11, 21))
        );
    }

    #[test]
    fn words_for_times() {
        let t = |h, m| NaiveTime::from_hms_opt(h, m, 0).unwrap();
        assert_eq!(format_hour(0, true), "12 AM");
        assert_eq!(format_hour(12, true), "12 PM");
        assert_eq!(format_hour(15, true), "3 PM");
        assert_eq!(format_hour(9, false), "09:00");
        assert_eq!(format_time(t(9, 30), true), "9:30 AM");
        assert_eq!(format_time(t(13, 0), true), "1 PM");
        assert_eq!(format_time(t(13, 5), false), "13:05");
        assert_eq!(format_time_short(t(9, 30), true), "9:30a");
        assert_eq!(format_range(t(9, 0), t(10, 30), true), "9 – 10:30 AM");
        assert_eq!(format_range(t(11, 0), t(13, 0), true), "11 AM – 1 PM");
        assert_eq!(format_range(t(9, 0), t(10, 30), false), "09:00 – 10:30");
    }

    #[test]
    fn titles() {
        let a = d(2026, 11, 18);
        assert_eq!(title(CalendarView::Month, a, Weekday::Mon), "November 2026");
        assert_eq!(title(CalendarView::Week, a, Weekday::Mon), "November 2026");
        assert_eq!(
            title(CalendarView::Day, d(2026, 11, 14), Weekday::Mon),
            "Saturday, November 14, 2026"
        );
        assert_eq!(
            title(CalendarView::Week, d(2026, 12, 1), Weekday::Mon),
            "Nov – Dec 2026"
        );
        assert_eq!(
            title(CalendarView::Week, d(2026, 12, 30), Weekday::Mon),
            "Dec 2026 – Jan 2027"
        );
        assert_eq!(
            title(CalendarView::Agenda(14), a, Weekday::Mon),
            "Nov 18 – Dec 1, 2026"
        );
        assert_eq!(
            title(CalendarView::Agenda(7), d(2026, 12, 28), Weekday::Mon),
            "Dec 28, 2026 – Jan 3, 2027"
        );
        assert_eq!(
            title(CalendarView::Agenda(1), a, Weekday::Mon),
            "Nov 18, 2026"
        );
        assert_eq!(
            title(CalendarView::Agenda(7), d(2026, 11, 2), Weekday::Mon),
            "Nov 2 – 8, 2026"
        );
        let short = |v, d| short_title(v, d, Weekday::Mon);
        assert_eq!(short(CalendarView::Day, a), "Wed, Nov 18");
        assert_eq!(short(CalendarView::Month, a), "Nov 2026");
        assert_eq!(short(CalendarView::Week, a), "Nov 2026");
        assert_eq!(short(CalendarView::Week, d(2026, 12, 1)), "Nov – Dec 2026");
        assert_eq!(
            short(CalendarView::Week, d(2026, 12, 30)),
            "Dec 26 – Jan 27"
        );
        assert_eq!(short(CalendarView::Agenda(14), a), "Nov 18 – Dec 1");
        assert_eq!(short(CalendarView::Agenda(7), d(2026, 11, 2)), "Nov 2 – 8");
    }

    #[test]
    fn a_second_zones_hours() {
        // Los Angeles and New York: three whole hours apart.
        let la = secondary_hours(d(2026, 11, 18), Los_Angeles, New_York, true);
        assert_eq!(la[9], "12 PM");
        assert_eq!(la[23], "2 AM");
        // India is half an hour off the hour.
        let india = secondary_hours(d(2026, 11, 18), Los_Angeles, chrono_tz::Asia::Kolkata, true);
        assert_eq!(india[0], "1:30p");
        assert_eq!(india[9], "10:30p");
        let india24 = secondary_hours(
            d(2026, 11, 18),
            Los_Angeles,
            chrono_tz::Asia::Kolkata,
            false,
        );
        assert_eq!(india24[9], "22:30");
        // On the day the clocks go back in Los Angeles, 1 AM happens twice: the 2 AM line is
        // an hour later than it would be, and India's labels jump with it.
        let fall = secondary_hours(d(2026, 11, 1), Los_Angeles, chrono_tz::Asia::Kolkata, true);
        assert_eq!(fall[1], "1:30p");
        assert_eq!(fall[2], "3:30p");
        // London's own change, seen from New York: its hours keep the gap until it moves.
        let london = secondary_hours(d(2026, 10, 25), New_York, chrono_tz::Europe::London, true);
        assert_eq!(london[0], "4 AM");
        assert_eq!(london[12], "4 PM");
    }

    #[test]
    fn zone_labels() {
        assert_eq!(zone_label(d(2026, 11, 18), Los_Angeles), "PST");
        assert_eq!(zone_label(d(2026, 7, 1), Los_Angeles), "PDT");
        assert_eq!(zone_label(d(2026, 7, 1), chrono_tz::Asia::Kolkata), "IST");
        assert_eq!(zone_label(d(2026, 7, 1), chrono_tz::Asia::Dubai), "GMT+4");
    }
}
