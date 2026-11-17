//! Changing a database's properties from the table, as in Notion: the + after the last column
//! adds one (a name and a type), and a column's menu renames it, changes its type, edits its
//! options, a number's format, a date's time, a relation's database and other side, and who a
//! person property allows, and deletes it.
//!
//! Every change is a batch of edits for the host: `propertyCreated` with a `viewChanged` adding
//! the new column, `propertyChanged` with the whole property, `propertyDeleted`, and for a
//! two-way relation the other database's `propertyCreated` or `propertyDeleted` in the same
//! batch. The table shows each change at once (as it does a view's) and keeps it until the host
//! hands it new docs. Rows are never rewritten: values read as the new type reads them.
//!
//! The menus are gpui-core's `PopupMenu` in the table's `Popover`. The column's menu opens the
//! types, the options (and each option's colour, place and delete), a number's format (and its
//! currency), a relation's database and who a person property allows as submenus beside their
//! entries. What needs a name typed or a delete confirmed takes a step of its own in the
//! popover, as Notion's field over its menu does: the name in gpui-core's `Input`, Enter saving
//! and Escape closing, Save and Back for a pointer or a finger, and back on the column's menu
//! where the step came from. The + menu is gpui-core's `Picker` naming rather than filtering:
//! the new property's name typed over its types.

use gpui::{App, AppContext as _, Context, Entity, Subscription, Window};
use gpui_core::{InputEvent, InputState};

use super::host::RelatedDatabase;
use super::ops;
use super::table::{Action, Entry, Popover, Reopen, TableView, type_icon};
use crate::engine::relation;
use crate::model::{
    Color, Edit, NumberFormat, PersonKind, Property, PropertyConfig, PropertyType, SelectOption,
    Value, new_id, truncate_chars,
};

/// A property's menu other than its column's: a step of the column's menu (a name typed, a
/// delete confirmed), or one of its submenus opened on its own ([`PropertyMenu`]).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Page {
    /// Its name, typed.
    Rename,
    /// The types it can change to.
    Type,
    /// A select's or multi-select's options.
    Options,
    /// An option's name, typed.
    RenameOption(String),
    /// A new option's name, typed.
    NewOption,
    /// A number's format and decimal places.
    Number,
    /// A relation's database, and whether it's two-way.
    Relation,
    /// The kinds of people a person property allows.
    People,
    /// Deleting it, confirmed.
    Delete,
}

impl Page {
    /// Whether it's a field to type in rather than a menu.
    pub(crate) fn is_field(&self) -> bool {
        matches!(self, Self::Rename | Self::RenameOption(_) | Self::NewOption)
    }
}

/// A part of a property's menu, which [`TableView::open_property_menu`] opens on its own,
/// hanging from the column's header: a step of the column's menu, or one of its submenus.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropertyMenu {
    /// Its name, in a field.
    Rename,
    /// The types it can change to (not for the title).
    Type,
    /// A select's or multi-select's options.
    Options,
    /// A number's format and decimal places.
    NumberFormat,
    /// A relation's database, and whether it's two-way.
    Relation,
    /// Who a person property allows.
    People,
    /// Deleting it, confirmed (not for the title).
    Delete,
}

impl From<PropertyMenu> for Page {
    fn from(m: PropertyMenu) -> Self {
        match m {
            PropertyMenu::Rename => Self::Rename,
            PropertyMenu::Type => Self::Type,
            PropertyMenu::Options => Self::Options,
            PropertyMenu::NumberFormat => Self::Number,
            PropertyMenu::Relation => Self::Relation,
            PropertyMenu::People => Self::People,
            PropertyMenu::Delete => Self::Delete,
        }
    }
}

/// Whether a page applies to a property of its type.
fn fits(p: &Property, page: &Page) -> bool {
    match page {
        Page::Type | Page::Delete => p.kind != PropertyType::Title,
        Page::Options | Page::RenameOption(_) | Page::NewOption => {
            matches!(p.kind, PropertyType::Select | PropertyType::MultiSelect)
        }
        Page::Number => p.kind == PropertyType::Number,
        Page::Relation => p.kind == PropertyType::Relation,
        Page::People => p.kind == PropertyType::Person,
        Page::Rename => true,
    }
}

/// What an entry of a property's menu does.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PropAction {
    Page(Page),
    /// From a step back to the column's menu (or to the + menu from a new relation).
    Back,
    /// The add-property menu.
    AddProperty,
    SetType(PropertyType),
    IncludeTime,
    Format(NumberFormat),
    Precision(Option<u8>),
    Currency(&'static str),
    Kind(PersonKind),
    Database(String),
    TwoWay,
    /// An option (by id) recoloured.
    Color(String, Color),
    DeleteOption(String),
    /// The property deleted (confirmed).
    Delete,
    /// The relation being added made.
    CreateRelation,
}

/// What a menu's field names.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum FieldFor {
    Rename(String),
    NewOption(String),
    RenameOption(String, String),
    /// A view's name.
    RenameView(String),
}

/// A menu's field: gpui-core's Input, what it names and its limit.
pub(crate) struct MenuField {
    pub state: Entity<InputState>,
    pub(crate) names: FieldFor,
    pub max_chars: usize,
    _events: Subscription,
}

/// A relation being added: its name (typed in the add-property menu, or its type's when none
/// was), its database and whether it's two-way.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NewRelation {
    pub name: String,
    /// What was typed, given back to the + menu's field on Back.
    pub typed: String,
    pub database_id: String,
    pub two_way: bool,
}

/// The reorder group of a property's options in its menus.
pub(crate) const OPTION_ORDER: &str = "options";

/// The currencies the number format offers (ISO 4217), those the cells have symbols for.
pub(crate) const CURRENCIES: &[&str] = &[
    "USD", "EUR", "GBP", "JPY", "CNY", "INR", "KRW", "CAD", "AUD", "CHF",
];

/// Decimal places, as the format page lists them: an example of each.
const PRECISIONS: &[(&str, Option<u8>)] = &[
    ("Default", None),
    ("1", Some(0)),
    ("1.0", Some(1)),
    ("1.00", Some(2)),
    ("1.000", Some(3)),
    ("1.0000", Some(4)),
];

/// An option colour's name, as the options' page lists it.
pub(crate) fn color_label(c: &Color) -> String {
    let s = c.as_str();
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn format_label(p: &Property) -> &'static str {
    match p.config.format {
        Some(NumberFormat::Percent) => "Percent",
        Some(NumberFormat::Currency) => "Currency",
        _ => "Number",
    }
}

/// A name as it's compared with others: trimmed, case aside.
fn folded(name: &str) -> String {
    name.trim().to_lowercase()
}

/// Whether a property other than `except` has this name.
fn name_taken(properties: &[Property], name: &str, except: Option<&str>) -> bool {
    let name = folded(name);
    properties
        .iter()
        .any(|p| Some(p.id.as_str()) != except && folded(&p.name) == name)
}

/// `base`, or `base 2`, `base 3`, ... whichever no property has.
fn unique_name(properties: &[Property], base: &str) -> String {
    if !name_taken(properties, base, None) {
        return base.to_owned();
    }
    (2..)
        .map(|n| format!("{base} {n}"))
        .find(|name| !name_taken(properties, name, None))
        .unwrap_or_default()
}

fn untitled(title: &str) -> String {
    if title.trim().is_empty() {
        "Untitled".into()
    } else {
        title.to_owned()
    }
}

fn full_note(title: &str, limit: usize) -> String {
    format!(
        "{} has {limit} properties, the most it can hold",
        untitled(title)
    )
}

impl TableView {
    /// Opens the + menu, as pressing the + after the last column does: a field for the new
    /// property's name over the types it can have (gpui-core's `Picker`, naming rather than
    /// filtering). Typing names it; Up and Down move through the types; Enter, or a press on a
    /// type, makes it.
    pub fn open_add_property_menu(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.open_add_property(Reopen::default(), cx);
    }

    /// Opens a part of a property's menu on its own, hanging from its column's header (or the +
    /// when it has no column shown): its name to type, its types, options, format, and so on,
    /// which the column's menu opens as submenus. A part that doesn't apply to the property's
    /// type (a number's options, the title's type) doesn't open.
    pub fn open_property_menu(
        &mut self,
        property_id: &str,
        menu: PropertyMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ensure();
        let page = Page::from(menu);
        let Some(p) = self
            .database
            .database
            .as_ref()
            .and_then(|f| f.property(property_id))
        else {
            return;
        };
        if !fits(p, &page) {
            return;
        }
        self.date = None;
        self.close_property_menu();
        self.open_page(property_id.to_owned(), page, window, cx);
    }

    /// The field of an open name step (a property's, an option's or a view's rename, a new
    /// option), while one is. (A + menu's name is typed in gpui-core's `Picker`, which keeps its
    /// own field.)
    pub fn menu_field_state(&self) -> Option<Entity<InputState>> {
        self.field.as_ref().map(|f| f.state.clone())
    }

    /// Opens the + menu, where `reopen` says (on Relation with its name as typed, back from a
    /// new relation's database).
    fn open_add_property(&mut self, reopen: Reopen, cx: &mut Context<Self>) {
        self.ensure();
        self.date = None;
        self.close_property_menu();
        self.menu_serial += 1;
        self.popover = Some(Popover::AddProperty);
        self.reopen = reopen;
        cx.notify();
    }

    /// Forgets a menu's field, note, new relation or view and where it reopens, as it closes.
    pub(crate) fn close_property_menu(&mut self) {
        self.field = None;
        self.note = None;
        self.new_relation = None;
        self.new_view = None;
        self.reopen = Reopen::default();
        self.move_highlight = None;
    }

    /// The property the open menu is about: the column's, or a property page's.
    pub(crate) fn menu_property(&self) -> Option<&Property> {
        let fields = self.database.database.as_ref()?;
        match &self.popover {
            Some(Popover::Header { property_id } | Popover::Property { property_id, .. }) => {
                fields.property(property_id)
            }
            _ => None,
        }
    }

    /// The databases a relation can point at: the host's, and this one.
    pub(crate) fn databases(&self) -> Vec<RelatedDatabase> {
        let mut out = self.host.databases();
        let own = RelatedDatabase {
            id: self.database.id.clone(),
            title: self.database.title.clone(),
            properties: self
                .database
                .database
                .as_ref()
                .map(|f| f.properties.clone())
                .unwrap_or_default(),
        };
        match out.iter_mut().find(|d| d.id == own.id) {
            Some(d) => *d = own,
            None => out.push(own),
        }
        out
    }

    fn database_info(&self, id: &str) -> Option<RelatedDatabase> {
        self.databases().into_iter().find(|d| d.id == id)
    }

    // --- Pages and fields --------------------------------------------------------------------

    fn open_page(
        &mut self,
        property_id: String,
        page: Page,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(p) = self
            .database
            .database
            .as_ref()
            .and_then(|f| f.property(&property_id))
            .cloned()
        else {
            return;
        };
        self.field = None;
        self.note = None;
        self.menu_serial += 1;
        let limits = self.host.limits();
        match &page {
            Page::Rename => self.open_field(
                FieldFor::Rename(p.id.clone()),
                p.name.clone(),
                limits.name_chars,
                "Name",
                window,
                cx,
            ),
            Page::NewOption => self.open_field(
                FieldFor::NewOption(p.id.clone()),
                String::new(),
                limits.option_name_chars,
                "Option name",
                window,
                cx,
            ),
            Page::RenameOption(oid) => {
                let name = p.option(oid).map(|o| o.name.clone()).unwrap_or_default();
                self.open_field(
                    FieldFor::RenameOption(p.id.clone(), oid.clone()),
                    name,
                    limits.option_name_chars,
                    "Option name",
                    window,
                    cx,
                )
            }
            _ => {}
        }
        // A menu's page takes the keyboard when it first draws (gpui-core's PopupMenu).
        if !page.is_field() {
            window.focus(&self.focus, cx);
        }
        self.popover = Some(Popover::Property { property_id, page });
        cx.notify();
    }

    pub(crate) fn open_field(
        &mut self,
        names: FieldFor,
        text: String,
        max: usize,
        placeholder: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(placeholder)
                .default_value(text)
                .max_chars(max)
        });
        let events = cx.subscribe_in(
            &state,
            window,
            |this: &mut Self, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => this.commit_field(window, cx),
                InputEvent::Change => {
                    this.note = None;
                    cx.notify();
                }
                InputEvent::Focus | InputEvent::Blur => {}
            },
        );
        // The name is selected, so typing replaces it.
        state.update(cx, |s, cx| {
            s.focus(window, cx);
            s.select_all(window, cx);
        });
        self.field = Some(MenuField {
            state,
            names,
            max_chars: max,
            _events: events,
        });
    }

    /// What's typed in the open menu's field, trimmed.
    fn typed(&self, cx: &App) -> String {
        self.field
            .as_ref()
            .map(|f| f.state.read(cx).value().trim().to_owned())
            .unwrap_or_default()
    }

    /// Enter in a menu's field: makes, or renames, what it names.
    pub(crate) fn commit_field(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(names) = self.field.as_ref().map(|f| f.names.clone()) else {
            return;
        };
        let name = self.typed(cx);
        match names {
            FieldFor::Rename(pid) => self.rename_property(&pid, &name, window, cx),
            FieldFor::NewOption(pid) => self.add_option(&pid, &name, window, cx),
            FieldFor::RenameOption(pid, oid) => self.rename_option(&pid, &oid, &name, window, cx),
            FieldFor::RenameView(id) => self.rename_view(&id, window, cx),
        }
    }

    /// Why the open menu can't do something now: a note it refused with, or a limit reached.
    pub(crate) fn page_note(&self, cx: &App) -> Option<String> {
        if matches!(
            self.popover,
            Some(Popover::AddView | Popover::NewBoard | Popover::View { .. })
        ) {
            return self.view_note(cx);
        }
        if let Some(n) = &self.note {
            return Some(n.clone());
        }
        let limits = self.host.limits();
        let fields = self.database.database.as_ref()?;
        let at_max = self
            .field
            .as_ref()
            .is_some_and(|f| f.state.read(cx).value().chars().count() >= f.max_chars);
        match &self.popover {
            Some(Popover::AddProperty) if fields.properties.len() >= limits.properties => {
                Some(full_note(&self.database.title, limits.properties))
            }
            // The limits its submenus are at, over the column's menu.
            Some(Popover::Header { .. }) => self.limit_note(self.menu_property()?),
            Some(Popover::Property { page, property_id }) => {
                let p = fields.property(property_id)?;
                match page {
                    Page::Options | Page::Relation => self.limit_note(p),
                    _ if at_max => Some(format!(
                        "Names are at most {} characters",
                        self.field.as_ref().map_or(0, |f| f.max_chars)
                    )),
                    _ => None,
                }
            }
            Some(Popover::AddRelation) => {
                let nr = self.new_relation.as_ref()?;
                let other = self.database_info(&nr.database_id)?;
                (!nr.two_way && other.properties.len() >= limits.properties)
                    .then(|| full_note(&other.title, limits.properties))
            }
            _ if at_max => Some(format!(
                "Names are at most {} characters",
                self.field.as_ref().map_or(0, |f| f.max_chars)
            )),
            _ => None,
        }
    }

    /// The limit a property's options or relation is at: its options all taken, or (one-way) the
    /// related database too full for its other side.
    fn limit_note(&self, p: &Property) -> Option<String> {
        let limits = self.host.limits();
        match p.kind {
            PropertyType::Select | PropertyType::MultiSelect
                if p.options().len() >= limits.options =>
            {
                Some(format!(
                    "A property holds at most {} options",
                    limits.options
                ))
            }
            PropertyType::Relation => {
                let t = p.relation()?;
                if t.two_way {
                    return None;
                }
                let other = self.database_info(t.database_id)?;
                (other.properties.len() >= limits.properties)
                    .then(|| full_note(&other.title, limits.properties))
            }
            _ => None,
        }
    }

    pub(crate) fn refuse(&mut self, note: String, cx: &mut Context<Self>) {
        self.note = Some(note);
        cx.notify();
    }

    // --- Entries -----------------------------------------------------------------------------

    /// The top of the column's menu, its Property section: rename, and the type and the type's
    /// settings as submenus.
    pub(crate) fn property_head(&self, p: &Property) -> Vec<Entry> {
        use PropAction as A;
        use gpui_core::IconName as I;
        let mut out = vec![Entry::heading("Property")];
        out.push(Entry::act("Rename", Action::Prop(A::Page(Page::Rename))).icon(I::Pencil));
        if p.kind != PropertyType::Title {
            out.push(
                Entry::sub("Change type", self.type_entries(p))
                    .icon(type_icon(&p.kind))
                    .hint(p.kind.label()),
            );
        }
        match p.kind {
            PropertyType::Select | PropertyType::MultiSelect => {
                out.push(Entry::sub("Edit options", self.options_entries(p)).icon(I::ListChecks))
            }
            PropertyType::Number => out.push(
                Entry::sub("Number format", self.number_entries(p))
                    .icon(I::Percent)
                    .hint(format_label(p)),
            ),
            PropertyType::Date => out.push(
                Entry::act("Include time", Action::Prop(A::IncludeTime))
                    .icon(I::Clock)
                    .checked(p.config.include_time == Some(true)),
            ),
            PropertyType::Relation => {
                let t = p.relation();
                let entries = self.relation_entries(
                    t.map_or("", |t| t.database_id),
                    t.is_some_and(|t| t.two_way),
                );
                out.push(Entry::sub("Relation", entries).icon(I::ArrowLeftRight))
            }
            PropertyType::Person => {
                out.push(Entry::sub("People", self.people_entries(p)).icon(I::Users))
            }
            _ => {}
        }
        out
    }

    /// The end of the column's menu: add a property, and delete this one.
    pub(crate) fn property_tail(&self, p: &Property) -> Vec<Entry> {
        use gpui_core::IconName as I;
        let mut out =
            vec![Entry::act("Add a property", Action::Prop(PropAction::AddProperty)).icon(I::Plus)];
        if p.kind != PropertyType::Title {
            out.push(
                Entry::act(
                    "Delete property",
                    Action::Prop(PropAction::Page(Page::Delete)),
                )
                .icon(I::Trash),
            );
        }
        out
    }

    /// The entries of a property's menu opened on its own (or of a step of the column's), or
    /// of the new relation's.
    pub(crate) fn property_entries(&self) -> Vec<Entry> {
        use PropAction as A;
        use gpui_core::IconName as I;
        let act = |label: &str, a: A| Entry::act(label, Action::Prop(a));
        if let Some(Popover::AddRelation) = &self.popover {
            let Some(nr) = &self.new_relation else {
                return Vec::new();
            };
            let mut out = self.relation_entries(&nr.database_id, nr.two_way);
            out.push(act("Add relation", A::CreateRelation).icon(I::Plus));
            out.push(act("Back", A::Back).icon(I::ArrowLeft));
            return out;
        }
        let Some(Popover::Property { page, .. }) = &self.popover else {
            return Vec::new();
        };
        let Some(p) = self.menu_property() else {
            return Vec::new();
        };
        match page {
            Page::Type => self.type_entries(p),
            Page::Options => self.options_entries(p),
            Page::Number => self.number_entries(p),
            Page::Relation => {
                let t = p.relation();
                self.relation_entries(
                    t.map_or("", |t| t.database_id),
                    t.is_some_and(|t| t.two_way),
                )
            }
            Page::People => self.people_entries(p),
            Page::Delete => {
                let mut out = vec![Entry::heading(format!("Delete {}?", p.name))];
                if let Some(t) = p.relation()
                    && t.two_way
                    && t.back_property_id.is_some()
                {
                    let other = self
                        .database_info(t.database_id)
                        .map_or_else(|| "Untitled".into(), |d| untitled(&d.title));
                    out.push(Entry::heading(format!(
                        "Its other side, in {other}, goes too"
                    )));
                }
                out.push(act("Delete property", A::Delete).icon(I::Trash));
                out.push(act("Cancel", A::Back).icon(I::X).highlighted(true));
                out
            }
            Page::Rename | Page::RenameOption(_) | Page::NewOption => Vec::new(),
        }
    }

    /// The types a property can change to, its own checked.
    fn type_entries(&self, p: &Property) -> Vec<Entry> {
        let mut out = vec![Entry::heading("Type")];
        for k in PropertyType::ADDABLE {
            let on = *k == p.kind;
            out.push(
                Entry::act(k.label(), Action::Prop(PropAction::SetType(k.clone())))
                    .icon(type_icon(k))
                    .checked(on)
                    .highlighted(on),
            );
        }
        out
    }

    /// A select's options as their pills, each opening its own submenu and dragged (or moved
    /// with Alt-Up and Alt-Down) to put them in another order, and Add an option.
    fn options_entries(&self, p: &Property) -> Vec<Entry> {
        use gpui_core::IconName as I;
        let mut out = vec![Entry::heading("Options")];
        for o in p.options() {
            out.push(
                Entry::sub(o.name.clone(), self.option_entries(p, &o.id))
                    .chip(o.color.clone())
                    .reorder(OPTION_ORDER),
            );
        }
        out.push(
            Entry::act(
                "Add an option",
                Action::Prop(PropAction::Page(Page::NewOption)),
            )
            .icon(I::Plus)
            .disabled(p.options().len() >= self.host.limits().options),
        );
        out
    }

    /// An option's submenu: rename it, recolour it, delete it. (It's moved by dragging its entry
    /// in the options, or with Alt-Up and Alt-Down: [`TableView::reorder_option`].)
    fn option_entries(&self, p: &Property, oid: &str) -> Vec<Entry> {
        use PropAction as A;
        use gpui_core::IconName as I;
        let act = |label: &str, a: A| Entry::act(label, Action::Prop(a));
        let Some(i) = p.options().iter().position(|o| o.id == oid) else {
            return Vec::new();
        };
        let o = &p.options()[i];
        let mut out = vec![
            act("Rename option", A::Page(Page::RenameOption(oid.to_owned()))).icon(I::Pencil),
            Entry::heading("Colour"),
        ];
        for c in Color::ALL {
            out.push(
                act(&color_label(c), A::Color(oid.to_owned(), c.clone()))
                    .chip(c.clone())
                    .checked(o.color == *c),
            );
        }
        out.push(Entry::heading("Option"));
        out.push(act("Delete option", A::DeleteOption(oid.to_owned())).icon(I::Trash));
        out
    }

    /// A number's format, its currency (a submenu of its own), and its decimal places.
    fn number_entries(&self, p: &Property) -> Vec<Entry> {
        use PropAction as A;
        use gpui_core::IconName as I;
        let act = |label: &str, a: A| Entry::act(label, Action::Prop(a));
        let format = p.config.format.clone().unwrap_or(NumberFormat::Plain);
        let mut out = vec![Entry::heading("Format")];
        for (label, f) in [
            ("Number", NumberFormat::Plain),
            ("Percent", NumberFormat::Percent),
            ("Currency", NumberFormat::Currency),
        ] {
            let on = format == f;
            out.push(act(label, A::Format(f)).checked(on).highlighted(on));
        }
        let current = p.config.currency.as_deref().unwrap_or("USD");
        if format == NumberFormat::Currency {
            let mut codes = vec![Entry::heading("Currency")];
            for c in CURRENCIES {
                let on = current == *c;
                codes.push(act(c, A::Currency(c)).checked(on).highlighted(on));
            }
            out.push(
                Entry::sub("Currency code", codes)
                    .icon(I::DollarSign)
                    .hint(current),
            );
        }
        out.push(Entry::heading("Decimal places"));
        for (label, n) in PRECISIONS {
            out.push(act(label, A::Precision(*n)).checked(p.config.precision == *n));
        }
        out
    }

    /// The kinds of people a person property allows.
    fn people_entries(&self, p: &Property) -> Vec<Entry> {
        use PropAction as A;
        use gpui_core::IconName as I;
        let kinds = p.person_kinds();
        vec![
            Entry::heading("Allows"),
            Entry::act("Owner", Action::Prop(A::Kind(PersonKind::Owner)))
                .icon(I::User)
                .checked(kinds.contains(&PersonKind::Owner)),
            Entry::act("Agents", Action::Prop(A::Kind(PersonKind::Agent)))
                .icon(I::Users)
                .checked(kinds.contains(&PersonKind::Agent)),
        ]
    }

    /// A relation's database choices and its two-way toggle.
    fn relation_entries(&self, database_id: &str, two_way: bool) -> Vec<Entry> {
        use PropAction as A;
        use gpui_core::IconName as I;
        let limits = self.host.limits();
        let dbs = self.databases();
        let mut out = vec![Entry::heading("Related to")];
        for d in &dbs {
            let on = d.id == database_id;
            out.push(
                Entry::act(untitled(&d.title), Action::Prop(A::Database(d.id.clone())))
                    .icon(I::Table)
                    .checked(on)
                    .highlighted(on),
            );
        }
        if let Some(other) = dbs.iter().find(|d| d.id == database_id) {
            out.push(Entry::heading("Two-way"));
            let full = !two_way && other.properties.len() >= limits.properties;
            out.push(
                Entry::act(
                    format!("Show on {}", untitled(&other.title)),
                    Action::Prop(A::TwoWay),
                )
                .icon(I::ArrowLeftRight)
                .checked(two_way)
                .disabled(full),
            );
        }
        out
    }

    // --- Doing -------------------------------------------------------------------------------

    /// Does what a property menu's entry says.
    pub(crate) fn activate_property(
        &mut self,
        action: PropAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pid = self.menu_property().map(|p| p.id.clone());
        let p = self.menu_property().cloned();
        match action {
            PropAction::Page(page) => {
                let Some(p) = p else { return };
                if !fits(&p, &page) {
                    if p.kind == PropertyType::Title {
                        self.refuse("The title can't change type or be deleted".into(), cx);
                    }
                    return;
                }
                if page == Page::NewOption && p.options().len() >= self.host.limits().options {
                    return;
                }
                self.open_page(p.id, page, window, cx);
            }
            PropAction::Back => self.back(window, cx),
            PropAction::AddProperty => self.open_add_property_menu(window, cx),
            PropAction::SetType(kind) => {
                if let Some(p) = p {
                    self.set_type(&p, kind, window, cx)
                }
            }
            PropAction::IncludeTime => {
                if let Some(mut p) = p.filter(|p| p.kind == PropertyType::Date) {
                    p.config.include_time = Some(p.config.include_time != Some(true));
                    self.change(vec![self.changed(p)], window, cx);
                }
            }
            PropAction::Format(f) => {
                if let Some(mut p) = p.filter(|p| p.kind == PropertyType::Number) {
                    p.config.currency = match f {
                        NumberFormat::Currency => {
                            Some(p.config.currency.clone().unwrap_or_else(|| "USD".into()))
                        }
                        _ => None,
                    };
                    p.config.format = Some(f);
                    self.change(vec![self.changed(p)], window, cx);
                }
            }
            PropAction::Precision(n) => {
                if let Some(mut p) = p.filter(|p| p.kind == PropertyType::Number) {
                    p.config.precision = n;
                    self.change(vec![self.changed(p)], window, cx);
                }
            }
            PropAction::Currency(c) => {
                if let Some(mut p) = p.filter(|p| p.kind == PropertyType::Number) {
                    p.config.currency = Some(c.into());
                    self.change(vec![self.changed(p)], window, cx);
                }
            }
            PropAction::Kind(k) => {
                if let Some(mut p) = p.filter(|p| p.kind == PropertyType::Person) {
                    let mut kinds = p.person_kinds().to_vec();
                    match kinds.iter().position(|x| *x == k) {
                        Some(i) => {
                            kinds.remove(i);
                        }
                        None => kinds.push(k),
                    }
                    if kinds.is_empty() {
                        self.refuse(
                            "A person property allows the owner, agents or both".into(),
                            cx,
                        );
                        return;
                    }
                    // In the order the page lists them.
                    kinds.sort_by_key(|k| *k != PersonKind::Owner);
                    p.config.kinds = Some(kinds);
                    self.change(vec![self.changed(p)], window, cx);
                }
            }
            PropAction::Database(id) => {
                if let Some(nr) = &mut self.new_relation {
                    if nr.database_id != id {
                        nr.database_id = id;
                        nr.two_way = false;
                    }
                    self.note = None;
                    cx.notify();
                } else if let Some(p) = p.filter(|p| p.kind == PropertyType::Relation) {
                    self.set_relation_database(p, id, window, cx);
                }
            }
            PropAction::TwoWay => {
                if self.new_relation.is_some() {
                    self.toggle_new_two_way(cx);
                } else if let Some(p) = p.filter(|p| p.kind == PropertyType::Relation) {
                    self.toggle_two_way(p, window, cx);
                }
            }
            PropAction::Color(oid, c) => {
                self.edit_option(pid, &oid, |options, i| options[i].color = c, window, cx)
            }
            PropAction::DeleteOption(oid) => self.edit_option(
                pid,
                &oid,
                |options, i| {
                    options.remove(i);
                },
                window,
                cx,
            ),
            PropAction::Delete => {
                if let Some(p) = p {
                    self.delete_property(&p, window, cx)
                }
            }
            PropAction::CreateRelation => self.create_relation(window, cx),
        }
    }

    /// Back from a step to the column's menu, on the entry the step came from: an option's
    /// in the submenus it was in, open again (or to the + menu, on Relation with the name as
    /// typed, from a new relation's database). A part of the menu opened on its own closes.
    fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(nr) = self.new_relation.take() {
            let reopen = Reopen {
                name: Some(nr.typed),
                ..Reopen::on(PropertyType::Relation.label())
            };
            self.open_add_property(reopen, cx);
            return;
        }
        let Some(Popover::Property { property_id, page }) = self.popover.clone() else {
            self.close_menu(window, cx);
            return;
        };
        let options = || "Edit options".to_owned();
        let from = match page {
            Page::Rename => Reopen::on("Rename"),
            Page::Delete => Reopen::on("Delete property"),
            Page::NewOption => Reopen::at(vec![options()], Some("Add an option".into())),
            Page::RenameOption(oid) => {
                // Its name now, which its entry has.
                let name = self
                    .database
                    .database
                    .as_ref()
                    .and_then(|f| f.property(&property_id))
                    .and_then(|p| p.option(&oid))
                    .map(|o| o.name.clone());
                match name {
                    Some(name) => Reopen::at(vec![options(), name], Some("Rename option".into())),
                    None => Reopen::at(vec![options()], None),
                }
            }
            _ => {
                self.close_menu(window, cx);
                return;
            }
        };
        self.close_property_menu();
        match self
            .shown
            .columns
            .iter()
            .position(|c| c.property_id == property_id)
        {
            Some(column) => {
                window.focus(&self.focus, cx);
                self.open_header_menu(column, cx);
                self.reopen = from;
            }
            None => self.close_menu(window, cx),
        }
    }

    fn close_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.popover = None;
        self.close_property_menu();
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// A `propertyChanged` for this database.
    fn changed(&self, property: Property) -> Edit {
        Edit::PropertyChanged {
            database_id: self.database.id.clone(),
            property,
        }
    }

    /// Shows edits at once and hands them to the host, the menu staying open.
    fn change(&mut self, edits: Vec<Edit>, window: &mut Window, cx: &mut Context<Self>) {
        self.note = None;
        self.apply_local(&edits);
        self.emit_edits(edits, window, cx);
        self.dirty = true;
        cx.notify();
    }

    /// Shows edits at once, hands them to the host and closes the menu.
    fn change_and_close(&mut self, edits: Vec<Edit>, window: &mut Window, cx: &mut Context<Self>) {
        self.change(edits, window, cx);
        self.close_menu(window, cx);
    }

    /// The edits to this database's properties and views, applied to the table's copy (as a
    /// view's change is) until the host hands it new docs.
    fn apply_local(&mut self, edits: &[Edit]) {
        let id = self.database.id.clone();
        let Some(fields) = self.database.database.as_mut() else {
            return;
        };
        for e in edits {
            match e {
                Edit::PropertyCreated {
                    database_id,
                    property,
                }
                | Edit::PropertyChanged {
                    database_id,
                    property,
                } if *database_id == id => {
                    match fields.properties.iter_mut().find(|p| p.id == property.id) {
                        Some(p) => *p = property.clone(),
                        None => fields.properties.push(property.clone()),
                    }
                }
                Edit::PropertyDeleted {
                    database_id,
                    property_id,
                } if *database_id == id => fields.properties.retain(|p| p.id != *property_id),
                Edit::ViewChanged { database_id, view } if *database_id == id => {
                    match fields.views.iter_mut().find(|v| v.id == view.id) {
                        Some(v) => *v = view.clone(),
                        None => fields.views.push(view.clone()),
                    }
                    self.view_id = Some(view.id.clone());
                }
                _ => {}
            }
        }
    }

    /// A new property named `typed` (its type's name when it's empty), of `kind`, after the
    /// others; a relation asks for its database first.
    pub(crate) fn add_property(
        &mut self,
        kind: PropertyType,
        typed: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(fields) = self.database.database.clone() else {
            return;
        };
        let limits = self.host.limits();
        if fields.properties.len() >= limits.properties {
            self.refuse(full_note(&self.database.title, limits.properties), cx);
            return;
        }
        let typed = truncate_chars(typed.trim(), limits.name_chars).to_owned();
        if !typed.is_empty() && name_taken(&fields.properties, &typed, None) {
            self.refuse(format!("There's already a property named {typed}"), cx);
            return;
        }
        let name = if typed.is_empty() {
            unique_name(&fields.properties, kind.label())
        } else {
            typed.clone()
        };
        if kind == PropertyType::Relation {
            // Its database (another one first, if there is one) and whether it's two-way.
            let own = self.database.id.clone();
            let dbs = self.databases();
            let database_id = dbs
                .iter()
                .find(|d| d.id != own)
                .or(dbs.first())
                .map_or(own, |d| d.id.clone());
            self.field = None;
            self.note = None;
            self.new_relation = Some(NewRelation {
                name,
                typed,
                database_id,
                two_way: false,
            });
            self.menu_serial += 1;
            self.popover = Some(Popover::AddRelation);
            window.focus(&self.focus, cx);
            cx.notify();
            return;
        }
        let order = ops::append_order(fields.properties.iter().map(|p| p.order.as_str()));
        let property = Property::new(new_id(), name, kind.clone(), order)
            .with_config(PropertyConfig::default_for(&kind));
        self.create(property, None, window, cx);
    }

    /// Emits a new property (and a two-way relation's other side) with the `viewChanged` that
    /// adds its column to the view shown, in one batch, and closes the menu.
    fn create(
        &mut self,
        property: Property,
        back: Option<(String, Property)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = self.database.id.clone();
        let mut edits = vec![Edit::PropertyCreated {
            database_id: id.clone(),
            property: property.clone(),
        }];
        let mut fields = self.database.database.clone().unwrap_or_default();
        fields.properties.push(property.clone());
        if let Some((other, back)) = back {
            if other == id {
                fields.properties.push(back.clone());
            }
            edits.push(Edit::PropertyCreated {
                database_id: other,
                property: back,
            });
        }
        if let Some(view) = self.shown.view.clone() {
            edits.push(Edit::ViewChanged {
                database_id: id,
                view: ops::add_column(&fields, &view, &property.id),
            });
        }
        self.pending_column = Some(property.id);
        self.change_and_close(edits, window, cx);
    }

    /// Makes the relation being added: one-way, or two-way with its other side in the other
    /// database (this one, for a relation to itself).
    fn create_relation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(nr) = self.new_relation.clone() else {
            return;
        };
        let Some(fields) = self.database.database.clone() else {
            return;
        };
        let limits = self.host.limits();
        let own = self.database.id.clone();
        let same = nr.database_id == own;
        let need = 1 + usize::from(nr.two_way && same);
        if fields.properties.len() + need > limits.properties {
            self.refuse(full_note(&self.database.title, limits.properties), cx);
            return;
        }
        let order = ops::append_order(fields.properties.iter().map(|p| p.order.as_str()));
        if !nr.two_way {
            let property = Property::new(new_id(), nr.name, PropertyType::Relation, order)
                .with_config(PropertyConfig {
                    database_id: Some(nr.database_id),
                    two_way: Some(false),
                    ..PropertyConfig::default()
                });
            self.create(property, None, window, cx);
            return;
        }
        let Some(other) = self.database_info(&nr.database_id) else {
            return;
        };
        if !same && other.properties.len() >= limits.properties {
            self.refuse(full_note(&other.title, limits.properties), cx);
            return;
        }
        // The other side is named for this database, unlike the other's properties (and the new
        // one's, in the same database), and goes after them.
        let mut theirs = other.properties.clone();
        let back_order = if same {
            crate::order::between(Some(&order), None).unwrap_or_else(|_| order.clone())
        } else {
            ops::append_order(theirs.iter().map(|p| p.order.as_str()))
        };
        if same {
            theirs.push(Property::new(
                "",
                nr.name.clone(),
                PropertyType::Relation,
                "",
            ));
        }
        let back_name = unique_name(&theirs, &untitled(&self.database.title));
        let (a, b) =
            relation::two_way_pair(&own, &nr.name, &order, &other.id, &back_name, &back_order);
        self.create(a, Some((other.id, b)), window, cx);
    }

    fn toggle_new_two_way(&mut self, cx: &mut Context<Self>) {
        let limits = self.host.limits();
        let Some(nr) = self.new_relation.clone() else {
            return;
        };
        if !nr.two_way
            && let Some(other) = self.database_info(&nr.database_id)
            && other.id != self.database.id
            && other.properties.len() >= limits.properties
        {
            self.refuse(full_note(&other.title, limits.properties), cx);
            return;
        }
        if let Some(nr) = &mut self.new_relation {
            nr.two_way = !nr.two_way;
        }
        cx.notify();
    }

    fn rename_property(
        &mut self,
        pid: &str,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(fields) = self.database.database.as_ref() else {
            return;
        };
        let Some(mut p) = fields.property(pid).cloned() else {
            return;
        };
        let name = truncate_chars(name, self.host.limits().name_chars).to_owned();
        if name.is_empty() {
            self.refuse("A property needs a name".into(), cx);
            return;
        }
        if name == p.name {
            self.close_menu(window, cx);
            return;
        }
        if name_taken(&fields.properties, &name, Some(pid)) {
            self.refuse(format!("There's already a property named {name}"), cx);
            return;
        }
        p.name = name;
        self.change_and_close(vec![self.changed(p)], window, cx);
    }

    /// The edits for a two-way relation `p` losing its other side: the other database's
    /// property deleted.
    fn back_deleted(p: &Property) -> Option<Edit> {
        let t = p.relation()?;
        if !t.two_way {
            return None;
        }
        let back = t.back_property_id?;
        (back != p.id).then(|| Edit::PropertyDeleted {
            database_id: t.database_id.to_owned(),
            property_id: back.to_owned(),
        })
    }

    fn set_type(
        &mut self,
        p: &Property,
        kind: PropertyType,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if p.kind == PropertyType::Title {
            self.refuse("The title can't change type or be deleted".into(), cx);
            return;
        }
        if kind == p.kind || kind == PropertyType::Title {
            self.back(window, cx);
            return;
        }
        let mut new = p.retyped(kind.clone());
        if kind == PropertyType::Relation {
            // To this database until another is picked, one-way.
            new.config.database_id = Some(self.database.id.clone());
            new.config.two_way = Some(false);
        }
        let mut edits = vec![self.changed(new)];
        edits.extend(Self::back_deleted(p));
        if kind == PropertyType::Relation {
            self.change(edits, window, cx);
            self.open_page(p.id.clone(), Page::Relation, window, cx);
        } else {
            self.change_and_close(edits, window, cx);
        }
    }

    fn set_relation_database(
        &mut self,
        p: Property,
        database_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if p.relation().map(|t| t.database_id) == Some(database_id.as_str()) {
            return;
        }
        let mut new = p.clone();
        new.config.database_id = Some(database_id);
        new.config.two_way = Some(false);
        new.config.back_property_id = None;
        let mut edits = vec![self.changed(new)];
        edits.extend(Self::back_deleted(&p));
        self.change(edits, window, cx);
    }

    /// Makes a relation two-way (its other side made in the related database, and the rows it
    /// already lists written there) or one-way (the other side deleted).
    fn toggle_two_way(&mut self, p: Property, window: &mut Window, cx: &mut Context<Self>) {
        let Some(t) = p.relation() else {
            self.refuse("Pick the database it relates to first".into(), cx);
            return;
        };
        let mut new = p.clone();
        if t.two_way {
            new.config.two_way = Some(false);
            new.config.back_property_id = None;
            let mut edits = vec![self.changed(new)];
            edits.extend(Self::back_deleted(&p));
            self.change(edits, window, cx);
            return;
        }
        let limits = self.host.limits();
        let Some(other) = self.database_info(t.database_id) else {
            return;
        };
        if other.properties.len() >= limits.properties {
            self.refuse(full_note(&other.title, limits.properties), cx);
            return;
        }
        let own = self.database.id.clone();
        let order = ops::append_order(other.properties.iter().map(|p| p.order.as_str()));
        let name = unique_name(&other.properties, &untitled(&self.database.title));
        let back = Property::new(new_id(), name, PropertyType::Relation, order).with_config(
            PropertyConfig {
                database_id: Some(own.clone()),
                two_way: Some(true),
                back_property_id: Some(p.id.clone()),
                ..PropertyConfig::default()
            },
        );
        new.config.two_way = Some(true);
        new.config.back_property_id = Some(back.id.clone());
        let mut edits = vec![
            self.changed(new),
            Edit::PropertyCreated {
                database_id: other.id.clone(),
                property: back.clone(),
            },
        ];
        // The rows already related, written on the other side as any change of a two-way
        // relation is (the host's `relation::back_edits`).
        for r in self.rows.iter().filter(|r| !r.is_deleted()) {
            let Some(ids) = r.value(&p.id).and_then(Value::as_ids) else {
                continue;
            };
            let (added, _) = relation::diff(&[], ids);
            if added.is_empty() {
                continue;
            }
            edits.push(Edit::RelationChanged {
                row_id: r.id.clone(),
                property_id: p.id.clone(),
                database_id: other.id.clone(),
                back_property_id: back.id.clone(),
                added,
                removed: Vec::new(),
            });
        }
        self.change(edits, window, cx);
    }

    fn delete_property(&mut self, p: &Property, window: &mut Window, cx: &mut Context<Self>) {
        if p.kind == PropertyType::Title {
            self.refuse("The title can't change type or be deleted".into(), cx);
            return;
        }
        let mut edits = vec![Edit::PropertyDeleted {
            database_id: self.database.id.clone(),
            property_id: p.id.clone(),
        }];
        edits.extend(Self::back_deleted(p));
        self.change_and_close(edits, window, cx);
    }

    // --- Options -----------------------------------------------------------------------------

    /// Changes a property's option (by its place in the options) and emits the property.
    fn edit_option(
        &mut self,
        pid: Option<String>,
        oid: &str,
        f: impl FnOnce(&mut Vec<SelectOption>, usize),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(mut p) = pid.and_then(|pid| {
            self.database
                .database
                .as_ref()
                .and_then(|f| f.property(&pid))
                .cloned()
        }) else {
            return;
        };
        let options = p.config.options.get_or_insert_with(Vec::new);
        let Some(i) = options.iter().position(|o| o.id == oid) else {
            return;
        };
        let before = options.clone();
        f(options, i);
        if *options != before {
            self.change(vec![self.changed(p)], window, cx);
        }
    }

    /// An option dragged to another place among the options (or moved with Alt-Up or
    /// Alt-Down): a `propertyChanged` with the options in their new order, the menu open.
    pub(crate) fn reorder_option(
        &mut self,
        from: usize,
        to: usize,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(p) = self.menu_property() else {
            return;
        };
        // The option the entry was drawn for: at `from`, named as the entry is.
        let Some(oid) = p
            .options()
            .get(from)
            .filter(|o| o.name == name)
            .map(|o| o.id.clone())
        else {
            return;
        };
        let pid = Some(p.id.clone());
        self.edit_option(
            pid,
            &oid,
            |options, i| {
                let o = options.remove(i);
                options.insert(to.min(options.len()), o);
            },
            window,
            cx,
        );
    }

    fn add_option(&mut self, pid: &str, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        let limits = self.host.limits();
        let Some(mut p) = self
            .database
            .database
            .as_ref()
            .and_then(|f| f.property(pid))
            .cloned()
        else {
            return;
        };
        if p.options().len() >= limits.options {
            self.refuse(
                format!("A property holds at most {} options", limits.options),
                cx,
            );
            return;
        }
        let name = truncate_chars(name, limits.option_name_chars).to_owned();
        if name.is_empty() {
            self.refuse("An option needs a name".into(), cx);
            return;
        }
        if p.options().iter().any(|o| folded(&o.name) == folded(&name)) {
            self.refuse(format!("There's already an option named {name}"), cx);
            return;
        }
        p.config
            .options
            .get_or_insert_with(Vec::new)
            .push(SelectOption {
                id: new_id(),
                name,
                color: Color::Default,
            });
        self.change(vec![self.changed(p)], window, cx);
        self.back(window, cx);
    }

    fn rename_option(
        &mut self,
        pid: &str,
        oid: &str,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let limits = self.host.limits();
        let Some(mut p) = self
            .database
            .database
            .as_ref()
            .and_then(|f| f.property(pid))
            .cloned()
        else {
            return;
        };
        let name = truncate_chars(name, limits.option_name_chars).to_owned();
        if name.is_empty() {
            self.refuse("An option needs a name".into(), cx);
            return;
        }
        if p.options()
            .iter()
            .any(|o| o.id != oid && folded(&o.name) == folded(&name))
        {
            self.refuse(format!("There's already an option named {name}"), cx);
            return;
        }
        let options = p.config.options.get_or_insert_with(Vec::new);
        let Some(o) = options.iter_mut().find(|o| o.id == oid) else {
            return;
        };
        if o.name != name {
            o.name = name;
            self.change(vec![self.changed(p)], window, cx);
        }
        self.back(window, cx);
    }
}
