//! Calendar days without a date library: days since 1970-01-01, civil dates, and the day a date
//! value or instant falls on in the viewer's zone. Howard Hinnant's algorithms
//! (<https://howardhinnant.github.io/date_algorithms.html>), exact for every day an i64 holds.

use crate::model::DateValue;

/// Milliseconds in a day.
pub const DAY_MS: i64 = 86_400_000;

/// Days since 1970-01-01 of a civil date (proleptic Gregorian). `month` is 1–12, `day` 1–31.
pub fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let (m, d) = (i64::from(month), i64::from(day));
    let y = if m <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The civil date (year, month 1–12, day 1–31) of a day since 1970-01-01.
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days.saturating_add(719_468);
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// The weekday of a day since 1970-01-01: 0 for Monday through 6 for Sunday.
pub fn weekday(days: i64) -> u32 {
    // 1970-01-01 was a Thursday.
    (days + 3).rem_euclid(7) as u32
}

/// The day an instant falls on, `offset_ms` east of UTC.
pub fn day_of_instant(ms: i64, offset_ms: i64) -> i64 {
    ms.saturating_add(offset_ms).div_euclid(DAY_MS)
}

/// The days a date value covers, first and last: an instant's day in the viewer's zone, or a
/// date's own day (it's stored as midnight UTC) for one without a time.
pub fn days_of(d: &DateValue, offset_ms: i64) -> (i64, i64) {
    let day = |ms: i64| {
        if d.include_time {
            day_of_instant(ms, offset_ms)
        } else {
            ms.div_euclid(DAY_MS)
        }
    };
    let first = day(d.start);
    let last = d.end.map_or(first, day).max(first);
    (first, last)
}

/// The same day `months` months on, clamped to the end of a shorter month (Jan 31 + 1 month is
/// Feb 28 or 29).
pub fn add_months(days: i64, months: i64) -> i64 {
    let (y, m, d) = civil_from_days(days);
    let total = y * 12 + i64::from(m) - 1 + months;
    let (ny, nm) = (total.div_euclid(12), (total.rem_euclid(12) + 1) as u32);
    let last = days_in_month(ny, nm);
    days_from_civil(ny, nm, d.min(last))
}

pub fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => 31,
    }
}

/// Reads `YYYY-MM-DD` as days since 1970-01-01.
pub fn parse_ymd(s: &str) -> Option<i64> {
    let mut parts = s.splitn(3, '-');
    let (y, m, d) = (parts.next()?, parts.next()?, parts.next()?);
    if y.len() != 4 || m.len() != 2 || d.len() != 2 {
        return None;
    }
    let (y, m, d): (i64, u32, u32) = (y.parse().ok()?, m.parse().ok()?, d.parse().ok()?);
    if !(1..=12).contains(&m) || d == 0 || d > days_in_month(y, m) {
        return None;
    }
    Some(days_from_civil(y, m, d))
}

/// Writes a day since 1970-01-01 as `YYYY-MM-DD`.
pub fn format_ymd(days: i64) -> String {
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates_round_trip() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
        for d in (-800_000..800_000).step_by(37) {
            let (y, m, dd) = civil_from_days(d);
            assert_eq!(days_from_civil(y, m, dd), d);
        }
        assert_eq!(weekday(days_from_civil(2026, 11, 15)), 6, "a Sunday");
        assert_eq!(weekday(days_from_civil(2026, 11, 16)), 0, "a Monday");
    }

    #[test]
    fn months_clamp_and_dates_parse() {
        let d = |y, m, dd| days_from_civil(y, m, dd);
        assert_eq!(add_months(d(2026, 1, 31), 1), d(2026, 2, 28));
        assert_eq!(add_months(d(2028, 1, 31), 1), d(2028, 2, 29));
        assert_eq!(add_months(d(2026, 3, 31), -1), d(2026, 2, 28));
        assert_eq!(add_months(d(2026, 11, 15), 12), d(2027, 11, 15));
        assert_eq!(add_months(d(2026, 1, 15), -13), d(2024, 12, 15));
        assert_eq!(parse_ymd("2026-11-15"), Some(d(2026, 11, 15)));
        assert_eq!(parse_ymd("2024-02-29"), Some(d(2024, 2, 29)));
        for bad in [
            "2026-02-29",
            "2026-13-01",
            "2026-1-01",
            "26-01-01",
            "2026-01-00",
            "x",
            "",
        ] {
            assert_eq!(parse_ymd(bad), None, "{bad}");
        }
        assert_eq!(format_ymd(d(2026, 3, 7)), "2026-03-07");
    }

    #[test]
    fn days_of_dates_and_instants() {
        let nov15 = days_from_civil(2026, 11, 15);
        let late = DateValue {
            start: nov15 * DAY_MS - 1_800_000,
            end: None,
            include_time: true,
        };
        assert_eq!(days_of(&late, 0), (nov15 - 1, nov15 - 1));
        assert_eq!(days_of(&late, 3_600_000), (nov15, nov15));
        let date = DateValue {
            start: nov15 * DAY_MS,
            end: Some((nov15 + 2) * DAY_MS),
            include_time: false,
        };
        // A date without a time is the same day in every zone.
        assert_eq!(days_of(&date, -10 * 3_600_000), (nov15, nov15 + 2));
        // Extremes don't overflow.
        let far = DateValue {
            start: i64::MAX,
            end: Some(i64::MIN),
            include_time: true,
        };
        let _ = days_of(&far, i64::from(i32::MAX) * 60_000);
        let _ = civil_from_days(i64::MAX / DAY_MS);
    }
}
