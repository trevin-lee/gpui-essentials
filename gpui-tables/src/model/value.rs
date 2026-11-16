//! A row's value for one property.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value as Json};

use super::{Person, PropertyType, number_json};

/// A date value: `{start, end?, includeTime}`, in milliseconds since the Unix epoch.
///
/// Without `includeTime`, `start` (and `end`) is midnight UTC of the date meant, and is read as
/// that date in every time zone. With it, they're instants, shown in the viewer's zone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateValue {
    pub start: i64,
    /// The end of a range, inclusive; at or after `start`.
    pub end: Option<i64>,
    pub include_time: bool,
}

/// One property's value in a row. The JSON's shape says which variant it is; the property's type
/// says what it means:
///
/// | type | JSON | variant |
/// |---|---|---|
/// | text, url, email, phone, select (an option id) | `"..."` | `Text` |
/// | number | `12.5` | `Number` |
/// | checkbox | `true` | `Bool` |
/// | multiSelect (option ids), relation (row doc ids) | `["...", "..."]` | `Ids` |
/// | person | `[{"kind": "owner"}, ...]` | `People` |
/// | date | `{"start": ms, "end": ms, "includeTime": false}` | `Date` |
///
/// An empty list reads as `Ids`; the accessors treat it as no people too. Anything else is kept
/// as `Other`, and the engine treats it as empty.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    /// Always finite.
    Number(f64),
    Text(String),
    Ids(Vec<String>),
    People(Vec<Person>),
    Date(DateValue),
    /// A shape this version doesn't read, kept as written.
    Other(Json),
}

impl Value {
    pub fn text(s: impl Into<String>) -> Self {
        Self::Text(s.into())
    }

    /// A number; NaN and the infinities (which JSON can't hold) are `Null`.
    pub fn number(x: f64) -> Self {
        if x.is_finite() {
            Self::Number(x)
        } else {
            Self::Null
        }
    }

    pub fn ids<S: Into<String>>(ids: impl IntoIterator<Item = S>) -> Self {
        Self::Ids(ids.into_iter().map(Into::into).collect())
    }

    /// A date (no time) given as days since 1970-01-01.
    pub fn date_on_day(day: i64) -> Self {
        Self::Date(DateValue {
            start: day * crate::engine::DAY_MS,
            end: None,
            include_time: false,
        })
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Text(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_number(&self) -> Option<f64> {
        match self {
            Self::Number(x) => Some(*x),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_ids(&self) -> Option<&[String]> {
        match self {
            Self::Ids(ids) => Some(ids),
            _ => None,
        }
    }

    /// The people, or an empty list for `[]`.
    pub fn as_people(&self) -> Option<&[Person]> {
        match self {
            Self::People(p) => Some(p),
            Self::Ids(ids) if ids.is_empty() => Some(&[]),
            _ => None,
        }
    }

    pub fn as_date(&self) -> Option<&DateValue> {
        match self {
            Self::Date(d) => Some(d),
            _ => None,
        }
    }

    /// Whether this value has the shape a property of `kind` holds. Null fits every type (it
    /// means no value); derived types hold nothing.
    pub fn fits(&self, kind: &PropertyType) -> bool {
        use PropertyType as T;
        match (kind, self) {
            (_, Self::Null) => true,
            (T::Title | T::Text | T::Url | T::Email | T::Phone | T::Select, Self::Text(_)) => true,
            (T::Number, Self::Number(_)) => true,
            (T::Checkbox, Self::Bool(_)) => true,
            (T::MultiSelect | T::Relation, Self::Ids(_)) => true,
            (T::Person, Self::People(_)) => true,
            (T::Person, Self::Ids(ids)) => ids.is_empty(),
            (T::Date, Self::Date(_)) => true,
            _ => false,
        }
    }

    /// The JSON it's written as.
    pub fn to_json(&self) -> Json {
        match self {
            Self::Null => Json::Null,
            Self::Bool(b) => Json::Bool(*b),
            Self::Number(x) => number_json(*x),
            Self::Text(s) => Json::String(s.clone()),
            Self::Ids(ids) => Json::Array(ids.iter().cloned().map(Json::String).collect()),
            Self::People(p) => Json::Array(p.iter().map(Person::to_json).collect()),
            Self::Date(d) => {
                let mut m = Map::new();
                m.insert("start".into(), d.start.into());
                if let Some(end) = d.end {
                    m.insert("end".into(), end.into());
                }
                m.insert("includeTime".into(), d.include_time.into());
                Json::Object(m)
            }
            Self::Other(j) => j.clone(),
        }
    }

    /// Reads one from JSON by its shape (see the table above). Never fails: what it doesn't
    /// read is `Other`.
    pub fn from_json(j: Json) -> Self {
        match j {
            Json::Null => Self::Null,
            Json::Bool(b) => Self::Bool(b),
            Json::Number(ref n) => match n.as_f64() {
                Some(x) if x.is_finite() => Self::Number(x),
                _ => Self::Other(j),
            },
            Json::String(s) => Self::Text(s),
            Json::Array(a) => {
                if a.iter().all(Json::is_string) {
                    Self::Ids(
                        a.into_iter()
                            .filter_map(|s| match s {
                                Json::String(s) => Some(s),
                                _ => None,
                            })
                            .collect(),
                    )
                } else if let Some(people) =
                    a.iter().map(Person::from_json).collect::<Option<Vec<_>>>()
                {
                    Self::People(people)
                } else {
                    Self::Other(Json::Array(a))
                }
            }
            Json::Object(ref o) => date_from(o).map_or(Self::Other(j), Self::Date),
        }
    }

    /// Empty for the engine: null, "", [], or a shape it doesn't read.
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Null | Self::Other(_) => true,
            Self::Text(s) => s.is_empty(),
            Self::Ids(v) => v.is_empty(),
            Self::People(v) => v.is_empty(),
            Self::Bool(_) | Self::Number(_) | Self::Date(_) => false,
        }
    }
}

fn date_from(o: &Map<String, Json>) -> Option<DateValue> {
    if !o
        .keys()
        .all(|k| matches!(k.as_str(), "start" | "end" | "includeTime"))
    {
        return None;
    }
    let start = o.get("start")?.as_i64()?;
    let end = match o.get("end") {
        None => None,
        Some(e) => Some(e.as_i64()?),
    };
    let include_time = match o.get("includeTime") {
        None => false,
        Some(b) => b.as_bool()?,
    };
    Some(DateValue {
        start,
        end,
        include_time,
    })
}

impl Serialize for Value {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.to_json().serialize(s)
    }
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Self::from_json(Json::deserialize(d)?))
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Self::Text(s.to_owned())
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Self::Text(s)
    }
}

impl From<f64> for Value {
    fn from(x: f64) -> Self {
        Self::number(x)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Self::Bool(b)
    }
}

impl From<DateValue> for Value {
    fn from(d: DateValue) -> Self {
        Self::Date(d)
    }
}

impl From<Vec<Person>> for Value {
    fn from(p: Vec<Person>) -> Self {
        Self::People(p)
    }
}
