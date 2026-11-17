//! Grouping rows: a board's columns and a table's or list's sections.
//!
//! - **Select:** a group per option, in the options' order, every one even when empty (a board
//!   needs them as drop targets), after an empty group for rows without one.
//! - **Multi-select:** the same, with a row in the group of each of its options.
//! - **Checkbox:** unchecked, then checked.
//! - **Person:** the empty group, then the owner, then agents by name; a row is in each of its
//!   people's groups. Only people some row has get a group (the empty one is always there).
//! - **Created by, edited by:** the same, each row in the one group of its doc's author (or
//!   last editor), or the empty group when the doc has none.
//! - **Date, created time, edited time:** buckets relative to today (Notion's default), or
//!   calendar days, weeks, months or years, oldest first, then the empty group last. Only
//!   buckets some row falls in get a group. A range goes by its first day.
//!
//! Rows keep the order they're given in within each group. Groups have stable string keys
//! ([`GroupKey`]), which a view's `collapsedGroups` holds.

use std::collections::HashMap;
use std::fmt;
use std::hash::{BuildHasherDefault, Hasher};

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

/// A hash for the few short keys grouping looks up once a row (option ids, people, date
/// buckets): rustc's FxHash, a multiply and a rotate a word. Several times faster than the
/// standard library's SipHash on these keys, and the keys are the host's own data, so there's
/// no one to collide them on purpose.
#[derive(Default)]
struct FastHasher(u64);

impl FastHasher {
    fn add(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }
}

impl Hasher for FastHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        let (words, rest) = bytes.as_chunks::<8>();
        for w in words {
            self.add(u64::from_le_bytes(*w));
        }
        if !rest.is_empty() {
            let mut last = [0; 8];
            last[..rest.len()].copy_from_slice(rest);
            self.add(u64::from_le_bytes(last));
        }
    }

    fn write_u8(&mut self, n: u8) {
        self.add(n.into());
    }

    fn write_u32(&mut self, n: u32) {
        self.add(n.into());
    }

    fn write_u64(&mut self, n: u64) {
        self.add(n);
    }

    fn write_i64(&mut self, n: i64) {
        self.add(n as u64);
    }

    fn write_usize(&mut self, n: usize) {
        self.add(n as u64);
    }
}

type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<FastHasher>>;

/// A person's group, found without building their key: the owner, or an agent by id.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum PersonId<'a> {
    Owner,
    Agent(&'a str),
}

impl<'a> PersonId<'a> {
    /// None for a kind of person this version doesn't know (they group as no one).
    fn of(person: &'a Person) -> Option<Self> {
        match person {
            Person::Owner => Some(Self::Owner),
            Person::Agent(id) => Some(Self::Agent(id)),
            Person::Other(_) => None,
        }
    }
}

/// Groups `indexes` (into `rows`, in the view's order) by `p`. None when `p`'s type can't group.
///
/// One pass over the rows, finding each row's group in a small map and pushing its index:
/// no string is built and nothing is sorted per row. The groups are ordered once, at the end.
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
            // (Options with the same id: the last one's group, as a map collected in order.)
            let place: FastMap<&str, usize> = p
                .options()
                .iter()
                .enumerate()
                .map(|(i, o)| (o.id.as_str(), i + 1))
                .collect();
            let mut seen = Vec::new();
            for &i in indexes {
                seen.clear();
                // Read leniently across the two types: a select's one id under a
                // multi-select, a multi-select's first under a select.
                match Cell::read(&rows[i], field) {
                    Cell::Value(v) if p.kind == PropertyType::Select => {
                        seen.extend(v.option_id().and_then(|id| place.get(id)).copied());
                    }
                    Cell::Value(v) => {
                        for id in v.option_ids().unwrap_or(&[]) {
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
        PropertyType::Person | PropertyType::CreatedBy | PropertyType::EditedBy => {
            let mut empty = Group::new(GroupKey::Empty);
            // Each person's group, by who they are, in the order they're first met; ordered
            // after.
            let mut place: FastMap<PersonId, usize> = FastMap::default();
            let mut people: Vec<Group> = Vec::new();
            for &i in indexes {
                let mut any = false;
                let row_people: &[Person] = match Cell::read(&rows[i], field) {
                    Cell::Value(Value::People(people)) => people,
                    Cell::Person(person) => std::slice::from_ref(person),
                    _ => &[],
                };
                for person in row_people {
                    let Some(id) = PersonId::of(person) else {
                        continue;
                    };
                    let g = *place.entry(id).or_insert_with(|| {
                        people.push(Group::new(GroupKey::Person(person.clone())));
                        people.len() - 1
                    });
                    let g = &mut people[g];
                    // (Once for a row that names someone twice.)
                    if g.rows.last() != Some(&i) {
                        g.rows.push(i);
                    }
                    any = true;
                }
                if !any {
                    empty.rows.push(i);
                }
            }
            // The owner, then agents, by name, then by key.
            let mut people: Vec<(bool, String, String, Group)> = people
                .into_iter()
                .map(|g| {
                    let GroupKey::Person(p) = &g.key else {
                        unreachable!()
                    };
                    let is_agent = matches!(p, Person::Agent(_));
                    let name = super::sort::person_label(p, cx)
                        .map(|n| super::sort::fold(&n))
                        .unwrap_or_default();
                    (is_agent, name, p.key().unwrap_or_default(), g)
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
            // Each bucket's group, in the order they're first met; sorted oldest first after.
            let mut place: FastMap<DateGroup, usize> = FastMap::default();
            let mut buckets: Vec<Group> = Vec::new();
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
                let g = *place.entry(key).or_insert_with(|| {
                    buckets.push(Group::new(GroupKey::Date(key)));
                    buckets.len() - 1
                });
                buckets[g].rows.push(i);
            }
            // (Each key is one bucket's, so no two compare equal.)
            buckets.sort_unstable_by_key(|g| match g.key {
                GroupKey::Date(d) => Some(d),
                _ => None,
            });
            let mut groups = buckets;
            if !empty.rows.is_empty() || groups.is_empty() {
                groups.push(empty);
            }
            Some(groups)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    //! The groups are the same as the first version's, which built each person's key as a
    //! string, found options and people by hashing and sorted dates in a tree: that version is
    //! kept here as written, and both group random rows (from a fixed seed) by every type.

    use std::collections::BTreeMap;

    use super::*;
    use crate::engine::Lookup;
    use crate::model::{PropertyConfig, SelectOption, Values};

    /// The first version of [`group`], as it was.
    fn reference(
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
                        Cell::Value(v) if p.kind == PropertyType::Select => {
                            seen.extend(v.option_id().and_then(|id| place.get(id)).copied());
                        }
                        Cell::Value(v) => {
                            for id in v.option_ids().unwrap_or(&[]) {
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
                    let checked =
                        matches!(Cell::read(&rows[i], field), Cell::Value(Value::Bool(true)));
                    groups[usize::from(checked)].rows.push(i);
                }
                Some(groups)
            }
            PropertyType::Person | PropertyType::CreatedBy | PropertyType::EditedBy => {
                let mut empty = Group::new(GroupKey::Empty);
                let mut by_person: HashMap<String, Group> = HashMap::new();
                for &i in indexes {
                    let mut any = false;
                    let people: &[Person] = match Cell::read(&rows[i], field) {
                        Cell::Value(Value::People(people)) => people,
                        Cell::Person(person) => std::slice::from_ref(person),
                        _ => &[],
                    };
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
                        let name = crate::engine::sort::person_label(p, cx)
                            .map(|n| crate::engine::sort::fold(&n))
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

    /// A deterministic generator (xorshift), so every run makes the same rows.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }

        fn chance(&mut self, in_ten: usize) -> bool {
            self.below(10) < in_ten
        }
    }

    /// Names for some agents (and the owner), with ties, case and accents, so people's groups
    /// sort by name, then by key.
    struct Names;

    impl Lookup for Names {
        fn person_name(&self, person: &Person) -> Option<String> {
            match person {
                Person::Owner => Some("Ünal".into()),
                Person::Agent(id) => match id.as_str() {
                    "a1" => Some("Zed".into()),
                    "a2" | "a3" => Some("ada".into()),
                    "a4" => Some("Ada".into()),
                    "a5" => Some("Émile".into()),
                    "a6" => Some(String::new()),
                    _ => None,
                },
                Person::Other(_) => Some("someone".into()),
            }
        }
    }

    const OPTIONS: &[&str] = &["o0", "o1", "o2", "o3", "o4", "o5", "o6", "o7", "", "o1"];

    fn person(rng: &mut Rng) -> Person {
        match rng.below(10) {
            0 | 1 => Person::Owner,
            2 => Person::Other(serde_json::json!({"kind": "team", "id": "x"})),
            n => Person::agent(format!("a{}", rng.below(n + 2))),
        }
    }

    fn date(rng: &mut Rng, today: i64) -> Value {
        let start = (today - 800 + rng.below(1_600) as i64) * time::DAY_MS
            + if rng.chance(5) {
                rng.below(time::DAY_MS as usize) as i64
            } else {
                0
            };
        Value::Date(crate::model::DateValue {
            start,
            end: rng
                .chance(2)
                .then(|| start + rng.below(40 * time::DAY_MS as usize) as i64),
            include_time: rng.chance(5),
        })
    }

    /// Any value at all: the right type mostly, sometimes another (a select's id under a
    /// multi-select, a list under a select), unknown ids and repeats.
    fn value(rng: &mut Rng, kind: &PropertyType, today: i64) -> Option<Value> {
        let id = |rng: &mut Rng| {
            if rng.chance(1) {
                "gone".to_owned()
            } else {
                OPTIONS[rng.below(OPTIONS.len())].to_owned()
            }
        };
        if rng.chance(2) {
            return None;
        }
        let odd = rng.chance(1);
        Some(match kind {
            PropertyType::Select if !odd => Value::Text(id(rng)),
            PropertyType::Select | PropertyType::MultiSelect => {
                if odd {
                    Value::Text(id(rng))
                } else {
                    Value::Ids((0..rng.below(5)).map(|_| id(rng)).collect())
                }
            }
            PropertyType::Checkbox if !odd => Value::Bool(rng.chance(5)),
            PropertyType::Person if !odd => {
                Value::People((0..rng.below(4)).map(|_| person(rng)).collect())
            }
            PropertyType::Date if !odd => date(rng, today),
            _ => match rng.below(4) {
                0 => Value::Text(id(rng)),
                1 => Value::Ids(Vec::new()),
                2 => Value::Number(1.0),
                _ => Value::Bool(true),
            },
        })
    }

    fn property(id: &str, kind: PropertyType) -> Property {
        Property::new(id, id, kind, "a0").with_config(PropertyConfig {
            options: Some(
                OPTIONS
                    .iter()
                    .map(|o| SelectOption {
                        id: (*o).to_owned(),
                        name: o.to_uppercase(),
                        color: Default::default(),
                    })
                    .collect(),
            ),
            ..PropertyConfig::default()
        })
    }

    #[test]
    fn groups_are_the_same_as_the_first_versions_on_random_rows() {
        let mut rng = Rng(0x2545_f491_4f6c_dd1d);
        let stored = [
            PropertyType::Select,
            PropertyType::MultiSelect,
            PropertyType::Checkbox,
            PropertyType::Person,
            PropertyType::Date,
        ];
        let derived = [
            PropertyType::CreatedBy,
            PropertyType::EditedBy,
            PropertyType::CreatedTime,
            PropertyType::EditedTime,
        ];
        let buckets = [
            DateBucket::Relative,
            DateBucket::Day,
            DateBucket::Week,
            DateBucket::Month,
            DateBucket::Year,
            DateBucket::Other("fortnight".into()),
        ];
        let mut compared = 0;
        for round in 0..40 {
            let now = time::days_from_civil(2026, 11, 15) * time::DAY_MS
                + rng.below(time::DAY_MS as usize) as i64
                - (round as i64 - 20) * 37 * time::DAY_MS;
            let lookup: &dyn Lookup = if rng.chance(5) {
                &Names
            } else {
                &crate::engine::NoLookup
            };
            let mut cx = Context::new(now)
                .with_offset(rng.below(26 * 60) as i32 - 12 * 60)
                .with_lookup(lookup);
            cx.week_starts_monday = rng.chance(5);
            let today = cx.today();
            let count = [0, 1, 7, 300, 2_000][round % 5];
            let rows: Vec<Doc> = (0..count)
                .map(|i| {
                    let mut values = Values::new();
                    for kind in &stored {
                        if let Some(v) = value(&mut rng, kind, today) {
                            values.insert(kind.to_string(), v);
                        }
                    }
                    let mut row = Doc::row(format!("r{i}"), "db", "", "a0", values);
                    row.created =
                        now - rng.below(900 * time::DAY_MS as usize) as i64 + 10 * time::DAY_MS;
                    row.updated = row.created + rng.below(60 * time::DAY_MS as usize) as i64;
                    row.author = rng.chance(8).then(|| person(&mut rng));
                    row.updated_by = rng.chance(5).then(|| person(&mut rng));
                    row
                })
                .collect();
            // Some of the rows, in any order, as a filter and sorts leave them.
            let mut indexes: Vec<usize> = (0..count).filter(|_| rng.chance(8)).collect();
            for i in (1..indexes.len()).rev() {
                indexes.swap(i, rng.below(i + 1));
            }
            for kind in stored.iter().chain(&derived) {
                // Each property under its own type's id, and the stored ones under every other
                // stored type too (a property whose type changed).
                let ids: Vec<String> = if derived.contains(kind) {
                    vec!["unused".into()]
                } else {
                    stored.iter().map(ToString::to_string).collect()
                };
                for id in ids {
                    let p = property(&id, kind.clone());
                    for bucket in &buckets {
                        let new = group(&p, bucket, &rows, &indexes, &cx);
                        let old = reference(&p, bucket, &rows, &indexes, &cx);
                        assert_eq!(new, old, "{kind} from {id}, by {bucket}, round {round}");
                        compared += 1;
                        if !kind.is_date() {
                            break;
                        }
                    }
                }
            }
        }
        assert!(compared > 1_000, "{compared}");
    }
}
