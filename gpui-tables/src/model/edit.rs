//! What the crate emits: edits for the host to store, and requests (opening a row) for it to
//! act on. The crate never changes the host's data itself; the host applies an edit and hands the
//! changed docs back.

use serde::{Deserialize, Serialize};

use super::{Property, Value, Values, View};

/// One change to a database or its rows. Edits come in batches ([`TableEvent::Edits`]) that the
/// host applies together: a card dropped in another board column is a `rowsMoved` and a
/// `rowValueChanged` for the grouped property, and a two-way relation's change is a
/// `rowValueChanged` and a `relationChanged`.
///
/// Written as JSON with a `type` tag, e.g.
/// `{"type": "rowValueChanged", "rowId": "...", "propertyId": "...", "value": 3}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Edit {
    /// A new row: a page doc under the database, with these fields. The host fills in the rest
    /// (times, author, rev, seq).
    RowCreated {
        row_id: String,
        database_id: String,
        order: String,
        title: String,
        values: Values,
    },
    /// The row doc's `title` (the title property's value) changed.
    RowTitleChanged {
        row_id: String,
        title: String,
    },
    /// One property's value in a row changed: the host's per-property `patch` on the row's
    /// `values`. `null` removes it.
    RowValueChanged {
        row_id: String,
        property_id: String,
        value: Option<Value>,
    },
    /// A two-way relation changed on this side (the same batch has its `rowValueChanged`), so
    /// the rows of `databaseId` whose ids are in `added` now list `rowId` in their
    /// `backPropertyId`, and those in `removed` no longer do.
    /// [`crate::engine::relation::back_edits`] turns this into their `rowValueChanged`s.
    RelationChanged {
        row_id: String,
        property_id: String,
        database_id: String,
        back_property_id: String,
        added: Vec<String>,
        removed: Vec<String>,
    },
    /// Rows took new places in the manual order: `orders[i]` is `rowIds[i]`'s new `order`.
    RowsMoved {
        row_ids: Vec<String>,
        orders: Vec<String>,
    },
    /// Rows were deleted: the host tombstones them (`deleted: true`).
    RowsDeleted {
        row_ids: Vec<String>,
    },
    /// A property was added to a database's `database.properties`. (A new two-way relation is
    /// two of these, one for each database.)
    PropertyCreated {
        database_id: String,
        property: Property,
    },
    /// A property changed (its name, type, config or order): the whole property, last write
    /// wins.
    PropertyChanged {
        database_id: String,
        property: Property,
    },
    /// A property was removed. Rows' values for it are left for the host to clear or keep; the
    /// crate ignores values without a property, and views' references to it.
    PropertyDeleted {
        database_id: String,
        property_id: String,
    },
    ViewCreated {
        database_id: String,
        view: View,
    },
    /// A view changed (its sort, filter, grouping, columns, ...): the whole view, last write
    /// wins.
    ViewChanged {
        database_id: String,
        view: View,
    },
    ViewDeleted {
        database_id: String,
        view_id: String,
    },
}

impl Edit {
    /// The rows this edit touches, for a host that locks or batches by row.
    pub fn row_ids(&self) -> Vec<&str> {
        match self {
            Self::RowCreated { row_id, .. }
            | Self::RowTitleChanged { row_id, .. }
            | Self::RowValueChanged { row_id, .. }
            | Self::RelationChanged { row_id, .. } => vec![row_id],
            Self::RowsMoved { row_ids, .. } | Self::RowsDeleted { row_ids } => {
                row_ids.iter().map(String::as_str).collect()
            }
            _ => Vec::new(),
        }
    }
}

/// What a table, board or list view hands its host.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TableEvent {
    /// Edits to apply together.
    Edits { edits: Vec<Edit> },
    /// Open a row as a page. The crate never navigates; the host opens the row doc.
    OpenRow { row_id: String },
}
