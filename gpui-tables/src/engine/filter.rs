//! Filtering rows: a view's filter tree, compiled once against the database's properties and
//! then tested against each row. A text condition reads the rows' text lowercased once and kept
//! ([`RowText`]) while it filters rows by [`super::query_with`], so it lowercases nothing per
//! query.
//!
//! What each operator means:
//!
//! - **Text** (title, text, url, email, phone): `is`, `isNot`, `contains`, `doesNotContain`,
//!   `startsWith`, ignoring case.
//! - **Number:** `is`, `isNot`, `greater`, `greaterOrEqual`, `less`, `lessOrEqual`.
//! - **Select:** `is` and `isNot` an option id.
//! - **Multi-select, person, relation:** `contains` and `doesNotContain` an option id, a person,
//!   or a row id.
//! - **Created by, edited by:** `is`, `isNot`, `contains` and `doesNotContain` a person (with
//!   one person each, `is` is `contains`). Edited by reads the doc's `author` when it has no
//!   `updatedBy`.
//! - **Date, created time, edited time:** `is`, `isNot`, `before`, `after`, `onOrBefore`,
//!   `onOrAfter` a day, and `within` a span around today, all in whole days in the viewer's zone.
//!   A range matches when any of its days does.
//! - **Checkbox:** `isChecked`, `isNotChecked`, or `is` true or false. A missing checkbox is
//!   unchecked.
//! - **Every type but checkbox:** `isEmpty`, `isNotEmpty`. A select whose option was deleted is
//!   empty.
//!
//! The negative operators (`isNot`, `doesNotContain`) match empty values, as in Notion. A
//! condition with no value yet (being edited) is left out quietly; one whose operator doesn't fit
//! its type, or whose value can't be read, is left out and reported. An `and` or `or` with
//! nothing left in it matches every row.

use std::collections::HashSet;

use serde_json::Value as Json;

use super::text::RowText;
use super::time::{self, days_of};
use super::{Cell, Context, Field, Issue, Part};
use crate::model::{
    Condition, DatabaseFields, Doc, Filter, FilterOp, Person, Property, PropertyType, Value, Within,
};

#[derive(Clone, Copy, Debug, PartialEq)]
enum NumOp {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
}

#[derive(Debug)]
enum Pred {
    /// isEmpty (true) or isNotEmpty (false).
    Empty(bool),
    /// Folded needle; negated for isNot.
    TextIs(String, bool),
    TextContains(String, bool),
    TextStarts(String),
    Num(NumOp, f64),
    SelectIs(String, bool),
    /// An id in a multi-select or relation; negated for doesNotContain.
    Has(String, bool),
    HasPerson(Person, bool),
    Checked(bool),
    /// Any of the value's days in `lo..=hi`; negated for isNot.
    Days {
        lo: i64,
        hi: i64,
        negate: bool,
    },
}

#[derive(Debug)]
struct Test {
    field: FieldRef,
    kind: PropertyType,
    pred: Pred,
    /// A select's or multi-select's option ids: others count as empty.
    known: Option<HashSet<String>>,
    /// A text-like property that kept a select's options (its type was changed): its values,
    /// option ids, are tested by the names its cells show ([`Property::shown_text`]).
    shown: Option<Box<Property>>,
    /// Where it is, a text condition's property: kept to find its rows' text in a [`RowText`].
    property: Option<Box<Property>>,
    /// The rows' lowercased text it reads, once [`CompiledFilter::read_text`] found it.
    list: Option<usize>,
}

impl Test {
    /// The text a cell shows: an option's name for an option id under a property that was a
    /// select, else the text as stored.
    fn text<'a>(&'a self, cell: Cell<'a>) -> Option<&'a str> {
        let s = cell.text()?;
        Some(self.shown.as_ref().map_or(s, |p| p.shown_text(s)))
    }
}

#[derive(Debug)]
enum FieldRef {
    Title,
    Created,
    Updated,
    CreatedBy,
    EditedBy,
    Stored(String),
}

#[derive(Debug)]
enum Node {
    All(Vec<Node>),
    Any(Vec<Node>),
    Test(Test),
}

/// A view's filter, ready to test rows.
#[derive(Debug, Default)]
pub struct CompiledFilter {
    root: Option<Node>,
    offset_ms: i64,
}

struct Compiler<'a> {
    db: &'a DatabaseFields,
    cx: &'a Context<'a>,
    nodes: usize,
    issues: Vec<Issue>,
    too_deep: bool,
    too_big: bool,
}

impl CompiledFilter {
    /// Compiles `filter` (None: every row passes) against `db`'s properties, saying what it left
    /// out.
    pub fn new(filter: Option<&Filter>, db: &DatabaseFields, cx: &Context) -> (Self, Vec<Issue>) {
        let Some(filter) = filter else {
            return (Self::default(), Vec::new());
        };
        let mut c = Compiler {
            db,
            cx,
            nodes: 0,
            issues: Vec::new(),
            too_deep: false,
            too_big: false,
        };
        let root = c.node(filter, 1);
        if c.too_deep {
            c.issues.push(Issue::FilterTooDeep {
                limit: cx.limits.filter_depth,
            });
        }
        if c.too_big {
            c.issues.push(Issue::FilterTooBig {
                limit: cx.limits.filter_nodes,
            });
        }
        (
            Self {
                root,
                offset_ms: cx.offset_ms(),
            },
            c.issues,
        )
    }

    /// Whether it filters anything at all.
    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    /// Whether `row` passes.
    pub fn matches(&self, row: &Doc) -> bool {
        self.root
            .as_ref()
            .is_none_or(|n| eval(n, row, None, self.offset_ms))
    }

    /// Finds (or makes, in `text`) the lowercased text of `rows` each text condition reads, for
    /// [`matches_at`](Self::matches_at).
    pub(crate) fn read_text(&mut self, rows: &[Doc], text: &mut RowText) {
        fn walk(node: &mut Node, rows: &[Doc], text: &mut RowText) {
            match node {
                Node::All(nodes) | Node::Any(nodes) => {
                    nodes.iter_mut().for_each(|n| walk(n, rows, text));
                }
                Node::Test(t) => {
                    if let Some(p) = &t.property {
                        t.list = text.list(rows, Field::of(p), t.shown.as_deref());
                    }
                }
            }
        }
        if let Some(root) = &mut self.root {
            walk(root, rows, text);
        }
    }

    /// Whether `rows[row]` passes, its text read from `text` (as [`read_text`](Self::read_text)
    /// left it, for these rows).
    pub(crate) fn matches_at(&self, rows: &[Doc], row: usize, text: &RowText) -> bool {
        self.root
            .as_ref()
            .is_none_or(|n| eval(n, &rows[row], Some((text, row)), self.offset_ms))
    }
}

impl Compiler<'_> {
    fn node(&mut self, f: &Filter, depth: usize) -> Option<Node> {
        if depth > self.cx.limits.filter_depth {
            self.too_deep = true;
            return None;
        }
        if self.nodes >= self.cx.limits.filter_nodes {
            self.too_big = true;
            return None;
        }
        self.nodes += 1;
        match f {
            Filter::And { and } => Some(Node::All(
                and.iter().filter_map(|f| self.node(f, depth + 1)).collect(),
            )),
            Filter::Or { or } => Some(Node::Any(
                or.iter().filter_map(|f| self.node(f, depth + 1)).collect(),
            )),
            Filter::Condition(c) => self.condition(c).map(Node::Test),
        }
    }

    fn condition(&mut self, c: &Condition) -> Option<Test> {
        let Some(p) = self.db.property(&c.property_id) else {
            self.issues.push(Issue::UnknownProperty {
                part: Part::Filter,
                property_id: c.property_id.clone(),
            });
            return None;
        };
        if !c.op.applies_to(&p.kind) {
            self.issues.push(Issue::OpNotForType {
                property_id: p.id.clone(),
                op: c.op.clone(),
            });
            return None;
        }
        let pred = match pred(&p.kind, &c.op, &c.value, self.cx) {
            Ok(Some(pred)) => pred,
            Ok(None) => return None,
            Err(()) => {
                self.issues.push(Issue::BadValue {
                    property_id: p.id.clone(),
                    op: c.op.clone(),
                });
                return None;
            }
        };
        let field = match Field::of(p) {
            Field::Title => FieldRef::Title,
            Field::Created => FieldRef::Created,
            Field::Updated => FieldRef::Updated,
            Field::CreatedBy => FieldRef::CreatedBy,
            Field::EditedBy => FieldRef::EditedBy,
            Field::Stored(p) => FieldRef::Stored(p.id.clone()),
        };
        let known = matches!(p.kind, PropertyType::Select | PropertyType::MultiSelect)
            .then(|| p.options().iter().map(|o| o.id.clone()).collect());
        let shown = (p.kind.is_text() && !p.options().is_empty()).then(|| Box::new(p.clone()));
        let property = matches!(
            pred,
            Pred::TextIs(..) | Pred::TextContains(..) | Pred::TextStarts(_)
        )
        .then(|| Box::new(p.clone()));
        Some(Test {
            field,
            kind: p.kind.clone(),
            pred,
            known,
            shown,
            property,
            list: None,
        })
    }
}

/// The predicate for an operator and value: None when the value is missing (a condition still
/// being written), Err when it's there but unusable.
fn pred(
    kind: &PropertyType,
    op: &FilterOp,
    value: &Json,
    cx: &Context,
) -> Result<Option<Pred>, ()> {
    use FilterOp as O;
    match op {
        O::IsEmpty => return Ok(Some(Pred::Empty(true))),
        O::IsNotEmpty => return Ok(Some(Pred::Empty(false))),
        O::IsChecked => return Ok(Some(Pred::Checked(true))),
        O::IsNotChecked => return Ok(Some(Pred::Checked(false))),
        _ => {}
    }
    // A text condition's folded value is never empty, so a row without text never matches
    // it (what lets [`RowText`] keep such a row's text as empty).
    if value.is_null() || value.as_str() == Some("") {
        return Ok(None);
    }
    let text = || value.as_str().map(super::sort::fold).ok_or(());
    let id = || value.as_str().map(str::to_owned).ok_or(());
    Ok(Some(match kind {
        k if k.is_text() => match op {
            O::Is => Pred::TextIs(text()?, false),
            O::IsNot => Pred::TextIs(text()?, true),
            O::Contains => Pred::TextContains(text()?, false),
            O::DoesNotContain => Pred::TextContains(text()?, true),
            O::StartsWith => Pred::TextStarts(text()?),
            _ => return Err(()),
        },
        PropertyType::Number => {
            let x = value.as_f64().filter(|x| x.is_finite()).ok_or(())?;
            let op = match op {
                O::Is => NumOp::Eq,
                O::IsNot => NumOp::Ne,
                O::Greater => NumOp::Gt,
                O::GreaterOrEqual => NumOp::Ge,
                O::Less => NumOp::Lt,
                O::LessOrEqual => NumOp::Le,
                _ => return Err(()),
            };
            Pred::Num(op, x)
        }
        PropertyType::Select => Pred::SelectIs(id()?, *op == O::IsNot),
        PropertyType::MultiSelect | PropertyType::Relation => {
            Pred::Has(id()?, *op == O::DoesNotContain)
        }
        PropertyType::Person | PropertyType::CreatedBy | PropertyType::EditedBy => {
            let person = match value {
                Json::String(s) if s == "owner" => Person::Owner,
                Json::String(s) => match s.strip_prefix("agent:") {
                    Some(id) if !id.is_empty() => Person::Agent(id.to_owned()),
                    _ => return Err(()),
                },
                j => match Person::from_json(j) {
                    Some(p @ (Person::Owner | Person::Agent(_))) => p,
                    _ => return Err(()),
                },
            };
            Pred::HasPerson(person, matches!(op, O::IsNot | O::DoesNotContain))
        }
        PropertyType::Checkbox => Pred::Checked(value.as_bool().ok_or(())?),
        k if k.is_date() => {
            if *op == O::Within {
                let within = Within::parse(value.as_str().ok_or(())?);
                let (lo, hi) = within_days(&within, cx.today()).ok_or(())?;
                Pred::Days {
                    lo,
                    hi,
                    negate: false,
                }
            } else {
                let d = filter_day(value, cx).ok_or(())?;
                let (lo, hi, negate) = match op {
                    O::Is => (d, d, false),
                    O::IsNot => (d, d, true),
                    O::Before => (i64::MIN, d.saturating_sub(1), false),
                    O::After => (d.saturating_add(1), i64::MAX, false),
                    O::OnOrBefore => (i64::MIN, d, false),
                    O::OnOrAfter => (d, i64::MAX, false),
                    _ => return Err(()),
                };
                Pred::Days { lo, hi, negate }
            }
        }
        _ => return Err(()),
    }))
}

/// The day a date filter's value names: `"YYYY-MM-DD"`, milliseconds since the epoch (an instant,
/// read in the viewer's zone), or a date value.
fn filter_day(value: &Json, cx: &Context) -> Option<i64> {
    match value {
        Json::String(s) => time::parse_ymd(s),
        Json::Number(n) => Some(time::day_of_instant(n.as_i64()?, cx.offset_ms())),
        j => match Value::from_json(j.clone()) {
            Value::Date(d) => Some(days_of(&d, cx.offset_ms()).0),
            _ => None,
        },
    }
}

/// The first and last day of a `within` span, today included.
pub fn within_days(w: &Within, today: i64) -> Option<(i64, i64)> {
    Some(match w {
        Within::Today => (today, today),
        Within::PastWeek => (today - 7, today),
        Within::PastMonth => (time::add_months(today, -1), today),
        Within::PastYear => (time::add_months(today, -12), today),
        Within::NextWeek => (today, today + 7),
        Within::NextMonth => (today, time::add_months(today, 1)),
        Within::NextYear => (today, time::add_months(today, 12)),
        Within::Other(_) => return None,
    })
}

/// A row's place in the rows a [`RowText`] was made of, to read its text there.
type At<'a> = Option<(&'a RowText, usize)>;

fn eval(node: &Node, row: &Doc, at: At, offset_ms: i64) -> bool {
    match node {
        Node::All(nodes) => nodes.iter().all(|n| eval(n, row, at, offset_ms)),
        Node::Any(nodes) => nodes.is_empty() || nodes.iter().any(|n| eval(n, row, at, offset_ms)),
        Node::Test(t) => test(t, row, at, offset_ms),
    }
}

/// Whether a text condition's `hit` holds for the row's text, lowercased: read from the
/// [`RowText`] when there's one, else lowercased now. A row without text never matches.
fn text_hit(t: &Test, cell: Cell, at: At, hit: impl Fn(&str) -> bool) -> bool {
    match (t.list, at) {
        (Some(list), Some((text, row))) => hit(text.get(list, row)),
        _ => t.text(cell).is_some_and(|s| hit(&super::sort::fold(s))),
    }
}

fn test(t: &Test, row: &Doc, at: At, offset_ms: i64) -> bool {
    let cell = match &t.field {
        FieldRef::Title => Cell::Text(&row.title),
        FieldRef::Created => Cell::Time(row.created),
        FieldRef::Updated => Cell::Time(row.updated),
        FieldRef::CreatedBy => row.created_by().map_or(Cell::Missing, Cell::Person),
        FieldRef::EditedBy => row.edited_by().map_or(Cell::Missing, Cell::Person),
        FieldRef::Stored(id) => row.value(id).map_or(Cell::Missing, Cell::Value),
    };
    match &t.pred {
        Pred::Empty(want) => is_empty(t, cell) == *want,
        Pred::TextIs(needle, negate) => text_hit(t, cell, at, |s| s == needle) != *negate,
        Pred::TextContains(needle, negate) => {
            text_hit(t, cell, at, |s| s.contains(needle.as_str())) != *negate
        }
        Pred::TextStarts(needle) => text_hit(t, cell, at, |s| s.starts_with(needle.as_str())),
        Pred::Num(op, x) => match cell {
            Cell::Value(Value::Number(v)) => match op {
                NumOp::Eq => v == x,
                NumOp::Ne => v != x,
                NumOp::Gt => v > x,
                NumOp::Ge => v >= x,
                NumOp::Lt => v < x,
                NumOp::Le => v <= x,
            },
            _ => *op == NumOp::Ne,
        },
        // A select and a multi-select read each other's values (a property changed between
        // them keeps its rows as they were).
        Pred::SelectIs(id, negate) => {
            let hit = match cell {
                Cell::Value(v) => v.option_id() == Some(id.as_str()),
                _ => false,
            };
            hit != *negate
        }
        Pred::Has(id, negate) => {
            let hit = match cell {
                Cell::Value(Value::Ids(ids)) => ids.iter().any(|x| x == id),
                Cell::Value(Value::Text(x)) => t.kind == PropertyType::MultiSelect && x == id,
                _ => false,
            };
            hit != *negate
        }
        Pred::HasPerson(p, negate) => {
            let hit = match cell {
                Cell::Value(Value::People(people)) => people.contains(p),
                Cell::Person(person) => person == p,
                _ => false,
            };
            hit != *negate
        }
        Pred::Checked(want) => {
            let checked = matches!(cell, Cell::Value(Value::Bool(true)));
            checked == *want
        }
        Pred::Days { lo, hi, negate } => {
            let days = match cell {
                Cell::Time(ms) => {
                    let d = time::day_of_instant(ms, offset_ms);
                    Some((d, d))
                }
                Cell::Value(Value::Date(d)) => Some(days_of(d, offset_ms)),
                _ => None,
            };
            let hit = days.is_some_and(|(first, last)| first <= *hi && last >= *lo);
            hit != *negate
        }
    }
}

fn is_empty(t: &Test, cell: Cell) -> bool {
    match cell {
        Cell::Missing => true,
        Cell::Time(_) | Cell::Person(_) => false,
        Cell::Text(s) => s.is_empty(),
        Cell::Value(v) => match (&t.kind, v, &t.known) {
            (PropertyType::Select, v, Some(known)) if v.option_id().is_some() => !v
                .option_id()
                .is_some_and(|id| known.iter().any(|k| k == id)),
            (PropertyType::MultiSelect, v, Some(known)) if v.option_ids().is_some() => !v
                .option_ids()
                .unwrap_or(&[])
                .iter()
                .any(|id| known.contains(id)),
            (kind, v, _) => !v.fits(kind) || v.is_empty(),
        },
    }
}

#[cfg(test)]
mod tests {
    //! The filter passes the same rows as the first version's, which lowercased each cell's text
    //! (when it wasn't ASCII) as it tested it: that version's tests are kept here as written,
    //! and both filter random rows (from a fixed seed) by random filters, with the rows' text
    //! kept between filters as a view keeps it.

    use super::*;
    use crate::engine::{RowText, query, query_with, sort};
    use crate::model::{PropertyConfig, SelectOption, Values, View, ViewType};

    fn reference_eval(node: &Node, row: &Doc, offset_ms: i64) -> bool {
        match node {
            Node::All(nodes) => nodes.iter().all(|n| reference_eval(n, row, offset_ms)),
            Node::Any(nodes) => {
                nodes.is_empty() || nodes.iter().any(|n| reference_eval(n, row, offset_ms))
            }
            Node::Test(t) => reference_test(t, row, offset_ms),
        }
    }

    fn reference_test(t: &Test, row: &Doc, offset_ms: i64) -> bool {
        let cell = match &t.field {
            FieldRef::Title => Cell::Text(&row.title),
            FieldRef::Created => Cell::Time(row.created),
            FieldRef::Updated => Cell::Time(row.updated),
            FieldRef::CreatedBy => row.created_by().map_or(Cell::Missing, Cell::Person),
            FieldRef::EditedBy => row.edited_by().map_or(Cell::Missing, Cell::Person),
            FieldRef::Stored(id) => row.value(id).map_or(Cell::Missing, Cell::Value),
        };
        match &t.pred {
            Pred::Empty(want) => is_empty(t, cell) == *want,
            Pred::TextIs(needle, negate) => {
                let hit = t.text(cell).is_some_and(|s| eq_folded(s, needle));
                hit != *negate
            }
            Pred::TextContains(needle, negate) => {
                let hit = t.text(cell).is_some_and(|s| contains_folded(s, needle));
                hit != *negate
            }
            Pred::TextStarts(needle) => t.text(cell).is_some_and(|s| starts_folded(s, needle)),
            Pred::Num(op, x) => match cell {
                Cell::Value(Value::Number(v)) => match op {
                    NumOp::Eq => v == x,
                    NumOp::Ne => v != x,
                    NumOp::Gt => v > x,
                    NumOp::Ge => v >= x,
                    NumOp::Lt => v < x,
                    NumOp::Le => v <= x,
                },
                _ => *op == NumOp::Ne,
            },
            // A select and a multi-select read each other's values (a property changed between
            // them keeps its rows as they were).
            Pred::SelectIs(id, negate) => {
                let hit = match cell {
                    Cell::Value(v) => v.option_id() == Some(id.as_str()),
                    _ => false,
                };
                hit != *negate
            }
            Pred::Has(id, negate) => {
                let hit = match cell {
                    Cell::Value(Value::Ids(ids)) => ids.iter().any(|x| x == id),
                    Cell::Value(Value::Text(x)) => t.kind == PropertyType::MultiSelect && x == id,
                    _ => false,
                };
                hit != *negate
            }
            Pred::HasPerson(p, negate) => {
                let hit = match cell {
                    Cell::Value(Value::People(people)) => people.contains(p),
                    Cell::Person(person) => person == p,
                    _ => false,
                };
                hit != *negate
            }
            Pred::Checked(want) => {
                let checked = matches!(cell, Cell::Value(Value::Bool(true)));
                checked == *want
            }
            Pred::Days { lo, hi, negate } => {
                let days = match cell {
                    Cell::Time(ms) => {
                        let d = time::day_of_instant(ms, offset_ms);
                        Some((d, d))
                    }
                    Cell::Value(Value::Date(d)) => Some(days_of(d, offset_ms)),
                    _ => None,
                };
                let hit = days.is_some_and(|(first, last)| first <= *hi && last >= *lo);
                hit != *negate
            }
        }
    }

    fn eq_folded(hay: &str, needle: &str) -> bool {
        if hay.is_ascii() && needle.is_ascii() {
            hay.eq_ignore_ascii_case(needle)
        } else {
            crate::engine::sort::fold(hay) == needle
        }
    }

    fn starts_folded(hay: &str, needle: &str) -> bool {
        if hay.is_ascii() && needle.is_ascii() {
            hay.len() >= needle.len()
                && hay.as_bytes()[..needle.len()].eq_ignore_ascii_case(needle.as_bytes())
        } else {
            crate::engine::sort::fold(hay).starts_with(needle)
        }
    }

    fn contains_folded(hay: &str, needle: &str) -> bool {
        if hay.is_ascii() && needle.is_ascii() {
            let (h, n) = (hay.as_bytes(), needle.as_bytes());
            n.is_empty() || h.windows(n.len()).any(|w| w.eq_ignore_ascii_case(n))
        } else {
            crate::engine::sort::fold(hay).contains(needle)
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

        fn pick<'a>(&mut self, of: &[&'a str]) -> &'a str {
            of[self.below(of.len())]
        }
    }

    /// Words with every kind of case: ASCII, accents (precomposed and combining), a Greek
    /// capital sigma (lowercased by where it stands), letters that lowercase to two, title
    /// case, the Kelvin sign, a dotted capital I, ß, and option ids.
    const WORDS: &[&str] = &[
        "harbour",
        "Harbour",
        "HARBOUR",
        "café",
        "Café",
        "CAFÉ",
        "cafe\u{301}",
        "CAFE\u{301}",
        "über",
        "Über",
        "ÜBER",
        "ΣΊΣΥΦΟΣ",
        "σίσυφος",
        "ΌΣΟΣ",
        "Σ",
        "aΣb",
        "İstanbul",
        "istanbul",
        "i\u{307}",
        "Straße",
        "STRASSE",
        "ǅ",
        "ǆ",
        "Ǆ",
        "\u{212A}elvin",
        "kelvin",
        "ﬃ",
        "ŉ",
        "東京",
        "o1",
        "o2",
        "O1",
        "Warm",
        "warm",
        "-",
        "",
        " ",
    ];

    const OPTIONS: &[(&str, &str)] = &[("o1", "Warm"), ("o2", "CAFÉ Σ"), ("o3", "")];

    fn text(rng: &mut Rng) -> String {
        let n = rng.below(5);
        let mut words: Vec<&str> = (0..n).map(|_| rng.pick(WORDS)).collect();
        if rng.chance(1) {
            words.push(rng.pick(&["Σ.", "ΑΣ", "ς", "Σa"]));
        }
        words.join(if rng.chance(8) { " " } else { "" })
    }

    fn value(rng: &mut Rng) -> Option<Value> {
        match rng.below(10) {
            0 => None,
            1 => Some(Value::Number(2.0)),
            2 => Some(Value::Ids(vec!["o1".into()])),
            3 => Some(Value::Text(rng.pick(&["o1", "o2", "o3", "o9"]).into())),
            _ => Some(Value::Text(text(rng))),
        }
    }

    fn options() -> PropertyConfig {
        PropertyConfig {
            options: Some(
                OPTIONS
                    .iter()
                    .map(|(id, name)| SelectOption {
                        id: (*id).into(),
                        name: (*name).into(),
                        color: Default::default(),
                    })
                    .collect(),
            ),
            ..PropertyConfig::default()
        }
    }

    /// Text properties, with and without a select's options kept (the title too, sometimes),
    /// and a number and a select, so text conditions sit in trees with others.
    fn database(rng: &mut Rng) -> DatabaseFields {
        let mut title = Property::new("name", "Name", PropertyType::Title, "a0");
        if rng.chance(3) {
            title = title.with_config(options());
        }
        DatabaseFields {
            properties: vec![
                title,
                Property::new("note", "Note", PropertyType::Text, "a1"),
                Property::new("was", "Was", PropertyType::Text, "a2").with_config(options()),
                Property::new("mail", "Mail", PropertyType::Email, "a3"),
                Property::new("url", "Url", PropertyType::Url, "a4"),
                Property::new("phone", "Phone", PropertyType::Phone, "a5").with_config(options()),
                Property::new("n", "N", PropertyType::Number, "a6"),
                Property::new("sel", "Sel", PropertyType::Select, "a7").with_config(options()),
            ],
            ..DatabaseFields::default()
        }
    }

    const TEXTS: &[&str] = &["name", "note", "was", "mail", "url", "phone"];

    fn rows(rng: &mut Rng, count: usize) -> Vec<Doc> {
        (0..count)
            .map(|i| {
                let mut values = Values::new();
                for id in &TEXTS[1..] {
                    if let Some(v) = value(rng) {
                        values.insert((*id).into(), v);
                    }
                }
                if rng.chance(8) {
                    values.insert("n".into(), Value::Number(rng.below(10) as f64));
                }
                values.insert("sel".into(), Value::Text(rng.pick(&["o1", "o2"]).into()));
                let title = if rng.chance(2) {
                    rng.pick(&["o1", "o2"]).to_owned()
                } else {
                    text(rng)
                };
                let order = format!("a{}", rng.below(5));
                let mut row = Doc::row(format!("r{i}"), "db", title, order, values);
                if rng.chance(1) {
                    row.deleted = Some(1);
                }
                row
            })
            .collect()
    }

    /// What a condition looks for: a word, part of one, or a whole row's text, often in
    /// another case.
    fn needle(rng: &mut Rng, rows: &[Doc]) -> String {
        let s = match rng.below(4) {
            0 => rng.pick(WORDS).to_owned(),
            1 if !rows.is_empty() => rows[rng.below(rows.len())].title.clone(),
            2 => rng
                .pick(&[
                    "Warm", "warm", "café σ", "AFÉ", "σ", "ς", "Σ", "ss", "i", "e",
                ])
                .into(),
            _ => {
                let w: Vec<char> = rng.pick(WORDS).chars().collect();
                let from = rng.below(w.len() + 1);
                let to = from + rng.below(w.len() - from + 1);
                w[from..to].iter().collect()
            }
        };
        match rng.below(3) {
            0 => s.to_uppercase(),
            1 => s.to_lowercase(),
            _ => s,
        }
    }

    fn condition(rng: &mut Rng, rows: &[Doc]) -> Filter {
        use FilterOp as O;
        match rng.below(10) {
            0 => Filter::condition("n", O::Greater, serde_json::json!(rng.below(10))),
            1 => Filter::condition("sel", O::Is, serde_json::json!(rng.pick(&["o1", "o2"]))),
            2 => Filter::condition(
                rng.pick(TEXTS),
                if rng.chance(5) {
                    O::IsEmpty
                } else {
                    O::IsNotEmpty
                },
                Json::Null,
            ),
            _ => {
                let op = [
                    O::Is,
                    O::IsNot,
                    O::Contains,
                    O::DoesNotContain,
                    O::StartsWith,
                ][rng.below(5)]
                .clone();
                let value = if rng.chance(1) {
                    serde_json::json!("")
                } else {
                    Json::String(needle(rng, rows))
                };
                Filter::condition(rng.pick(TEXTS), op, value)
            }
        }
    }

    fn filter(rng: &mut Rng, rows: &[Doc], depth: usize) -> Filter {
        if depth == 0 || rng.chance(5) {
            return condition(rng, rows);
        }
        let parts = (0..rng.below(4))
            .map(|_| filter(rng, rows, depth - 1))
            .collect();
        if rng.chance(5) {
            Filter::And { and: parts }
        } else {
            Filter::Or { or: parts }
        }
    }

    #[test]
    fn filters_pass_the_same_rows_as_the_first_versions_on_random_rows() {
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        let cx = Context::new(1_794_744_000_000);
        let mut compared = 0;
        let mut passed = 0;
        for round in 0..60 {
            let db = database(&mut rng);
            let count = [0, 1, 9, 200, 1_500][round % 5];
            let rows = rows(&mut rng, count);
            // Kept across this round's filters, as a view keeps it while its rows don't change.
            let mut kept = RowText::new();
            for _ in 0..25 {
                let f = filter(&mut rng, &rows, 3);
                let (compiled, _) = CompiledFilter::new(Some(&f), &db, &cx);
                let mut old: Vec<usize> = (0..rows.len())
                    .filter(|&i| {
                        !rows[i].is_deleted()
                            && compiled
                                .root
                                .as_ref()
                                .is_none_or(|n| reference_eval(n, &rows[i], compiled.offset_ms))
                    })
                    .collect();
                sort::manual(&rows, &mut old);
                let mut view = View::new("v", "V", ViewType::Table, "a0");
                view.filter = Some(f);
                let new = query_with(&db, &view, &rows, &cx, &mut kept);
                assert_eq!(new.rows, old, "round {round}: {:?}", view.filter);
                assert_eq!(
                    query(&db, &view, &rows, &cx).rows,
                    old,
                    "round {round}, fresh"
                );
                // A row at a time, lowercased as it's tested.
                let one: Vec<bool> = rows.iter().map(|r| compiled.matches(r)).collect();
                for &i in &old {
                    assert!(one[i], "round {round}, row {i}: {:?}", view.filter);
                }
                let off = one
                    .iter()
                    .zip(&rows)
                    .filter(|(m, r)| **m && !r.is_deleted());
                assert_eq!(off.count(), old.len(), "round {round}: {:?}", view.filter);
                compared += 1;
                passed += old.len();
            }
        }
        assert!(compared >= 1_500 && passed > 10_000, "{compared}, {passed}");
    }
}
