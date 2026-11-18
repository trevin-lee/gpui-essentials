//! Sorting rows: by the manual order, and by a view's sorts.
//!
//! A sort builds each row's keys once (lowercased text, a number, an option's place, ...), all
//! in one list, and then sorts the rows by them, stably: rows that compare equal keep the order
//! they came in, which is the manual order when [`super::query`] sorts. Text keys are the rows'
//! text lowercased once and kept ([`RowText`]), so a sort by text lowercases nothing a second
//! time. Empty values go last whichever way a sort runs, as in Notion.

use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::HashMap;

use super::text::RowText;
use super::{Cell, Context, Field, Issue, Part};
use crate::model::{DatabaseFields, Direction, Doc, Person, Property, PropertyType, Sort, Value};

/// A row's key for one sort.
#[derive(Clone, Debug, PartialEq)]
enum Key<'t> {
    Empty,
    /// Lowercased: a row's kept text, or a name made for this sort.
    Text(Cow<'t, str>),
    Num(f64),
    Int(i64),
    /// A select's option's place, or a checkbox (0 or 1).
    Place(u32),
    /// A multi-select's options' places.
    Seq(Vec<u32>),
}

fn cmp_present(a: &Key, b: &Key) -> Ordering {
    match (a, b) {
        (Key::Text(a), Key::Text(b)) => a.cmp(b),
        (Key::Num(a), Key::Num(b)) => a.total_cmp(b),
        (Key::Int(a), Key::Int(b)) => a.cmp(b),
        (Key::Place(a), Key::Place(b)) => a.cmp(b),
        (Key::Seq(a), Key::Seq(b)) => a.cmp(b),
        _ => Ordering::Equal,
    }
}

fn cmp_keys(a: &Key, b: &Key, descending: bool) -> Ordering {
    match (a, b) {
        (Key::Empty, Key::Empty) => Ordering::Equal,
        (Key::Empty, _) => Ordering::Greater,
        (_, Key::Empty) => Ordering::Less,
        _ if descending => cmp_present(a, b).reverse(),
        _ => cmp_present(a, b),
    }
}

/// Text folded for comparing: lowercased, cheaply when it's ASCII.
pub(crate) fn fold(s: &str) -> String {
    if s.is_ascii() {
        s.to_ascii_lowercase()
    } else {
        s.to_lowercase()
    }
}

/// Puts `indexes` in the manual order: by each row's `order`, then its id (so equal or missing
/// orders are still stable).
pub fn manual(rows: &[Doc], indexes: &mut [usize]) {
    indexes.sort_by(|&a, &b| {
        let (a, b) = (&rows[a], &rows[b]);
        (a.order.as_str(), a.id.as_str()).cmp(&(b.order.as_str(), b.id.as_str()))
    });
}

/// One sort, ready to make keys.
struct Sorter<'p> {
    field: Field<'p>,
    kind: &'p PropertyType,
    descending: bool,
    /// A select's or multi-select's option places.
    places: HashMap<&'p str, u32>,
    /// A relation's database, for looking titles up.
    database_id: Option<&'p str>,
    /// A text property's: the rows' lowercased text, in a [`RowText`].
    list: Option<usize>,
}

impl<'p> Sorter<'p> {
    fn new(p: &'p Property, descending: bool, rows: &[Doc], text: &mut RowText) -> Self {
        let field = Field::of(p);
        // As the cell shows it: an option's name, under a property that was a select.
        let shown = matches!(field, Field::Stored(_)) && !p.options().is_empty();
        Self {
            list: p
                .kind
                .is_text()
                .then(|| text.list(rows, field, shown.then_some(p)))
                .flatten(),
            field,
            kind: &p.kind,
            descending,
            places: p
                .options()
                .iter()
                .enumerate()
                .map(|(i, o)| (o.id.as_str(), i as u32))
                .collect(),
            database_id: p.relation().map(|r| r.database_id),
        }
    }

    /// Row `rows[at]`'s key, its text read from `text`.
    fn key<'t>(&self, rows: &[Doc], at: usize, text: &'t RowText, cx: &Context) -> Key<'t> {
        if let Some(list) = self.list {
            return match text.get(list, at) {
                "" => Key::Empty,
                s => Key::Text(Cow::Borrowed(s)),
            };
        }
        let row = &rows[at];
        let cell = Cell::read(row, self.field);
        let value = match cell {
            Cell::Time(t) => return Key::Int(t),
            Cell::Person(p) => return person_label(p, cx).map_or(Key::Empty, |n| text_key(&n)),
            Cell::Text("") => return Key::Empty,
            Cell::Text(s) => return Key::Text(Cow::Owned(fold(s))),
            Cell::Missing => {
                return if *self.kind == PropertyType::Checkbox {
                    Key::Place(0)
                } else {
                    Key::Empty
                };
            }
            Cell::Value(v) => v,
        };
        match (self.kind, value) {
            // Read leniently across the two: a select's id under a multi-select, a
            // multi-select's first under a select.
            (PropertyType::Select, v) if v.option_id().is_some() => v
                .option_id()
                .and_then(|id| self.places.get(id))
                .map_or(Key::Empty, |&i| Key::Place(i)),
            (PropertyType::MultiSelect, v) if v.option_ids().is_some() => {
                let ids = v.option_ids().unwrap_or(&[]);
                let mut places: Vec<u32> = ids
                    .iter()
                    .filter_map(|id| self.places.get(id.as_str()).copied())
                    .collect();
                if places.is_empty() {
                    return Key::Empty;
                }
                places.sort_unstable();
                places.dedup();
                Key::Seq(places)
            }
            (PropertyType::Checkbox, v) => Key::Place(u32::from(v.as_bool().unwrap_or(false))),
            (PropertyType::Number, Value::Number(x)) => Key::Num(*x),
            (PropertyType::Date, Value::Date(d)) => Key::Int(d.start),
            (PropertyType::Person, Value::People(people)) => {
                let names: Vec<String> =
                    people.iter().filter_map(|p| person_label(p, cx)).collect();
                text_key(&names.join(", "))
            }
            (PropertyType::Relation, Value::Ids(ids)) => {
                let db = self.database_id.unwrap_or("");
                let titles: Vec<String> = ids
                    .iter()
                    .map(|id| cx.lookup.row_title(db, id).unwrap_or_else(|| id.clone()))
                    .collect();
                text_key(&titles.join(", "))
            }
            // As the cell shows it: an option's name, under a property that was a select.
            (k, Value::Text(s)) if k.is_text() => match self.field {
                Field::Stored(p) => text_key(p.shown_text(s)),
                _ => text_key(s),
            },
            _ => Key::Empty,
        }
    }
}

fn text_key(s: &str) -> Key<'static> {
    if s.is_empty() {
        Key::Empty
    } else {
        Key::Text(Cow::Owned(fold(s)))
    }
}

/// The name a person sorts and shows by: the host's, else "Owner" or the agent's id.
pub(crate) fn person_label(p: &Person, cx: &Context) -> Option<String> {
    cx.lookup.person_name(p).or_else(|| match p {
        Person::Owner => Some("Owner".into()),
        Person::Agent(id) => Some(id.clone()),
        Person::Other(_) => None,
    })
}

/// Sorts `indexes` (into `rows`) by `sorts` in turn, stably. Sorts by properties the database
/// hasn't, and sorts past the limit, are left out and reported.
pub fn sort(
    db: &DatabaseFields,
    sorts: &[Sort],
    rows: &[Doc],
    indexes: &mut Vec<usize>,
    cx: &Context,
) -> Vec<Issue> {
    let mut text = RowText::new();
    text.begin(rows);
    sort_with(db, sorts, rows, indexes, cx, &mut text)
}

/// [`sort`], reading rows' lowercased text from `text` (made there when it isn't kept);
/// [`RowText::begin`] was called for `rows`.
pub(crate) fn sort_with(
    db: &DatabaseFields,
    sorts: &[Sort],
    rows: &[Doc],
    indexes: &mut Vec<usize>,
    cx: &Context,
    text: &mut RowText,
) -> Vec<Issue> {
    let mut issues = Vec::new();
    if sorts.len() > cx.limits.sorts {
        issues.push(Issue::TooManySorts {
            limit: cx.limits.sorts,
        });
    }
    let sorters: Vec<Sorter> = sorts
        .iter()
        .take(cx.limits.sorts)
        .filter_map(|s| match db.property(&s.property_id) {
            Some(p) => Some(Sorter::new(
                p,
                s.direction == Direction::Descending,
                rows,
                text,
            )),
            None => {
                issues.push(Issue::UnknownProperty {
                    part: Part::Sort,
                    property_id: s.property_id.clone(),
                });
                None
            }
        })
        .collect();
    if sorters.is_empty() || indexes.len() < 2 {
        return issues;
    }
    // Decorate: every row's keys, made once, a row's beside each other.
    let text = &*text;
    let mut keys = Vec::with_capacity(indexes.len() * sorters.len());
    for &i in indexes.iter() {
        keys.extend(sorters.iter().map(|s| s.key(rows, i, text, cx)));
    }
    let mut keyed: Vec<(&[Key], usize)> = keys
        .chunks_exact(sorters.len())
        .zip(indexes.iter().copied())
        .collect();
    // Sort, stably, comparing keys only.
    keyed.sort_by(|(a, _), (b, _)| {
        for (s, (ka, kb)) in sorters.iter().zip(a.iter().zip(*b)) {
            match cmp_keys(ka, kb, s.descending) {
                Ordering::Equal => continue,
                o => return o,
            }
        }
        Ordering::Equal
    });
    // Undecorate.
    indexes.clear();
    indexes.extend(keyed.into_iter().map(|(_, i)| i));
    issues
}

#[cfg(test)]
mod tests {
    //! Sorts put rows in the same order as the first version's, which made each row's text key
    //! a string of its own and its keys a list of their own: that version is kept here as
    //! written, and both sort random rows (from a fixed seed) by random sorts, with the rows'
    //! text kept between sorts as a view keeps it.

    use super::*;
    use crate::engine::{Lookup, RowText};
    use crate::model::{DateValue, PropertyConfig, SelectOption, Values};

    /// A row's key for one sort.
    #[derive(Clone, Debug, PartialEq)]
    enum OldKey {
        Empty,
        Text(String),
        Num(f64),
        Int(i64),
        /// Options' places, or a checkbox (0 or 1).
        Seq(Vec<u32>),
    }

    fn old_cmp_present(a: &OldKey, b: &OldKey) -> Ordering {
        match (a, b) {
            (OldKey::Text(a), OldKey::Text(b)) => a.cmp(b),
            (OldKey::Num(a), OldKey::Num(b)) => a.total_cmp(b),
            (OldKey::Int(a), OldKey::Int(b)) => a.cmp(b),
            (OldKey::Seq(a), OldKey::Seq(b)) => a.cmp(b),
            _ => Ordering::Equal,
        }
    }

    fn old_cmp_keys(a: &OldKey, b: &OldKey, descending: bool) -> Ordering {
        match (a, b) {
            (OldKey::Empty, OldKey::Empty) => Ordering::Equal,
            (OldKey::Empty, _) => Ordering::Greater,
            (_, OldKey::Empty) => Ordering::Less,
            _ if descending => old_cmp_present(a, b).reverse(),
            _ => old_cmp_present(a, b),
        }
    }

    /// One sort, ready to make keys.
    struct OldSorter<'p> {
        field: Field<'p>,
        kind: &'p PropertyType,
        descending: bool,
        /// A select's or multi-select's option places.
        places: HashMap<&'p str, u32>,
        /// A relation's database, for looking titles up.
        database_id: Option<&'p str>,
    }

    impl<'p> OldSorter<'p> {
        fn new(p: &'p Property, descending: bool) -> Self {
            Self {
                field: Field::of(p),
                kind: &p.kind,
                descending,
                places: p
                    .options()
                    .iter()
                    .enumerate()
                    .map(|(i, o)| (o.id.as_str(), i as u32))
                    .collect(),
                database_id: p.relation().map(|r| r.database_id),
            }
        }

        fn key(&self, row: &Doc, cx: &Context) -> OldKey {
            let cell = Cell::read(row, self.field);
            let value = match cell {
                Cell::Time(t) => return OldKey::Int(t),
                Cell::Person(p) => {
                    return person_label(p, cx).map_or(OldKey::Empty, |n| old_text_key(&n));
                }
                Cell::Text("") => return OldKey::Empty,
                Cell::Text(s) => return OldKey::Text(fold(s)),
                Cell::Missing => {
                    return if *self.kind == PropertyType::Checkbox {
                        OldKey::Seq(vec![0])
                    } else {
                        OldKey::Empty
                    };
                }
                Cell::Value(v) => v,
            };
            match (self.kind, value) {
                // Read leniently across the two: a select's id under a multi-select, a
                // multi-select's first under a select.
                (PropertyType::Select, v) if v.option_id().is_some() => v
                    .option_id()
                    .and_then(|id| self.places.get(id))
                    .map_or(OldKey::Empty, |&i| OldKey::Seq(vec![i])),
                (PropertyType::MultiSelect, v) if v.option_ids().is_some() => {
                    let ids = v.option_ids().unwrap_or(&[]);
                    let mut places: Vec<u32> = ids
                        .iter()
                        .filter_map(|id| self.places.get(id.as_str()).copied())
                        .collect();
                    if places.is_empty() {
                        return OldKey::Empty;
                    }
                    places.sort_unstable();
                    places.dedup();
                    OldKey::Seq(places)
                }
                (PropertyType::Checkbox, v) => {
                    OldKey::Seq(vec![u32::from(v.as_bool().unwrap_or(false))])
                }
                (PropertyType::Number, Value::Number(x)) => OldKey::Num(*x),
                (PropertyType::Date, Value::Date(d)) => OldKey::Int(d.start),
                (PropertyType::Person, Value::People(people)) => {
                    let names: Vec<String> =
                        people.iter().filter_map(|p| person_label(p, cx)).collect();
                    old_text_key(&names.join(", "))
                }
                (PropertyType::Relation, Value::Ids(ids)) => {
                    let db = self.database_id.unwrap_or("");
                    let titles: Vec<String> = ids
                        .iter()
                        .map(|id| cx.lookup.row_title(db, id).unwrap_or_else(|| id.clone()))
                        .collect();
                    old_text_key(&titles.join(", "))
                }
                // As the cell shows it: an option's name, under a property that was a select.
                (k, Value::Text(s)) if k.is_text() => match self.field {
                    Field::Stored(p) => old_text_key(p.shown_text(s)),
                    _ => old_text_key(s),
                },
                _ => OldKey::Empty,
            }
        }
    }

    fn old_text_key(s: &str) -> OldKey {
        if s.is_empty() {
            OldKey::Empty
        } else {
            OldKey::Text(fold(s))
        }
    }

    /// The first version of [`sort`], as it was: sorts `indexes` (into `rows`) by `sorts` in turn, stably. Sorts by properties the database
    /// hasn't, and sorts past the limit, are left out and reported.
    fn reference(
        db: &DatabaseFields,
        sorts: &[Sort],
        rows: &[Doc],
        indexes: &mut Vec<usize>,
        cx: &Context,
    ) -> Vec<Issue> {
        let mut issues = Vec::new();
        if sorts.len() > cx.limits.sorts {
            issues.push(Issue::TooManySorts {
                limit: cx.limits.sorts,
            });
        }
        let sorters: Vec<OldSorter> = sorts
            .iter()
            .take(cx.limits.sorts)
            .filter_map(|s| match db.property(&s.property_id) {
                Some(p) => Some(OldSorter::new(p, s.direction == Direction::Descending)),
                None => {
                    issues.push(Issue::UnknownProperty {
                        part: Part::Sort,
                        property_id: s.property_id.clone(),
                    });
                    None
                }
            })
            .collect();
        if sorters.is_empty() || indexes.len() < 2 {
            return issues;
        }
        let mut keyed: Vec<(usize, Vec<OldKey>)> = indexes
            .iter()
            .map(|&i| (i, sorters.iter().map(|s| s.key(&rows[i], cx)).collect()))
            .collect();
        keyed.sort_by(|(_, a), (_, b)| {
            for (s, (ka, kb)) in sorters.iter().zip(a.iter().zip(b)) {
                match old_cmp_keys(ka, kb, s.descending) {
                    Ordering::Equal => continue,
                    o => return o,
                }
            }
            Ordering::Equal
        });
        indexes.clear();
        indexes.extend(keyed.into_iter().map(|(i, _)| i));
        issues
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

        fn pick<'a>(&mut self, of: &[&'a str]) -> &'a str {
            of[self.below(of.len())]
        }
    }

    /// Names with ties, case and accents.
    struct Names;

    impl Lookup for Names {
        fn person_name(&self, person: &Person) -> Option<String> {
            match person {
                Person::Owner => Some("Ünal".into()),
                Person::Agent(id) if id == "a1" => Some("ada".into()),
                Person::Agent(id) if id == "a2" => Some("Ada".into()),
                _ => None,
            }
        }

        fn row_title(&self, _: &str, row_id: &str) -> Option<String> {
            (row_id == "r1").then(|| "ÉCOLE".into())
        }
    }

    /// Words with ties in every case, accents and a Greek capital sigma.
    const WORDS: &[&str] = &[
        "kiln",
        "Kiln",
        "KILN",
        "café",
        "Café",
        "cafe\u{301}",
        "über",
        "Über",
        "ΣΊΣΥΦΟΣ",
        "σίσυφος",
        "Σ",
        "İstanbul",
        "Straße",
        "ǅ",
        "東京",
        "o1",
        "o2",
        "",
        " ",
        "a",
        "A",
        "b",
    ];

    const OPTIONS: &[&str] = &["o0", "o1", "o2", "o3"];

    fn text(rng: &mut Rng) -> String {
        let n = rng.below(4);
        let words: Vec<&str> = (0..n).map(|_| rng.pick(WORDS)).collect();
        words.join(" ")
    }

    /// Any value at all under any property: the right type mostly, sometimes another.
    fn value(rng: &mut Rng, kind: &PropertyType) -> Option<Value> {
        if rng.chance(1) {
            return None;
        }
        let odd = rng.chance(1);
        let id = |rng: &mut Rng| rng.pick(&["o0", "o1", "o2", "o3", "gone"]).to_owned();
        Some(match kind {
            k if k.is_text() && !odd => {
                Value::Text(if rng.chance(3) { id(rng) } else { text(rng) })
            }
            PropertyType::Number if !odd => Value::Number(rng.below(5) as f64 - 2.0),
            PropertyType::Select if !odd => Value::Text(id(rng)),
            PropertyType::Select | PropertyType::MultiSelect => {
                Value::Ids((0..rng.below(4)).map(|_| id(rng)).collect())
            }
            PropertyType::Checkbox if !odd => Value::Bool(rng.chance(5)),
            PropertyType::Date if !odd => Value::Date(DateValue {
                start: rng.below(5) as i64 * 86_400_000,
                end: None,
                include_time: false,
            }),
            PropertyType::Person if !odd => Value::People(
                (0..rng.below(3))
                    .map(|_| match rng.below(4) {
                        0 => Person::Owner,
                        n => Person::agent(format!("a{n}")),
                    })
                    .collect(),
            ),
            PropertyType::Relation if !odd => Value::Ids(
                (0..rng.below(3))
                    .map(|_| format!("r{}", rng.below(3)))
                    .collect(),
            ),
            _ => match rng.below(3) {
                0 => Value::Text(text(rng)),
                1 => Value::Number(1.0),
                _ => Value::Bool(true),
            },
        })
    }

    const KINDS: &[PropertyType] = &[
        PropertyType::Text,
        PropertyType::Url,
        PropertyType::Email,
        PropertyType::Phone,
        PropertyType::Number,
        PropertyType::Select,
        PropertyType::MultiSelect,
        PropertyType::Checkbox,
        PropertyType::Date,
        PropertyType::Person,
        PropertyType::Relation,
    ];

    /// Every type under its own id, a text one with a select's options kept, the title (with
    /// options sometimes), and the derived ones.
    fn database(rng: &mut Rng) -> DatabaseFields {
        let options = || PropertyConfig {
            options: Some(
                OPTIONS
                    .iter()
                    .map(|o| SelectOption {
                        id: (*o).into(),
                        name: if *o == "o2" {
                            "Ünal".into()
                        } else {
                            o.to_uppercase()
                        },
                        color: Default::default(),
                    })
                    .collect(),
            ),
            database_id: Some("other".into()),
            ..PropertyConfig::default()
        };
        let mut title = Property::new("name", "Name", PropertyType::Title, "a0");
        if rng.chance(3) {
            title = title.with_config(options());
        }
        let mut properties = vec![title];
        for (i, kind) in KINDS.iter().enumerate() {
            let p = Property::new(kind.to_string(), "P", kind.clone(), format!("b{i}"));
            properties.push(if rng.chance(7) {
                p.with_config(options())
            } else {
                p
            });
        }
        for kind in [
            PropertyType::CreatedTime,
            PropertyType::EditedTime,
            PropertyType::CreatedBy,
            PropertyType::EditedBy,
        ] {
            properties.push(Property::new(kind.to_string(), "P", kind, "c0"));
        }
        DatabaseFields {
            properties,
            ..DatabaseFields::default()
        }
    }

    fn rows(rng: &mut Rng, count: usize) -> Vec<Doc> {
        (0..count)
            .map(|i| {
                let mut values = Values::new();
                for kind in KINDS {
                    if let Some(v) = value(rng, kind) {
                        values.insert(kind.to_string(), v);
                    }
                }
                let title = if rng.chance(2) {
                    "o1".into()
                } else {
                    text(rng)
                };
                let order = format!("a{}", rng.below(4));
                let mut row = Doc::row(format!("r{i}"), "db", title, order, values);
                row.created = rng.below(4) as i64;
                row.updated = rng.below(4) as i64;
                row.author = rng
                    .chance(7)
                    .then(|| Person::agent(format!("a{}", rng.below(4))));
                row
            })
            .collect()
    }

    #[test]
    fn sorts_order_rows_as_the_first_versions_on_random_rows() {
        let mut rng = Rng(0x2545_f491_4f6c_dd1d);
        let mut compared = 0;
        for round in 0..60 {
            let lookup: &dyn Lookup = if rng.chance(5) {
                &Names
            } else {
                &crate::engine::NoLookup
            };
            let cx = Context::new(1_794_744_000_000).with_lookup(lookup);
            let db = database(&mut rng);
            let count = [0, 1, 9, 200, 1_500][round % 5];
            let rows = rows(&mut rng, count);
            let ids: Vec<String> = db
                .properties
                .iter()
                .map(|p| p.id.clone())
                .chain(["gone".into()])
                .collect();
            // Kept across this round's sorts, as a view keeps it while its rows don't change.
            let mut kept = RowText::new();
            for _ in 0..25 {
                let sorts: Vec<Sort> = (0..rng.below(4))
                    .map(|_| {
                        let id = ids[rng.below(ids.len())].clone();
                        if rng.chance(5) {
                            Sort::ascending(id)
                        } else {
                            Sort::descending(id)
                        }
                    })
                    .collect();
                // Some of the rows, in the manual order, as a filter leaves them.
                let mut start: Vec<usize> = (0..count).filter(|_| rng.chance(8)).collect();
                manual(&rows, &mut start);
                let mut old = start.clone();
                let old_issues = reference(&db, &sorts, &rows, &mut old, &cx);
                let mut new = start.clone();
                kept.begin(&rows);
                let issues = sort_with(&db, &sorts, &rows, &mut new, &cx, &mut kept);
                assert_eq!(new, old, "round {round}: {sorts:?}");
                assert_eq!(issues, old_issues);
                let mut fresh = start;
                sort(&db, &sorts, &rows, &mut fresh, &cx);
                assert_eq!(fresh, old, "round {round}, fresh: {sorts:?}");
                compared += 1;
            }
        }
        assert!(compared >= 1_500, "{compared}");
    }
}
