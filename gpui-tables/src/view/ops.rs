//! What the table's controls do to a view, without drawing anything: its columns' layout, and the
//! changed view that resizing, moving, hiding, sorting, filtering and folding make. Each change
//! returns the whole view, for a `viewChanged`.

use serde_json::Value as Json;

use super::format;
use crate::engine::{DAY_MS, GroupKey};
use crate::model::{
    Column, Condition, DatabaseFields, DateValue, Direction, Filter, FilterOp, Limits, Person,
    Property, PropertyType, Sort, Value, Values, View,
};
use crate::order;

/// A column as the table draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct ColumnLayout {
    pub property_id: String,
    /// In points.
    pub width: f32,
    /// The property's place in the database's `properties`.
    pub property: usize,
}

/// Every property's column in the view's order, the view's own columns first (by their
/// `order`) and then the properties it has none for (in the properties' order), as `Column`s.
pub fn all_columns(db: &DatabaseFields, view: &View) -> Vec<Column> {
    let mut out: Vec<Column> = Vec::new();
    for c in view.columns_in_order() {
        if db.property(&c.property_id).is_some()
            && !out.iter().any(|o| o.property_id == c.property_id)
        {
            out.push(c.clone());
        }
    }
    for p in db.properties_in_order() {
        if !out.iter().any(|o| o.property_id == p.id) {
            out.push(Column {
                property_id: p.id.clone(),
                width: None,
                hidden: false,
                order: String::new(),
            });
        }
    }
    out
}

/// The columns the table shows, the title's first (it can't be hidden or moved), each at its
/// width (or the default) within the theme's least and most.
pub fn layout(
    db: &DatabaseFields,
    view: &View,
    default_width: f32,
    title_width: f32,
    min: f32,
    max: f32,
) -> Vec<ColumnLayout> {
    let title = db.title_property().map(|p| p.id.as_str());
    let mut out: Vec<ColumnLayout> = all_columns(db, view)
        .into_iter()
        .filter_map(|c| {
            let is_title = Some(c.property_id.as_str()) == title;
            if c.hidden && !is_title {
                return None;
            }
            let property = db.properties.iter().position(|p| p.id == c.property_id)?;
            let fallback = if is_title { title_width } else { default_width };
            let width = c
                .width
                .map(|w| w as f32)
                .filter(|w| w.is_finite())
                .unwrap_or(fallback)
                .clamp(min, max.max(min));
            Some(ColumnLayout {
                property_id: c.property_id,
                width,
                property,
            })
        })
        .collect();
    if let Some(i) = out
        .iter()
        .position(|c| Some(c.property_id.as_str()) == title)
    {
        let t = out.remove(i);
        out.insert(0, t);
    }
    out
}

/// Gives every column an order key in the list's order: keeps the keys that are already valid
/// and rising, makes one between its neighbours for `moved`, and renumbers the whole list when
/// that can't be done (keys missing, malformed or out of order).
fn place(cols: &mut [Column], moved: Option<usize>) {
    let ok = cols
        .iter()
        .enumerate()
        .filter(|(i, _)| Some(*i) != moved)
        .map(|(_, c)| c.order.as_str())
        .collect::<Vec<_>>();
    let rising = ok.iter().all(|k| order::is_valid(k)) && ok.windows(2).all(|w| w[0] < w[1]);
    if rising && let Some(m) = moved {
        let before = m.checked_sub(1).map(|i| cols[i].order.clone());
        let after = cols.get(m + 1).map(|c| c.order.clone());
        if let Ok(k) = order::between(before.as_deref(), after.as_deref()) {
            cols[m].order = k;
            return;
        }
    } else if rising {
        return;
    }
    let keys = order::spread(cols.len());
    for (c, k) in cols.iter_mut().zip(keys) {
        c.order = k;
    }
}

/// The view with every property's column written out (the ones it lacked at the default
/// width), keys placed.
fn materialized(db: &DatabaseFields, view: &View) -> (View, Vec<Column>) {
    let cols = all_columns(db, view);
    (view.clone(), cols)
}

fn with_columns(mut view: View, mut cols: Vec<Column>, moved: Option<usize>) -> View {
    place(&mut cols, moved);
    view.columns = cols;
    view
}

/// The view with `property_id`'s column at `width` points (rounded to a whole point).
pub fn resize(db: &DatabaseFields, view: &View, property_id: &str, width: f32) -> View {
    let (view, mut cols) = materialized(db, view);
    if let Some(c) = cols.iter_mut().find(|c| c.property_id == property_id) {
        c.width = Some(f64::from(width.round()));
    }
    with_columns(view, cols, None)
}

/// The view with a column for `property_id` (a property just added to `db`) shown after every
/// other, the view's columns written out with order keys.
pub fn add_column(db: &DatabaseFields, view: &View, property_id: &str) -> View {
    let (view, mut cols) = materialized(db, view);
    let mut col = match cols.iter().position(|c| c.property_id == property_id) {
        Some(i) => cols.remove(i),
        None => Column {
            property_id: property_id.to_owned(),
            width: None,
            hidden: false,
            order: String::new(),
        },
    };
    col.hidden = false;
    cols.push(col);
    let last = cols.len() - 1;
    with_columns(view, cols, Some(last))
}

/// The view with `property_id`'s column hidden or shown. The title's can't be hidden.
pub fn set_hidden(db: &DatabaseFields, view: &View, property_id: &str, hidden: bool) -> View {
    if hidden && db.title_property().is_some_and(|p| p.id == property_id) {
        return view.clone();
    }
    let (view, mut cols) = materialized(db, view);
    if let Some(c) = cols.iter_mut().find(|c| c.property_id == property_id) {
        c.hidden = hidden;
    }
    with_columns(view, cols, None)
}

/// The view with `property_id`'s column moved to be the `to`th of the shown columns (counting
/// the title's, which stays first). Hidden columns keep their places among the others.
pub fn move_column(db: &DatabaseFields, view: &View, property_id: &str, to: usize) -> View {
    let title = db.title_property().map(|p| p.id.clone());
    if title.as_deref() == Some(property_id) {
        return view.clone();
    }
    let (view, mut cols) = materialized(db, view);
    let Some(from) = cols.iter().position(|c| c.property_id == property_id) else {
        return view;
    };
    // The shown columns as the table lays them out: the title's first.
    let mut shown: Vec<String> = cols
        .iter()
        .filter(|c| !c.hidden || Some(&c.property_id) == title.as_ref())
        .map(|c| c.property_id.clone())
        .collect();
    if let Some(t) = &title
        && let Some(i) = shown.iter().position(|p| p == t)
    {
        let t = shown.remove(i);
        shown.insert(0, t);
    }
    let Some(now) = shown.iter().position(|p| p == property_id) else {
        return view;
    };
    let to = to.clamp(usize::from(title.is_some()), shown.len().saturating_sub(1));
    if to == now {
        return view;
    }
    shown.remove(now);
    shown.insert(to, property_id.to_owned());
    // Put it just before the column that now follows it (or after the one before it, at the
    // end), among all the columns.
    let moving = cols.remove(from);
    let at = match shown.get(to + 1) {
        Some(next) => cols
            .iter()
            .position(|c| &c.property_id == next)
            .unwrap_or(cols.len()),
        None => {
            let prev = &shown[to - 1];
            cols.iter()
                .position(|c| &c.property_id == prev)
                .map_or(cols.len(), |i| i + 1)
        }
    };
    cols.insert(at, moving);
    with_columns(view, cols, Some(at))
}

/// The view sorted by `property_id`: by it alone, or (with `add`, a shift-click) added to the
/// sorts it has, replacing that property's sort if it had one. Sorts past the limit are dropped.
pub fn sort(
    view: &View,
    property_id: &str,
    direction: Direction,
    add: bool,
    limits: &Limits,
) -> View {
    let mut view = view.clone();
    let sort = Sort {
        property_id: property_id.to_owned(),
        direction,
    };
    if add {
        match view.sort.iter_mut().find(|s| s.property_id == property_id) {
            Some(s) => *s = sort,
            None => view.sort.push(sort),
        }
        view.sort.truncate(limits.sorts.max(1));
    } else {
        view.sort = vec![sort];
    }
    view
}

/// The view without its sort by `property_id`.
pub fn unsort(view: &View, property_id: &str) -> View {
    let mut view = view.clone();
    view.sort.retain(|s| s.property_id != property_id);
    view
}

/// The conditions at the top of a view's filter: the filter itself when it's one condition, or
/// the conditions directly in its `and`.
pub fn top_conditions(view: &View) -> Vec<&Condition> {
    match &view.filter {
        Some(Filter::Condition(c)) => vec![c],
        Some(Filter::And { and }) => and
            .iter()
            .filter_map(|f| match f {
                Filter::Condition(c) => Some(c),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Whether the view's filter tests `property_id` anywhere.
pub fn filters_by(view: &View, property_id: &str) -> bool {
    view.filter.as_ref().is_some_and(|f| {
        f.walk()
            .any(|(n, _)| matches!(n, Filter::Condition(c) if c.property_id == property_id))
    })
}

/// The view with `condition` in its filter: in place of the conditions at the top that test
/// the same property, and-ed with the rest.
pub fn filter(view: &View, condition: Condition) -> View {
    let mut view = clear_filter(view, &condition.property_id);
    let new = Filter::Condition(condition);
    view.filter = Some(match view.filter.take() {
        None => new,
        Some(Filter::And { mut and }) => {
            and.push(new);
            Filter::And { and }
        }
        Some(other) => Filter::And {
            and: vec![other, new],
        },
    });
    view
}

/// The view without the conditions at the top of its filter that test `property_id` (ones
/// deeper in an `or` are the filter's own business and stay).
pub fn clear_filter(view: &View, property_id: &str) -> View {
    let mut view = view.clone();
    view.filter = match view.filter.take() {
        Some(Filter::Condition(c)) if c.property_id == property_id => None,
        Some(Filter::And { and }) => {
            let mut and: Vec<Filter> = and
                .into_iter()
                .filter(|f| !matches!(f, Filter::Condition(c) if c.property_id == property_id))
                .collect();
            match and.len() {
                0 => None,
                1 => and.pop(),
                _ => Some(Filter::And { and }),
            }
        }
        other => other,
    };
    view
}

/// The view with the group `key` folded or unfolded.
pub fn toggle_group(view: &View, key: &GroupKey, limits: &Limits) -> View {
    let mut view = view.clone();
    let key = key.to_string();
    if let Some(i) = view.collapsed_groups.iter().position(|k| *k == key) {
        view.collapsed_groups.remove(i);
    } else if view.collapsed_groups.len() < limits.collapsed_groups {
        view.collapsed_groups.push(key);
    }
    view
}

fn person_of(j: &Json) -> Option<Person> {
    match j {
        Json::String(s) if s == "owner" => Some(Person::Owner),
        Json::String(s) => s.strip_prefix("agent:").map(Person::agent),
        other => Person::from_json(other).filter(|p| p.kind().is_some()),
    }
}

/// The values a new row gets so it stays where it was made: the group's value, and what the
/// conditions at the top of the filter ask for (an option, a checked box, a person, a related
/// row, a text it must be). The derived properties (created and edited times and people) get
/// nothing: the host fills the doc's own fields.
pub fn prefill(db: &DatabaseFields, view: &View, group: Option<&GroupKey>, today: i64) -> Values {
    let mut values = Values::new();
    for c in top_conditions(view) {
        let Some(p) = db.property(&c.property_id) else {
            continue;
        };
        let v = match (&p.kind, &c.op) {
            (PropertyType::Select, FilterOp::Is) => c
                .value
                .as_str()
                .filter(|id| p.option(id).is_some())
                .map(Value::text),
            (PropertyType::MultiSelect, FilterOp::Contains) => c
                .value
                .as_str()
                .filter(|id| p.option(id).is_some())
                .map(|id| Value::ids([id])),
            (PropertyType::Relation, FilterOp::Contains) => {
                c.value.as_str().map(|id| Value::ids([id]))
            }
            (PropertyType::Checkbox, FilterOp::IsChecked) => Some(Value::Bool(true)),
            (PropertyType::Checkbox, FilterOp::IsNotChecked) => Some(Value::Bool(false)),
            (PropertyType::Checkbox, FilterOp::Is) => c.value.as_bool().map(Value::Bool),
            (PropertyType::Person, FilterOp::Contains) => {
                person_of(&c.value).map(|p| Value::People(vec![p]))
            }
            (
                PropertyType::Text | PropertyType::Url | PropertyType::Email | PropertyType::Phone,
                FilterOp::Is,
            ) => c.value.as_str().filter(|s| !s.is_empty()).map(Value::text),
            _ => None,
        };
        if let Some(v) = v {
            values.insert(p.id.clone(), v);
        }
    }
    if let (Some(key), Some(p)) = (group, view.group_by.as_deref().and_then(|g| db.property(g))) {
        if let Some(v) = group_value(p, key, today) {
            values.insert(p.id.clone(), v);
        } else if matches!(key, GroupKey::Empty) {
            values.remove(&p.id);
        }
    }
    values
}

/// The value that puts a row in group `key` of property `p`.
pub fn group_value(p: &Property, key: &GroupKey, today: i64) -> Option<Value> {
    match (&p.kind, key) {
        (PropertyType::Select, GroupKey::Option(id)) => Some(Value::text(id.clone())),
        (PropertyType::MultiSelect, GroupKey::Option(id)) => Some(Value::ids([id.clone()])),
        (PropertyType::Checkbox, GroupKey::Checked(b)) => Some(Value::Bool(*b)),
        (PropertyType::Person, GroupKey::Person(person)) => {
            Some(Value::People(vec![person.clone()]))
        }
        (PropertyType::Date, GroupKey::Date(g)) => {
            format::group_day(g, today).map(Value::date_on_day)
        }
        _ => None,
    }
}

/// The order key for a row added at the end of the manual order: after the last valid key
/// among `orders`, or the first key when there's none.
pub fn append_order<'a>(orders: impl IntoIterator<Item = &'a str>) -> String {
    let last = orders.into_iter().filter(|k| order::is_valid(k)).max();
    order::between(last, None).unwrap_or_else(|_| order::first())
}

/// A date value moved to `day` (days since 1970-01-01, in the viewer's zone): a value with a
/// time keeps its time of day, a range keeps its length, and no value becomes that whole day.
pub fn date_on(old: Option<&DateValue>, day: i64, offset_ms: i64) -> Value {
    match old {
        Some(d) if d.include_time => {
            let local = d.start.saturating_add(offset_ms);
            let time_of_day = local.rem_euclid(DAY_MS);
            let start = day * DAY_MS + time_of_day - offset_ms;
            let end = d.end.map(|e| e - d.start + start).filter(|e| *e >= start);
            Value::Date(DateValue {
                start,
                end,
                include_time: true,
            })
        }
        Some(d) => {
            let start = day * DAY_MS;
            let end = d.end.map(|e| e - d.start + start).filter(|e| *e >= start);
            Value::Date(DateValue {
                start,
                end,
                include_time: false,
            })
        }
        None => Value::date_on_day(day),
    }
}

/// The value of `p` (a board's group property) for a card moved from the column `from` to
/// the column `to`, given its value `old`: Some(None) empties it, and None means the card
/// can't go there (a derived property, a type that doesn't group, or a date bucket with no one
/// day, like "Past 7 days").
///
/// - A select takes `to`'s option.
/// - A multi-select or person value swaps `from`'s option or person for `to`'s, in its place
///   (or just loses `from`'s if it has `to`'s already). Moved to the empty column, it empties.
/// - A checkbox is checked or not.
/// - A date moves to the bucket's day, keeping its time of day and length.
pub fn moved_value(
    p: &Property,
    old: Option<&Value>,
    from: &GroupKey,
    to: &GroupKey,
    today: i64,
    offset_ms: i64,
) -> Option<Option<Value>> {
    if p.kind.is_derived() {
        return None;
    }
    match (&p.kind, to) {
        (PropertyType::Select, GroupKey::Option(id)) => Some(Some(Value::text(id.clone()))),
        (
            PropertyType::Select | PropertyType::MultiSelect | PropertyType::Person,
            GroupKey::Empty,
        ) => Some(None),
        (PropertyType::MultiSelect, GroupKey::Option(b)) => {
            let mut ids = old.and_then(Value::option_ids).unwrap_or(&[]).to_vec();
            let a = match from {
                GroupKey::Option(a) => ids.iter().position(|x| x == a),
                _ => None,
            };
            match (a, ids.contains(b)) {
                (Some(i), false) => ids[i] = b.clone(),
                (Some(i), true) => {
                    ids.remove(i);
                }
                (None, false) => ids.push(b.clone()),
                (None, true) => {}
            }
            Some((!ids.is_empty()).then_some(Value::Ids(ids)))
        }
        (PropertyType::Person, GroupKey::Person(b)) => {
            let mut people = old.and_then(Value::as_people).unwrap_or(&[]).to_vec();
            let a = match from {
                GroupKey::Person(a) => people.iter().position(|x| x == a),
                _ => None,
            };
            match (a, people.contains(b)) {
                (Some(i), false) => people[i] = b.clone(),
                (Some(i), true) => {
                    people.remove(i);
                }
                (None, false) => people.push(b.clone()),
                (None, true) => {}
            }
            Some((!people.is_empty()).then_some(Value::People(people)))
        }
        (PropertyType::Checkbox, GroupKey::Checked(b)) => Some(Some(Value::Bool(*b))),
        (PropertyType::Date, GroupKey::Empty) => Some(None),
        (PropertyType::Date, GroupKey::Date(g)) => {
            let day = format::group_day(g, today)?;
            Some(Some(date_on(old.and_then(Value::as_date), day, offset_ms)))
        }
        _ => None,
    }
}

// --- Views ----------------------------------------------------------------------------------

/// The first select property, in the properties' order: what a new board groups by.
pub fn first_select(db: &DatabaseFields) -> Option<&Property> {
    db.properties_in_order()
        .into_iter()
        .find(|p| p.kind == PropertyType::Select)
}

/// The properties a view can group by, in the properties' order.
pub fn groupable(db: &DatabaseFields) -> Vec<&Property> {
    db.properties_in_order()
        .into_iter()
        .filter(|p| p.kind.can_group())
        .collect()
}

/// What a board made from (or changed from) `view` groups by: the view's own `groupBy` when it
/// can group, else the first select property. None when it has neither: the host is asked to
/// pick one of [`groupable`], or told there's none.
pub fn board_group(db: &DatabaseFields, view: Option<&View>) -> Option<String> {
    view.and_then(|v| v.group_by.as_deref())
        .and_then(|g| db.property(g))
        .filter(|p| p.kind.can_group())
        .or_else(|| first_select(db))
        .map(|p| p.id.clone())
}

/// A view type's name, as a new view of it is called and the menus list it.
pub fn view_type_label(kind: &crate::model::ViewType) -> &str {
    use crate::model::ViewType as T;
    match kind {
        T::Table => "Table",
        T::Board => "Board",
        T::List => "List",
        T::Gallery => "Gallery",
        T::Other(s) => s,
    }
}

/// `base`, or `base 2`, `base 3`, ... whichever no view of `db` is called (case aside).
pub fn unique_view_name(db: &DatabaseFields, base: &str) -> String {
    let taken = |name: &str| {
        let name = name.trim().to_lowercase();
        db.views
            .iter()
            .any(|v| v.name.trim().to_lowercase() == name)
    };
    if !taken(base) {
        return base.to_owned();
    }
    (2..)
        .map(|n| format!("{base} {n}"))
        .find(|name| !taken(name))
        .unwrap_or_default()
}

/// A new view of `kind` called `name`, after every view of `db`, with the type's defaults: no
/// sort, filter or columns (a table shows every property), and a board grouped by `group_by`
/// (see [`board_group`]). A table and a list start ungrouped.
pub fn new_view(
    db: &DatabaseFields,
    id: impl Into<String>,
    kind: crate::model::ViewType,
    name: impl Into<String>,
    group_by: Option<String>,
) -> View {
    let order = append_order(db.views.iter().map(|v| v.order.as_str()));
    let board = kind == crate::model::ViewType::Board;
    let mut view = View::new(id, name, kind, order);
    if board {
        view.group_by = group_by;
    }
    view
}

/// A copy of `view` with a new id, called "`name` copy" (cut to `max_chars`), placed straight
/// after it among `db`'s views (after the last view, where its neighbours' keys can't be put
/// between).
pub fn duplicate_view(
    db: &DatabaseFields,
    view: &View,
    id: impl Into<String>,
    max_chars: usize,
) -> View {
    let mut copy = view.clone();
    copy.id = id.into();
    let name = format!("{} copy", view.name.trim());
    copy.name = crate::model::truncate_chars(&name, max_chars).to_owned();
    let views = db.views_in_order();
    let at = views.iter().position(|v| v.id == view.id);
    let next = at.and_then(|i| views.get(i + 1)).map(|v| v.order.as_str());
    copy.order = order::between(Some(&view.order), next)
        .or_else(|_| order::between(views.last().map(|v| v.order.as_str()), None))
        .unwrap_or_else(|_| append_order(db.views.iter().map(|v| v.order.as_str())));
    copy
}

/// The views that change when `view_id` moves to place `to` among `db`'s views (by `order`):
/// that one with a key between its new neighbours', or, where they can't be put between (two
/// the same, an older scheme's), every view renumbered with [`order::spread`]. Empty when it
/// doesn't move.
pub fn move_view(db: &DatabaseFields, view_id: &str, to: usize) -> Vec<View> {
    let mut views: Vec<View> = db.views_in_order().into_iter().cloned().collect();
    let Some(from) = views.iter().position(|v| v.id == view_id) else {
        return Vec::new();
    };
    let to = to.min(views.len().saturating_sub(1));
    if to == from {
        return Vec::new();
    }
    let moving = views.remove(from);
    views.insert(to, moving);
    let before = to.checked_sub(1).map(|i| views[i].order.clone());
    let after = views.get(to + 1).map(|v| v.order.clone());
    let others_rise = views
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != to)
        .map(|(_, v)| v.order.as_str())
        .collect::<Vec<_>>();
    let rising = others_rise.iter().all(|k| order::is_valid(k))
        && others_rise.windows(2).all(|w| w[0] < w[1]);
    if rising && let Ok(k) = order::between(before.as_deref(), after.as_deref()) {
        views[to].order = k;
        return vec![views.swap_remove(to)];
    }
    let keys = order::spread(views.len());
    views
        .into_iter()
        .zip(keys)
        .filter_map(|(mut v, k)| {
            let changed = v.order != k;
            v.order = k;
            changed.then_some(v)
        })
        .collect()
}

/// The view of a new type: a board keeps a `groupBy` that can group, else takes `group_by`
/// (see [`board_group`]); a table or a list keeps its grouping.
pub fn retype_view(view: &View, kind: crate::model::ViewType, group_by: Option<String>) -> View {
    let mut view = view.clone();
    if kind == crate::model::ViewType::Board && group_by.is_some() {
        if view.group_by != group_by {
            view.collapsed_groups.clear();
        }
        view.group_by = group_by;
    }
    view.kind = kind;
    view
}

/// The view grouped by `property_id`, or not grouped. Its folded groups were another
/// property's and are let go; a date bucket stays only while it groups by a date.
pub fn set_group_by(db: &DatabaseFields, view: &View, property_id: Option<&str>) -> View {
    let mut view = view.clone();
    if view.group_by.as_deref() == property_id {
        return view;
    }
    view.group_by = property_id.map(str::to_owned);
    view.collapsed_groups.clear();
    let date = property_id
        .and_then(|id| db.property(id))
        .is_some_and(|p| p.kind.is_date());
    if !date {
        view.group_date = None;
    }
    view
}

/// The properties a view's cards (or lines) show, as it shows them: its `cardProperties` that
/// name a property of `db` other than the title, each once, in order.
pub fn card_properties(db: &DatabaseFields, view: &View) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for id in &view.card_properties {
        if db
            .property(id)
            .is_some_and(|p| p.kind != PropertyType::Title)
            && !out.contains(id)
        {
            out.push(id.clone());
        }
    }
    out
}

/// The view with `property_id` shown on its cards (after the others) or not, up to the limit
/// on a view's card properties (the properties limit).
pub fn toggle_card_property(
    db: &DatabaseFields,
    view: &View,
    property_id: &str,
    limits: &Limits,
) -> View {
    let mut view = view.clone();
    let mut shown = card_properties(db, &view);
    match shown.iter().position(|p| p == property_id) {
        Some(i) => {
            shown.remove(i);
        }
        None if shown.len() < limits.properties => shown.push(property_id.to_owned()),
        None => {}
    }
    view.card_properties = shown;
    view
}

/// The view with its card property `property_id` moved `by` places (up is negative).
pub fn move_card_property(db: &DatabaseFields, view: &View, property_id: &str, by: isize) -> View {
    let mut view = view.clone();
    let mut shown = card_properties(db, &view);
    if let Some(i) = shown.iter().position(|p| p == property_id) {
        let j = (i as isize + by).clamp(0, shown.len() as isize - 1) as usize;
        let p = shown.remove(i);
        shown.insert(j, p);
    }
    view.card_properties = shown;
    view
}
