//! The JSON the host stores and the crate reads: docs, a database's properties and views, rows'
//! values, the edits the crate emits, and the limits on all of it.
//!
//! A database is a doc of kind `"database"` whose `database` field holds its [`Property`]s and
//! [`View`]s ([`DatabaseFields`]). Its rows are docs of kind `"page"` whose `parentDocId` is the
//! database doc and whose `values` hold a [`Value`] per property id. Every type here keeps the
//! fields it doesn't know (`extra`) and every enum keeps the strings it doesn't know (`Other`),
//! so a doc written by a newer host, or an agent, reads and writes back unchanged.

mod doc;
mod edit;
mod limits;
mod property;
mod value;
mod view;

pub use doc::{DatabaseFields, Doc, DocKind, Person, PersonKind, Values};
pub use edit::{Edit, TableEvent};
#[cfg(feature = "gpui")]
pub(crate) use limits::truncate_chars;
pub use limits::{Limits, Problem, ProblemKind};
pub use property::{Color, NumberFormat, Property, PropertyConfig, PropertyType, SelectOption};
pub use value::{DateValue, Value};
pub use view::{
    Calculation, CardPreview, CardSize, Column, Condition, DateBucket, Direction, Filter, FilterOp,
    Sort, View, ViewType, Within,
};

/// A fresh id: a lowercase v4 UUID, as the creator of a row, property, view or option makes it.
pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// The JSON object fields a type doesn't know, kept as they came.
pub type Extra = serde_json::Map<String, serde_json::Value>;

/// An enum written in JSON as one of a set of strings, keeping a string it doesn't know as
/// `Other`, so data from a newer writer round-trips.
macro_rules! string_enum {
    (
        $(#[$meta:meta])*
        pub enum $name:ident {
            $( $(#[$vmeta:meta])* $variant:ident = $text:literal, )*
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        pub enum $name {
            $( $(#[$vmeta])* $variant, )*
            /// A string this version doesn't know, kept as written.
            Other(String),
        }

        impl $name {
            /// Every value this version knows, in declaration order.
            pub const ALL: &'static [$name] = &[$($name::$variant),*];

            /// The string it's written as.
            pub fn as_str(&self) -> &str {
                match self {
                    $( Self::$variant => $text, )*
                    Self::Other(s) => s,
                }
            }

            /// Reads one; a string it doesn't know becomes `Other`.
            pub fn parse(s: &str) -> Self {
                match s {
                    $( $text => Self::$variant, )*
                    _ => Self::Other(s.to_owned()),
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(self.as_str())
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = std::borrow::Cow::<'de, str>::deserialize(d)?;
                Ok(Self::parse(&s))
            }
        }
    };
}
pub(crate) use string_enum;

/// A number written as an integer when it is one (and small enough to be exact in a double), so
/// `240` reads and writes back as `240`, not `240.0`.
pub(crate) fn number_json(x: f64) -> serde_json::Value {
    const EXACT: f64 = 9_007_199_254_740_992.0; // 2^53
    if x.fract() == 0.0 && x.abs() < EXACT {
        serde_json::Value::from(x as i64)
    } else {
        serde_json::Number::from_f64(x).map_or(serde_json::Value::Null, serde_json::Value::Number)
    }
}

/// Serde helpers for an `Option<f64>` written as [`number_json`] does.
pub(crate) mod opt_number {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(x: &Option<f64>, s: S) -> Result<S::Ok, S::Error> {
        match x {
            Some(x) => super::number_json(*x).serialize(s),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
        Ok(Option::<f64>::deserialize(d)?.filter(|x| x.is_finite()))
    }
}
