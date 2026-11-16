//! Filtering rows: a view's filter tree, compiled once against the database's properties and
//! then tested against each row without allocating (for ASCII text).
//!
//! What each operator means:
//!
//! - **Text** (title, text, url, email, phone): `is`, `isNot`, `contains`, `doesNotContain`,
//!   `startsWith`, ignoring case.
//! - **Number:** `is`, `isNot`, `greater`, `greaterOrEqual`, `less`, `lessOrEqual`.
//! - **Select:** `is` and `isNot` an option id.
//! - **Multi-select, person, relation:** `contains` and `doesNotContain` an option id, a person,
//!   or a row id.
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

use super::time::{self, days_of};
use super::{Cell, Context, Field, Issue, Part};
use crate::model::{
    Condition, DatabaseFields, Doc, Filter, FilterOp, Person, PropertyType, Value, Within,
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
}

#[derive(Debug)]
enum FieldRef {
    Title,
    Created,
    Updated,
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
            .is_none_or(|n| eval(n, row, self.offset_ms))
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
            Field::Stored(p) => FieldRef::Stored(p.id.clone()),
        };
        let known = matches!(p.kind, PropertyType::Select | PropertyType::MultiSelect)
            .then(|| p.options().iter().map(|o| o.id.clone()).collect());
        Some(Test {
            field,
            kind: p.kind.clone(),
            pred,
            known,
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
        PropertyType::Person => {
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
            Pred::HasPerson(person, *op == O::DoesNotContain)
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

fn eval(node: &Node, row: &Doc, offset_ms: i64) -> bool {
    match node {
        Node::All(nodes) => nodes.iter().all(|n| eval(n, row, offset_ms)),
        Node::Any(nodes) => nodes.is_empty() || nodes.iter().any(|n| eval(n, row, offset_ms)),
        Node::Test(t) => test(t, row, offset_ms),
    }
}

fn test(t: &Test, row: &Doc, offset_ms: i64) -> bool {
    let cell = match &t.field {
        FieldRef::Title => Cell::Text(&row.title),
        FieldRef::Created => Cell::Time(row.created),
        FieldRef::Updated => Cell::Time(row.updated),
        FieldRef::Stored(id) => row.value(id).map_or(Cell::Missing, Cell::Value),
    };
    match &t.pred {
        Pred::Empty(want) => is_empty(t, cell) == *want,
        Pred::TextIs(needle, negate) => {
            let hit = cell.text().is_some_and(|s| eq_folded(s, needle));
            hit != *negate
        }
        Pred::TextContains(needle, negate) => {
            let hit = cell.text().is_some_and(|s| contains_folded(s, needle));
            hit != *negate
        }
        Pred::TextStarts(needle) => cell.text().is_some_and(|s| starts_folded(s, needle)),
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
        Pred::SelectIs(id, negate) => {
            let hit = cell.text().is_some_and(|s| s == id);
            hit != *negate
        }
        Pred::Has(id, negate) => {
            let hit = match cell {
                Cell::Value(Value::Ids(ids)) => ids.iter().any(|x| x == id),
                _ => false,
            };
            hit != *negate
        }
        Pred::HasPerson(p, negate) => {
            let hit = match cell {
                Cell::Value(Value::People(people)) => people.contains(p),
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
        Cell::Time(_) => false,
        Cell::Text(s) => s.is_empty(),
        Cell::Value(v) => match (&t.kind, v, &t.known) {
            (PropertyType::Select, Value::Text(id), Some(known)) => !known.contains(id),
            (PropertyType::MultiSelect, Value::Ids(ids), Some(known)) => {
                !ids.iter().any(|id| known.contains(id))
            }
            (kind, v, _) => !v.fits(kind) || v.is_empty(),
        },
    }
}

fn eq_folded(hay: &str, needle: &str) -> bool {
    if hay.is_ascii() && needle.is_ascii() {
        hay.eq_ignore_ascii_case(needle)
    } else {
        super::sort::fold(hay) == needle
    }
}

fn starts_folded(hay: &str, needle: &str) -> bool {
    if hay.is_ascii() && needle.is_ascii() {
        hay.len() >= needle.len()
            && hay.as_bytes()[..needle.len()].eq_ignore_ascii_case(needle.as_bytes())
    } else {
        super::sort::fold(hay).starts_with(needle)
    }
}

fn contains_folded(hay: &str, needle: &str) -> bool {
    if hay.is_ascii() && needle.is_ascii() {
        let (h, n) = (hay.as_bytes(), needle.as_bytes());
        n.is_empty() || h.windows(n.len()).any(|w| w.eq_ignore_ascii_case(n))
    } else {
        super::sort::fold(hay).contains(needle)
    }
}
