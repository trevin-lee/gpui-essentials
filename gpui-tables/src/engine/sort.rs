//! Sorting rows: by the manual order, and by a view's sorts.
//!
//! A sort builds each row's key once (lowercased text, a number, an option's place, ...) and
//! then sorts the keys, stably: rows that compare equal keep the order they came in, which is the
//! manual order when [`super::query`] sorts. Empty values go last whichever way a sort runs, as in
//! Notion.

use std::cmp::Ordering;
use std::collections::HashMap;

use super::{Cell, Context, Field, Issue, Part};
use crate::model::{DatabaseFields, Direction, Doc, Person, Property, PropertyType, Sort, Value};

/// A row's key for one sort.
#[derive(Clone, Debug, PartialEq)]
enum Key {
    Empty,
    Text(String),
    Num(f64),
    Int(i64),
    /// Options' places, or a checkbox (0 or 1).
    Seq(Vec<u32>),
}

fn cmp_present(a: &Key, b: &Key) -> Ordering {
    match (a, b) {
        (Key::Text(a), Key::Text(b)) => a.cmp(b),
        (Key::Num(a), Key::Num(b)) => a.total_cmp(b),
        (Key::Int(a), Key::Int(b)) => a.cmp(b),
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
}

impl<'p> Sorter<'p> {
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

    fn key(&self, row: &Doc, cx: &Context) -> Key {
        let cell = Cell::read(row, self.field);
        let value = match cell {
            Cell::Time(t) => return Key::Int(t),
            Cell::Person(p) => return person_label(p, cx).map_or(Key::Empty, |n| text_key(&n)),
            Cell::Text("") => return Key::Empty,
            Cell::Text(s) => return Key::Text(fold(s)),
            Cell::Missing => {
                return if *self.kind == PropertyType::Checkbox {
                    Key::Seq(vec![0])
                } else {
                    Key::Empty
                };
            }
            Cell::Value(v) => v,
        };
        match (self.kind, value) {
            (PropertyType::Select, Value::Text(id)) => self
                .places
                .get(id.as_str())
                .map_or(Key::Empty, |&i| Key::Seq(vec![i])),
            (PropertyType::MultiSelect, Value::Ids(ids)) => {
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
            (PropertyType::Checkbox, v) => Key::Seq(vec![u32::from(v.as_bool().unwrap_or(false))]),
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
            (k, Value::Text(s)) if k.is_text() => text_key(s),
            _ => Key::Empty,
        }
    }
}

fn text_key(s: &str) -> Key {
    if s.is_empty() {
        Key::Empty
    } else {
        Key::Text(fold(s))
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
            Some(p) => Some(Sorter::new(p, s.direction == Direction::Descending)),
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
    let mut keyed: Vec<(usize, Vec<Key>)> = indexes
        .iter()
        .map(|&i| (i, sorters.iter().map(|s| s.key(&rows[i], cx)).collect()))
        .collect();
    keyed.sort_by(|(_, a), (_, b)| {
        for (s, (ka, kb)) in sorters.iter().zip(a.iter().zip(b)) {
            match cmp_keys(ka, kb, s.descending) {
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
