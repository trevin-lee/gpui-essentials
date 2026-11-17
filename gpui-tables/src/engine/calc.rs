//! Calculations: what a table's footer shows under a column (Count all, Sum, Earliest, Percent
//! checked, ...), worked out over the rows a view shows, or one group's.
//!
//! Each is one pass over the rows given (a median sorts the numbers once), reading values where
//! they lie: 10,000 rows take well under a millisecond each (`tests/calculations.rs`). The rows
//! are indexes into the host's slice of row docs, as [`super::query`] gives them: a view's
//! `rows`, or a group's.
//!
//! What counts as empty is what the `isEmpty` filter matches: no value, an empty text or list, a
//! select's option the property no longer has, and a value whose shape isn't its type's (a text
//! left in a number after a change of type). A checkbox is empty when it isn't checked. Created
//! and edited times are never empty; created-by and edited-by are when the doc has no author.

use std::collections::HashSet;

use super::{Cell, Field};
use crate::model::{Calculation, DatabaseFields, Doc, Person, Property, PropertyType, Value, View};

/// What a calculation gives: the footer formats it as its column's cells are (a number in its
/// format and currency, a date as its cells show dates).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Calculated {
    /// Rows or values counted.
    Count(usize),
    /// A share from 0 to 1.
    Percent(f64),
    /// A number from the column's values (a sum, an average, ...): in the column's format.
    Number(f64),
    /// A date from the column's values (milliseconds since the epoch): with its time when the
    /// value it came from has one (and always for created and edited times), else a date.
    Date { ms: i64, include_time: bool },
    /// A length of time, in milliseconds: a date column's range.
    Duration(i64),
}

/// Works out `calc` for `property` over the rows `indexes` names in `rows` (a view's shown rows,
/// or a group's, as [`super::query`] gives them). None when it doesn't apply to the property's
/// type, or there's nothing to work it out from: an average, median, min, max, range, earliest
/// or latest of no values, or a percent of no rows. A sum of no numbers is 0, and counts of no
/// rows are 0.
pub fn calculate(
    property: &Property,
    calc: &Calculation,
    rows: &[Doc],
    indexes: &[usize],
) -> Option<Calculated> {
    use Calculation as C;
    if !calc.applies_to(&property.kind) {
        return None;
    }
    let reader = Reader::new(property);
    let all = indexes.len();
    let share = |n: usize| (all > 0).then(|| Calculated::Percent(n as f64 / all as f64));
    let cells = || {
        indexes
            .iter()
            .filter_map(|&i| rows.get(i))
            .map(|r| reader.cell(r))
    };
    match calc {
        C::CountAll => Some(Calculated::Count(all)),
        C::CountEmpty => Some(Calculated::Count(all - reader.filled(rows, indexes))),
        C::CountNotEmpty => Some(Calculated::Count(reader.filled(rows, indexes))),
        C::PercentEmpty => share(all - reader.filled(rows, indexes)),
        C::PercentNotEmpty => share(reader.filled(rows, indexes)),
        C::CountValues => Some(Calculated::Count(
            cells().map(|c| reader.values(c).count()).sum(),
        )),
        C::CountUnique => {
            let mut seen: HashSet<Key> = HashSet::new();
            for c in cells() {
                seen.extend(reader.values(c));
            }
            Some(Calculated::Count(seen.len()))
        }
        C::Checked | C::Unchecked | C::PercentChecked | C::PercentUnchecked => {
            let checked = reader.filled(rows, indexes);
            match calc {
                C::Checked => Some(Calculated::Count(checked)),
                C::Unchecked => Some(Calculated::Count(all - checked)),
                C::PercentChecked => share(checked),
                _ => share(all - checked),
            }
        }
        C::Sum | C::Average | C::Median | C::Min | C::Max | C::Range
            if property.kind == PropertyType::Number =>
        {
            numbers(calc, cells().filter_map(number))
        }
        C::Earliest | C::Latest | C::Range => dates(calc, cells().filter_map(date)),
        _ => None,
    }
}

/// Every calculation `view` has on a property `db` has, worked out over `indexes` (as
/// [`calculate`]), by property id in the view's order of them (by id). Those on properties the
/// database hasn't are left out.
pub fn calculate_view<'v>(
    db: &DatabaseFields,
    view: &'v View,
    rows: &[Doc],
    indexes: &[usize],
) -> Vec<(&'v str, &'v Calculation, Option<Calculated>)> {
    view.calculations
        .iter()
        .filter_map(|(id, calc)| {
            let p = db.property(id)?;
            Some((id.as_str(), calc, calculate(p, calc, rows, indexes)))
        })
        .collect()
}

fn numbers(calc: &Calculation, xs: impl Iterator<Item = f64>) -> Option<Calculated> {
    use Calculation as C;
    let number = |x: f64| x.is_finite().then_some(Calculated::Number(x));
    if *calc == C::Median {
        let mut v: Vec<f64> = xs.collect();
        let n = v.len();
        if n == 0 {
            return None;
        }
        let mid = n / 2;
        let (_, &mut hi, _) = v.select_nth_unstable_by(mid, f64::total_cmp);
        if !n.is_multiple_of(2) {
            return number(hi);
        }
        // The largest of the lower half, which select_nth left before `mid`.
        let lo = v[..mid].iter().copied().fold(f64::NEG_INFINITY, f64::max);
        return number(lo / 2. + hi / 2.);
    }
    let (mut n, mut sum, mut min, mut max) = (0usize, 0f64, f64::INFINITY, f64::NEG_INFINITY);
    // A running mean too, for when the sum runs past a double.
    let mut mean = 0f64;
    for x in xs {
        n += 1;
        sum += x;
        mean += (x - mean) / n as f64;
        min = min.min(x);
        max = max.max(x);
    }
    match calc {
        // A sum too big for a double (two values near its limit) is nothing rather than
        // infinity, which no cell can show.
        C::Sum => number(sum),
        _ if n == 0 => None,
        C::Average => number(sum / n as f64).or_else(|| number(mean)),
        C::Min => number(min),
        C::Max => number(max),
        _ => number(max - min),
    }
}

fn dates(calc: &Calculation, ds: impl Iterator<Item = When>) -> Option<Calculated> {
    use Calculation as C;
    let mut first: Option<(i64, bool)> = None;
    let mut last: Option<(i64, bool)> = None;
    for w in ds {
        if first.is_none_or(|(ms, _)| w.start < ms) {
            first = Some((w.start, w.include_time));
        }
        if last.is_none_or(|(ms, _)| w.end > ms) {
            last = Some((w.end, w.include_time));
        }
    }
    let ((start, start_time), (end, end_time)) = (first?, last?);
    Some(match calc {
        C::Earliest => Calculated::Date {
            ms: start,
            include_time: start_time,
        },
        C::Latest => Calculated::Date {
            ms: end,
            include_time: end_time,
        },
        _ => Calculated::Duration(end.saturating_sub(start)),
    })
}

/// A date's first and last instant: a range's start and end, else its start twice.
#[derive(Clone, Copy)]
struct When {
    start: i64,
    end: i64,
    include_time: bool,
}

fn number(c: Cell) -> Option<f64> {
    match c {
        Cell::Value(Value::Number(x)) if x.is_finite() => Some(*x),
        _ => None,
    }
}

fn date(c: Cell) -> Option<When> {
    match c {
        Cell::Time(ms) => Some(When {
            start: ms,
            end: ms,
            include_time: true,
        }),
        Cell::Value(Value::Date(d)) => Some(When {
            start: d.start,
            // A range ending before it starts (which the limits report) counts as its start.
            end: d.end.unwrap_or(d.start).max(d.start),
            include_time: d.include_time,
        }),
        _ => None,
    }
}

/// One value, as Count unique tells values apart: text exactly, numbers by value, dates by
/// their start, end and whether they have a time, list items by id, people by who.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Key<'r> {
    Str(&'r str),
    Num(u64),
    Date(i64, Option<i64>, bool),
    Time(i64),
    Checked,
    Owner,
    Agent(&'r str),
}

/// Reads one property's cells as the calculations need them.
struct Reader<'p> {
    field: Field<'p>,
    kind: &'p PropertyType,
    /// A select's or multi-select's option ids: an id not among them is no value.
    known: Option<HashSet<&'p str>>,
}

impl<'p> Reader<'p> {
    fn new(p: &'p Property) -> Self {
        let known = matches!(p.kind, PropertyType::Select | PropertyType::MultiSelect)
            .then(|| p.options().iter().map(|o| o.id.as_str()).collect());
        Self {
            field: Field::of(p),
            kind: &p.kind,
            known,
        }
    }

    fn cell<'r>(&self, row: &'r Doc) -> Cell<'r> {
        Cell::read(row, self.field)
    }

    /// How many of the rows have a value (for a checkbox, are checked).
    fn filled(&self, rows: &[Doc], indexes: &[usize]) -> usize {
        indexes
            .iter()
            .filter_map(|&i| rows.get(i))
            .filter(|r| !self.is_empty(self.cell(r)))
            .count()
    }

    fn is_known(&self, id: &str) -> bool {
        self.known.as_ref().is_none_or(|k| k.contains(id))
    }

    /// Empty as the `isEmpty` filter has it; a checkbox when it isn't checked.
    fn is_empty(&self, c: Cell) -> bool {
        match c {
            Cell::Missing => true,
            Cell::Time(_) | Cell::Person(_) => false,
            Cell::Text(s) => s.is_empty(),
            Cell::Value(v) => match self.kind {
                PropertyType::Checkbox => v.as_bool() != Some(true),
                PropertyType::Select | PropertyType::MultiSelect => !v
                    .option_ids()
                    .is_some_and(|ids| ids.iter().any(|id| self.is_known(id))),
                kind => !v.fits(kind) || v.is_empty(),
            },
        }
    }

    /// A cell's values: a list's items (a select's or multi-select's known options, people,
    /// related rows), else the one value when it isn't empty.
    fn values<'r>(&'r self, c: Cell<'r>) -> impl Iterator<Item = Key<'r>> + 'r {
        let mut one = None;
        let mut ids: &[String] = &[];
        let mut people: &[Person] = &[];
        if !self.is_empty(c) {
            match c {
                Cell::Text(s) => one = Some(Key::Str(s)),
                Cell::Time(ms) => one = Some(Key::Time(ms)),
                Cell::Person(p) => one = person(p),
                Cell::Missing => {}
                Cell::Value(v) => match (self.kind, v) {
                    (PropertyType::Select | PropertyType::MultiSelect, v) => {
                        ids = v.option_ids().unwrap_or(&[])
                    }
                    (_, Value::Ids(v)) => ids = v,
                    (_, Value::People(v)) => people = v,
                    (_, Value::Text(s)) => one = Some(Key::Str(s)),
                    // -0 and 0 are one value.
                    (_, Value::Number(x)) => one = Some(Key::Num((x + 0.).to_bits())),
                    (_, Value::Date(d)) => one = Some(Key::Date(d.start, d.end, d.include_time)),
                    (_, Value::Bool(_)) => one = Some(Key::Checked),
                    (_, Value::Null | Value::Other(_)) => {}
                },
            }
        }
        one.into_iter()
            .chain(
                ids.iter()
                    .filter(|id| self.is_known(id))
                    .map(|id| Key::Str(id)),
            )
            .chain(people.iter().filter_map(person))
    }
}

/// A person as Count unique tells them apart; a kind this version doesn't know is none.
fn person(p: &Person) -> Option<Key<'_>> {
    match p {
        Person::Owner => Some(Key::Owner),
        Person::Agent(id) => Some(Key::Agent(id)),
        Person::Other(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DateValue, PropertyConfig, SelectOption, Values};
    use Calculation as C;

    fn prop(kind: PropertyType) -> Property {
        Property::new("p", "P", kind, "a0")
    }

    fn select(kind: PropertyType) -> Property {
        prop(kind).with_config(PropertyConfig {
            options: Some(
                ["a", "b", "c"]
                    .iter()
                    .map(|id| SelectOption {
                        id: (*id).into(),
                        name: id.to_uppercase(),
                        color: Default::default(),
                    })
                    .collect(),
            ),
            ..PropertyConfig::default()
        })
    }

    /// Rows holding these values for `p` (None: no value at all).
    fn rows(values: Vec<Option<Value>>) -> Vec<Doc> {
        values
            .into_iter()
            .enumerate()
            .map(|(i, v)| {
                let mut values = Values::new();
                if let Some(v) = v {
                    values.insert("p".into(), v);
                }
                Doc::row(format!("r{i}"), "db", format!("Row {i}"), "a0", values)
            })
            .collect()
    }

    fn all(rows: &[Doc]) -> Vec<usize> {
        (0..rows.len()).collect()
    }

    fn calc(p: &Property, c: Calculation, rows: &[Doc]) -> Option<Calculated> {
        calculate(p, &c, rows, &all(rows))
    }

    fn n(x: f64) -> Option<Value> {
        Some(Value::number(x))
    }

    fn day(d: i64) -> Option<Value> {
        Some(Value::date_on_day(d))
    }

    const DAY: i64 = crate::engine::DAY_MS;

    #[test]
    fn counts_over_text() {
        let p = prop(PropertyType::Text);
        let r = rows(vec![
            Some("kiln".into()),
            Some("Kiln".into()),
            Some("kiln".into()),
            Some("".into()),
            None,
            Some(Value::Null),
        ]);
        assert_eq!(calc(&p, C::CountAll, &r), Some(Calculated::Count(6)));
        assert_eq!(calc(&p, C::CountValues, &r), Some(Calculated::Count(3)));
        assert_eq!(calc(&p, C::CountUnique, &r), Some(Calculated::Count(2)));
        assert_eq!(calc(&p, C::CountEmpty, &r), Some(Calculated::Count(3)));
        assert_eq!(calc(&p, C::CountNotEmpty, &r), Some(Calculated::Count(3)));
        assert_eq!(
            calc(&p, C::PercentEmpty, &r),
            Some(Calculated::Percent(0.5))
        );
        assert_eq!(
            calc(&p, C::PercentNotEmpty, &r),
            Some(Calculated::Percent(0.5))
        );
        // Not a text's.
        assert_eq!(calc(&p, C::Sum, &r), None);
        assert_eq!(calc(&p, C::Checked, &r), None);
        assert_eq!(calc(&p, C::Other("mode".into()), &r), None);
    }

    #[test]
    fn counts_over_the_title_and_a_subset() {
        let p = prop(PropertyType::Title);
        let mut r = rows(vec![None, None, None]);
        r[1].title = String::new();
        r[2].title = r[0].title.clone();
        assert_eq!(calc(&p, C::CountNotEmpty, &r), Some(Calculated::Count(2)));
        assert_eq!(calc(&p, C::CountUnique, &r), Some(Calculated::Count(1)));
        // Only the rows given: a group's.
        assert_eq!(
            calculate(&p, &C::CountEmpty, &r, &[1, 2]),
            Some(Calculated::Count(1))
        );
        // A place past the rows is skipped, never a panic.
        assert_eq!(
            calculate(&p, &C::CountValues, &r, &[0, 99]),
            Some(Calculated::Count(1))
        );
    }

    #[test]
    fn numbers() {
        let p = prop(PropertyType::Number);
        let r = rows(vec![n(4.), n(-1.), None, n(10.), n(2.), n(-0.), n(0.)]);
        assert_eq!(calc(&p, C::Sum, &r), Some(Calculated::Number(15.)));
        assert_eq!(calc(&p, C::Average, &r), Some(Calculated::Number(2.5)));
        // -1, -0, 0, 2, 4, 10: between 0 and 2.
        assert_eq!(calc(&p, C::Median, &r), Some(Calculated::Number(1.)));
        assert_eq!(calc(&p, C::Min, &r), Some(Calculated::Number(-1.)));
        assert_eq!(calc(&p, C::Max, &r), Some(Calculated::Number(10.)));
        assert_eq!(calc(&p, C::Range, &r), Some(Calculated::Number(11.)));
        assert_eq!(calc(&p, C::CountValues, &r), Some(Calculated::Count(6)));
        // -0 and 0 are one value.
        assert_eq!(calc(&p, C::CountUnique, &r), Some(Calculated::Count(5)));
        let odd = rows(vec![n(3.), n(1.), n(2.)]);
        assert_eq!(calc(&p, C::Median, &odd), Some(Calculated::Number(2.)));
        // A zero isn't empty.
        assert_eq!(calc(&p, C::CountEmpty, &r), Some(Calculated::Count(1)));
        // Not a number's.
        assert_eq!(calc(&p, C::Earliest, &r), None);
    }

    #[test]
    fn numbers_at_the_edges() {
        let p = prop(PropertyType::Number);
        let huge = rows(vec![n(f64::MAX), n(f64::MAX)]);
        assert_eq!(calc(&p, C::Sum, &huge), None, "past a double: nothing");
        assert_eq!(
            calc(&p, C::Average, &huge),
            Some(Calculated::Number(f64::MAX))
        );
        assert_eq!(
            calc(&p, C::Median, &huge),
            Some(Calculated::Number(f64::MAX))
        );
        let wide = rows(vec![n(f64::MAX), n(-f64::MAX)]);
        assert_eq!(calc(&p, C::Range, &wide), None);
        assert_eq!(calc(&p, C::Median, &wide), Some(Calculated::Number(0.)));
    }

    #[test]
    fn dates() {
        let p = prop(PropertyType::Date);
        let range = Value::Date(DateValue {
            start: 10 * DAY,
            end: Some(20 * DAY),
            include_time: false,
        });
        let timed = Value::Date(DateValue {
            start: 5 * DAY + 3_600_000,
            end: None,
            include_time: true,
        });
        let r = rows(vec![day(12), Some(range), None, Some(timed), day(12)]);
        assert_eq!(
            calc(&p, C::Earliest, &r),
            Some(Calculated::Date {
                ms: 5 * DAY + 3_600_000,
                include_time: true
            })
        );
        assert_eq!(
            calc(&p, C::Latest, &r),
            Some(Calculated::Date {
                ms: 20 * DAY,
                include_time: false
            })
        );
        assert_eq!(
            calc(&p, C::Range, &r),
            Some(Calculated::Duration(15 * DAY - 3_600_000))
        );
        assert_eq!(calc(&p, C::CountUnique, &r), Some(Calculated::Count(3)));
        assert_eq!(calc(&p, C::Sum, &r), None, "not a date's");
        // A range ending before it starts counts as its start.
        let backwards = rows(vec![Some(Value::Date(DateValue {
            start: 9 * DAY,
            end: Some(DAY),
            include_time: false,
        }))]);
        assert_eq!(
            calc(&p, C::Range, &backwards),
            Some(Calculated::Duration(0))
        );
    }

    #[test]
    fn created_and_edited_times() {
        let p = prop(PropertyType::CreatedTime);
        let mut r = rows(vec![None, None, None]);
        for (i, row) in r.iter_mut().enumerate() {
            row.created = 1_000 * (3 - i as i64);
        }
        assert_eq!(
            calc(&p, C::Earliest, &r),
            Some(Calculated::Date {
                ms: 1_000,
                include_time: true
            })
        );
        assert_eq!(calc(&p, C::Range, &r), Some(Calculated::Duration(2_000)));
        assert_eq!(calc(&p, C::CountEmpty, &r), Some(Calculated::Count(0)));
    }

    #[test]
    fn checkboxes() {
        let p = prop(PropertyType::Checkbox);
        let r = rows(vec![
            Some(true.into()),
            Some(false.into()),
            None,
            Some(true.into()),
        ]);
        assert_eq!(calc(&p, C::Checked, &r), Some(Calculated::Count(2)));
        assert_eq!(calc(&p, C::Unchecked, &r), Some(Calculated::Count(2)));
        assert_eq!(
            calc(&p, C::PercentChecked, &r),
            Some(Calculated::Percent(0.5))
        );
        assert_eq!(
            calc(&p, C::PercentUnchecked, &r),
            Some(Calculated::Percent(0.5))
        );
        // Unchecked is a checkbox's empty.
        assert_eq!(calc(&p, C::CountEmpty, &r), Some(Calculated::Count(2)));
        assert_eq!(calc(&p, C::CountUnique, &r), Some(Calculated::Count(1)));
        assert_eq!(calc(&p, C::Sum, &r), None);
    }

    #[test]
    fn options_people_and_relations() {
        let multi = select(PropertyType::MultiSelect);
        let r = rows(vec![
            Some(Value::ids(["a", "b"])),
            Some(Value::ids(["b", "gone"])),
            Some(Value::ids(["gone"])),
            Some(Value::ids(Vec::<String>::new())),
            None,
        ]);
        // An option the property no longer has is no value.
        assert_eq!(calc(&multi, C::CountValues, &r), Some(Calculated::Count(3)));
        assert_eq!(calc(&multi, C::CountUnique, &r), Some(Calculated::Count(2)));
        assert_eq!(calc(&multi, C::CountEmpty, &r), Some(Calculated::Count(3)));
        // A select reads a list left from a multi-select, as its cells do.
        let one = select(PropertyType::Select);
        let r = rows(vec![
            Some("a".into()),
            Some(Value::ids(["c", "a"])),
            Some("gone".into()),
        ]);
        assert_eq!(calc(&one, C::CountNotEmpty, &r), Some(Calculated::Count(2)));
        assert_eq!(calc(&one, C::CountUnique, &r), Some(Calculated::Count(2)));

        let people = prop(PropertyType::Person);
        let r = rows(vec![
            Some(Value::People(vec![Person::Owner, Person::agent("x")])),
            Some(Value::People(vec![Person::agent("x")])),
            Some(Value::ids(Vec::<String>::new())),
        ]);
        assert_eq!(
            calc(&people, C::CountValues, &r),
            Some(Calculated::Count(3))
        );
        assert_eq!(
            calc(&people, C::CountUnique, &r),
            Some(Calculated::Count(2))
        );
        assert_eq!(calc(&people, C::CountEmpty, &r), Some(Calculated::Count(1)));

        let rel = prop(PropertyType::Relation);
        let r = rows(vec![Some(Value::ids(["x", "y"])), Some(Value::ids(["y"]))]);
        assert_eq!(calc(&rel, C::CountValues, &r), Some(Calculated::Count(3)));
        assert_eq!(calc(&rel, C::CountUnique, &r), Some(Calculated::Count(2)));

        let by = prop(PropertyType::CreatedBy);
        let mut r = rows(vec![None, None, None]);
        r[0].author = Some(Person::Owner);
        r[1].author = Some(Person::Owner);
        assert_eq!(calc(&by, C::CountUnique, &r), Some(Calculated::Count(1)));
        assert_eq!(calc(&by, C::CountEmpty, &r), Some(Calculated::Count(1)));
    }

    #[test]
    fn no_rows() {
        for kind in [
            PropertyType::Text,
            PropertyType::Number,
            PropertyType::Date,
            PropertyType::Checkbox,
            PropertyType::MultiSelect,
            PropertyType::EditedTime,
        ] {
            let p = prop(kind.clone());
            for c in Calculation::ANY
                .iter()
                .chain(Calculation::of_type(&kind).iter())
            {
                let want = match c {
                    C::PercentEmpty
                    | C::PercentNotEmpty
                    | C::PercentChecked
                    | C::PercentUnchecked
                    | C::Average
                    | C::Median
                    | C::Min
                    | C::Max
                    | C::Range
                    | C::Earliest
                    | C::Latest => None,
                    C::Sum => Some(Calculated::Number(0.)),
                    _ => Some(Calculated::Count(0)),
                };
                assert_eq!(calculate(&p, c, &[], &[]), want, "{kind} {c}");
            }
        }
    }

    #[test]
    fn all_empty() {
        let r = rows(vec![None, Some(Value::Null), None]);
        let num = prop(PropertyType::Number);
        assert_eq!(calc(&num, C::Sum, &r), Some(Calculated::Number(0.)));
        assert_eq!(calc(&num, C::Average, &r), None);
        assert_eq!(calc(&num, C::Median, &r), None);
        assert_eq!(calc(&num, C::Range, &r), None);
        assert_eq!(
            calc(&num, C::PercentEmpty, &r),
            Some(Calculated::Percent(1.))
        );
        assert_eq!(calc(&num, C::CountUnique, &r), Some(Calculated::Count(0)));
        let date = prop(PropertyType::Date);
        assert_eq!(calc(&date, C::Earliest, &r), None);
        assert_eq!(calc(&date, C::Range, &r), None);
        let check = prop(PropertyType::Checkbox);
        assert_eq!(calc(&check, C::Checked, &r), Some(Calculated::Count(0)));
        assert_eq!(
            calc(&check, C::PercentUnchecked, &r),
            Some(Calculated::Percent(1.))
        );
    }

    #[test]
    fn mixed_shapes_after_a_change_of_type() {
        // A text column made a number: its old texts are no values, its numbers count.
        let r = rows(vec![
            Some("12".into()),
            n(3.),
            Some(Value::ids(["a"])),
            day(4),
            Some(true.into()),
            Some(Value::Other(serde_json::json!({"x": 1}))),
            n(5.),
        ]);
        let num = prop(PropertyType::Number);
        assert_eq!(calc(&num, C::Sum, &r), Some(Calculated::Number(8.)));
        assert_eq!(calc(&num, C::Median, &r), Some(Calculated::Number(4.)));
        assert_eq!(calc(&num, C::CountNotEmpty, &r), Some(Calculated::Count(2)));
        assert_eq!(calc(&num, C::CountValues, &r), Some(Calculated::Count(2)));
        assert_eq!(calc(&num, C::CountUnique, &r), Some(Calculated::Count(2)));
        // The same rows as a date: the one date.
        let date = prop(PropertyType::Date);
        assert_eq!(calc(&date, C::Range, &r), Some(Calculated::Duration(0)));
        assert_eq!(calc(&date, C::CountEmpty, &r), Some(Calculated::Count(6)));
        // As a checkbox: the one true.
        let check = prop(PropertyType::Checkbox);
        assert_eq!(calc(&check, C::Checked, &r), Some(Calculated::Count(1)));
        // As text: the two texts' shape.
        let text = prop(PropertyType::Text);
        assert_eq!(calc(&text, C::CountValues, &r), Some(Calculated::Count(1)));
        // As a multi-select without options: nothing it knows.
        let multi = prop(PropertyType::MultiSelect);
        assert_eq!(
            calc(&multi, C::CountNotEmpty, &r),
            Some(Calculated::Count(0))
        );
        // A sum left on a column now a date works out as nothing.
        assert_eq!(calc(&date, C::Sum, &r), None);
    }

    #[test]
    fn a_views_calculations() {
        let mut db = DatabaseFields::default();
        db.properties.push(prop(PropertyType::Number));
        let mut view = View::new("v", "V", crate::model::ViewType::Table, "a0");
        view.calculations.insert("p".into(), C::Sum);
        view.calculations.insert("gone".into(), C::CountAll);
        let r = rows(vec![n(1.), n(2.)]);
        assert_eq!(
            calculate_view(&db, &view, &r, &all(&r)),
            vec![("p", &C::Sum, Some(Calculated::Number(3.)))]
        );
    }

    #[test]
    fn which_apply() {
        assert!(C::Sum.applies_to(&PropertyType::Number));
        assert!(!C::Sum.applies_to(&PropertyType::Date));
        assert!(C::Range.applies_to(&PropertyType::Date));
        assert!(C::Range.applies_to(&PropertyType::EditedTime));
        assert!(C::Latest.applies_to(&PropertyType::CreatedTime));
        assert!(C::PercentChecked.applies_to(&PropertyType::Checkbox));
        assert!(!C::PercentChecked.applies_to(&PropertyType::Text));
        assert!(C::CountUnique.applies_to(&PropertyType::Other("formula".into())));
        assert!(!C::Other("mode".into()).applies_to(&PropertyType::Number));
        // Every one this version knows is in a menu.
        for c in Calculation::ALL {
            assert!(
                Calculation::ANY.contains(c)
                    || [
                        Calculation::NUMBER,
                        Calculation::DATE,
                        Calculation::CHECKBOX
                    ]
                    .iter()
                    .any(|l| l.contains(c)),
                "{c}"
            );
        }
    }
}
