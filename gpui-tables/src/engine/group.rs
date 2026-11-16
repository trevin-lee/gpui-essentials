//! Grouping rows: a board's columns and a table's or list's sections.
//!
//! - **Select:** a group per option, in the options' order, every one even when empty (a board
//!   needs them as drop targets), after an empty group for rows without one.
//! - **Multi-select:** the same, with a row in the group of each of its options.
//! - **Checkbox:** unchecked, then checked.
//! - **Person:** the empty group, then the owner, then agents by name; a row is in each of its
//!   people's groups. Only people some row has get a group (the empty one is always there).
//! - **Date, created time, edited time:** buckets relative to today (Notion's default), or
//!   calendar days, weeks, months or years, oldest first, then the empty group last. Only
//!   buckets some row falls in get a group. A range goes by its first day.
//!
//! Rows keep the order they're given in within each group. Groups have stable string keys
//! ([`GroupKey`]), which a view's `collapsedGroups` holds.

use std::collections::{BTreeMap, HashMap};
use std::fmt;

use super::time::{self, days_of};
use super::{Cell, Context, Field};
use crate::model::{DateBucket, Doc, Person, Property, PropertyType, Value};

/// A bucket relative to today.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Relative {
    /// More than 30 days ago.
    Older,
    /// 8 to 30 days ago.
    Past30,
    /// 2 to 7 days ago.
    Past7,
    Yesterday,
    Today,
    Tomorrow,
    /// 2 to 7 days on.
    Next7,
    /// 8 to 30 days on.
    Next30,
    /// More than 30 days on.
    Later,
}

impl Relative {
    pub const ALL: [Relative; 9] = [
        Self::Older,
        Self::Past30,
        Self::Past7,
        Self::Yesterday,
        Self::Today,
        Self::Tomorrow,
        Self::Next7,
        Self::Next30,
        Self::Later,
    ];

    /// The bucket of a day, `diff` days from today.
    pub fn of(diff: i64) -> Self {
        match diff {
            ..-30 => Self::Older,
            -30..=-8 => Self::Past30,
            -7..=-2 => Self::Past7,
            -1 => Self::Yesterday,
            0 => Self::Today,
            1 => Self::Tomorrow,
            2..=7 => Self::Next7,
            8..=30 => Self::Next30,
            _ => Self::Later,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Older => "older",
            Self::Past30 => "past30",
            Self::Past7 => "past7",
            Self::Yesterday => "yesterday",
            Self::Today => "today",
            Self::Tomorrow => "tomorrow",
            Self::Next7 => "next7",
            Self::Next30 => "next30",
            Self::Later => "later",
        }
    }
}

/// A date group.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DateGroup {
    Relative(Relative),
    /// Days since 1970-01-01.
    Day(i64),
    /// The week's first day, as days since 1970-01-01.
    Week(i64),
    Month {
        year: i64,
        month: u32,
    },
    Year(i64),
}

/// Which group a row is in. Written as a string for `collapsedGroups`: `empty`, `option:<id>`,
/// `checked`, `unchecked`, `owner`, `agent:<id>`, `relative:today`, `day:2026-11-15`,
/// `week:2026-11-09`, `month:2026-11`, `year:2026`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GroupKey {
    /// No value: "No status", "No date".
    Empty,
    /// A select's or multi-select's option, by id.
    Option(String),
    Checked(bool),
    Person(Person),
    Date(DateGroup),
}

impl fmt::Display for GroupKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("empty"),
            Self::Option(id) => write!(f, "option:{id}"),
            Self::Checked(true) => f.write_str("checked"),
            Self::Checked(false) => f.write_str("unchecked"),
            Self::Person(p) => f.write_str(&p.key().unwrap_or_default()),
            Self::Date(DateGroup::Relative(r)) => write!(f, "relative:{}", r.as_str()),
            Self::Date(DateGroup::Day(d)) => write!(f, "day:{}", time::format_ymd(*d)),
            Self::Date(DateGroup::Week(d)) => write!(f, "week:{}", time::format_ymd(*d)),
            Self::Date(DateGroup::Month { year, month }) => write!(f, "month:{year:04}-{month:02}"),
            Self::Date(DateGroup::Year(y)) => write!(f, "year:{y:04}"),
        }
    }
}

impl GroupKey {
    /// Reads the string [`Display`](fmt::Display) writes; None for one it can't read.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "empty" => Self::Empty,
            "checked" => Self::Checked(true),
            "unchecked" => Self::Checked(false),
            "owner" => Self::Person(Person::Owner),
            _ => {
                let (kind, rest) = s.split_once(':')?;
                match kind {
                    "option" => Self::Option(rest.to_owned()),
                    "agent" => Self::Person(Person::Agent(rest.to_owned())),
                    "relative" => Self::Date(DateGroup::Relative(
                        *Relative::ALL.iter().find(|r| r.as_str() == rest)?,
                    )),
                    "day" => Self::Date(DateGroup::Day(time::parse_ymd(rest)?)),
                    "week" => Self::Date(DateGroup::Week(time::parse_ymd(rest)?)),
                    "month" => {
                        let (y, m) = rest.split_once('-')?;
                        let (year, month) = (y.parse().ok()?, m.parse().ok()?);
                        if !(1..=12).contains(&month) {
                            return None;
                        }
                        Self::Date(DateGroup::Month { year, month })
                    }
                    "year" => Self::Date(DateGroup::Year(rest.parse().ok()?)),
                    _ => return None,
                }
            }
        })
    }
}

/// One group: its key and its rows (indexes into the rows given), in order.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub key: GroupKey,
    pub rows: Vec<usize>,
}

impl Group {
    fn new(key: GroupKey) -> Self {
        Self {
            key,
            rows: Vec::new(),
        }
    }
}

/// Groups `indexes` (into `rows`, in the view's order) by `p`. None when `p`'s type can't group.
pub fn group(
    p: &Property,
    bucket: &DateBucket,
    rows: &[Doc],
    indexes: &[usize],
    cx: &Context,
) -> Option<Vec<Group>> {
    let field = Field::of(p);
    match &p.kind {
        PropertyType::Select | PropertyType::MultiSelect => {
            let mut groups = vec![Group::new(GroupKey::Empty)];
            groups.extend(
                p.options()
                    .iter()
                    .map(|o| Group::new(GroupKey::Option(o.id.clone()))),
            );
            let place: HashMap<&str, usize> = p
                .options()
                .iter()
                .enumerate()
                .map(|(i, o)| (o.id.as_str(), i + 1))
                .collect();
            let mut seen = Vec::new();
            for &i in indexes {
                seen.clear();
                match Cell::read(&rows[i], field) {
                    Cell::Value(Value::Text(id)) if p.kind == PropertyType::Select => {
                        seen.extend(place.get(id.as_str()).copied());
                    }
                    Cell::Value(Value::Ids(ids)) if p.kind == PropertyType::MultiSelect => {
                        for id in ids {
                            if let Some(&g) = place.get(id.as_str())
                                && !seen.contains(&g)
                            {
                                seen.push(g);
                            }
                        }
                    }
                    _ => {}
                }
                if seen.is_empty() {
                    groups[0].rows.push(i);
                }
                for &g in &seen {
                    groups[g].rows.push(i);
                }
            }
            Some(groups)
        }
        PropertyType::Checkbox => {
            let mut groups = vec![
                Group::new(GroupKey::Checked(false)),
                Group::new(GroupKey::Checked(true)),
            ];
            for &i in indexes {
                let checked = matches!(Cell::read(&rows[i], field), Cell::Value(Value::Bool(true)));
                groups[usize::from(checked)].rows.push(i);
            }
            Some(groups)
        }
        PropertyType::Person => {
            let mut empty = Group::new(GroupKey::Empty);
            // By key, so each person gets one group; ordered after.
            let mut by_person: HashMap<String, Group> = HashMap::new();
            for &i in indexes {
                let mut any = false;
                if let Cell::Value(Value::People(people)) = Cell::read(&rows[i], field) {
                    for person in people {
                        let Some(key) = person.key() else { continue };
                        let g = by_person
                            .entry(key)
                            .or_insert_with(|| Group::new(GroupKey::Person(person.clone())));
                        if g.rows.last() != Some(&i) {
                            g.rows.push(i);
                        }
                        any = true;
                    }
                }
                if !any {
                    empty.rows.push(i);
                }
            }
            let mut people: Vec<(bool, String, String, Group)> = by_person
                .into_iter()
                .map(|(key, g)| {
                    let GroupKey::Person(p) = &g.key else {
                        unreachable!()
                    };
                    let is_agent = matches!(p, Person::Agent(_));
                    let name = super::sort::person_label(p, cx)
                        .map(|n| super::sort::fold(&n))
                        .unwrap_or_default();
                    (is_agent, name, key, g)
                })
                .collect();
            people.sort_by(|a, b| (a.0, &a.1, &a.2).cmp(&(b.0, &b.1, &b.2)));
            let mut groups = vec![empty];
            groups.extend(people.into_iter().map(|(.., g)| g));
            Some(groups)
        }
        k if k.is_date() => {
            let today = cx.today();
            let offset = cx.offset_ms();
            let week_start = if cx.week_starts_monday { 0 } else { 6 };
            let mut buckets: BTreeMap<DateGroup, Group> = BTreeMap::new();
            let mut empty = Group::new(GroupKey::Empty);
            for &i in indexes {
                let day = match Cell::read(&rows[i], field) {
                    Cell::Time(ms) => Some(time::day_of_instant(ms, offset)),
                    Cell::Value(Value::Date(d)) => Some(days_of(d, offset).0),
                    _ => None,
                };
                let Some(day) = day else {
                    empty.rows.push(i);
                    continue;
                };
                let key = match bucket {
                    DateBucket::Day => DateGroup::Day(day),
                    DateBucket::Week => {
                        let back = (i64::from(time::weekday(day)) - week_start).rem_euclid(7);
                        DateGroup::Week(day - back)
                    }
                    DateBucket::Month => {
                        let (year, month, _) = time::civil_from_days(day);
                        DateGroup::Month { year, month }
                    }
                    DateBucket::Year => DateGroup::Year(time::civil_from_days(day).0),
                    DateBucket::Relative | DateBucket::Other(_) => {
                        DateGroup::Relative(Relative::of(day.saturating_sub(today)))
                    }
                };
                buckets
                    .entry(key)
                    .or_insert_with(|| Group::new(GroupKey::Date(key)))
                    .rows
                    .push(i);
            }
            let mut groups: Vec<Group> = buckets.into_values().collect();
            if !empty.rows.is_empty() || groups.is_empty() {
                groups.push(empty);
            }
            Some(groups)
        }
        _ => None,
    }
}
