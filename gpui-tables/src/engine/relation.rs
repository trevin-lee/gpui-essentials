//! Relations: rows that point at rows of another database (or the same one) by doc id.
//!
//! A two-way relation is a pair of relation properties, one in each database, each naming the
//! other as its `backPropertyId`. When one side changes, the crate emits the change on that side
//! (`rowValueChanged`) and what it means for the other (`relationChanged`, with the rows added
//! and removed); [`back_edits`] turns the latter into the other side's `rowValueChanged`s from the
//! host's current values, and the host writes both in one batch.

use std::collections::{HashMap, HashSet};

use crate::model::{
    DatabaseFields, Doc, Edit, Limits, Problem, ProblemKind, Property, PropertyConfig,
    PropertyType, Value, new_id,
};

/// The ids in `new` not in `old` (added) and in `old` not in `new` (removed), each in the order
/// it appears and without repeats.
pub fn diff(old: &[String], new: &[String]) -> (Vec<String>, Vec<String>) {
    let old_set: HashSet<&str> = old.iter().map(String::as_str).collect();
    let new_set: HashSet<&str> = new.iter().map(String::as_str).collect();
    let mut seen = HashSet::new();
    let added = new
        .iter()
        .filter(|id| !old_set.contains(id.as_str()) && seen.insert(id.as_str()))
        .cloned()
        .collect();
    let mut seen = HashSet::new();
    let removed = old
        .iter()
        .filter(|id| !new_set.contains(id.as_str()) && seen.insert(id.as_str()))
        .cloned()
        .collect();
    (added, removed)
}

/// The edits for setting row `row_id`'s relation `p` from `old` to `new`: its `rowValueChanged`
/// (with `new` deduplicated and cut to the limit), and for a two-way relation a
/// `relationChanged` saying which rows on the other side gain or lose this one. Nothing when
/// nothing changes, or when `p` isn't a relation.
pub fn set(
    row_id: &str,
    p: &Property,
    old: &[String],
    new: Vec<String>,
    limits: &Limits,
) -> Vec<Edit> {
    let Some(target) = p.relation() else {
        return Vec::new();
    };
    let Value::Ids(new) = limits.clamp_value(p, Value::Ids(new)) else {
        unreachable!("clamping keeps the shape")
    };
    if new == old {
        return Vec::new();
    }
    let (added, removed) = diff(old, &new);
    let mut edits = vec![Edit::RowValueChanged {
        row_id: row_id.to_owned(),
        property_id: p.id.clone(),
        value: Some(Value::Ids(new)),
    }];
    if let (true, Some(back)) = (target.two_way, target.back_property_id)
        && !(added.is_empty() && removed.is_empty())
    {
        edits.push(Edit::RelationChanged {
            row_id: row_id.to_owned(),
            property_id: p.id.clone(),
            database_id: target.database_id.to_owned(),
            back_property_id: back.to_owned(),
            added,
            removed,
        });
    }
    edits
}

/// The other side's edits for a `relationChanged`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BackEdits {
    /// A `rowValueChanged` for each row on the other side whose back property changes.
    pub edits: Vec<Edit>,
    /// Rows the host doesn't have (or has deleted): nothing to write.
    pub missing: Vec<String>,
    /// Rows whose back property is already at `relation_values`: not written, so the two sides
    /// disagree until the host decides (it can refuse the whole batch instead).
    pub full: Vec<String>,
}

/// Turns a `relationChanged` into `rowValueChanged`s on the other side: each row in `added`
/// gains `rowId` in its back property and each in `removed` loses it. `current` gives a row's
/// back-property ids now (an empty list when it has none), or None when the host hasn't the row.
/// Any other edit gives nothing.
pub fn back_edits(
    edit: &Edit,
    mut current: impl FnMut(&str) -> Option<Vec<String>>,
    limits: &Limits,
) -> BackEdits {
    let mut out = BackEdits::default();
    let Edit::RelationChanged {
        row_id,
        back_property_id,
        added,
        removed,
        ..
    } = edit
    else {
        return out;
    };
    for (other, adding) in added
        .iter()
        .map(|id| (id, true))
        .chain(removed.iter().map(|id| (id, false)))
    {
        let Some(mut ids) = current(other) else {
            out.missing.push(other.clone());
            continue;
        };
        let has = ids.iter().any(|x| x == row_id);
        if adding {
            if has {
                continue;
            }
            if ids.len() >= limits.relation_values {
                out.full.push(other.clone());
                continue;
            }
            ids.push(row_id.clone());
        } else {
            if !has {
                continue;
            }
            ids.retain(|x| x != row_id);
        }
        out.edits.push(Edit::RowValueChanged {
            row_id: other.clone(),
            property_id: back_property_id.clone(),
            value: Some(Value::Ids(ids)),
        });
    }
    out
}

/// A pair of properties for a new two-way relation between database `a_id` and `b_id` (which
/// may be the same): the first goes in `a` and lists rows of `b`, the second the reverse. Emit
/// them as two `propertyCreated`s.
pub fn two_way_pair(
    a_id: &str,
    a_name: &str,
    a_order: &str,
    b_id: &str,
    b_name: &str,
    b_order: &str,
) -> (Property, Property) {
    let (pa, pb) = (new_id(), new_id());
    let config = |db: &str, back: &str| PropertyConfig {
        database_id: Some(db.to_owned()),
        two_way: Some(true),
        back_property_id: Some(back.to_owned()),
        ..PropertyConfig::default()
    };
    (
        Property::new(pa.clone(), a_name, PropertyType::Relation, a_order)
            .with_config(config(b_id, &pb)),
        Property::new(pb, b_name, PropertyType::Relation, b_order).with_config(config(a_id, &pa)),
    )
}

/// Problems with the two-way relations from database `a` (id `a_id`) to database `b` (id
/// `b_id`): each must name a back property in `b` that is a two-way relation to `a` naming it
/// back. Check both directions (and a database against itself) to check a pair whole.
pub fn check_two_way(
    a_id: &str,
    a: &DatabaseFields,
    b_id: &str,
    b: &DatabaseFields,
) -> Vec<Problem> {
    let mut problems = Vec::new();
    for (i, p) in a.properties.iter().enumerate() {
        let Some(t) = p.relation() else { continue };
        if !t.two_way || t.database_id != b_id {
            continue;
        }
        let ok = t
            .back_property_id
            .and_then(|back| b.property(back))
            .and_then(Property::relation)
            .is_some_and(|bt| {
                bt.two_way && bt.database_id == a_id && bt.back_property_id == Some(p.id.as_str())
            });
        if !ok {
            problems.push(Problem {
                at: format!("properties[{i}].config.backPropertyId"),
                kind: ProblemKind::BrokenTwoWay,
            });
        }
    }
    problems
}

/// Finds rows by id, for resolving relation values to the rows they name.
pub struct RowIndex<'a> {
    rows: &'a [Doc],
    by_id: HashMap<&'a str, usize>,
}

impl<'a> RowIndex<'a> {
    pub fn new(rows: &'a [Doc]) -> Self {
        Self {
            rows,
            by_id: rows
                .iter()
                .enumerate()
                .map(|(i, r)| (r.id.as_str(), i))
                .collect(),
        }
    }

    /// A row by id, unless it's tombstoned.
    pub fn get(&self, id: &str) -> Option<&'a Doc> {
        let row = &self.rows[*self.by_id.get(id)?];
        (!row.is_deleted()).then_some(row)
    }

    /// The rows a relation value names, in its order, skipping ids it can't find.
    pub fn resolve(&self, ids: &[String]) -> Vec<&'a Doc> {
        ids.iter().filter_map(|id| self.get(id)).collect()
    }

    /// The ids a relation value names that it can't find (deleted rows, rows not loaded).
    pub fn dangling<'i>(&self, ids: &'i [String]) -> Vec<&'i str> {
        ids.iter()
            .map(String::as_str)
            .filter(|id| self.get(id).is_none())
            .collect()
    }
}
