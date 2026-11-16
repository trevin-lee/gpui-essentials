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

    /// The types a person can give a property, in the order the menus list them: every type
    /// but the title (a database has exactly one, which no other can become).
    pub const ADDABLE: &'static [PropertyType] = &[
        Self::Text,
        Self::Number,
        Self::Select,
        Self::MultiSelect,
        Self::Date,
        Self::Checkbox,
        Self::Url,
        Self::Email,
        Self::Phone,
        Self::Person,
        Self::Relation,
        Self::CreatedTime,
        Self::EditedTime,
        Self::CreatedBy,
        Self::EditedBy,
    ];

    /// Its name in menus, and a new property's name until it's given one: "Multi-select",
    /// "Created time". A type this version doesn't know is named as it's written.
    pub fn label(&self) -> &str {
        match self {
            Self::Title => "Title",
            Self::Text => "Text",
            Self::Number => "Number",
            Self::Select => "Select",
            Self::MultiSelect => "Multi-select",
            Self::Date => "Date",
            Self::Checkbox => "Checkbox",
            Self::Url => "URL",
            Self::Email => "Email",
            Self::Phone => "Phone",
            Self::Person => "Person",
            Self::Relation => "Relation",
            Self::CreatedTime => "Created time",
            Self::EditedTime => "Edited time",
            Self::CreatedBy => "Created by",
            Self::EditedBy => "Edited by",
            Self::Other(s) => s,
        }
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
/// Keys that don't apply are left out when written, except `options`, which a property keeps
/// through a change of type ([`Property::retyped`]). Absent and null mean the same.
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

impl PropertyConfig {
    /// What a new property of `kind` starts with: a number plain, a select or multi-select
    /// with no options, a date without its time, a person allowing the owner and agents. A
    /// relation's database (and whether it's two-way) is the maker's to set; every other type
    /// has nothing to set.
    pub fn default_for(kind: &PropertyType) -> Self {
        let mut config = Self::default();
        match kind {
            PropertyType::Number => config.format = Some(NumberFormat::Plain),
            PropertyType::Select | PropertyType::MultiSelect => config.options = Some(Vec::new()),
            PropertyType::Date => config.include_time = Some(false),
            PropertyType::Person => config.kinds = Some(vec![PersonKind::Owner, PersonKind::Agent]),
            _ => {}
        }
        config
    }
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

    /// The property changed to `kind`, as changing its type from a view does: the new type's
    /// [`PropertyConfig::default_for`], keeping the options it had (so a select becomes a
    /// multi-select with the same options, and a select changed to text and back finds them
    /// again) and the config keys this version doesn't know. Rows are never rewritten: their
    /// values stay as they were and are read as the new type reads them
    /// ([`Value::option_ids`](super::Value::option_ids), [`Property::shown_text`]), empty
    /// where they don't fit. The title can't change type, and nothing can become a title:
    /// either way the property comes back unchanged.
    pub fn retyped(&self, kind: PropertyType) -> Property {
        if self.kind == PropertyType::Title || kind == PropertyType::Title || kind == self.kind {
            return self.clone();
        }
        let mut config = PropertyConfig::default_for(&kind);
        if let Some(options) = &self.config.options
            && !options.is_empty()
        {
            config.options = Some(options.clone());
        }
        config.extra = self.config.extra.clone();
        Property {
            kind,
            config,
            ..self.clone()
        }
    }

    /// Text as a title, text, URL, email or phone cell shows it: the name of the option it
    /// names, when the property was a select or multi-select and kept its options (its values
    /// are option ids), else the text itself.
    pub fn shown_text<'a>(&'a self, text: &'a str) -> &'a str {
        match self.config.options.as_deref() {
            Some(options) if !options.is_empty() => options
                .iter()
                .find(|o| o.id == text)
                .map_or(text, |o| o.name.as_str()),
            _ => text,
        }
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
