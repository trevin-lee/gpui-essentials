//! The engine: what a view shows of a database's rows, worked out without any UI. It filters,
//! sorts and groups rows the way a view says, and helps keep relations consistent.
//!
//! It also works out a table footer's calculations ([`calc`]) over the rows a view shows, or a
//! group's.
//!
//! Everything here works on indexes into the host's slice of row docs, so nothing is copied, and
//! each step costs one pass to build its keys plus the sort: 10,000 rows filter, sort and group
//! in a few milliseconds (see the README's performance section and `tests/performance.rs`).
//!
//! What it can't use (a sort by a property that's gone, an operator that doesn't apply, a filter
//! deeper than the limit) it leaves out and reports as an [`Issue`]; it never panics on the
//! host's data.

pub mod calc;
pub mod filter;
pub mod group;
pub mod relation;
pub mod search;
pub mod sort;
pub mod time;

pub use calc::{Calculated, calculate, calculate_view};
pub use filter::CompiledFilter;
pub use group::{DateGroup, Group, GroupKey, Relative};
pub use time::DAY_MS;

use crate::model::{
    DatabaseFields, DateBucket, Doc, FilterOp, Limits, Person, Property, PropertyType, Value, View,
};

/// Names the engine can't know: the host's names for people and the titles of rows in other
/// databases. Sorting and grouping by a person or relation use them; without them they fall back
/// to agents' ids and rows' ids.
pub trait Lookup {
    /// The name to show for a person: the owner's name, an agent's display name.
    fn person_name(&self, person: &Person) -> Option<String> {
        let _ = person;
        None
    }

    /// The title of a row of another database (or this one), for a relation.
    fn row_title(&self, database_id: &str, row_id: &str) -> Option<String> {
        let _ = (database_id, row_id);
        None
    }
}

/// A [`Lookup`] that knows nothing.
pub struct NoLookup;

impl Lookup for NoLookup {}

/// What the engine needs besides the data: the time now and the viewer's time zone (for date
/// filters and groups relative to today), the week's first day, the host's names, and the limits.
#[derive(Clone, Copy)]
pub struct Context<'a> {
    /// Milliseconds since the Unix epoch.
    pub now: i64,
    /// The viewer's offset from UTC now, in minutes (east positive).
    pub utc_offset_minutes: i32,
    /// Whether weeks start on Monday (else Sunday), for grouping by week.
    pub week_starts_monday: bool,
    pub lookup: &'a dyn Lookup,
    pub limits: Limits,
}

impl Context<'static> {
    /// At `now`, in UTC, weeks from Monday, with no names and the default limits.
    pub fn new(now: i64) -> Self {
        Self {
            now,
            utc_offset_minutes: 0,
            week_starts_monday: true,
            lookup: &NoLookup,
            limits: Limits::default(),
        }
    }
}

impl<'a> Context<'a> {
    pub fn with_lookup<'b>(self, lookup: &'b dyn Lookup) -> Context<'b> {
        Context {
            now: self.now,
            utc_offset_minutes: self.utc_offset_minutes,
            week_starts_monday: self.week_starts_monday,
            lookup,
            limits: self.limits,
        }
    }

    pub fn with_offset(mut self, minutes: i32) -> Self {
        self.utc_offset_minutes = minutes;
        self
    }

    pub(crate) fn offset_ms(&self) -> i64 {
        i64::from(self.utc_offset_minutes) * 60_000
    }

    /// Today, as days since 1970-01-01 in the viewer's zone.
    pub fn today(&self) -> i64 {
        time::day_of_instant(self.now, self.offset_ms())
    }
}

/// Where in a view something the engine couldn't use was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Sort,
    Filter,
    GroupBy,
}

/// Something in a view the engine left out, and why. A UI can show these beside the view's
/// controls ("this filter's property was deleted").
#[derive(Clone, Debug, PartialEq)]
pub enum Issue {
    /// The view names a property the database hasn't.
    UnknownProperty { part: Part, property_id: String },
    /// A filter operator that doesn't apply to its property's type.
    OpNotForType { property_id: String, op: FilterOp },
    /// A filter value the operator can't use.
    BadValue { property_id: String, op: FilterOp },
    /// Filter levels past the limit were left out.
    FilterTooDeep { limit: usize },
    /// Filter nodes past the limit were left out.
    FilterTooBig { limit: usize },
    /// Sorts past the limit were left out.
    TooManySorts { limit: usize },
    /// The property's type can't group (text, number, ...), so the view isn't grouped.
    CantGroupBy { property_id: String },
}

/// What a view shows: the rows that pass its filter, in its order, and its groups.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ViewRows {
    /// Indexes into the rows given, in the view's order: by its sorts, then the manual order.
    pub rows: Vec<usize>,
    /// When the view groups, its groups in order, each with its rows in the view's order. A row
    /// with several options (or people) is in each of their groups.
    pub groups: Option<Vec<Group>>,
    pub issues: Vec<Issue>,
}

/// Works out what `view` shows of `rows` (the database's row docs, in any order): drops
/// tombstoned rows, applies the filter, puts the rest in the manual order (`order`, then id),
/// sorts them by the view's sorts (stably, so the manual order breaks ties), and groups them.
pub fn query(db: &DatabaseFields, view: &View, rows: &[Doc], cx: &Context) -> ViewRows {
    let mut issues = Vec::new();
    let (filter, filter_issues) = CompiledFilter::new(view.filter.as_ref(), db, cx);
    issues.extend(filter_issues);

    let mut indexes: Vec<usize> = (0..rows.len())
        .filter(|&i| !rows[i].is_deleted() && filter.matches(&rows[i]))
        .collect();
    sort::manual(rows, &mut indexes);
    issues.extend(sort::sort(db, &view.sort, rows, &mut indexes, cx));

    let groups = view
        .group_by
        .as_deref()
        .and_then(|id| match db.property(id) {
            None => {
                issues.push(Issue::UnknownProperty {
                    part: Part::GroupBy,
                    property_id: id.to_owned(),
                });
                None
            }
            Some(p) => {
                let bucket = view.group_date.clone().unwrap_or(DateBucket::Relative);
                let groups = group::group(p, &bucket, rows, &indexes, cx);
                if groups.is_none() {
                    issues.push(Issue::CantGroupBy {
                        property_id: id.to_owned(),
                    });
                }
                groups
            }
        });
    ViewRows {
        rows: indexes,
        groups,
        issues,
    }
}

/// Where a property's value lives on a row doc.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Field<'p> {
    Title,
    Created,
    Updated,
    /// The doc's `author`.
    CreatedBy,
    /// The doc's `updatedBy`, else its `author`.
    EditedBy,
    Stored(&'p Property),
}

impl<'p> Field<'p> {
    pub(crate) fn of(p: &'p Property) -> Self {
        match p.kind {
            PropertyType::Title => Self::Title,
            PropertyType::CreatedTime => Self::Created,
            PropertyType::EditedTime => Self::Updated,
            PropertyType::CreatedBy => Self::CreatedBy,
            PropertyType::EditedBy => Self::EditedBy,
            _ => Self::Stored(p),
        }
    }
}

/// A row's value for a field, read without copying.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Cell<'r> {
    Text(&'r str),
    Time(i64),
    /// A derived person: who created or last changed the row.
    Person(&'r Person),
    Value(&'r Value),
    Missing,
}

impl<'r> Cell<'r> {
    pub(crate) fn read(row: &'r Doc, field: Field<'_>) -> Self {
        match field {
            Field::Title => Cell::Text(&row.title),
            Field::Created => Cell::Time(row.created),
            Field::Updated => Cell::Time(row.updated),
            Field::CreatedBy => row.created_by().map_or(Cell::Missing, Cell::Person),
            Field::EditedBy => row.edited_by().map_or(Cell::Missing, Cell::Person),
            Field::Stored(p) => row.value(&p.id).map_or(Cell::Missing, Cell::Value),
        }
    }

    pub(crate) fn text(self) -> Option<&'r str> {
        match self {
            Cell::Text(s) => Some(s),
            Cell::Value(Value::Text(s)) => Some(s),
            _ => None,
        }
    }
}
