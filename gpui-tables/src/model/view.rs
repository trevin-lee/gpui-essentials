//! Views: saved ways of looking at a database's rows (a table, a board, a list), each with its
//! own sort, filter, grouping and columns.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use super::{Extra, opt_number, string_enum};

string_enum! {
    /// How a view lays its rows out.
    pub enum ViewType {
        /// Rows and columns: resizable, reorderable columns, sort, filter and group.
        Table = "table",
        /// Cards in columns, one per group of a select (or other groupable property).
        Board = "board",
        /// One line per row: the title and a few properties.
        List = "list",
        /// Cards in a grid.
        Gallery = "gallery",
    }
}

string_enum! {
    pub enum Direction {
        Ascending = "ascending",
        Descending = "descending",
    }
}

string_enum! {
    /// How a view groups by a date: relative to today (Notion's default), or by calendar day,
    /// week, month or year.
    pub enum DateBucket {
        /// Older, past 30 days, past 7 days, yesterday, today, tomorrow, next 7 days, next 30
        /// days, later.
        Relative = "relative",
        Day = "day",
        Week = "week",
        Month = "month",
        Year = "year",
    }
}

string_enum! {
    /// What a gallery's cards show above their title (Notion's card preview): nothing, or the
    /// row's page cover.
    pub enum CardPreview {
        None = "none",
        /// The row doc's `cover`: its image, or its colour.
        Cover = "cover",
    }
}

string_enum! {
    /// How big a gallery's cards are (Notion's card size): their width, 172, 252 or 340 points
    /// (never wider than the view). Absent is medium, today's card.
    pub enum CardSize {
        Small = "small",
        Medium = "medium",
        Large = "large",
    }
}

string_enum! {
    /// A filter condition's operator. Which apply depends on the property's type (see
    /// [`FilterOp::applies_to`]).
    pub enum FilterOp {
        Is = "is",
        IsNot = "isNot",
        Contains = "contains",
        DoesNotContain = "doesNotContain",
        StartsWith = "startsWith",
        IsEmpty = "isEmpty",
        IsNotEmpty = "isNotEmpty",
        Greater = "greater",
        GreaterOrEqual = "greaterOrEqual",
        Less = "less",
        LessOrEqual = "lessOrEqual",
        Before = "before",
        After = "after",
        OnOrBefore = "onOrBefore",
        OnOrAfter = "onOrAfter",
        /// Within a span around today, named by a [`Within`] string.
        Within = "within",
        IsChecked = "isChecked",
        IsNotChecked = "isNotChecked",
    }
}

string_enum! {
    /// The spans `within` takes, each counted in whole days in the viewer's time zone and
    /// including today.
    pub enum Within {
        Today = "today",
        PastWeek = "pastWeek",
        PastMonth = "pastMonth",
        PastYear = "pastYear",
        NextWeek = "nextWeek",
        NextMonth = "nextMonth",
        NextYear = "nextYear",
    }
}

impl FilterOp {
    /// Whether this operator applies to a property of `kind`.
    pub fn applies_to(&self, kind: &super::PropertyType) -> bool {
        use super::PropertyType as T;
        use FilterOp as O;
        let empty = matches!(self, O::IsEmpty | O::IsNotEmpty);
        match kind {
            T::Title | T::Text | T::Url | T::Email | T::Phone => {
                empty
                    || matches!(
                        self,
                        O::Is | O::IsNot | O::Contains | O::DoesNotContain | O::StartsWith
                    )
            }
            T::Number => {
                empty
                    || matches!(
                        self,
                        O::Is
                            | O::IsNot
                            | O::Greater
                            | O::GreaterOrEqual
                            | O::Less
                            | O::LessOrEqual
                    )
            }
            T::Select => empty || matches!(self, O::Is | O::IsNot),
            T::MultiSelect | T::Person | T::Relation => {
                empty || matches!(self, O::Contains | O::DoesNotContain)
            }
            T::Date | T::CreatedTime | T::EditedTime => {
                // The derived times are never empty, but asking is harmless.
                empty
                    || matches!(
                        self,
                        O::Is
                            | O::IsNot
                            | O::Before
                            | O::After
                            | O::OnOrBefore
                            | O::OnOrAfter
                            | O::Within
                    )
            }
            // One person each: `is` and `contains` mean the same, as do `isNot` and
            // `doesNotContain`. Empty when the doc has no author.
            T::CreatedBy | T::EditedBy => {
                empty || matches!(self, O::Is | O::IsNot | O::Contains | O::DoesNotContain)
            }
            T::Checkbox => matches!(self, O::Is | O::IsChecked | O::IsNotChecked),
            T::Other(_) => false,
        }
    }
}

string_enum! {
    /// What a table's footer works out for a column, over the rows shown (of each group, when it
    /// groups): written in a view's `calculations` by property id. Which apply depends on the
    /// property's type ([`Calculation::applies_to`]); the engine works them out
    /// ([`crate::engine::calc`]).
    pub enum Calculation {
        /// Every row.
        CountAll = "countAll",
        /// Every value: a list's items each (options, people, related rows), else a row's
        /// non-empty value.
        CountValues = "countValues",
        /// The distinct values among them.
        CountUnique = "countUnique",
        CountEmpty = "countEmpty",
        CountNotEmpty = "countNotEmpty",
        PercentEmpty = "percentEmpty",
        PercentNotEmpty = "percentNotEmpty",
        /// Numbers only, as are the next five.
        Sum = "sum",
        Average = "average",
        Median = "median",
        Min = "min",
        Max = "max",
        /// A number's max less its min; a date's latest less its earliest, as a duration.
        Range = "range",
        /// Dates (and created and edited times) only, as is the next.
        Earliest = "earliest",
        Latest = "latest",
        /// Checkboxes only, as are the next three.
        Checked = "checked",
        Unchecked = "unchecked",
        PercentChecked = "percentChecked",
        PercentUnchecked = "percentUnchecked",
    }
}

impl Calculation {
    /// The ones that apply to every type, in the menu's order.
    pub const ANY: &'static [Calculation] = &[
        Self::CountAll,
        Self::CountValues,
        Self::CountUnique,
        Self::CountEmpty,
        Self::CountNotEmpty,
        Self::PercentEmpty,
        Self::PercentNotEmpty,
    ];

    /// A number's, in the menu's order.
    pub const NUMBER: &'static [Calculation] = &[
        Self::Sum,
        Self::Average,
        Self::Median,
        Self::Min,
        Self::Max,
        Self::Range,
    ];

    /// A date's (and a created or edited time's), in the menu's order.
    pub const DATE: &'static [Calculation] = &[Self::Earliest, Self::Latest, Self::Range];

    /// A checkbox's, in the menu's order.
    pub const CHECKBOX: &'static [Calculation] = &[
        Self::Checked,
        Self::Unchecked,
        Self::PercentChecked,
        Self::PercentUnchecked,
    ];

    /// Those of a property's type alone (a number's, a date's, a checkbox's), after
    /// [`Calculation::ANY`] in the menu.
    pub fn of_type(kind: &super::PropertyType) -> &'static [Calculation] {
        use super::PropertyType as T;
        match kind {
            T::Number => Self::NUMBER,
            T::Date | T::CreatedTime | T::EditedTime => Self::DATE,
            T::Checkbox => Self::CHECKBOX,
            _ => &[],
        }
    }

    /// Whether it applies to a property of `kind`. One that doesn't (left from before a change
    /// of type, or a name this version doesn't know) works out as nothing.
    pub fn applies_to(&self, kind: &super::PropertyType) -> bool {
        Self::ANY.contains(self) || Self::of_type(kind).contains(self)
    }

    /// Its name in the menu: "Count unique", "Percent checked".
    pub fn label(&self) -> &str {
        match self {
            Self::CountAll => "Count all",
            Self::CountValues => "Count values",
            Self::CountUnique => "Count unique",
            Self::CountEmpty => "Count empty",
            Self::CountNotEmpty => "Count not empty",
            Self::PercentEmpty => "Percent empty",
            Self::PercentNotEmpty => "Percent not empty",
            Self::Sum => "Sum",
            Self::Average => "Average",
            Self::Median => "Median",
            Self::Min => "Min",
            Self::Max => "Max",
            Self::Range => "Range",
            Self::Earliest => "Earliest",
            Self::Latest => "Latest",
            Self::Checked => "Checked",
            Self::Unchecked => "Unchecked",
            Self::PercentChecked => "Percent checked",
            Self::PercentUnchecked => "Percent unchecked",
            Self::Other(s) => s,
        }
    }

    /// Its short name before its result in the footer: "Count", "Unique", "Sum", "Empty"
    /// (before a count or a percent).
    pub fn short_label(&self) -> &str {
        match self {
            Self::CountAll => "Count",
            Self::CountValues => "Values",
            Self::CountUnique => "Unique",
            Self::CountEmpty | Self::PercentEmpty => "Empty",
            Self::CountNotEmpty | Self::PercentNotEmpty => "Not empty",
            Self::Checked | Self::PercentChecked => "Checked",
            Self::Unchecked | Self::PercentUnchecked => "Unchecked",
            other => other.label(),
        }
    }
}

/// One way a view sorts: by a property, up or down. A view's sorts apply in turn, each breaking
/// the ties of the one before.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sort {
    pub property_id: String,
    pub direction: Direction,
}

impl Sort {
    pub fn ascending(property_id: impl Into<String>) -> Self {
        Self {
            property_id: property_id.into(),
            direction: Direction::Ascending,
        }
    }

    pub fn descending(property_id: impl Into<String>) -> Self {
        Self {
            property_id: property_id.into(),
            direction: Direction::Descending,
        }
    }
}

/// One test of a row: `{propertyId, op, value}`. `value` depends on the operator and the
/// property's type: a string for text, an option id for a select, a number, a row id for a
/// relation, a person for a person, a date (`{start, includeTime}`, milliseconds, or
/// `"2026-11-15"`) for a date, a [`Within`] name for `within`, and nothing for `isEmpty`,
/// `isNotEmpty`, `isChecked` and `isNotChecked`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Condition {
    pub property_id: String,
    pub op: FilterOp,
    #[serde(default, skip_serializing_if = "Json::is_null")]
    pub value: Json,
}

/// A view's filter: a tree of `{"and": [..]}` and `{"or": [..]}` over conditions.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Filter {
    And { and: Vec<Filter> },
    Or { or: Vec<Filter> },
    Condition(Condition),
}

impl Filter {
    pub fn condition(property_id: impl Into<String>, op: FilterOp, value: Json) -> Self {
        Self::Condition(Condition {
            property_id: property_id.into(),
            op,
            value,
        })
    }

    /// How deep the tree goes: a lone condition is 1. (Walked without recursion, so a tree
    /// built in code to any depth can be measured.)
    pub fn depth(&self) -> usize {
        self.walk().map(|(_, d)| d).max().unwrap_or(0)
    }

    /// How many nodes it has, groups and conditions both.
    pub fn nodes(&self) -> usize {
        self.walk().count()
    }

    /// Every node with its depth (the root is 1), depth first.
    pub fn walk(&self) -> impl Iterator<Item = (&Filter, usize)> {
        let mut stack = vec![(self, 1)];
        std::iter::from_fn(move || {
            let (f, d) = stack.pop()?;
            if let Self::And { and: c } | Self::Or { or: c } = f {
                stack.extend(c.iter().rev().map(|x| (x, d + 1)));
            }
            Some((f, d))
        })
    }
}

/// A table view's column: a property shown at a width, in an order, or hidden.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Column {
    pub property_id: String,
    /// In points; null for the default width.
    #[serde(default, with = "opt_number")]
    pub width: Option<f64>,
    #[serde(default)]
    pub hidden: bool,
    /// A fractional index among the view's columns.
    #[serde(default)]
    pub order: String,
}

/// A saved view of a database.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    /// A lowercase UUID.
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: ViewType,
    /// A fractional index among the database's views (their tabs' order).
    #[serde(default)]
    pub order: String,
    #[serde(default)]
    pub sort: Vec<Sort>,
    #[serde(default)]
    pub filter: Option<Filter>,
    /// The property the rows are grouped by: a board's columns, a table's or list's sections.
    #[serde(default)]
    pub group_by: Option<String>,
    /// How a date `groupBy` buckets; relative when absent. (An addition to the proposed JSON.)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_date: Option<DateBucket>,
    /// A table's columns. Properties without one show after them, at the default width.
    #[serde(default)]
    pub columns: Vec<Column>,
    /// The properties a board's card or a list's line shows besides the title, in order.
    #[serde(default)]
    pub card_properties: Vec<String>,
    /// Groups folded shut, by [`crate::engine::GroupKey`] string.
    #[serde(default)]
    pub collapsed_groups: Vec<String>,
    /// A table's footer: a [`Calculation`] by property id, worked out under its column (and
    /// each group's). A property without one shows none. Left out of the JSON when empty.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub calculations: BTreeMap<String, Calculation>,
    /// What a gallery's cards show above their title. Absent (or null) is none; left out of
    /// the JSON when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_preview: Option<CardPreview>,
    /// How big a gallery's cards are. Absent (or null) is medium; left out of the JSON when
    /// absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_size: Option<CardSize>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl View {
    /// An empty view: no sort, filter, grouping or columns.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        kind: ViewType,
        order: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind,
            order: order.into(),
            sort: Vec::new(),
            filter: None,
            group_by: None,
            group_date: None,
            columns: Vec::new(),
            card_properties: Vec::new(),
            collapsed_groups: Vec::new(),
            calculations: BTreeMap::new(),
            card_preview: None,
            card_size: None,
            extra: Extra::new(),
        }
    }

    /// Its columns by their `order` (then property id).
    pub fn columns_in_order(&self) -> Vec<&Column> {
        let mut c: Vec<&Column> = self.columns.iter().collect();
        c.sort_by(|a, b| (&a.order, &a.property_id).cmp(&(&b.order, &b.property_id)));
        c
    }
}
