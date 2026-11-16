//! Properties: a database's columns, each with a type and that type's settings.

use serde::{Deserialize, Serialize};

use super::{Extra, PersonKind, string_enum};

string_enum! {
    /// A property's type, which says what its values are and how they sort, filter and group.
    pub enum PropertyType {
        /// The row's name. Every database has exactly one; its value is the row doc's `title`.
        Title = "title",
        /// Plain text.
        Text = "text",
        Number = "number",
        /// One option, by id.
        Select = "select",
        /// Any number of options, by id.
        MultiSelect = "multiSelect",
        /// A date, or a date and time, or a range of either.
        Date = "date",
        Checkbox = "checkbox",
        Url = "url",
        Email = "email",
        Phone = "phone",
        /// The owner, agents, or both.
        Person = "person",
        /// Rows of another database (or this one), by row doc id.
        Relation = "relation",
        /// The row doc's `created`, never stored in `values`.
        CreatedTime = "createdTime",
        /// The row doc's `updated`, never stored in `values`.
        EditedTime = "editedTime",
        /// The row doc's `author`, never stored in `values`.
        CreatedBy = "createdBy",
        /// The row doc's `updatedBy` (its `author` when that's missing), never stored in
        /// `values`.
        EditedBy = "editedBy",
    }
}

impl PropertyType {
    /// Text-like: compared, filtered and limited as strings.
    pub fn is_text(&self) -> bool {
        matches!(
            self,
            Self::Title | Self::Text | Self::Url | Self::Email | Self::Phone
        )
    }

    /// Dates: the date type and the two derived times.
    pub fn is_date(&self) -> bool {
        matches!(self, Self::Date | Self::CreatedTime | Self::EditedTime)
    }

    /// Values the crate derives from the row doc, which an edit never sets in `values`.
    pub fn is_derived(&self) -> bool {
        matches!(
            self,
            Self::CreatedTime | Self::EditedTime | Self::CreatedBy | Self::EditedBy
        )
    }

    /// Whether a view can group by it.
    pub fn can_group(&self) -> bool {
        matches!(
            self,
            Self::Select
                | Self::MultiSelect
                | Self::Checkbox
                | Self::Person
                | Self::Date
                | Self::CreatedTime
                | Self::EditedTime
                | Self::CreatedBy
                | Self::EditedBy
        )
    }
}

string_enum! {
    /// An option's colour, by name: the host's theme says what each looks like.
    #[derive(Default)]
    pub enum Color {
        #[default]
        Default = "default",
        Gray = "gray",
        Brown = "brown",
        Orange = "orange",
        Yellow = "yellow",
        Green = "green",
        Blue = "blue",
        Purple = "purple",
        Pink = "pink",
        Red = "red",
    }
}

string_enum! {
    /// How a number property shows its values.
    pub enum NumberFormat {
        Plain = "plain",
        /// 0.25 shows as 25%.
        Percent = "percent",
        /// In `config.currency` (an ISO 4217 code).
        Currency = "currency",
    }
}

/// One choice of a select or multi-select property.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SelectOption {
    /// A lowercase UUID: values hold this, so renaming an option changes no row.
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub color: Color,
}

/// A property's settings. Which keys apply depends on its type:
///
/// - number: `format`, `currency`, `precision`
/// - select and multiSelect: `options`
/// - date: `includeTime`, `range` (what a new value gets)
/// - relation: `databaseId`, `twoWay`, `backPropertyId`
/// - person: `kinds`
///
/// Keys that don't apply are left out when written. Absent and null mean the same.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PropertyConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<NumberFormat>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    /// Digits after the point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub precision: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<SelectOption>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_time: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range: Option<bool>,
    /// The database doc a relation's rows belong to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database_id: Option<String>,
    /// Whether the related database has a property listing this side's rows back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub two_way: Option<bool>,
    /// That property's id, in the related database.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub back_property_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kinds: Option<Vec<PersonKind>>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// A database's column: a name, a type and its settings.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Property {
    /// A lowercase UUID: row values are keyed by it.
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: PropertyType,
    #[serde(default)]
    pub config: PropertyConfig,
    /// A fractional index: the property's place among the database's properties.
    #[serde(default)]
    pub order: String,
    #[serde(flatten)]
    pub extra: Extra,
}

/// A relation property's settings, read from its config.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RelationTarget<'a> {
    pub database_id: &'a str,
    pub two_way: bool,
    pub back_property_id: Option<&'a str>,
}

impl Property {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        kind: PropertyType,
        order: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            kind,
            config: PropertyConfig::default(),
            order: order.into(),
            extra: Extra::new(),
        }
    }

    pub fn with_config(mut self, config: PropertyConfig) -> Self {
        self.config = config;
        self
    }

    /// A select's or multi-select's options, in their order (empty for other types).
    pub fn options(&self) -> &[SelectOption] {
        self.config.options.as_deref().unwrap_or(&[])
    }

    pub fn option(&self, id: &str) -> Option<&SelectOption> {
        self.options().iter().find(|o| o.id == id)
    }

    /// A relation's target, if it is one and names its database.
    pub fn relation(&self) -> Option<RelationTarget<'_>> {
        if self.kind != PropertyType::Relation {
            return None;
        }
        Some(RelationTarget {
            database_id: self.config.database_id.as_deref()?,
            two_way: self.config.two_way.unwrap_or(false),
            back_property_id: self.config.back_property_id.as_deref(),
        })
    }

    /// The kinds of people a person property may hold: both when it doesn't say.
    pub fn person_kinds(&self) -> &[PersonKind] {
        const BOTH: &[PersonKind] = &[PersonKind::Owner, PersonKind::Agent];
        self.config.kinds.as_deref().unwrap_or(BOTH)
    }
}
