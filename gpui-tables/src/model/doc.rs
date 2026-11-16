//! Docs: the host's tree of pages, where a database is a doc of kind "database" and each of its
//! rows is a doc of kind "page" under it.

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Value as Json, json};

use super::limits::{ICON_ASSET, ICON_BYTES, icon_problems, is_sha256};
use super::{Extra, Property, Value, View, string_enum};

string_enum! {
    /// What a doc is.
    pub enum DocKind {
        /// A page: a document, or a database's row.
        Page = "page",
        /// A database: its `database` field holds the properties and views.
        Database = "database",
    }
}

string_enum! {
    /// The kinds of people a `person` property may hold (its `config.kinds`).
    pub enum PersonKind {
        /// The person who owns the workspace.
        Owner = "owner",
        /// One of the owner's agents.
        Agent = "agent",
    }
}

/// Who wrote something, or who a `person` value names: the owner or one of their agents.
/// Written as `{"kind": "owner"}` or `{"kind": "agent", "agentId": "career-planning"}`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Person {
    Owner,
    /// An agent, by its resource name.
    Agent(String),
    /// A kind this version doesn't know, kept as written. It sorts and groups as no one.
    Other(Json),
}

impl Person {
    pub fn agent(id: impl Into<String>) -> Self {
        Self::Agent(id.into())
    }

    pub fn kind(&self) -> Option<PersonKind> {
        match self {
            Self::Owner => Some(PersonKind::Owner),
            Self::Agent(_) => Some(PersonKind::Agent),
            Self::Other(_) => None,
        }
    }

    /// The JSON it's written as.
    pub fn to_json(&self) -> Json {
        match self {
            Self::Owner => json!({ "kind": "owner" }),
            Self::Agent(id) => json!({ "kind": "agent", "agentId": id }),
            Self::Other(j) => j.clone(),
        }
    }

    /// Reads one. Anything that isn't an object with a string `kind` is None; a kind it doesn't
    /// know, or an agent without an id, is `Other`.
    pub fn from_json(j: &Json) -> Option<Self> {
        let o = j.as_object()?;
        let kind = o.get("kind")?.as_str()?;
        Some(match kind {
            "owner" if o.len() == 1 => Self::Owner,
            "agent" if o.len() == 2 => match o.get("agentId").and_then(Json::as_str) {
                Some(id) => Self::Agent(id.to_owned()),
                None => Self::Other(j.clone()),
            },
            _ => Self::Other(j.clone()),
        })
    }

    /// A stable string for it: `owner`, `agent:<id>`, or None for an unknown kind. Group keys
    /// and sorts use it.
    pub fn key(&self) -> Option<String> {
        match self {
            Self::Owner => Some("owner".into()),
            Self::Agent(id) => Some(format!("agent:{id}")),
            Self::Other(_) => None,
        }
    }
}

impl Serialize for Person {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.to_json().serialize(s)
    }
}

impl<'de> Deserialize<'de> for Person {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let j = Json::deserialize(d)?;
        Person::from_json(&j).ok_or_else(|| serde::de::Error::custom("a person is {kind, ...}"))
    }
}

/// A row's values: a [`Value`] per property id. The title, created and edited times and authors
/// aren't here: they're the row doc's own `title`, `created`, `updated`, `author` and
/// `updatedBy`.
pub type Values = BTreeMap<String, Value>;

/// What makes a doc a database: its properties (the columns) and its saved views. The host
/// writes this field last-write-wins as a whole.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DatabaseFields {
    #[serde(default)]
    pub properties: Vec<Property>,
    #[serde(default)]
    pub views: Vec<View>,
    #[serde(flatten)]
    pub extra: Extra,
}

impl DatabaseFields {
    pub fn property(&self, id: &str) -> Option<&Property> {
        self.properties.iter().find(|p| p.id == id)
    }

    /// The title property: the first property of type title. A database has exactly one.
    pub fn title_property(&self) -> Option<&Property> {
        self.properties
            .iter()
            .find(|p| p.kind == super::PropertyType::Title)
    }

    pub fn view(&self, id: &str) -> Option<&View> {
        self.views.iter().find(|v| v.id == id)
    }

    /// The properties by their `order` (then id, so equal orders are still stable).
    pub fn properties_in_order(&self) -> Vec<&Property> {
        let mut p: Vec<&Property> = self.properties.iter().collect();
        p.sort_by(|a, b| (&a.order, &a.id).cmp(&(&b.order, &b.id)));
        p
    }

    /// The views by their `order` (then id).
    pub fn views_in_order(&self) -> Vec<&View> {
        let mut v: Vec<&View> = self.views.iter().collect();
        v.sort_by(|a, b| (&a.order, &a.id).cmp(&(&b.order, &b.id)));
        v
    }
}

/// A doc in the host's tree: a page, a database, or a database's row (a page whose
/// `parentDocId` is the database). The crate reads `id`, `kind`, `title`, `order`, the times and
/// authors, `deleted`, `database` and `values`; the rest is carried for the host.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Doc {
    /// A lowercase UUID, made by whoever created the doc.
    pub id: String,
    pub kind: DocKind,
    /// Null reads as empty.
    #[serde(default, deserialize_with = "null_as_empty")]
    pub title: String,
    /// A fractional index among its siblings (see [`crate::order`]). For a row, its place in the
    /// database's manual order.
    #[serde(default, deserialize_with = "null_as_empty")]
    pub order: String,
    /// The folder a full-page doc is in.
    #[serde(default)]
    pub folder_id: Option<String>,
    /// The page it's under: for a row, the database doc; for an inline database, the page that
    /// embeds it.
    #[serde(default)]
    pub parent_doc_id: Option<String>,
    /// For an inline database, the block in its parent page that embeds it.
    #[serde(default)]
    pub parent_object_id: Option<String>,
    /// The host's, carried as written.
    #[serde(default)]
    pub cover: Json,
    #[serde(default)]
    pub favorite: bool,
    /// The host's, carried as written.
    #[serde(default)]
    pub agents: Json,
    /// Who created it: a row's created-by ([`Doc::created_by`]).
    #[serde(default)]
    pub author: Option<Person>,
    /// Who changed it last: a row's edited-by ([`Doc::edited_by`], which falls back to
    /// `author`).
    #[serde(default)]
    pub updated_by: Option<Person>,
    /// Milliseconds since the Unix epoch: a row's created time.
    #[serde(default)]
    pub created: i64,
    /// Milliseconds since the Unix epoch: a row's edited time.
    #[serde(default)]
    pub updated: i64,
    /// The host's revision number, carried and never read.
    #[serde(default)]
    pub rev: Option<i64>,
    /// The host's sequence number, carried and never read.
    #[serde(default)]
    pub seq: Option<i64>,
    /// A tombstone: the doc is gone, and views leave it out.
    #[serde(default)]
    pub deleted: bool,
    /// A database's properties and views (kind "database" only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<DatabaseFields>,
    /// A row's values (a page under a database only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub values: Option<Values>,
    #[serde(flatten)]
    pub extra: Extra,
}

fn null_as_empty<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(Option::<String>::deserialize(d)?.unwrap_or_default())
}

impl Doc {
    /// A doc with just an id, kind and title, everything else empty.
    pub fn new(id: impl Into<String>, kind: DocKind, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            kind,
            title: title.into(),
            order: String::new(),
            folder_id: None,
            parent_doc_id: None,
            parent_object_id: None,
            cover: Json::Null,
            favorite: false,
            agents: Json::Null,
            author: None,
            updated_by: None,
            created: 0,
            updated: 0,
            rev: None,
            seq: None,
            deleted: false,
            database: None,
            values: None,
            extra: Extra::new(),
        }
    }

    /// A row of `database_id`: a page under it with these values.
    pub fn row(
        id: impl Into<String>,
        database_id: impl Into<String>,
        title: impl Into<String>,
        order: impl Into<String>,
        values: Values,
    ) -> Self {
        let mut doc = Self::new(id, DocKind::Page, title);
        doc.parent_doc_id = Some(database_id.into());
        doc.order = order.into();
        doc.values = Some(values);
        doc
    }

    /// A row's value for a property id, if it has one.
    pub fn value(&self, property_id: &str) -> Option<&Value> {
        self.values.as_ref()?.get(property_id)
    }

    /// Who created it: a `createdBy` property's value.
    pub fn created_by(&self) -> Option<&Person> {
        self.author.as_ref()
    }

    /// Who changed it last: an `editedBy` property's value. A doc written before `updatedBy`
    /// was kept has only its `author`, and reads as that.
    pub fn edited_by(&self) -> Option<&Person> {
        self.updated_by.as_ref().or(self.author.as_ref())
    }

    /// The doc's emoji icon, when it has a valid one: its `icon` field (which the crate carries
    /// in `extra`) when that's one grapheme cluster of at most 16 bytes, as
    /// [`Limits::check_doc`](super::Limits::check_doc) requires. An image icon
    /// (`asset:<sha256>`, see [`Doc::icon_asset`]), an empty or invalid string, or anything
    /// that isn't a string is None, so it's never drawn as text. A list view draws a row's
    /// before its title.
    pub fn icon(&self) -> Option<&str> {
        let icon = self.extra.get("icon")?.as_str()?;
        // The byte check first, so a long string is never segmented.
        let valid = icon.len() <= ICON_BYTES
            && !icon.starts_with(ICON_ASSET)
            && icon_problems(icon).is_empty();
        valid.then_some(icon)
    }

    /// The SHA-256 (64 lowercase hex characters) of the doc's image icon, when its `icon` field
    /// is a valid `asset:<sha256>`, for a host that draws images. The views don't draw these
    /// yet.
    pub fn icon_asset(&self) -> Option<&str> {
        let hash = self.extra.get("icon")?.as_str()?.strip_prefix(ICON_ASSET)?;
        is_sha256(hash).then_some(hash)
    }
}
