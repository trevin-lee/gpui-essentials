//! Caps on what a database and its rows may hold. Agents write rows too, so the host (and its
//! server) checks what it's given against these, and the crate clamps what people type to them.
//! The engine never panics past them: it ignores what's over (a filter too deep, a sort too many)
//! and says so.

use std::collections::HashSet;
use std::fmt;

use serde_json::Value as Json;
use unicode_segmentation::UnicodeSegmentation;

use super::{DatabaseFields, Doc, Filter, Property, PropertyType, Value};

/// The most grapheme clusters an emoji icon may be (a doc's `icon` field): one.
const ICON_GRAPHEMES: usize = 1;
/// The most bytes an emoji icon may take: room for a flag, a skin tone or a family of two, not
/// a family of three (18 bytes).
pub(crate) const ICON_BYTES: usize = 16;
/// What starts an image icon: `asset:` and the image's SHA-256 in lowercase hex.
pub(crate) const ICON_ASSET: &str = "asset:";

/// The problems with a doc's `icon` string: an emoji must be one grapheme cluster of at most 16
/// bytes, and an image `asset:` and 64 lowercase hex characters. Empty when it's fine
/// (and it allocates nothing then).
pub(crate) fn icon_problems(icon: &str) -> Vec<ProblemKind> {
    let mut problems = Vec::new();
    if icon.is_empty() {
        problems.push(ProblemKind::EmptyIcon);
    } else if let Some(hash) = icon.strip_prefix(ICON_ASSET) {
        if !is_sha256(hash) {
            problems.push(ProblemKind::NotAsset);
        }
    } else {
        // Bytes bound graphemes from above, so most icons are never segmented.
        if icon.len() > ICON_GRAPHEMES {
            let found = icon.graphemes(true).count();
            if found > ICON_GRAPHEMES {
                problems.push(ProblemKind::TooManyGraphemes {
                    limit: ICON_GRAPHEMES,
                    found,
                });
            }
        }
        if icon.len() > ICON_BYTES {
            problems.push(ProblemKind::TooManyBytes {
                limit: ICON_BYTES,
                found: icon.len(),
            });
        }
    }
    problems
}

/// A SHA-256 as an `asset:` icon writes it: 64 lowercase hex characters.
pub(crate) fn is_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// The caps, with defaults sized for a personal CRM with room to spare. Strings count Unicode
/// characters (scalar values), not bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Properties per database.
    pub properties: usize,
    /// Views per database.
    pub views: usize,
    /// Sorts per view.
    pub sorts: usize,
    /// Options per select or multi-select property.
    pub options: usize,
    /// Option ids in one multi-select value.
    pub multi_select_values: usize,
    /// Row ids in one relation value (a two-way relation's back side counts too).
    pub relation_values: usize,
    /// People in one person value.
    pub person_values: usize,
    /// Collapsed groups per view.
    pub collapsed_groups: usize,
    /// A row's title (the title property).
    pub title_chars: usize,
    /// A text value.
    pub text_chars: usize,
    pub url_chars: usize,
    /// RFC 5321 allows 254 in practice; this leaves room for display names.
    pub email_chars: usize,
    pub phone_chars: usize,
    /// An option's name.
    pub option_name_chars: usize,
    /// A property's or view's name.
    pub name_chars: usize,
    /// Any id (a UUID is 36; agent resource names are shorter).
    pub id_chars: usize,
    /// A fractional index: [`crate::order::MAX_LEN`]. Keys grow about a character per six
    /// inserts at the same place; a host that meets this renumbers the list
    /// ([`crate::order::spread`]).
    pub order_chars: usize,
    /// Levels of a filter tree, a lone condition being 1: and/or groups nested three deep, as
    /// Notion allows, and their conditions.
    pub filter_depth: usize,
    /// Nodes in a filter tree, groups and conditions both.
    pub filter_nodes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            properties: 200,
            views: 50,
            sorts: 10,
            options: 200,
            multi_select_values: 100,
            relation_values: 2_000,
            person_values: 50,
            collapsed_groups: 500,
            title_chars: 2_000,
            text_chars: 10_000,
            url_chars: 2_048,
            email_chars: 320,
            phone_chars: 64,
            option_name_chars: 100,
            name_chars: 200,
            id_chars: 128,
            order_chars: crate::order::MAX_LEN,
            filter_depth: 4,
            filter_nodes: 100,
        }
    }
}

/// Something over a limit, or malformed, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    /// A path into the JSON, like `properties[2].config.options` or `values.<propertyId>`.
    pub at: String,
    pub kind: ProblemKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProblemKind {
    TooMany {
        limit: usize,
        found: usize,
    },
    TooLong {
        limit: usize,
        found: usize,
    },
    TooDeep {
        limit: usize,
        found: usize,
    },
    /// Two of something share this id.
    Duplicate(String),
    /// A database has no title property.
    NoTitle,
    /// A database has more than one title property.
    SeveralTitles,
    /// A value doesn't have the shape its property's type holds.
    WrongShape {
        expected: PropertyType,
    },
    /// A select or multi-select value names an option the property hasn't.
    UnknownOption(String),
    /// A person value names a kind of person its property doesn't allow.
    KindNotAllowed,
    /// A date range ends before it starts.
    EndsBeforeStart,
    /// A value is stored for a property the crate derives (created or edited time, created
    /// or edited by).
    Derived,
    /// A two-way relation whose back property is missing, or doesn't name it back.
    BrokenTwoWay,
    /// A doc's emoji icon of more than one grapheme cluster.
    TooManyGraphemes {
        limit: usize,
        found: usize,
    },
    /// A doc's emoji icon over 16 bytes.
    TooManyBytes {
        limit: usize,
        found: usize,
    },
    /// A doc's icon is an empty string (no icon is null).
    EmptyIcon,
    /// A doc's icon starts with `asset:` but isn't followed by 64 lowercase hex characters.
    NotAsset,
    /// A doc's icon is neither a string nor null.
    IconNotString,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: ", self.at)?;
        match &self.kind {
            ProblemKind::TooMany { limit, found } => write!(f, "{found} is more than {limit}"),
            ProblemKind::TooLong { limit, found } => {
                write!(f, "{found} characters is more than {limit}")
            }
            ProblemKind::TooDeep { limit, found } => {
                write!(f, "{found} levels is more than {limit}")
            }
            ProblemKind::Duplicate(id) => write!(f, "{id} is used twice"),
            ProblemKind::NoTitle => write!(f, "no title property"),
            ProblemKind::SeveralTitles => write!(f, "more than one title property"),
            ProblemKind::WrongShape { expected } => write!(f, "not a {expected} value"),
            ProblemKind::UnknownOption(id) => write!(f, "no option {id}"),
            ProblemKind::KindNotAllowed => {
                write!(f, "a kind of person this property doesn't allow")
            }
            ProblemKind::EndsBeforeStart => write!(f, "ends before it starts"),
            ProblemKind::Derived => write!(f, "derived, never stored"),
            ProblemKind::BrokenTwoWay => write!(f, "the back property doesn't name this one"),
            ProblemKind::TooManyGraphemes { limit, found } => {
                write!(f, "{found} graphemes is more than {limit}")
            }
            ProblemKind::TooManyBytes { limit, found } => {
                write!(f, "{found} bytes is more than {limit}")
            }
            ProblemKind::EmptyIcon => write!(f, "an empty string"),
            ProblemKind::NotAsset => write!(f, "not asset: and 64 lowercase hex characters"),
            ProblemKind::IconNotString => write!(f, "not a string or null"),
        }
    }
}

impl std::error::Error for Problem {}

/// The first `max` characters of `s` (all of it when it's no longer).
pub(crate) fn truncate_chars(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    match s.char_indices().nth(max) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

fn chars(s: &str) -> usize {
    s.chars().count()
}

struct Checker<'a> {
    limits: &'a Limits,
    problems: Vec<Problem>,
}

impl Checker<'_> {
    fn push(&mut self, at: impl Into<String>, kind: ProblemKind) {
        self.problems.push(Problem {
            at: at.into(),
            kind,
        });
    }

    fn count(&mut self, at: impl FnOnce() -> String, found: usize, limit: usize) {
        if found > limit {
            self.push(at(), ProblemKind::TooMany { limit, found });
        }
    }

    fn text(&mut self, at: impl FnOnce() -> String, s: &str, limit: usize) {
        // Bytes bound characters from above, so most strings never need counting.
        if s.len() > limit {
            let found = chars(s);
            if found > limit {
                self.push(at(), ProblemKind::TooLong { limit, found });
            }
        }
    }

    fn id(&mut self, at: impl FnOnce() -> String, id: &str) {
        self.text(at, id, self.limits.id_chars);
    }

    fn order(&mut self, at: impl FnOnce() -> String, order: &str) {
        self.text(at, order, self.limits.order_chars);
    }

    fn unique<'s>(&mut self, at: &str, ids: impl Iterator<Item = &'s str>) {
        let mut seen = HashSet::new();
        for id in ids {
            if !seen.insert(id) {
                self.push(at, ProblemKind::Duplicate(id.to_owned()));
            }
        }
    }
}

impl Limits {
    /// Every problem with a doc, with paths from the doc's root: its `id`, `order`, `title` and
    /// `icon`; for a database doc, its `database` field (paths `database.properties[2].name` and
    /// so on, as [`Limits::check_database`]); for a row, its `values` against `database`, the
    /// properties of the database it's in (as [`Limits::check_row`]). Without `database`, a
    /// row's values are only counted. `tests/fixtures/limits` holds cases with their expected
    /// problems, each written as [`Problem`]'s `Display` writes it.
    pub fn check_doc(&self, doc: &Doc, database: Option<&DatabaseFields>) -> Vec<Problem> {
        let empty = DatabaseFields::default();
        let mut problems = self.check_row(database.unwrap_or(&empty), doc);
        if let Some(fields) = &doc.database {
            problems.extend(self.check_database(fields).into_iter().map(|mut p| {
                p.at = format!("database.{}", p.at);
                p
            }));
        }
        problems
    }

    /// Every problem with a database's properties and views: counts and lengths over the
    /// limits, ids used twice, a missing or second title property, and filters too deep or too
    /// big. Empty when it's fine. Paths are from the `database` field (`properties[2].name`).
    pub fn check_database(&self, db: &DatabaseFields) -> Vec<Problem> {
        let mut c = Checker {
            limits: self,
            problems: Vec::new(),
        };
        c.count(|| "properties".into(), db.properties.len(), self.properties);
        c.count(|| "views".into(), db.views.len(), self.views);
        c.unique("properties", db.properties.iter().map(|p| p.id.as_str()));
        c.unique("views", db.views.iter().map(|v| v.id.as_str()));
        let titles = db
            .properties
            .iter()
            .filter(|p| p.kind == PropertyType::Title)
            .count();
        match titles {
            0 => c.push("properties", ProblemKind::NoTitle),
            1 => {}
            _ => c.push("properties", ProblemKind::SeveralTitles),
        }
        for (i, p) in db.properties.iter().enumerate() {
            let at = || format!("properties[{i}]");
            c.id(|| format!("{}.id", at()), &p.id);
            c.text(|| format!("{}.name", at()), &p.name, self.name_chars);
            c.order(|| format!("{}.order", at()), &p.order);
            let options = p.options();
            c.count(
                || format!("{}.config.options", at()),
                options.len(),
                self.options,
            );
            c.unique(
                &format!("{}.config.options", at()),
                options.iter().map(|o| o.id.as_str()),
            );
            for (j, o) in options.iter().enumerate() {
                c.id(|| format!("{}.config.options[{j}].id", at()), &o.id);
                c.text(
                    || format!("{}.config.options[{j}].name", at()),
                    &o.name,
                    self.option_name_chars,
                );
            }
            for (key, id) in [
                ("databaseId", &p.config.database_id),
                ("backPropertyId", &p.config.back_property_id),
            ] {
                if let Some(id) = id {
                    c.id(|| format!("{}.config.{key}", at()), id);
                }
            }
        }
        for (i, v) in db.views.iter().enumerate() {
            let at = || format!("views[{i}]");
            c.id(|| format!("{}.id", at()), &v.id);
            c.text(|| format!("{}.name", at()), &v.name, self.name_chars);
            c.order(|| format!("{}.order", at()), &v.order);
            c.count(|| format!("{}.sort", at()), v.sort.len(), self.sorts);
            c.count(
                || format!("{}.columns", at()),
                v.columns.len(),
                self.properties,
            );
            c.unique(
                &format!("{}.columns", at()),
                v.columns.iter().map(|col| col.property_id.as_str()),
            );
            for (j, col) in v.columns.iter().enumerate() {
                c.order(|| format!("{}.columns[{j}].order", at()), &col.order);
            }
            c.count(
                || format!("{}.cardProperties", at()),
                v.card_properties.len(),
                self.properties,
            );
            c.count(
                || format!("{}.collapsedGroups", at()),
                v.collapsed_groups.len(),
                self.collapsed_groups,
            );
            if let Some(f) = &v.filter {
                c.problems
                    .extend(self.check_filter(f, &format!("{}.filter", at())));
            }
        }
        c.problems
    }

    /// Problems with a filter tree's size: deeper than `filter_depth` or more nodes than
    /// `filter_nodes`. It stops counting a little past the limits, so a huge tree costs little.
    pub fn check_filter(&self, filter: &Filter, at: &str) -> Vec<Problem> {
        let mut problems = Vec::new();
        let (mut nodes, mut depth) = (0, 0);
        for (_, d) in filter.walk() {
            nodes += 1;
            depth = depth.max(d);
            if nodes > self.filter_nodes.saturating_mul(4) {
                break;
            }
        }
        if depth > self.filter_depth {
            problems.push(Problem {
                at: at.into(),
                kind: ProblemKind::TooDeep {
                    limit: self.filter_depth,
                    found: depth,
                },
            });
        }
        if nodes > self.filter_nodes {
            problems.push(Problem {
                at: at.into(),
                kind: ProblemKind::TooMany {
                    limit: self.filter_nodes,
                    found: nodes,
                },
            });
        }
        problems
    }

    /// Every problem with a row against its database: the lengths of its id, order and title,
    /// its `icon` (as [`Doc::icon`] reads it), values over their limits or of the wrong shape,
    /// options the property hasn't, people it doesn't allow, and values stored for derived
    /// properties. Values for properties the database hasn't are ignored (a deleted
    /// property's), but count toward `properties`.
    pub fn check_row(&self, db: &DatabaseFields, row: &Doc) -> Vec<Problem> {
        let mut c = Checker {
            limits: self,
            problems: Vec::new(),
        };
        c.id(|| "id".into(), &row.id);
        c.order(|| "order".into(), &row.order);
        c.text(|| "title".into(), &row.title, self.title_chars);
        match row.extra.get("icon") {
            None | Some(Json::Null) => {}
            Some(Json::String(icon)) => {
                for kind in icon_problems(icon) {
                    c.push("icon", kind);
                }
            }
            Some(_) => c.push("icon", ProblemKind::IconNotString),
        }
        if let Some(values) = &row.values {
            c.count(|| "values".into(), values.len(), self.properties);
            for (id, value) in values {
                if let Some(p) = db.property(id) {
                    for mut problem in self.check_value(p, value) {
                        problem.at = format!("values.{id}{}", problem.at);
                        c.problems.push(problem);
                    }
                }
            }
        }
        c.problems
    }

    /// Problems with one value for a property. Each problem's `at` is empty or a suffix like
    /// `[3]`, for the caller to prefix.
    ///
    /// A select and a multi-select each take both shapes, as a property changed from one to the
    /// other keeps its rows' values: an option id (`"lead"`), or a list of option ids
    /// (`["lead", "won"]`, a select reading its first). Either way every id names one of the
    /// property's options and is an id's length at most, and a list holds no id twice and at
    /// most `multi_select_values`, the select's too (a multi-select's values changed to a
    /// select stay valid).
    pub fn check_value(&self, p: &Property, value: &Value) -> Vec<Problem> {
        let mut c = Checker {
            limits: self,
            problems: Vec::new(),
        };
        if p.kind.is_derived() {
            if !matches!(value, Value::Null) {
                c.push("", ProblemKind::Derived);
            }
            return c.problems;
        }
        if matches!(p.kind, PropertyType::Other(_)) {
            return c.problems;
        }
        if !value.fits(&p.kind) {
            c.push(
                "",
                ProblemKind::WrongShape {
                    expected: p.kind.clone(),
                },
            );
            return c.problems;
        }
        match value {
            Value::Text(s) => {
                let options = matches!(p.kind, PropertyType::Select | PropertyType::MultiSelect);
                if options {
                    c.id(String::new, s);
                } else if let Some(limit) = self.text_limit(&p.kind) {
                    c.text(String::new, s, limit);
                }
                if options && p.option(s).is_none() {
                    c.push("", ProblemKind::UnknownOption(s.clone()));
                }
            }
            Value::Ids(ids) => {
                let limit = if p.kind == PropertyType::Relation {
                    self.relation_values
                } else {
                    self.multi_select_values
                };
                c.count(String::new, ids.len(), limit);
                c.unique("", ids.iter().map(String::as_str));
                for (i, id) in ids.iter().enumerate().take(limit) {
                    c.id(|| format!("[{i}]"), id);
                    if matches!(p.kind, PropertyType::Select | PropertyType::MultiSelect)
                        && p.option(id).is_none()
                    {
                        c.push(format!("[{i}]"), ProblemKind::UnknownOption(id.clone()));
                    }
                }
            }
            Value::People(people) => {
                c.count(String::new, people.len(), self.person_values);
                let kinds = p.person_kinds();
                for (i, person) in people.iter().enumerate().take(self.person_values) {
                    match person.kind() {
                        Some(k) if kinds.contains(&k) => {}
                        _ => c.push(format!("[{i}]"), ProblemKind::KindNotAllowed),
                    }
                    if let super::Person::Agent(id) = person {
                        c.id(|| format!("[{i}].agentId"), id);
                    }
                }
            }
            Value::Date(d) => {
                if d.end.is_some_and(|end| end < d.start) {
                    c.push("", ProblemKind::EndsBeforeStart);
                }
            }
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::Other(_) => {}
        }
        c.problems
    }

    /// The most characters a text-like value of `kind` may hold (a select's option id counts as
    /// an id).
    pub fn text_limit(&self, kind: &PropertyType) -> Option<usize> {
        Some(match kind {
            PropertyType::Title => self.title_chars,
            PropertyType::Text => self.text_chars,
            PropertyType::Url => self.url_chars,
            PropertyType::Email => self.email_chars,
            PropertyType::Phone => self.phone_chars,
            PropertyType::Select => self.id_chars,
            _ => return None,
        })
    }

    /// A value cut to the limits: text to its length, lists to their count, duplicates in a
    /// list dropped. What a person typed or pasted goes through this before it's emitted.
    pub fn clamp_value(&self, p: &Property, value: Value) -> Value {
        match value {
            Value::Text(s) => match self.text_limit(&p.kind) {
                Some(limit) if s.len() > limit => Value::Text(truncate_chars(&s, limit).to_owned()),
                _ => Value::Text(s),
            },
            Value::Ids(ids) => {
                let limit = if p.kind == PropertyType::Relation {
                    self.relation_values
                } else {
                    self.multi_select_values
                };
                let mut seen = HashSet::new();
                Value::Ids(
                    ids.into_iter()
                        .filter(|id| seen.insert(id.clone()))
                        .take(limit)
                        .collect(),
                )
            }
            Value::People(people) => {
                let mut kept: Vec<super::Person> = Vec::new();
                for person in people {
                    if kept.len() == self.person_values {
                        break;
                    }
                    if !kept.contains(&person) {
                        kept.push(person);
                    }
                }
                Value::People(kept)
            }
            other => other,
        }
    }

    /// A title cut to `title_chars`.
    pub fn clamp_title<'s>(&self, title: &'s str) -> &'s str {
        truncate_chars(title, self.title_chars)
    }
}
