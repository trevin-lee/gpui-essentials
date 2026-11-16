//! What cells and group headers say: numbers in their format, dates and times in the viewer's
//! zone, and groups' names. Nothing here draws.

use crate::engine::time::{self, DAY_MS, civil_from_days};
use crate::engine::{DateGroup, GroupKey, Relative};
use crate::model::{DateValue, NumberFormat, PropertyConfig};

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

const MONTHS_LONG: [&str; 12] = [
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

/// A day since 1970-01-01 as "Nov 15, 2026".
pub fn day(days: i64) -> String {
    let (y, m, d) = civil_from_days(days);
    format!("{} {d}, {y}", MONTHS[(m as usize - 1) % 12])
}

/// A month's name and year: "November 2026".
pub fn month(year: i64, month: u32) -> String {
    format!(
        "{} {year}",
        MONTHS_LONG[(month as usize).saturating_sub(1) % 12]
    )
}

/// The time of day of an instant, `offset_ms` east of UTC: "3:05 PM".
pub fn time_of_day(ms: i64, offset_ms: i64) -> String {
    let local = ms.saturating_add(offset_ms);
    let minutes = local.rem_euclid(DAY_MS) / 60_000;
    let (h, m) = (minutes / 60, minutes % 60);
    let (h12, half) = match h {
        0 => (12, "AM"),
        1..=11 => (h, "AM"),
        12 => (12, "PM"),
        _ => (h - 12, "PM"),
    };
    format!("{h12}:{m:02} {half}")
}

/// An instant as "Nov 15, 2026 3:05 PM" in the viewer's zone.
pub fn instant(ms: i64, offset_ms: i64) -> String {
    format!(
        "{} {}",
        day(time::day_of_instant(ms, offset_ms)),
        time_of_day(ms, offset_ms)
    )
}

/// A date value as a cell shows it: "Nov 15, 2026", with its time when it has one, and a range
/// as "Nov 15, 2026 → Nov 17, 2026".
pub fn date(d: &DateValue, offset_ms: i64) -> String {
    let one = |ms: i64| {
        if d.include_time {
            instant(ms, offset_ms)
        } else {
            day(ms.div_euclid(DAY_MS))
        }
    };
    match d.end {
        Some(end) if end != d.start => format!("{} → {}", one(d.start), one(end)),
        _ => one(d.start),
    }
}

/// Digits grouped in threes: "1234567.5" as "1,234,567.5".
fn grouped(s: &str) -> String {
    let (sign, rest) = s.strip_prefix('-').map_or(("", s), |r| ("-", r));
    let (int, frac) = rest
        .split_once('.')
        .map_or((rest, None), |(i, f)| (i, Some(f)));
    let mut out = String::with_capacity(s.len() + int.len() / 3);
    out.push_str(sign);
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if let Some(f) = frac {
        out.push('.');
        out.push_str(f);
    }
    out
}

fn currency_symbol(code: &str) -> Option<&'static str> {
    Some(match code {
        "USD" | "AUD" | "CAD" | "NZD" | "SGD" | "HKD" | "MXN" => "$",
        "EUR" => "€",
        "GBP" => "£",
        "JPY" | "CNY" => "¥",
        "INR" => "₹",
        "KRW" => "₩",
        "CHF" => "CHF ",
        _ => return None,
    })
}

/// A number in its property's format: grouped digits at its precision, a percent (0.25 as
/// "25%"), or a currency ("$1,200"). Very large and very small numbers fall back to Rust's own
/// shortest form, so no cell holds a hundred digits.
pub fn number(x: f64, config: &PropertyConfig) -> String {
    let precision = config.precision.map(|p| p.min(10) as usize);
    let (value, suffix) = match config.format {
        Some(NumberFormat::Percent) => (x * 100., "%"),
        _ => (x, ""),
    };
    let body = if value.abs() >= 1e15 || (value != 0. && value.abs() < 1e-9) {
        format!("{value:e}")
    } else {
        match precision {
            Some(p) => grouped(&format!("{value:.p$}")),
            None => grouped(&trimmed(value)),
        }
    };
    match (&config.format, config.currency.as_deref()) {
        (Some(NumberFormat::Currency), Some(code)) => {
            let (sign, digits) = body
                .strip_prefix('-')
                .map_or(("", body.as_str()), |d| ("-", d));
            match currency_symbol(code) {
                Some(sym) => format!("{sign}{sym}{digits}"),
                None => format!("{sign}{code} {digits}"),
            }
        }
        _ => format!("{body}{suffix}"),
    }
}

/// A number with as few digits as show it (at most six after the point).
fn trimmed(x: f64) -> String {
    let s = format!("{x:.6}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

/// A number as someone types it, for a text field: no grouping, no symbol, a percent as its
/// percentage.
pub fn number_for_editing(x: f64, config: &PropertyConfig) -> String {
    match config.format {
        Some(NumberFormat::Percent) => trimmed(x * 100.),
        _ => trimmed(x),
    }
}

/// Reads a typed number: digits with or without grouping commas, a currency symbol or code, a
/// percent sign. For a percent property, "25" and "25%" are both 0.25. None for what isn't a
/// number.
pub fn parse_number(text: &str, config: &PropertyConfig) -> Option<f64> {
    let mut s: String = text
        .trim()
        .chars()
        .filter(|c| !matches!(c, ',' | ' ' | '_' | '$' | '€' | '£' | '¥' | '₹' | '₩'))
        .collect();
    if let Some(code) = config.currency.as_deref() {
        s = s.replace(code, "");
    }
    let percent = s.ends_with('%');
    let s = s.trim_end_matches('%');
    let x: f64 = s.parse().ok().filter(|x: &f64| x.is_finite())?;
    if matches!(config.format, Some(NumberFormat::Percent)) || percent {
        Some(x / 100.)
    } else {
        Some(x)
    }
}

/// A relative bucket's name.
pub fn relative(r: Relative) -> &'static str {
    match r {
        Relative::Older => "Older",
        Relative::Past30 => "Past 30 days",
        Relative::Past7 => "Past 7 days",
        Relative::Yesterday => "Yesterday",
        Relative::Today => "Today",
        Relative::Tomorrow => "Tomorrow",
        Relative::Next7 => "Next 7 days",
        Relative::Next30 => "Next 30 days",
        Relative::Later => "Later",
    }
}

/// A date group's name: "Today", "Nov 15, 2026", "Week of Nov 9, 2026", "November 2026",
/// "2026".
pub fn date_group(g: &DateGroup) -> String {
    match g {
        DateGroup::Relative(r) => relative(*r).into(),
        DateGroup::Day(d) => day(*d),
        DateGroup::Week(d) => format!("Week of {}", day(*d)),
        DateGroup::Month { year, month: m } => month(*year, *m),
        DateGroup::Year(y) => y.to_string(),
    }
}

/// The day a new row in a date group gets: the group's day, or its first day; None for a
/// relative group that isn't one day.
pub fn group_day(g: &DateGroup, today: i64) -> Option<i64> {
    match g {
        DateGroup::Relative(Relative::Today) => Some(today),
        DateGroup::Relative(Relative::Yesterday) => Some(today - 1),
        DateGroup::Relative(Relative::Tomorrow) => Some(today + 1),
        DateGroup::Relative(_) => None,
        DateGroup::Day(d) | DateGroup::Week(d) => Some(*d),
        DateGroup::Month { year, month } => Some(time::days_from_civil(*year, *month, 1)),
        DateGroup::Year(y) => Some(time::days_from_civil(*y, 1, 1)),
    }
}

/// Whether a group key is the empty group.
pub fn is_empty_group(key: &GroupKey) -> bool {
    matches!(key, GroupKey::Empty)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(
        format: Option<NumberFormat>,
        currency: Option<&str>,
        precision: Option<u8>,
    ) -> PropertyConfig {
        PropertyConfig {
            format,
            currency: currency.map(Into::into),
            precision,
            ..PropertyConfig::default()
        }
    }

    #[test]
    fn numbers() {
        assert_eq!(number(1234567.5, &cfg(None, None, None)), "1,234,567.5");
        assert_eq!(
            number(3., &cfg(Some(NumberFormat::Plain), None, Some(0))),
            "3"
        );
        assert_eq!(number(-1234.567, &cfg(None, None, Some(2))), "-1,234.57");
        assert_eq!(
            number(0.25, &cfg(Some(NumberFormat::Percent), None, None)),
            "25%"
        );
        assert_eq!(
            number(
                240000.,
                &cfg(Some(NumberFormat::Currency), Some("USD"), Some(0))
            ),
            "$240,000"
        );
        assert_eq!(
            number(-5., &cfg(Some(NumberFormat::Currency), Some("XYZ"), None)),
            "-XYZ 5"
        );
        assert_eq!(number(1e300, &cfg(None, None, Some(2))), "1e300");
        assert_eq!(number(0.1 + 0.2, &cfg(None, None, None)), "0.3");
        assert_eq!(number(-0., &cfg(None, None, None)), "0");
    }

    #[test]
    fn typed_numbers() {
        let plain = cfg(None, None, None);
        assert_eq!(parse_number(" 1,234.5 ", &plain), Some(1234.5));
        assert_eq!(parse_number("$240,000", &plain), Some(240000.));
        assert_eq!(parse_number("12%", &plain), Some(0.12));
        assert_eq!(parse_number("abc", &plain), None);
        assert_eq!(parse_number("inf", &plain), None);
        let pct = cfg(Some(NumberFormat::Percent), None, None);
        assert_eq!(parse_number("25", &pct), Some(0.25));
        assert_eq!(number_for_editing(0.25, &pct), "25");
        let chf = cfg(Some(NumberFormat::Currency), Some("CHF"), None);
        assert_eq!(parse_number("CHF 12", &chf), Some(12.));
    }

    #[test]
    fn dates_and_times() {
        let nov15 = time::days_from_civil(2026, 11, 15);
        assert_eq!(day(nov15), "Nov 15, 2026");
        let d = DateValue {
            start: nov15 * DAY_MS,
            end: Some((nov15 + 2) * DAY_MS),
            include_time: false,
        };
        assert_eq!(date(&d, -8 * 3_600_000), "Nov 15, 2026 → Nov 17, 2026");
        let t = DateValue {
            start: nov15 * DAY_MS + 15 * 3_600_000 + 5 * 60_000,
            end: None,
            include_time: true,
        };
        assert_eq!(date(&t, 0), "Nov 15, 2026 3:05 PM");
        assert_eq!(date(&t, -16 * 3_600_000), "Nov 14, 2026 11:05 PM");
        assert_eq!(time_of_day(0, 0), "12:00 AM");
        assert_eq!(time_of_day(12 * 3_600_000, 0), "12:00 PM");
        assert_eq!(
            date_group(&DateGroup::Month {
                year: 2026,
                month: 11
            }),
            "November 2026"
        );
        assert_eq!(
            date_group(&DateGroup::Week(nov15 - 6)),
            "Week of Nov 9, 2026"
        );
        assert_eq!(
            group_day(&DateGroup::Relative(Relative::Tomorrow), nov15),
            Some(nov15 + 1)
        );
        assert_eq!(
            group_day(&DateGroup::Relative(Relative::Later), nov15),
            None
        );
        // Extremes don't panic.
        let _ = date(
            &DateValue {
                start: i64::MAX,
                end: Some(i64::MIN),
                include_time: true,
            },
            i64::from(i32::MAX),
        );
    }
}
