//! What the table asks of the app around it: its look, names it can't know, pickers' choices,
//! optionally a text field of its own for typed values, and where its events go.

use gpui::{App, Bounds, Pixels, WeakEntity, Window};

use super::TableTheme;
use super::table::TableView;
use crate::engine::Lookup;
use crate::model::{Limits, Person, Property, PropertyType, TableEvent};

/// A row of another database (or this one), as a relation picker lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelatedRow {
    pub id: String,
    pub title: String,
}

/// A database a relation property can point at, as the relation menu lists it, with its
/// properties: a two-way relation's other side is made among them, its `order` after theirs,
/// its name unlike theirs, and only while there's room under the properties' limit.
#[derive(Clone, Debug, PartialEq)]
pub struct RelatedDatabase {
    /// The database doc's id.
    pub id: String,
    pub title: String,
    pub properties: Vec<Property>,
}

/// What a typed value is for.
#[derive(Clone, Debug, PartialEq)]
pub enum TextTarget {
    /// A cell: a row's title (the title property) or a text, number, URL, email or phone value.
    Cell { row_id: String, property_id: String },
    /// The value of a filter condition the header menu is making (`contains` for text,
    /// `is`, `greater` or `less` for a number).
    Filter {
        property_id: String,
        op: crate::model::FilterOp,
    },
}

/// A request for a typed value, made to a host that shows its own field
/// ([`TableHost::edit_text`]): the host shows one (over `bounds`, the cell's place in the window,
/// or wherever it likes), and answers through `reply`.
pub struct TextEdit {
    pub target: TextTarget,
    /// The property's type: what the field is for (a number, an email, ...).
    pub kind: PropertyType,
    /// The property's name, for a label or placeholder.
    pub label: String,
    /// What's there now, as someone would type it (a number without grouping or symbols).
    pub text: String,
    /// The most characters the value may have (the limits'), if it has a limit.
    pub max_chars: Option<usize>,
    /// The cell's (or the header's) bounds in the window.
    pub bounds: Bounds<Pixels>,
    pub reply: TextReply,
}

/// Where a typed value goes back to. Answering more than once, or after the table has moved on
/// (another edit began, the row was deleted), does nothing.
#[derive(Clone)]
pub struct TextReply {
    pub(crate) view: WeakEntity<TableView>,
    pub(crate) token: u64,
}

impl TextReply {
    /// The typed text: the table reads it as its property's type (a number from "1,200" or
    /// "$1,200", a percent from "25" or "25%"), cuts it to the limits, and emits the edit. Empty
    /// text clears the value. Text that isn't a number, for a number, changes nothing.
    pub fn commit(&self, text: &str, window: &mut Window, cx: &mut App) {
        let (token, text) = (self.token, text.to_owned());
        let _ = self.view.update(cx, |view, cx| {
            view.finish_text_edit(token, Some(text), window, cx)
        });
    }

    /// The field was closed without a value.
    pub fn cancel(&self, window: &mut Window, cx: &mut App) {
        let token = self.token;
        let _ = self.view.update(cx, |view, cx| {
            view.finish_text_edit(token, None, window, cx)
        });
    }

    /// Hands the edit back to the table, which edits it in place with gpui-core's `Input` (what
    /// [`TableHost::edit_text`] does unless the host overrides it). For a host that shows its
    /// own field for some edits and not others.
    pub fn edit_in_place(&self, window: &mut Window, cx: &mut App) {
        let token = self.token;
        let _ = self
            .view
            .update(cx, |view, cx| view.open_editor(token, window, cx));
    }
}

/// The app around a table: it stores the data, opens rows, knows people and other databases'
/// rows, and gives the theme. The table hands it every change as a [`TableEvent`] and changes
/// nothing itself; the host applies the edits and gives the table the changed docs
/// ([`TableView::set_docs`]).
pub trait TableHost {
    /// The table's look. Asked once each time the table draws.
    fn theme(&self, cx: &App) -> TableTheme;

    /// An event: edits to apply together, or a row to open. Called after the table's own
    /// update, so the host may apply the edits and hand the table its new docs from here.
    fn event(&self, event: TableEvent, window: &mut Window, cx: &mut App);

    /// The name to show for a person: the owner's, an agent's. Without one, the owner shows as
    /// "Owner" and an agent as its id.
    fn person_name(&self, person: &Person) -> Option<String> {
        let _ = person;
        None
    }

    /// The title of a row of another database (or this one), for a relation cell. Without one,
    /// the row shows as "Untitled".
    fn row_title(&self, database_id: &str, row_id: &str) -> Option<String> {
        let _ = (database_id, row_id);
        None
    }

    /// The people a person property's picker offers (the table keeps only the kinds the
    /// property allows), and the people a created-by or edited-by column's menu filters by.
    /// The owner alone by default.
    fn people(&self, property: &Property) -> Vec<Person> {
        let _ = property;
        vec![Person::Owner]
    }

    /// The rows a relation picker offers: `database_id`'s rows, in the order to list them.
    fn relation_rows(&self, database_id: &str) -> Vec<RelatedRow> {
        let _ = database_id;
        Vec::new()
    }

    /// The databases a relation property can relate to, in the order to list them, each with
    /// its properties (for a two-way relation's other side). The table's own database is always
    /// offered, whether or not it's here; by default it's the only one.
    fn databases(&self) -> Vec<RelatedDatabase> {
        Vec::new()
    }

    /// A typed value is wanted: a text, number, URL, email, phone or title cell's, or the value
    /// of a filter of those types. By default the table edits it in place, with gpui-core's
    /// `Input` in the cell (or in the column's menu, for a filter).
    ///
    /// A host with a field of its own overrides this: it shows its field and answers through
    /// `edit.reply` (`commit` or `cancel`), and the table reads, limits and emits the answer as
    /// it does its own. It may also hand an edit back with `edit.reply.edit_in_place`. Called
    /// after the table's own update, so the host may change the table from here. An override
    /// that does nothing leaves such values to be edited on the row's page.
    fn edit_text(&self, edit: TextEdit, window: &mut Window, cx: &mut App) {
        edit.reply.edit_in_place(window, cx);
    }

    /// The time now, in milliseconds since the epoch: for date groups and filters relative to
    /// today. Read when the rows are worked out again, never on a timer.
    fn now(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64)
    }

    /// The viewer's offset from UTC, in minutes east.
    fn utc_offset_minutes(&self) -> i32 {
        0
    }

    /// Whether weeks start on Monday (else Sunday), for date groups and the date picker.
    fn week_starts_monday(&self) -> bool {
        true
    }

    /// The limits typed and picked values are held to.
    fn limits(&self) -> Limits {
        Limits::default()
    }
}

/// The host as the engine's [`Lookup`].
pub(crate) struct HostLookup<'a>(pub &'a dyn TableHost);

impl Lookup for HostLookup<'_> {
    fn person_name(&self, person: &Person) -> Option<String> {
        self.0.person_name(person)
    }

    fn row_title(&self, database_id: &str, row_id: &str) -> Option<String> {
        self.0.row_title(database_id, row_id)
    }
}

/// A person's name as the table shows it.
pub(crate) fn person_label(host: &dyn TableHost, person: &Person) -> String {
    host.person_name(person).unwrap_or_else(|| match person {
        Person::Owner => "Owner".into(),
        Person::Agent(id) => id.clone(),
        Person::Other(_) => "Someone".into(),
    })
}
