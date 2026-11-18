//! The table view's state: the docs, the active view, what it shows (worked out again only when
//! the docs or the view change), the focused cell, and what the controls do.

use std::cell::Cell as StdCell;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    App, AppContext as _, Autocapitalize, Bounds, Context, Entity, FocusHandle, Focusable, Pixels,
    ScrollStrategy, Subscription, TextInputAction, TextInputConfiguration, UniformListScrollHandle,
    Window, point, px, size,
};
use gpui_core::{DatePickerEvent, DatePickerState, InputEvent, InputState};

use super::host::{HostLookup, TableHost, TextEdit, TextReply, TextTarget, person_label};
use super::ops::{self, ColumnLayout};
use super::{TableTheme, format};
use crate::engine::{self, Context as EngineContext, Group, GroupKey, Issue, relation, time};
use crate::model::{
    Color, Condition, Direction, Doc, Edit, FilterOp, Person, Property, PropertyType, SelectOption,
    TableEvent, Value, View, ViewType, new_id,
};

/// One line of the table: a group's header, a row, the New row at the end of a group (or of the
/// table, when it isn't grouped), or a table's calculation footer under it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    /// A group's header, by its place in [`TableView::groups`].
    Group(usize),
    /// A row: its place in the rows given, and the group it's drawn in.
    Row { row: usize, group: Option<usize> },
    /// The New row of a group, or of the table.
    New(Option<usize>),
    /// The calculation footer of a group, or of the table, after its New row: a table's
    /// alone (boards and lists have none).
    Calc(Option<usize>),
}

/// The focused cell: a line of the table and a column (columns don't matter on a group's header
/// or a New row).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellRef {
    pub item: usize,
    pub column: usize,
}

/// What a menu entry does.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Action {
    Sort(Direction),
    Unsort,
    Filter(Condition),
    FilterText(FilterOp),
    ClearFilter,
    Hide,
    MoveBy(isize),
    ToggleColumn(String),
    SetSelect(Option<String>),
    ToggleOption(String),
    TogglePerson(Person),
    ToggleRelation(String),
    ClearValue,
    /// Something done to a property: its pages, and the edits they make.
    Prop(super::properties::PropAction),
    /// Something done to a view: its pages, and the edits they make.
    View(super::views::ViewAction),
    /// Something done to rows from their menu.
    Row(super::rows::RowAction),
    /// A footer's calculation picked for its column (None takes it off).
    Calc(Option<crate::model::Calculation>),
}

/// One entry of a menu or picker.
#[derive(Clone, Debug)]
pub(crate) struct Entry {
    pub label: String,
    /// A section's heading when None: drawn as gpui-core's menu heading, which does nothing and
    /// which the keyboard skips.
    pub action: Option<Action>,
    pub icon: Option<gpui_core::IconName>,
    /// Whether it's a toggle or one option of several, and checked.
    pub checked: Option<bool>,
    /// An option's colour: the entry is drawn as the option's pill, as in a cell.
    pub chip: Option<crate::model::Color>,
    /// Muted text at the entry's end: the current type, number format.
    pub hint: Option<String>,
    /// Shown and skipped (an action past a limit).
    pub disabled: bool,
    /// Where the menu opens, rather than its first entry.
    pub highlight: bool,
    /// The entries it opens beside it (gpui-core's submenu), rather than being picked.
    pub submenu: Option<Vec<Entry>>,
    /// The reorder group it's in (gpui-core's): dragged, or moved with Alt-Up and Alt-Down,
    /// among its neighbours of the same group ([`TableView::reorder_entry`]).
    pub reorder: Option<&'static str>,
}

impl Entry {
    pub(crate) fn heading(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            action: None,
            icon: None,
            checked: None,
            chip: None,
            hint: None,
            disabled: false,
            highlight: false,
            submenu: None,
            reorder: None,
        }
    }

    /// An entry that opens `entries` as a submenu beside it.
    pub(crate) fn sub(label: impl Into<String>, entries: Vec<Entry>) -> Self {
        Self {
            submenu: Some(entries),
            ..Self::heading(label)
        }
    }

    pub(crate) fn act(label: impl Into<String>, action: Action) -> Self {
        Self {
            action: Some(action),
            ..Self::heading(label)
        }
    }

    pub(crate) fn icon(mut self, icon: gpui_core::IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    pub(crate) fn checked(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }

    pub(crate) fn chip(mut self, color: crate::model::Color) -> Self {
        self.chip = Some(color);
        self
    }

    pub(crate) fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub(crate) fn highlighted(mut self, on: bool) -> Self {
        self.highlight = on;
        self
    }

    pub(crate) fn reorder(mut self, group: &'static str) -> Self {
        self.reorder = Some(group);
        self
    }
}

/// Where a menu opens, rather than on its first entry (or the one it highlights): the submenus
/// open on the way (gpui-core's `open_submenu`, by their entries' ids), the entry highlighted in
/// the last of them, and for a + menu the name typed before (gpui-core's `Picker::query`).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Reopen {
    /// The entries whose submenus open, from the menu's own down.
    pub path: Vec<String>,
    /// The entry highlighted in the last menu of `path` (in the menu itself when it's empty).
    pub on: Option<String>,
    /// A + menu's name, as typed before.
    pub name: Option<String>,
}

impl Reopen {
    /// On `on` in the menu itself.
    pub(crate) fn on(on: impl Into<String>) -> Self {
        Self {
            on: Some(on.into()),
            ..Self::default()
        }
    }

    /// On `on` in the submenu reached through `path`.
    pub(crate) fn at(path: Vec<String>, on: Option<String>) -> Self {
        Self {
            path,
            on,
            name: None,
        }
    }
}

/// A menu or picker open over the table, in gpui-core's `Popover`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Popover {
    /// A column's menu: sort, filter, hide, move. By its property, as the columns' places
    /// change while it's open (another column deleted or hidden from it).
    Header { property_id: String },
    /// The list of every property, to show and hide columns.
    Columns,
    /// A cell's picker: options, people or related rows.
    Picker { cell: CellRef },
    /// A cell's month, in gpui-core's `MonthGrid` ([`TableView::date`] holds its state).
    Date { cell: CellRef },
    /// A column's filter value, typed in place: the field under its header (its property's).
    FilterValue { property_id: String },
    /// The + after the last column: a new property's name and type.
    AddProperty,
    /// A new relation's database and whether it's two-way, after its name
    /// ([`TableView::new_relation`] holds them).
    AddRelation,
    /// A page of a property's menu past the column's own: its type, options, format, ...
    Property {
        property_id: String,
        page: super::properties::Page,
    },
    /// The bar's +: a new view's name and type.
    AddView,
    /// A new board's group, asked for when the database has no select.
    NewBoard,
    /// A page of a view's menu: rename, type, settings, delete, ...
    View {
        view_id: String,
        page: super::views::ViewPage,
        anchor: super::views::ViewAnchor,
    },
    /// The menu of rows (the selected ones, or the one pressed): open, duplicate, move,
    /// delete. It hangs from the first row's handle, or from the point a right-click pressed.
    Row {
        rows: Vec<String>,
        at: Option<gpui::Point<Pixels>>,
        /// Asking whether to delete them (more than one row).
        confirm: bool,
    },
    /// A column's calculation in a footer: the table's (`group` None) or a group's, by its
    /// key.
    Calc {
        property_id: String,
        group: Option<String>,
    },
}

impl Popover {
    /// Whether it's one of the bar's menus, drawn over the bar rather than in the table.
    pub(crate) fn is_view_menu(&self) -> bool {
        matches!(self, Self::AddView | Self::NewBoard | Self::View { .. })
    }
}

/// A typed value asked for: of the host's field, or (when the host hands it back) of the
/// table's own.
pub(crate) struct PendingText {
    token: u64,
    target: TextTarget,
    kind: PropertyType,
    label: String,
    text: String,
    /// The most characters the value may have (the limits'), if it has a limit.
    max_chars: Option<usize>,
}

/// A typed value being edited in place, with gpui-core's Input.
pub(crate) struct Editor {
    token: u64,
    pub target: TextTarget,
    pub kind: PropertyType,
    /// The property's name, for assistive technology.
    pub label: String,
    /// What it opened with: committed unchanged, it emits nothing.
    original: String,
    pub state: Entity<InputState>,
    _events: Subscription,
}

impl Editor {
    /// Whether it's editing this row's cell of this property.
    pub(crate) fn edits(&self, row_id: &str, property_id: &str) -> bool {
        matches!(&self.target, TextTarget::Cell { row_id: r, property_id: p }
            if r == row_id && p == property_id)
    }

    /// Whether it grows over the cells below as a textarea (a text value), rather than staying
    /// in its cell on one line.
    pub(crate) fn multiline(&self) -> bool {
        self.kind == PropertyType::Text && matches!(self.target, TextTarget::Cell { .. })
    }
}

/// A drag in progress.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Drag {
    /// A column's edge, from `start_x` (window pixels) and `start` points wide.
    Resize {
        column: usize,
        start_x: Pixels,
        start: f32,
        width: f32,
        touch: bool,
    },
    /// A column's header, pressed at `start_x` and now at `x`; `moved` once it went far
    /// enough to be a drag rather than a press.
    Header {
        column: usize,
        start_x: Pixels,
        x: Pixels,
        moved: bool,
    },
}

/// What the view shows, worked out from the docs and the active view.
#[derive(Default)]
pub(crate) struct Shown {
    pub view: Option<View>,
    pub columns: Vec<ColumnLayout>,
    pub items: Vec<Item>,
    pub groups: Vec<Group>,
    pub collapsed: Vec<bool>,
    pub rows: Vec<usize>,
    pub issues: Vec<Issue>,
    pub today: i64,
    /// A board's columns, when the view is a board.
    pub board: Option<Vec<super::board::BoardColumn>>,
    /// A list's groups (or its one group of every row, when it doesn't group), when the view
    /// is a list: what a drag moves rows between, as a board's columns.
    pub list: Option<Vec<super::board::BoardColumn>>,
    /// A table's groups (or its one group of every row, when it doesn't group), when the view
    /// is a table: what a dragged row moves between, as a list's groups.
    pub table: Option<Vec<super::board::BoardColumn>>,
    /// A gallery's one group of every row (it doesn't draw groups yet), when the view is a
    /// gallery: what a dragged card moves within.
    pub gallery: Option<Vec<super::board::BoardColumn>>,
    /// The properties a board's cards and a list's lines show beside the title (places in
    /// `properties`).
    pub card_properties: Vec<usize>,
    /// A table's footers' results, by column: the table's one, or each group's by its place
    /// ([`super::calc::footers`]). Empty when the view has no calculations.
    pub calcs: Vec<Vec<Option<crate::engine::Calculated>>>,
}

/// A table of a database's rows, with the active view's sort, filter, grouping and columns.
/// It draws only the rows on screen, works the rows out again only when the docs or the view
/// change, and hands every change to its [`TableHost`].
pub struct TableView {
    pub(crate) host: Rc<dyn TableHost>,
    pub(crate) database: Doc,
    /// Shared, so a search can fold their words off the main thread.
    pub(crate) rows: Arc<Vec<Doc>>,
    pub(crate) view_id: Option<String>,
    pub(crate) focus: FocusHandle,
    pub(crate) dirty: bool,
    pub(crate) shown: Shown,
    /// Counts the times the lines were worked out, so a draw's handlers can tell whether the
    /// places they kept are still the lines'.
    pub(crate) shown_serial: u64,
    pub(crate) list: UniformListScrollHandle,
    /// How far the columns after the title are scrolled sideways, in window pixels.
    pub(crate) scroll_x: Pixels,
    /// The table's bounds when it last drew.
    pub(crate) bounds: Rc<StdCell<Bounds<Pixels>>>,
    /// The window's rem when it last drew.
    pub(crate) rem: Pixels,
    /// The theme it last drew with.
    pub(crate) theme: TableTheme,
    pub(crate) cell: Option<CellRef>,
    pub(crate) popover: Option<Popover>,
    /// Counts the menus opened, so each draws as a new gpui-core `PopupMenu` (its highlight
    /// cleared, taking the keyboard).
    pub(crate) menu_serial: u64,
    /// The open date cell's month: gpui-core's `DatePickerState`, which its `MonthGrid` draws,
    /// and the table's subscription to its picks.
    pub(crate) date: Option<(Entity<DatePickerState>, Subscription)>,
    pub(crate) drag: Option<Drag>,
    text_token: u64,
    /// A typed value asked of the host (or not yet handed back to be edited in place).
    pub(crate) pending_text: Option<PendingText>,
    /// The typed value being edited in place.
    pub(crate) editor: Option<Editor>,
    /// A row asked for with `rowCreated`, focused when the host hands it back.
    pub(crate) pending_row: Option<String>,
    /// The focused line's row, read when the rows were first replaced since they were last
    /// worked out (later the lines' places name other rows): what the focus follows then.
    focus_before_rows: Option<Option<String>>,
    /// A row whose focus should land in this group (by key) when the rows are worked out
    /// again: a list's row dropped into another group, which a multi-select row may share
    /// with others.
    pub(crate) focus_group: Option<(String, String)>,
    /// The board's state, when the view is a board.
    pub(crate) board: super::board::Board,
    /// The field of a property's menu (a name, an option's name), while one's open.
    pub(crate) field: Option<super::properties::MenuField>,
    /// Why the open menu refused what was asked (a name taken, a limit reached).
    pub(crate) note: Option<String>,
    /// Where the next menu opens: on the entry a step of it (a name typed, a delete confirmed)
    /// came from when it goes back, or on what a change moves the keyboard to.
    pub(crate) reopen: Reopen,
    /// Where an open menu's highlight is moved (gpui-core's `move_highlight_request`): the
    /// place of the menu (the entries whose submenus lead to it), the entry, and the request's
    /// number, as Date grouping after grouping by a date.
    pub(crate) move_highlight: Option<(Vec<String>, String, u64)>,
    /// Counts the highlight moves asked for, so each is a new request, even to the entry the
    /// last one moved to.
    pub(crate) move_requests: u64,
    /// A relation being added: its name, database and whether it's two-way.
    pub(crate) new_relation: Option<super::properties::NewRelation>,
    /// A property just added, whose column is scrolled into view once the host hands it back.
    pub(crate) pending_column: Option<String>,
    /// The bar of views, when it's drawn.
    pub(crate) bar: super::views::Bar,
    /// A board being added that waits for what it groups by.
    pub(crate) new_view: Option<super::views::NewView>,
    /// The table's selected rows, by id, in the order they were selected.
    pub(crate) selected: Vec<String>,
    /// The row a Shift-click or Shift-arrow extends the selection from.
    pub(crate) anchor: Option<String>,
    /// The row a pointer is over: its handle is made, the others' aren't.
    pub(crate) hovered_row: Option<String>,
    /// The row in the peek panel ([`TableView::peek`]), and whether opening a row peeks it.
    pub(crate) peek: Option<String>,
    pub(crate) peek_mode: bool,
    /// While a value is edited from the peek: where its editor hangs (the value in the panel)
    /// in place of its cell, and each value's place as last drawn.
    pub(crate) peek_anchor: Option<Bounds<Pixels>>,
    pub(crate) peek_places:
        Rc<std::cell::RefCell<std::collections::HashMap<String, Bounds<Pixels>>>>,
    /// While a property the view hides as a column is edited from the peek: its index, the
    /// property the open picker or month sets in place of its cell's (the row's title cell).
    pub(crate) peek_property: Option<usize>,
    /// The gallery's card with the keyboard, by place, and its grid as last drawn.
    pub(crate) gallery_focus: Option<usize>,
    pub(crate) gallery_grid: StdCell<super::gallery::GalleryGrid>,
    /// The one drag source every row's handle, card and line shares, built on first use:
    /// each gives only its row and group.
    pub(crate) card_drag: std::cell::OnceCell<gpui_dnd::SharedSource<super::board::DraggedCard>>,
    /// The search from the bar of views, and what it keeps between keystrokes.
    pub(crate) search: super::search::Search,
    /// The rows' text the view's filter and sorts lowercased, kept until the rows change.
    row_text: engine::RowText,
}

impl Focusable for TableView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl TableView {
    /// A table of `database`'s `rows` (its row docs, in any order; tombstoned ones are left
    /// out), showing its first table view (or its first view of any kind).
    pub fn new(
        host: Rc<dyn TableHost>,
        database: Doc,
        rows: Vec<Doc>,
        cx: &mut Context<Self>,
    ) -> Self {
        let theme = host.theme(cx);
        Self {
            host,
            database,
            rows: Arc::new(rows),
            view_id: None,
            focus: cx.focus_handle(),
            dirty: true,
            shown: Shown::default(),
            shown_serial: 0,
            list: UniformListScrollHandle::new(),
            scroll_x: px(0.),
            bounds: Rc::default(),
            rem: px(15.),
            theme,
            cell: None,
            popover: None,
            menu_serial: 0,
            date: None,
            drag: None,
            text_token: 0,
            pending_text: None,
            editor: None,
            pending_row: None,
            focus_before_rows: None,
            focus_group: None,
            board: super::board::Board::default(),
            field: None,
            note: None,
            reopen: Reopen::default(),
            move_highlight: None,
            move_requests: 0,
            new_relation: None,
            pending_column: None,
            bar: super::views::Bar::default(),
            new_view: None,
            selected: Vec::new(),
            anchor: None,
            hovered_row: None,
            peek: None,
            peek_mode: false,
            peek_anchor: None,
            peek_places: Default::default(),
            peek_property: None,
            gallery_focus: None,
            gallery_grid: StdCell::new(Default::default()),
            card_drag: std::cell::OnceCell::new(),
            search: Default::default(),
            row_text: engine::RowText::new(),
        }
    }

    /// Shows the view `view_id`: a board for a view of type `board`, a list for `list`, a
    /// table for any other.
    pub fn with_view(mut self, view_id: impl Into<String>) -> Self {
        self.view_id = Some(view_id.into());
        self
    }

    /// The database doc and its rows, after the host applied edits (or anything else changed
    /// them). The rows are worked out again on the next draw.
    pub fn set_docs(&mut self, database: Doc, rows: Vec<Doc>, cx: &mut Context<Self>) {
        self.database = database;
        self.replace_rows(rows);
        self.invalidate(cx);
    }

    /// The database doc alone (its properties or views changed).
    pub fn set_database(&mut self, database: Doc, cx: &mut Context<Self>) {
        self.database = database;
        self.invalidate(cx);
    }

    /// The rows alone.
    pub fn set_rows(&mut self, rows: Vec<Doc>, cx: &mut Context<Self>) {
        self.replace_rows(rows);
        self.invalidate(cx);
    }

    /// Replaces the rows, noting first which row the focused line is on, while the lines'
    /// places are still places in these rows.
    fn replace_rows(&mut self, rows: Vec<Doc>) {
        if self.focus_before_rows.is_none() {
            self.focus_before_rows = Some(self.focused_row_id());
        }
        self.rows = Arc::new(rows);
        self.row_text.clear();
    }

    /// The row the focused line is on, as the lines were last worked out.
    fn focused_row_id(&self) -> Option<String> {
        match self.shown.items.get(self.cell?.item)? {
            Item::Row { row, .. } => self.rows.get(*row).map(|r| r.id.clone()),
            _ => None,
        }
    }

    /// A row by its place in [`TableView::rows`], as the lines were last worked out: None while
    /// the rows or the view changed since (until they're worked out again, before the next
    /// draw), so a place kept from a draw never names another row, or none.
    pub(crate) fn shown_row(&self, row: usize) -> Option<&Doc> {
        if self.dirty {
            return None;
        }
        self.rows.get(row)
    }

    /// Whether the lines are still those worked out as `serial` (a draw's, which its click
    /// handlers keep): not once the docs or the view changed, as a place kept then may be
    /// another row's now, or none.
    pub(crate) fn still_shown(&self, serial: u64) -> bool {
        !self.dirty && self.shown_serial == serial
    }

    /// Shows another of the database's views: a board for a view of type `board`, a list for
    /// `list`, a table for any other. It's the same entity and host either way.
    pub fn set_view(&mut self, view_id: impl Into<String>, cx: &mut Context<Self>) {
        self.view_id = Some(view_id.into());
        self.cell = None;
        self.close_popover(cx);
        // A filter value being typed was for the view left; a cell's is kept until its field
        // loses the focus (it commits then).
        if self
            .editor
            .as_ref()
            .is_some_and(|e| matches!(e.target, TextTarget::Filter { .. }))
        {
            self.editor = None;
        }
        self.scroll_x = px(0.);
        self.list.scroll_to_item(0, ScrollStrategy::Top);
        self.board = super::board::Board::default();
        self.focus_group = None;
        self.selected.clear();
        self.anchor = None;
        self.invalidate(cx);
    }

    pub fn database(&self) -> &Doc {
        &self.database
    }

    pub fn rows(&self) -> &[Doc] {
        &self.rows
    }

    /// The view shown: the one asked for, else the first table view, else the first view.
    pub fn view(&self) -> Option<&View> {
        let fields = self.database.database.as_ref()?;
        if let Some(id) = &self.view_id
            && let Some(v) = fields.view(id)
        {
            return Some(v);
        }
        let views = fields.views_in_order();
        views
            .iter()
            .find(|v| v.kind == ViewType::Table)
            .or(views.first())
            .copied()
    }

    /// The rows shown (indexes into [`TableView::rows`]), in the view's order: those that pass
    /// its filter, whether or not their group is folded.
    pub fn shown_rows(&mut self) -> &[usize] {
        self.ensure();
        &self.shown.rows
    }

    /// The lines of the table, in order.
    pub fn items(&mut self) -> &[Item] {
        self.ensure();
        &self.shown.items
    }

    /// The view's groups, when it groups.
    pub fn groups(&mut self) -> &[Group] {
        self.ensure();
        &self.shown.groups
    }

    /// The columns shown, in order (the title's first), and their widths in points.
    pub fn columns(&mut self) -> Vec<(String, f32)> {
        self.ensure();
        self.shown
            .columns
            .iter()
            .map(|c| (c.property_id.clone(), c.width))
            .collect()
    }

    /// A row's cell as words, as assistive technology reads it: an option's name, a person's
    /// name from the host, a date in the viewer's zone. None for a row or property the table
    /// hasn't.
    pub fn cell_words(&self, row_id: &str, property_id: &str) -> Option<String> {
        let p = self.database.database.as_ref()?.property(property_id)?;
        let r = self.rows.iter().find(|r| r.id == row_id)?;
        Some(self.cell_text(r, p))
    }

    /// What the engine couldn't use in the view.
    pub fn issues(&mut self) -> &[Issue] {
        self.ensure();
        &self.shown.issues
    }

    /// The focused cell, if any.
    pub fn focused_cell(&self) -> Option<CellRef> {
        self.cell
    }

    /// The focused cell as a row id and property id, when it's a row's cell.
    pub fn focused_row_cell(&mut self) -> Option<(String, String)> {
        self.ensure();
        let c = self.cell?;
        let Item::Row { row, .. } = *self.shown.items.get(c.item)? else {
            return None;
        };
        let col = self.shown.columns.get(c.column)?;
        Some((self.rows[row].id.clone(), col.property_id.clone()))
    }

    /// Focuses a row's cell, scrolling it into view.
    pub fn focus_cell(&mut self, row_id: &str, property_id: &str, cx: &mut Context<Self>) {
        self.ensure();
        let item = self
            .shown
            .items
            .iter()
            .position(|i| matches!(i, Item::Row { row, .. } if self.rows[*row].id == row_id));
        let column = self
            .shown
            .columns
            .iter()
            .position(|c| c.property_id == property_id);
        if let (Some(item), Some(column)) = (item, column) {
            self.set_cell(CellRef { item, column }, cx);
        }
    }

    /// Scrolls the columns after the title sideways to `x` window pixels (within what they can).
    pub fn set_scroll_x(&mut self, x: Pixels, cx: &mut Context<Self>) {
        self.scroll_x = x;
        self.clamp_scroll();
        cx.notify();
    }

    /// Opens a column's menu (sort, filter, move, hide), as pressing its header does.
    pub fn open_column_menu(&mut self, property_id: &str, cx: &mut Context<Self>) {
        self.ensure();
        if let Some(c) = self
            .shown
            .columns
            .iter()
            .position(|c| c.property_id == property_id)
        {
            self.open_header_menu(c, cx);
        }
    }

    /// Edits a row's cell as Enter (or a second press) does: a picker, the month, a toggle,
    /// or a typed value, in place (or in the host's field, if it has one).
    pub fn edit(
        &mut self,
        row_id: &str,
        property_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_cell(row_id, property_id, cx);
        if let Some(c) = self.cell {
            self.edit_cell(c, window, cx);
        }
    }

    /// Closes an open menu or picker.
    pub fn close_popover(&mut self, cx: &mut Context<Self>) {
        self.date = None;
        self.close_property_menu();
        if self.popover.take().is_some() {
            cx.notify();
        }
    }

    /// How far the columns after the title are scrolled sideways, in window pixels.
    pub fn scroll_x(&self) -> Pixels {
        self.scroll_x
    }

    /// Whether a menu or picker is open.
    pub fn has_popover(&self) -> bool {
        self.popover.is_some()
    }

    /// What's being typed in place, and what for: a cell's value or a filter's.
    pub fn editing(&self, cx: &App) -> Option<(TextTarget, String)> {
        let e = self.editor.as_ref()?;
        Some((e.target.clone(), e.state.read(cx).value().to_string()))
    }

    /// The field a value is typed in place in, while one is.
    pub fn editor_state(&self) -> Option<Entity<InputState>> {
        self.editor.as_ref().map(|e| e.state.clone())
    }

    fn invalidate(&mut self, cx: &mut Context<Self>) {
        self.dirty = true;
        cx.notify();
    }

    /// Works out what the view shows, if the docs or the view changed since it last did.
    pub(crate) fn ensure(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        self.shown_serial = self.shown_serial.wrapping_add(1);
        let previous_row = match self.focus_before_rows.take() {
            Some(id) => id,
            None => self.focused_row_id(),
        };
        let previous_item = self
            .cell
            .and_then(|c| self.shown.items.get(c.item).copied());
        let host = self.host.clone();
        let lookup = HostLookup(host.as_ref());
        let mut ecx = EngineContext::new(host.now()).with_lookup(&lookup);
        ecx.utc_offset_minutes = host.utc_offset_minutes();
        ecx.week_starts_monday = host.week_starts_monday();
        ecx.limits = host.limits();
        let today = ecx.today();
        let Some(fields) = self.database.database.as_ref() else {
            self.shown = Shown {
                today,
                ..Shown::default()
            };
            return;
        };
        let view = self.view().cloned().unwrap_or_else(|| {
            View::new(new_id(), "Table", ViewType::Table, crate::order::first())
        });
        let s = &self.theme.sizes;
        let columns = ops::layout(
            fields,
            &view,
            s.column_width,
            s.title_width,
            s.min_column_width,
            s.max_column_width,
        );
        // The bar's search, if any, on top of the view's own rows.
        let mut row_text = std::mem::take(&mut self.row_text);
        let result = self.searched(fields, &view, &columns, || {
            engine::query_with(fields, &view, &self.rows, &ecx, &mut row_text)
        });
        self.row_text = row_text;
        let mut items = Vec::with_capacity(result.rows.len() + 1);
        let mut collapsed = Vec::new();
        let groups = result.groups.unwrap_or_default();
        let board = (view.kind == ViewType::Board)
            .then(|| super::board::lanes_of(&view, &groups, &result.rows));
        let list = (view.kind == ViewType::List)
            .then(|| super::board::lanes_of(&view, &groups, &result.rows));
        let gallery = (view.kind == ViewType::Gallery)
            .then(|| super::board::lanes_of(&view, &[], &result.rows));
        let table = (board.is_none() && list.is_none() && gallery.is_none())
            .then(|| super::board::lanes_of(&view, &groups, &result.rows));
        let mut card_properties: Vec<usize> = Vec::new();
        for id in &view.card_properties {
            if let Some(i) = fields.properties.iter().position(|p| p.id == *id)
                && fields.properties[i].kind != PropertyType::Title
                && !card_properties.contains(&i)
            {
                card_properties.push(i);
            }
        }
        // A table's footers, under each New row.
        let footed = table.is_some();
        let grouped =
            view.group_by.is_some() && !groups.is_empty() && view.kind != ViewType::Gallery;
        if grouped {
            for (gi, g) in groups.iter().enumerate() {
                let folded = view.collapsed_groups.contains(&g.key.to_string());
                collapsed.push(folded);
                items.push(Item::Group(gi));
                if !folded {
                    items.extend(g.rows.iter().map(|&row| Item::Row {
                        row,
                        group: Some(gi),
                    }));
                    items.push(Item::New(Some(gi)));
                    if footed {
                        items.push(Item::Calc(Some(gi)));
                    }
                }
            }
        } else {
            items.extend(
                result
                    .rows
                    .iter()
                    .map(|&row| Item::Row { row, group: None }),
            );
            items.push(Item::New(None));
            if footed {
                items.push(Item::Calc(None));
            }
        }
        let calcs = if footed {
            super::calc::footers(
                fields,
                &view,
                &columns,
                grouped.then_some((&groups[..], &collapsed[..])),
                &result.rows,
                &self.rows,
            )
        } else {
            Vec::new()
        };
        self.shown = Shown {
            view: Some(view),
            columns,
            items,
            groups,
            collapsed,
            rows: result.rows,
            issues: result.issues,
            today,
            board,
            list,
            table,
            gallery,
            card_properties,
            calcs,
        };
        // The focus follows its row (or a pending new row), or stays on the same kind of line.
        let wanted = self.pending_row.clone().or(previous_row);
        if let Some(id) = wanted {
            let is_row = |i: &Item| matches!(i, Item::Row { row, .. } if self.rows[*row].id == id);
            // A row dropped into a group is focused there, though it may be in others too.
            let in_group = self
                .focus_group
                .take()
                .filter(|(row, _)| *row == id)
                .and_then(|(_, key)| {
                    self.shown.items.iter().position(|i| {
                        is_row(i)
                            && matches!(i, Item::Row { group: Some(g), .. }
                                if self.shown.groups.get(*g).is_some_and(|g| g.key.to_string() == key))
                    })
                });
            let found = in_group.or_else(|| self.shown.items.iter().position(is_row));
            if let Some(item) = found {
                let column = if self.pending_row.as_deref() == Some(id.as_str()) {
                    self.pending_row = None;
                    0
                } else {
                    self.cell.map_or(0, |c| c.column)
                };
                self.cell = Some(CellRef { item, column });
                self.list.scroll_to_item(item, ScrollStrategy::Nearest);
            } else if self.pending_row.is_none() {
                self.cell = None;
            }
        } else if let (Some(c), Some(prev)) = (self.cell, previous_item) {
            let same = self.shown.items.iter().position(|i| *i == prev);
            self.cell = same.map(|item| CellRef { item, ..c });
        }
        if let Some(c) = &mut self.cell {
            c.column = c.column.min(self.shown.columns.len().saturating_sub(1));
            if c.item >= self.shown.items.len() {
                self.cell = None;
            }
        }
        // A popover on a line that's gone closes.
        if let Some(Popover::Picker { cell } | Popover::Date { cell }) = &self.popover
            && self.cell != Some(*cell)
        {
            self.popover = None;
            self.date = None;
        }
        if let Some(Popover::Header { property_id } | Popover::FilterValue { property_id }) =
            &self.popover
            && self.column_of(property_id).is_none()
        {
            self.popover = None;
        }
        // A property's page whose property is gone closes; a property just added has its
        // column scrolled into view.
        // A view's menu whose view is gone closes too.
        let gone = match &self.popover {
            Some(Popover::Property { property_id, .. }) => fields.property(property_id).is_none(),
            Some(Popover::View { view_id, .. }) => fields.view(view_id).is_none(),
            _ => false,
        };
        if gone {
            self.popover = None;
            self.close_property_menu();
        }
        self.close_gone_calc_menu();
        // Selected rows the view no longer shows (deleted, filtered out) are let go, and the
        // rows' menu closes when one of its rows is gone.
        self.prune_selection();
        if let Some(id) = self.pending_column.clone()
            && let Some(c) = self.shown.columns.iter().position(|c| c.property_id == id)
        {
            // The new column is the last: scrolled to the end, the + after it in view too.
            self.pending_column = None;
            self.reveal_column(c);
            if c + 1 == self.shown.columns.len() {
                self.scroll_x = self.max_scroll_x();
            }
        }
    }

    // --- Geometry ---------------------------------------------------------------------------

    /// A line's height in window pixels: the theme's row height, or the touch minimum.
    /// (Its lower border is a pixel more, so what's pressable in it is the full height.)
    pub(crate) fn row_height(&self) -> Pixels {
        self.theme.hit(self.theme.sizes.row_height, self.rem) + px(1.)
    }

    /// The header row's height in window pixels, its lower border included.
    pub(crate) fn header_height(&self) -> Pixels {
        self.theme.hit(self.theme.sizes.header_height, self.rem) + px(1.)
    }

    /// A column's width in window pixels (a column being resized at its new width).
    pub(crate) fn column_px(&self, column: usize) -> Pixels {
        let points = match &self.drag {
            Some(Drag::Resize {
                column: c, width, ..
            }) if *c == column => *width,
            _ => self.shown.columns.get(column).map_or(0., |c| c.width),
        };
        self.theme.pixels(points, self.rem)
    }

    /// The columns after the title, in pixels.
    pub(crate) fn strip_width(&self) -> Pixels {
        (1..self.shown.columns.len()).fold(px(0.), |w, c| w + self.column_px(c))
            + self.theme.hit(32., self.rem)
    }

    /// The farthest the columns can scroll sideways.
    pub(crate) fn max_scroll_x(&self) -> Pixels {
        let b = self.bounds.get();
        let room = b.size.width - self.column_px(0);
        (self.strip_width() - room).max(px(0.))
    }

    pub(crate) fn clamp_scroll(&mut self) {
        self.scroll_x = self.scroll_x.clamp(px(0.), self.max_scroll_x());
    }

    /// A column's left edge in the window.
    pub(crate) fn column_left(&self, column: usize) -> Pixels {
        let origin = self.bounds.get().origin.x;
        if column == 0 {
            return origin;
        }
        let before = (1..column).fold(px(0.), |w, c| w + self.column_px(c));
        origin + self.column_px(0) + before - self.scroll_x
    }

    /// A column header's bounds in the window.
    pub(crate) fn header_bounds(&self, column: usize) -> Bounds<Pixels> {
        let top = self.bounds.get().origin.y;
        Bounds::new(
            point(self.column_left(column), top),
            size(self.column_px(column), self.header_height()),
        )
    }

    /// A cell's bounds in the window (as the list is scrolled now).
    pub(crate) fn cell_bounds(&self, cell: CellRef) -> Bounds<Pixels> {
        let b = self.bounds.get();
        let scrolled = self.list.0.borrow().base_handle.offset().y;
        let top =
            b.origin.y + self.header_height() + self.row_height() * cell.item as f32 + scrolled;
        Bounds::new(
            point(self.column_left(cell.column), top),
            size(self.column_px(cell.column), self.row_height()),
        )
    }

    // --- Focus and keys ---------------------------------------------------------------------

    pub(crate) fn set_cell(&mut self, cell: CellRef, cx: &mut Context<Self>) {
        self.ensure();
        if cell.item >= self.shown.items.len() {
            return;
        }
        let cell = CellRef {
            column: cell.column.min(self.shown.columns.len().saturating_sub(1)),
            ..cell
        };
        self.cell = Some(cell);
        self.list.scroll_to_item(cell.item, ScrollStrategy::Nearest);
        self.reveal_column(cell.column);
        cx.notify();
    }

    /// Scrolls sideways so a column is in view (the title's always is).
    pub(crate) fn reveal_column(&mut self, column: usize) {
        if column == 0 || self.bounds.get().size.width <= px(0.) {
            return;
        }
        let start = (1..column).fold(px(0.), |w, c| w + self.column_px(c));
        let end = start + self.column_px(column);
        let room = (self.bounds.get().size.width - self.column_px(0)).max(px(0.));
        if start < self.scroll_x {
            self.scroll_x = start;
        } else if end > self.scroll_x + room {
            self.scroll_x = (end - room).min(start);
        }
        self.clamp_scroll();
    }

    pub(crate) fn move_cell(&mut self, d_item: isize, d_col: isize, cx: &mut Context<Self>) {
        self.ensure();
        let n = self.shown.items.len();
        if n == 0 {
            return;
        }
        let cols = self.shown.columns.len().max(1);
        let c = self.cell.unwrap_or(CellRef { item: 0, column: 0 });
        if self.cell.is_none() {
            self.set_cell(c, cx);
            return;
        }
        let item = (c.item as isize + d_item).clamp(0, n as isize - 1) as usize;
        let column = (c.column as isize + d_col).clamp(0, cols as isize - 1) as usize;
        self.set_cell(CellRef { item, column }, cx);
    }

    /// Tab: the next cell, on to the next line after a line's last (or back, with shift).
    pub(crate) fn tab(&mut self, back: bool, cx: &mut Context<Self>) {
        self.ensure();
        let n = self.shown.items.len();
        let cols = self.shown.columns.len().max(1);
        let Some(c) = self.cell else {
            self.move_cell(0, 0, cx);
            return;
        };
        let wide = matches!(self.shown.items.get(c.item), Some(Item::Row { .. }));
        let (item, column) = if back {
            if wide && c.column > 0 {
                (c.item, c.column - 1)
            } else if c.item > 0 {
                (c.item - 1, cols - 1)
            } else {
                (0, 0)
            }
        } else if wide && c.column + 1 < cols {
            (c.item, c.column + 1)
        } else if c.item + 1 < n {
            (c.item + 1, 0)
        } else {
            (c.item, c.column)
        };
        self.set_cell(CellRef { item, column }, cx);
    }

    // --- Events -----------------------------------------------------------------------------

    /// Hands an event to the host after the current update, so the host can change the table
    /// in its handler.
    pub(crate) fn emit(&self, event: TableEvent, window: &mut Window, cx: &mut App) {
        let host = self.host.clone();
        window.defer(cx, move |window, cx| host.event(event, window, cx));
    }

    pub(crate) fn emit_edits(&self, edits: Vec<Edit>, window: &mut Window, cx: &mut App) {
        if !edits.is_empty() {
            self.emit(TableEvent::Edits { edits }, window, cx);
        }
    }

    pub(crate) fn database_id(&self) -> String {
        self.database.id.clone()
    }

    /// Shows a changed view at once and hands it to the host as a `viewChanged`.
    pub(crate) fn change_view(&mut self, view: View, window: &mut Window, cx: &mut Context<Self>) {
        if self.shown.view.as_ref() == Some(&view) {
            return;
        }
        if let Some(fields) = self.database.database.as_mut() {
            match fields.views.iter_mut().find(|v| v.id == view.id) {
                Some(v) => *v = view.clone(),
                None => fields.views.push(view.clone()),
            }
        }
        self.view_id = Some(view.id.clone());
        self.emit_edits(
            vec![Edit::ViewChanged {
                database_id: self.database_id(),
                view,
            }],
            window,
            cx,
        );
        self.invalidate(cx);
    }

    /// Opens a row's page.
    pub(crate) fn open_row(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.peek_mode {
            if let Some(id) = self.shown_row(row).map(|r| r.id.clone()) {
                self.peek(&id, cx);
            }
            return;
        }
        if let Some(r) = self.shown_row(row) {
            self.emit(
                TableEvent::OpenRow {
                    row_id: r.id.clone(),
                },
                window,
                cx,
            );
        }
    }

    /// The row and property of a cell, when it's a row's: for the picker or month open from
    /// the peek on a property the view hides, that property ([`Self::peek_property`]).
    pub(crate) fn cell_target(&self, cell: CellRef) -> Option<(usize, &Property)> {
        let col = self.shown.columns.get(cell.column)?;
        let property = match (&self.popover, self.peek_property) {
            (Some(Popover::Picker { cell: c } | Popover::Date { cell: c }), Some(p))
                if *c == cell =>
            {
                p
            }
            _ => col.property,
        };
        self.cell_property(cell, property)
    }

    /// The row of a cell, when it's a row's, and the property at index `property`.
    fn cell_property(&self, cell: CellRef, property: usize) -> Option<(usize, &Property)> {
        let Item::Row { row, .. } = *self.shown.items.get(cell.item)? else {
            return None;
        };
        self.shown_row(row)?;
        let p = self.database.database.as_ref()?.properties.get(property)?;
        Some((row, p))
    }

    /// Enter: opens a row from its title, edits any other cell, folds a group, makes a row.
    pub(crate) fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ensure();
        let Some(c) = self.cell else {
            self.move_cell(0, 0, cx);
            return;
        };
        match self.shown.items.get(c.item).copied() {
            Some(Item::Group(g)) => self.toggle_group(g, window, cx),
            Some(Item::New(g)) => self.new_row(g, window, cx),
            Some(Item::Calc(g)) => self.open_calc_menu_on(c.column, g, cx),
            Some(Item::Row { row, .. }) => {
                let is_title = self
                    .cell_target(c)
                    .is_some_and(|(_, p)| p.kind == PropertyType::Title);
                if is_title {
                    self.open_row(row, window, cx);
                } else {
                    self.edit_cell(c, window, cx);
                }
            }
            None => {}
        }
    }

    /// Edits a cell in the way its type edits: a checkbox toggles, an option, person or relation
    /// opens a picker (gpui-core's Picker), a date its month (gpui-core's MonthGrid), and a typed
    /// value opens a field in place (or the host's own).
    pub(crate) fn edit_cell(&mut self, cell: CellRef, window: &mut Window, cx: &mut Context<Self>) {
        // Not from the peek: its editor is the cell's own.
        self.peek_anchor = None;
        self.edit_cell_as(cell, None, window, cx);
    }

    /// [`Self::edit_cell`], editing the property at index `property` of the cell's row in
    /// place of the cell's own when given: the peek's way to a property the view hides.
    pub(crate) fn edit_cell_as(
        &mut self,
        cell: CellRef,
        property: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ensure();
        self.peek_property = property;
        let target = match property {
            Some(property) => self.cell_property(cell, property),
            None => self.cell_target(cell),
        };
        let Some((row, p)) = target else {
            return;
        };
        let p = p.clone();
        self.cell = Some(cell);
        self.clear_selection();
        match p.kind {
            PropertyType::Checkbox => {
                let on = self.rows[row]
                    .value(&p.id)
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let edit = Edit::RowValueChanged {
                    row_id: self.rows[row].id.clone(),
                    property_id: p.id.clone(),
                    value: Some(Value::Bool(!on)),
                };
                self.emit_edits(vec![edit], window, cx);
            }
            PropertyType::Select
            | PropertyType::MultiSelect
            | PropertyType::Person
            | PropertyType::Relation => {
                self.menu_serial += 1;
                self.popover = Some(Popover::Picker { cell });
            }
            PropertyType::Date => {
                let offset = i64::from(self.host.utc_offset_minutes()) * 60_000;
                let current = self.rows[row]
                    .value(&p.id)
                    .and_then(Value::as_date)
                    .map(|d| time::days_of(d, offset).0);
                let today = self.shown.today;
                let monday = self.host.week_starts_monday();
                let state = cx.new(|cx| {
                    let mut s = DatePickerState::new(window, cx)
                        .week_start(if monday {
                            chrono::Weekday::Mon
                        } else {
                            chrono::Weekday::Sun
                        })
                        .today(naive_day(today));
                    if let Some(day) = current {
                        s.set_value(Some(naive_day(day)), None, cx);
                    }
                    s
                });
                let picks = cx.subscribe_in(
                    &state,
                    window,
                    |this: &mut Self, _, event: &DatePickerEvent, window, cx| {
                        let DatePickerEvent::Change(value) = event;
                        if let Some(day) = value.start() {
                            this.pick_day(Some(day_of(day)), window, cx);
                        }
                    },
                );
                // The month takes the keyboard: its arrows move by days and weeks, Page Up and
                // Page Down by months, Enter picks, and Escape closes its popover.
                window.focus(&state.read(cx).focus_handle(cx), cx);
                self.date = Some((state, picks));
                self.popover = Some(Popover::Date { cell });
            }
            PropertyType::Title
            | PropertyType::Text
            | PropertyType::Number
            | PropertyType::Url
            | PropertyType::Email
            | PropertyType::Phone => {
                let r = &self.rows[row];
                let text = match p.kind {
                    PropertyType::Title => r.title.clone(),
                    PropertyType::Number => r
                        .value(&p.id)
                        .and_then(Value::as_number)
                        .map(|x| format::number_for_editing(x, &p.config))
                        .unwrap_or_default(),
                    _ => p
                        .shown_text(r.value(&p.id).and_then(Value::as_str).unwrap_or_default())
                        .to_owned(),
                };
                let target = TextTarget::Cell {
                    row_id: r.id.clone(),
                    property_id: p.id.clone(),
                };
                let bounds = self.peek_anchor.unwrap_or_else(|| self.cell_bounds(cell));
                self.begin_text(target, &p, text, bounds, window, cx);
            }
            _ => {}
        }
        cx.notify();
    }

    fn begin_text(
        &mut self,
        target: TextTarget,
        p: &Property,
        text: String,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Another field open in place commits first.
        self.finish_editor(true, false, window, cx);
        self.text_token += 1;
        let max_chars = self.host.limits().text_limit(&p.kind);
        self.pending_text = Some(PendingText {
            token: self.text_token,
            target: target.clone(),
            kind: p.kind.clone(),
            label: p.name.clone(),
            text: text.clone(),
            max_chars,
        });
        let edit = TextEdit {
            target,
            kind: p.kind.clone(),
            label: p.name.clone(),
            text,
            max_chars,
            bounds,
            reply: TextReply {
                view: cx.entity().downgrade(),
                token: self.text_token,
            },
        };
        // The host's field, if it has one; by default the host hands it straight back
        // (`TextReply::edit_in_place`) and it opens in place.
        let host = self.host.clone();
        window.defer(cx, move |window, cx| host.edit_text(edit, window, cx));
    }

    /// Opens the field in place for a typed value asked for, as [`TextReply::edit_in_place`]
    /// asks: in the cell, or under the column's header for a filter.
    pub(crate) fn open_editor(&mut self, token: u64, window: &mut Window, cx: &mut Context<Self>) {
        if self.pending_text.as_ref().map(|p| p.token) != Some(token) {
            return;
        }
        let Some(pending) = self.pending_text.take() else {
            return;
        };
        let cell = matches!(pending.target, TextTarget::Cell { .. });
        let multiline = cell && pending.kind == PropertyType::Text;
        let placeholder = match (&pending.target, &pending.kind) {
            (TextTarget::Filter { .. }, _) => "Value".to_owned(),
            (_, PropertyType::Title) => "Untitled".to_owned(),
            _ => String::new(),
        };
        let keyboard = keyboard_for(&pending.kind);
        let text = pending.text.clone();
        let state = cx.new(|cx| {
            let state = if multiline {
                // Enter commits, as on one line; Shift-Enter starts a new line.
                InputState::textarea(window, cx)
                    .auto_grow(1, 8)
                    .submit_on_enter(true)
            } else {
                InputState::new(window, cx)
            };
            let state = state
                .placeholder(placeholder)
                .default_value(text)
                .keyboard(keyboard);
            // Typing stops at the limit and a paste is cut to fit; the value it opens with
            // isn't cut, so what's committed is cut again (`apply_text`).
            match pending.max_chars {
                Some(max) => state.max_chars(max),
                None => state,
            }
        });
        let events = cx.subscribe_in(
            &state,
            window,
            move |this: &mut Self, state, event: &InputEvent, window, cx| {
                if this.editor.as_ref().map(|e| e.token) != Some(token) {
                    return;
                }
                match event {
                    InputEvent::PressEnter { .. } => this.finish_editor(true, true, window, cx),
                    // Clicking elsewhere commits. The window going to the background doesn't:
                    // the field is still open when it comes back. If the field lost the focus by
                    // being scrolled away (it still holds it, undrawn), the table takes it back.
                    InputEvent::Blur if window.is_window_active() => {
                        let held = state.read(cx).is_focused(window);
                        this.finish_editor(true, held, window, cx)
                    }
                    InputEvent::Blur => {}
                    InputEvent::Change | InputEvent::Focus => {}
                }
            },
        );
        if let TextTarget::Filter { property_id, .. } = &pending.target {
            if self.column_of(property_id).is_none() {
                return;
            }
            self.popover = Some(Popover::FilterValue {
                property_id: property_id.clone(),
            });
        }
        state.update(cx, |s, cx| s.focus(window, cx));
        self.editor = Some(Editor {
            token,
            target: pending.target,
            kind: pending.kind,
            label: pending.label,
            original: pending.text,
            state,
            _events: events,
        });
        cx.notify();
    }

    /// Whether a value is being typed in place.
    pub(crate) fn is_editing(&self) -> bool {
        self.editor.is_some()
    }

    /// Closes the field in place: committing what's typed in it (read, limited and emitted as
    /// the host's answer would be) or not, and giving the table back the focus if `refocus`.
    pub(crate) fn finish_editor(
        &mut self,
        commit: bool,
        refocus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self.editor.take() else {
            return;
        };
        if matches!(self.popover, Some(Popover::FilterValue { .. })) {
            self.popover = None;
        }
        if refocus {
            window.focus(&self.focus, cx);
        }
        let text = editor.state.read(cx).value().to_string();
        if commit && text != editor.original {
            self.apply_text(editor.target, text, window, cx);
        }
        cx.notify();
    }

    /// A host's answer to [`TableHost::edit_text`].
    pub(crate) fn finish_text_edit(
        &mut self,
        token: u64,
        text: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.pending_text.as_ref().map(|p| p.token) != Some(token) {
            return;
        }
        let Some(pending) = self.pending_text.take() else {
            return;
        };
        window.focus(&self.focus, cx);
        if let Some(text) = text {
            self.apply_text(pending.target, text, window, cx);
        }
    }

    /// Reads typed text as its property's type (a number from "1,200" or "$1,200", a percent
    /// from "25%"), cuts it to the limits and emits the edit: a cell's value or title, or a
    /// filter. Text that isn't a number, for a number, changes nothing.
    fn apply_text(
        &mut self,
        target: TextTarget,
        text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(fields) = self.database.database.clone() else {
            return;
        };
        let limits = self.host.limits();
        match target {
            TextTarget::Cell {
                row_id,
                property_id,
            } => {
                let (Some(r), Some(p)) = (
                    self.rows.iter().find(|r| r.id == row_id),
                    fields.property(&property_id),
                ) else {
                    return;
                };
                let edit = match p.kind {
                    PropertyType::Title => {
                        let title = limits.clamp_title(text.trim()).to_owned();
                        (title != r.title).then(|| Edit::RowTitleChanged {
                            row_id: row_id.clone(),
                            title,
                        })
                    }
                    PropertyType::Number => {
                        let value = if text.trim().is_empty() {
                            None
                        } else {
                            match format::parse_number(&text, &p.config) {
                                Some(x) => Some(Value::number(x)),
                                None => return,
                            }
                        };
                        (value.as_ref() != r.value(&p.id)).then(|| Edit::RowValueChanged {
                            row_id: row_id.clone(),
                            property_id: p.id.clone(),
                            value,
                        })
                    }
                    _ => {
                        let t = text.trim();
                        let value = (!t.is_empty()).then(|| limits.clamp_value(p, Value::text(t)));
                        (value.as_ref() != r.value(&p.id)).then(|| Edit::RowValueChanged {
                            row_id: row_id.clone(),
                            property_id: p.id.clone(),
                            value,
                        })
                    }
                };
                self.emit_edits(edit.into_iter().collect(), window, cx);
            }
            TextTarget::Filter { property_id, op } => {
                let Some(p) = fields.property(&property_id) else {
                    return;
                };
                let value = if p.kind == PropertyType::Number {
                    match format::parse_number(&text, &p.config) {
                        Some(x) => crate::model::Value::number(x).to_json(),
                        None => return,
                    }
                } else {
                    let t = text.trim();
                    if t.is_empty() {
                        return;
                    }
                    // Cut to the limit a value of its type has, as a cell's is.
                    match limits.clamp_value(p, Value::text(t)) {
                        Value::Text(t) => serde_json::Value::String(t),
                        _ => serde_json::Value::String(t.to_owned()),
                    }
                };
                if let Some(view) = self.shown.view.clone() {
                    let view = ops::filter(
                        &view,
                        Condition {
                            property_id,
                            op,
                            value,
                        },
                    );
                    self.change_view(view, window, cx);
                }
            }
        }
    }

    /// Delete: empties a cell (a title becomes empty, a checkbox unchecked; the derived created
    /// and edited times and people can't be emptied).
    pub(crate) fn clear_cell(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ensure();
        let Some(c) = self.cell else { return };
        let Some((row, p)) = self.cell_target(c) else {
            return;
        };
        let r = &self.rows[row];
        let edits = match p.kind {
            _ if p.kind.is_derived() => Vec::new(),
            PropertyType::Title if r.title.is_empty() => Vec::new(),
            PropertyType::Title => vec![Edit::RowTitleChanged {
                row_id: r.id.clone(),
                title: String::new(),
            }],
            PropertyType::Relation => {
                let old = r
                    .value(&p.id)
                    .and_then(Value::as_ids)
                    .unwrap_or(&[])
                    .to_vec();
                relation::set(&r.id, p, &old, Vec::new(), &self.host.limits())
            }
            _ if r.value(&p.id).is_none_or(Value::is_empty)
                && !matches!(r.value(&p.id), Some(Value::Bool(true))) =>
            {
                Vec::new()
            }
            _ => vec![Edit::RowValueChanged {
                row_id: r.id.clone(),
                property_id: p.id.clone(),
                value: None,
            }],
        };
        self.emit_edits(edits, window, cx);
    }

    /// Escape: closes the field a value is typed in (keeping the cell focused), else a menu,
    /// else lets go of the focused cell.
    pub(crate) fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_editing() {
            self.finish_editor(false, true, window, cx);
            return;
        }
        // A dragged row goes back where it was (gpui-dnd takes Escape first during a drag; this
        // is for a drag it has already let go of).
        if self.board.drag.take().is_some() {
            cx.notify();
            return;
        }
        self.date = None;
        if self.popover.take().is_none() && self.drag.take().is_none() {
            // Selected rows are let go first, then the focused cell.
            if self.selected.is_empty() {
                self.cell = None;
            } else {
                self.clear_selection();
            }
        }
        cx.notify();
    }

    /// Folds or unfolds a group.
    pub(crate) fn toggle_group(
        &mut self,
        group: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ensure();
        let (Some(view), Some(g)) = (self.shown.view.clone(), self.shown.groups.get(group)) else {
            return;
        };
        let view = ops::toggle_group(&view, &g.key, &self.host.limits());
        self.change_view(view, window, cx);
    }

    /// Asks for a new row at the end of a group (or the table), with the values that keep it
    /// there and an order key after every row's.
    pub(crate) fn new_row(
        &mut self,
        group: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ensure();
        let (Some(fields), Some(view)) =
            (self.database.database.as_ref(), self.shown.view.as_ref())
        else {
            return;
        };
        let key: Option<GroupKey> = group
            .and_then(|g| self.shown.groups.get(g))
            .map(|g| g.key.clone());
        let values = ops::prefill(fields, view, key.as_ref(), self.shown.today);
        let order = ops::append_order(self.rows.iter().map(|r| r.order.as_str()));
        let row_id = new_id();
        self.pending_row = Some(row_id.clone());
        let edit = Edit::RowCreated {
            row_id,
            database_id: self.database_id(),
            order,
            title: String::new(),
            values,
        };
        self.emit_edits(vec![edit], window, cx);
    }

    // --- Menus ------------------------------------------------------------------------------

    /// Opens a column's menu.
    pub(crate) fn open_header_menu(&mut self, column: usize, cx: &mut Context<Self>) {
        self.ensure();
        let Some(col) = self.shown.columns.get(column) else {
            return;
        };
        self.date = None;
        self.menu_serial += 1;
        self.popover = Some(Popover::Header {
            property_id: col.property_id.clone(),
        });
        cx.notify();
    }

    /// Where a property's column is among those shown, as last worked out.
    pub(crate) fn column_of(&self, property_id: &str) -> Option<usize> {
        self.shown
            .columns
            .iter()
            .position(|c| c.property_id == property_id)
    }

    pub(crate) fn open_columns_menu(&mut self, cx: &mut Context<Self>) {
        self.date = None;
        self.menu_serial += 1;
        self.popover = Some(Popover::Columns);
        cx.notify();
    }

    /// The entries of the open menu or picker.
    pub(crate) fn entries(&self) -> Vec<Entry> {
        let Some(fields) = self.database.database.as_ref() else {
            return Vec::new();
        };
        let Some(view) = self.shown.view.as_ref() else {
            return Vec::new();
        };
        use gpui_core::IconName as I;
        match &self.popover {
            Some(Popover::Header { property_id }) => {
                let (Some(p), Some(column)) =
                    (fields.property(property_id), self.column_of(property_id))
                else {
                    return Vec::new();
                };
                // Its property's entries first, as Notion's; it opens on Sort ascending.
                let mut out = self.property_head(p);
                let sorted = view.sort.iter().find(|s| s.property_id == p.id);
                out.push(Entry::heading("Sort"));
                out.push(
                    Entry::act("Sort ascending", Action::Sort(Direction::Ascending))
                        .icon(I::ArrowUp)
                        .checked(sorted.is_some_and(|s| s.direction == Direction::Ascending))
                        .highlighted(true),
                );
                out.push(
                    Entry::act("Sort descending", Action::Sort(Direction::Descending))
                        .icon(I::ArrowDown)
                        .checked(sorted.is_some_and(|s| s.direction == Direction::Descending)),
                );
                if sorted.is_some() {
                    out.push(Entry::act("Remove sort", Action::Unsort).icon(I::X));
                }
                out.push(Entry::heading("Filter"));
                out.extend(self.filter_entries(p, view));
                if ops::filters_by(view, &p.id) {
                    out.push(Entry::act("Clear filter", Action::ClearFilter).icon(I::X));
                }
                out.push(Entry::heading("Column"));
                if column > 0 {
                    if column > 1 {
                        out.push(Entry::act("Move left", Action::MoveBy(-1)).icon(I::ArrowLeft));
                    }
                    if column + 1 < self.shown.columns.len() {
                        out.push(Entry::act("Move right", Action::MoveBy(1)).icon(I::ArrowRight));
                    }
                    out.push(Entry::act("Hide", Action::Hide).icon(I::EyeOff));
                }
                out.push(
                    Entry::act(
                        "Show and hide properties",
                        Action::ToggleColumn(String::new()),
                    )
                    .icon(I::Columns3),
                );
                out.extend(self.property_tail(p));
                out
            }
            Some(Popover::Property { .. } | Popover::AddRelation) => self.property_entries(),
            Some(Popover::View { .. } | Popover::NewBoard) => self.view_entries(),
            Some(Popover::Row { .. }) => self.row_entries(),
            Some(Popover::Calc { .. }) => self.calc_entries(),
            Some(Popover::Columns) => {
                let shown: Vec<&str> = self
                    .shown
                    .columns
                    .iter()
                    .map(|c| c.property_id.as_str())
                    .collect();
                let title = fields.title_property().map(|p| p.id.as_str());
                let mut out = vec![Entry::heading("Properties")];
                out.extend(
                    ops::all_columns(fields, view)
                        .into_iter()
                        .filter(|c| Some(c.property_id.as_str()) != title)
                        .filter_map(|c| {
                            let p = fields.property(&c.property_id)?;
                            Some(
                                Entry::act(p.name.clone(), Action::ToggleColumn(p.id.clone()))
                                    .icon(type_icon(&p.kind))
                                    .checked(shown.contains(&p.id.as_str())),
                            )
                        }),
                );
                out
            }
            Some(Popover::Picker { cell }) => {
                let Some((row, p)) = self.cell_target(*cell) else {
                    return Vec::new();
                };
                let value = self.rows[row].value(&p.id);
                match p.kind {
                    PropertyType::Select => {
                        let current = value.and_then(Value::option_id);
                        let mut out: Vec<Entry> = p
                            .options()
                            .iter()
                            .map(|o| {
                                Entry::act(o.name.clone(), Action::SetSelect(Some(o.id.clone())))
                                    .chip(o.color.clone())
                                    .checked(current == Some(o.id.as_str()))
                            })
                            .collect();
                        if current.is_some() {
                            out.push(Entry::act("Clear", Action::SetSelect(None)).icon(I::X));
                        }
                        out
                    }
                    PropertyType::MultiSelect => {
                        let current = value.and_then(Value::option_ids).unwrap_or(&[]);
                        let mut out: Vec<Entry> = p
                            .options()
                            .iter()
                            .map(|o| {
                                Entry::act(o.name.clone(), Action::ToggleOption(o.id.clone()))
                                    .chip(o.color.clone())
                                    .checked(current.contains(&o.id))
                            })
                            .collect();
                        if !current.is_empty() {
                            out.push(Entry::act("Clear", Action::ClearValue).icon(I::X));
                        }
                        out
                    }
                    PropertyType::Person => {
                        let current = value.and_then(Value::as_people).unwrap_or(&[]);
                        let kinds = p.person_kinds();
                        let mut out: Vec<Entry> = self
                            .host
                            .people(p)
                            .into_iter()
                            .filter(|person| person.kind().is_some_and(|k| kinds.contains(&k)))
                            .map(|person| {
                                let on = current.contains(&person);
                                Entry::act(
                                    person_label(self.host.as_ref(), &person),
                                    Action::TogglePerson(person),
                                )
                                .icon(I::User)
                                .checked(on)
                            })
                            .collect();
                        if !current.is_empty() {
                            out.push(Entry::act("Clear", Action::ClearValue).icon(I::X));
                        }
                        out
                    }
                    PropertyType::Relation => {
                        let current = value.and_then(Value::as_ids).unwrap_or(&[]);
                        let Some(target) = p.relation() else {
                            return Vec::new();
                        };
                        let mut out: Vec<Entry> = self
                            .host
                            .relation_rows(target.database_id)
                            .into_iter()
                            .map(|r| {
                                let on = current.contains(&r.id);
                                let title = if r.title.is_empty() {
                                    "Untitled".into()
                                } else {
                                    r.title
                                };
                                Entry::act(title, Action::ToggleRelation(r.id))
                                    .icon(I::FileText)
                                    .checked(on)
                            })
                            .collect();
                        if !current.is_empty() {
                            out.push(Entry::act("Clear", Action::ClearValue).icon(I::X));
                        }
                        out
                    }
                    _ => Vec::new(),
                }
            }
            _ => Vec::new(),
        }
    }

    /// The header menu's filter choices for a property.
    fn filter_entries(&self, p: &Property, view: &View) -> Vec<Entry> {
        use gpui_core::IconName as I;
        let current: Vec<&Condition> = ops::top_conditions(view)
            .into_iter()
            .filter(|c| c.property_id == p.id)
            .collect();
        let is = |op: &FilterOp, value: &serde_json::Value| {
            current.iter().any(|c| c.op == *op && c.value == *value)
        };
        let cond = |op: FilterOp, value: serde_json::Value| Condition {
            property_id: p.id.clone(),
            op,
            value,
        };
        let pick = |label: String, op: FilterOp, value: serde_json::Value| {
            let on = is(&op, &value);
            Entry::act(label, Action::Filter(cond(op, value))).checked(on)
        };
        let mut out = Vec::new();
        match &p.kind {
            PropertyType::Select | PropertyType::MultiSelect => {
                let op = if p.kind == PropertyType::Select {
                    FilterOp::Is
                } else {
                    FilterOp::Contains
                };
                for o in p.options() {
                    out.push(
                        pick(o.name.clone(), op.clone(), o.id.clone().into()).chip(o.color.clone()),
                    );
                }
            }
            PropertyType::Checkbox => {
                out.push(pick(
                    "Checked".into(),
                    FilterOp::IsChecked,
                    serde_json::Value::Null,
                ));
                out.push(pick(
                    "Unchecked".into(),
                    FilterOp::IsNotChecked,
                    serde_json::Value::Null,
                ));
            }
            PropertyType::Person => {
                let kinds = p.person_kinds();
                for person in self.host.people(p) {
                    if !person.kind().is_some_and(|k| kinds.contains(&k)) {
                        continue;
                    }
                    let Some(key) = person.key() else { continue };
                    out.push(
                        pick(
                            person_label(self.host.as_ref(), &person),
                            FilterOp::Contains,
                            key.into(),
                        )
                        .icon(I::User),
                    );
                }
            }
            PropertyType::CreatedBy | PropertyType::EditedBy => {
                for person in self.host.people(p) {
                    let Some(key) = person.key() else { continue };
                    out.push(
                        pick(
                            person_label(self.host.as_ref(), &person),
                            FilterOp::Is,
                            key.into(),
                        )
                        .icon(I::User),
                    );
                }
            }
            PropertyType::Relation => {
                if let Some(t) = p.relation() {
                    for r in self.host.relation_rows(t.database_id).into_iter().take(50) {
                        let title = if r.title.is_empty() {
                            "Untitled".into()
                        } else {
                            r.title
                        };
                        out.push(pick(title, FilterOp::Contains, r.id.into()).icon(I::FileText));
                    }
                }
            }
            PropertyType::Date | PropertyType::CreatedTime | PropertyType::EditedTime => {
                for (label, w) in [
                    ("Today", "today"),
                    ("Past week", "pastWeek"),
                    ("Past month", "pastMonth"),
                    ("Next week", "nextWeek"),
                    ("Next month", "nextMonth"),
                ] {
                    out.push(pick(label.into(), FilterOp::Within, w.into()).icon(I::Calendar));
                }
            }
            PropertyType::Number => {
                for (label, op) in [
                    ("Equals…", FilterOp::Is),
                    ("Greater than…", FilterOp::Greater),
                    ("Less than…", FilterOp::Less),
                ] {
                    let on = current.iter().any(|c| c.op == op);
                    out.push(Entry::act(label, Action::FilterText(op)).checked(on));
                }
            }
            k if k.is_text() => {
                let on = current.iter().any(|c| c.op == FilterOp::Contains);
                out.push(
                    Entry::act("Contains…", Action::FilterText(FilterOp::Contains)).checked(on),
                );
            }
            _ => {}
        }
        if !matches!(
            p.kind,
            PropertyType::Checkbox | PropertyType::CreatedTime | PropertyType::EditedTime
        ) {
            out.push(pick(
                "Is empty".into(),
                FilterOp::IsEmpty,
                serde_json::Value::Null,
            ));
            out.push(pick(
                "Is not empty".into(),
                FilterOp::IsNotEmpty,
                serde_json::Value::Null,
            ));
        }
        out
    }

    /// Does what a menu entry says. `add` is a shift-click (a sort added to the others).
    pub(crate) fn activate(
        &mut self,
        action: Action,
        add: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ensure();
        if let Action::Prop(a) = action {
            self.activate_property(a, window, cx);
            return;
        }
        if let Action::View(a) = action {
            self.activate_view(a, window, cx);
            return;
        }
        if let Action::Row(a) = action {
            self.activate_row(a, window, cx);
            return;
        }
        if let Action::Calc(c) = action {
            self.activate_calc(c, window, cx);
            return;
        }
        let Some(fields) = self.database.database.clone() else {
            return;
        };
        let Some(view) = self.shown.view.clone() else {
            return;
        };
        let limits = self.host.limits();
        let header_property = match &self.popover {
            Some(Popover::Header { property_id }) => self
                .column_of(property_id)
                .map(|column| (column, property_id.clone())),
            _ => None,
        };
        let close = !matches!(
            action,
            Action::ToggleOption(_)
                | Action::TogglePerson(_)
                | Action::ToggleRelation(_)
                | Action::ToggleColumn(_)
        );
        match action {
            Action::Sort(direction) => {
                if let Some((_, pid)) = &header_property {
                    // A sort added past the limit is refused, the menu staying open to say so.
                    if add
                        && view.sort.len() >= limits.sorts
                        && !view.sort.iter().any(|s| s.property_id == *pid)
                    {
                        self.refuse(
                            format!("A view sorts by at most {} properties", limits.sorts),
                            cx,
                        );
                        return;
                    }
                    let v = ops::sort(&view, pid, direction, add, &limits);
                    self.change_view(v, window, cx);
                }
            }
            Action::Unsort => {
                if let Some((_, pid)) = &header_property {
                    self.change_view(ops::unsort(&view, pid), window, cx);
                }
            }
            Action::Filter(c) => self.change_view(ops::filter(&view, c), window, cx),
            Action::ClearFilter => {
                if let Some((_, pid)) = &header_property {
                    self.change_view(ops::clear_filter(&view, pid), window, cx);
                }
            }
            Action::FilterText(op) => {
                if let Some((column, pid)) = header_property.clone()
                    && let Some(p) = fields.property(&pid)
                {
                    let current = ops::top_conditions(&view)
                        .into_iter()
                        .find(|c| c.property_id == pid && c.op == op)
                        .map(|c| match &c.value {
                            serde_json::Value::String(s) => s.clone(),
                            serde_json::Value::Number(n) => n.to_string(),
                            _ => String::new(),
                        })
                        .unwrap_or_default();
                    let bounds = self.header_bounds(column);
                    self.begin_text(
                        TextTarget::Filter {
                            property_id: pid,
                            op,
                        },
                        p,
                        current,
                        bounds,
                        window,
                        cx,
                    );
                }
            }
            Action::Hide => {
                if let Some((_, pid)) = &header_property {
                    self.change_view(ops::set_hidden(&fields, &view, pid, true), window, cx);
                }
            }
            Action::MoveBy(d) => {
                if let Some((column, pid)) = &header_property {
                    let to = (*column as isize + d).max(1) as usize;
                    self.change_view(ops::move_column(&fields, &view, pid, to), window, cx);
                }
            }
            Action::ToggleColumn(pid) if pid.is_empty() => {
                self.open_columns_menu(cx);
                return;
            }
            Action::ToggleColumn(pid) => {
                let shown = self.shown.columns.iter().any(|c| c.property_id == pid);
                self.change_view(ops::set_hidden(&fields, &view, &pid, shown), window, cx);
            }
            Action::SetSelect(id) => self.set_value(id.map(Value::text), window, cx),
            Action::ToggleOption(id) => {
                if let Some((row, p)) = self.picker_target() {
                    let mut ids = self.rows[row]
                        .value(&p.id)
                        .and_then(Value::option_ids)
                        .unwrap_or(&[])
                        .to_vec();
                    match ids.iter().position(|x| *x == id) {
                        Some(i) => {
                            ids.remove(i);
                        }
                        None => ids.push(id),
                    }
                    let value = (!ids.is_empty()).then(|| limits.clamp_value(&p, Value::Ids(ids)));
                    self.set_value(value, window, cx);
                }
            }
            Action::TogglePerson(person) => {
                if let Some((row, p)) = self.picker_target() {
                    let mut people = self.rows[row]
                        .value(&p.id)
                        .and_then(Value::as_people)
                        .unwrap_or(&[])
                        .to_vec();
                    match people.iter().position(|x| *x == person) {
                        Some(i) => {
                            people.remove(i);
                        }
                        None => people.push(person),
                    }
                    let value =
                        (!people.is_empty()).then(|| limits.clamp_value(&p, Value::People(people)));
                    self.set_value(value, window, cx);
                }
            }
            Action::ToggleRelation(id) => {
                if let Some((row, p)) = self.picker_target() {
                    let r = &self.rows[row];
                    let old = r
                        .value(&p.id)
                        .and_then(Value::as_ids)
                        .unwrap_or(&[])
                        .to_vec();
                    let mut new = old.clone();
                    match new.iter().position(|x| *x == id) {
                        Some(i) => {
                            new.remove(i);
                        }
                        None => new.push(id),
                    }
                    let edits = relation::set(&r.id, &p, &old, new, &limits);
                    self.emit_edits(edits, window, cx);
                }
            }
            Action::Prop(_) | Action::View(_) | Action::Row(_) | Action::Calc(_) => {}
            Action::ClearValue => {
                if let Some((row, p)) = self.picker_target() {
                    if p.kind == PropertyType::Relation {
                        let r = &self.rows[row];
                        let old = r
                            .value(&p.id)
                            .and_then(Value::as_ids)
                            .unwrap_or(&[])
                            .to_vec();
                        let edits = relation::set(&r.id, &p, &old, Vec::new(), &limits);
                        self.emit_edits(edits, window, cx);
                    } else {
                        self.set_value(None, window, cx);
                    }
                }
            }
        }
        // A menu that stays open (a multi-select's, the columns') keeps the keyboard; one that
        // closes gives it back to the table.
        if close {
            self.popover = None;
            window.focus(&self.focus, cx);
        }
        cx.notify();
    }

    /// Whether the open picker can make an option from what's typed: a select's or a
    /// multi-select's, with room for another option.
    pub(crate) fn can_create_option(&self) -> bool {
        self.picker_target().is_some_and(|(_, p)| {
            matches!(p.kind, PropertyType::Select | PropertyType::MultiSelect)
                && p.options().len() < self.host.limits().options
        })
    }

    /// Makes an option named `name` in the open picker's property and gives it to the cell,
    /// in one batch: a `propertyChanged` with the whole property, the new option (a new id, the
    /// default colour) after the others, then the row's `rowValueChanged`, the option for a
    /// select and added to the others for a multi-select. A select's picker closes; a
    /// multi-select's stays open, its search emptied, with the new option checked once the host
    /// hands the docs back.
    pub(crate) fn create_option(
        &mut self,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_create_option() {
            return;
        }
        let Some((row, p)) = self.picker_target() else {
            return;
        };
        let limits = self.host.limits();
        let name = crate::model::truncate_chars(name.trim(), limits.option_name_chars);
        if name.is_empty() {
            return;
        }
        let option = SelectOption {
            id: new_id(),
            name: name.to_owned(),
            color: Color::Default,
        };
        let mut property = p.clone();
        property
            .config
            .options
            .get_or_insert_with(Vec::new)
            .push(option.clone());
        let r = &self.rows[row];
        let value = match p.kind {
            PropertyType::Select => Value::text(option.id),
            _ => {
                let mut ids = r
                    .value(&p.id)
                    .and_then(Value::option_ids)
                    .unwrap_or(&[])
                    .to_vec();
                ids.push(option.id);
                limits.clamp_value(&property, Value::Ids(ids))
            }
        };
        let edits = vec![
            Edit::PropertyChanged {
                database_id: self.database_id(),
                property,
            },
            Edit::RowValueChanged {
                row_id: r.id.clone(),
                property_id: p.id.clone(),
                value: Some(value),
            },
        ];
        let select = p.kind == PropertyType::Select;
        self.emit_edits(edits, window, cx);
        if select {
            self.popover = None;
            window.focus(&self.focus, cx);
        } else {
            // A new picker: what was typed is gone, and it opens on the checked options.
            self.menu_serial += 1;
        }
        cx.notify();
    }

    fn picker_target(&self) -> Option<(usize, Property)> {
        let cell = match &self.popover {
            Some(Popover::Picker { cell } | Popover::Date { cell }) => *cell,
            _ => return None,
        };
        self.cell_target(cell).map(|(r, p)| (r, p.clone()))
    }

    /// Sets the open picker's cell to `value` (None clears it), if that changes it.
    fn set_value(&mut self, value: Option<Value>, window: &mut Window, cx: &mut Context<Self>) {
        let Some((row, p)) = self.picker_target() else {
            return;
        };
        let r = &self.rows[row];
        if r.value(&p.id) == value.as_ref() {
            return;
        }
        let edit = Edit::RowValueChanged {
            row_id: r.id.clone(),
            property_id: p.id.clone(),
            value,
        };
        self.emit_edits(vec![edit], window, cx);
    }

    /// The month's choice: a day (keeping a time of day the value had, and a range's length),
    /// or none to clear. A new value gets the property's `includeTime`: with it, the day's
    /// start in the viewer's zone.
    pub(crate) fn pick_day(
        &mut self,
        day: Option<i64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((row, p)) = self.picker_target() else {
            return;
        };
        let offset = i64::from(self.host.utc_offset_minutes()) * 60_000;
        let value = day.map(|day| {
            let old = self.rows[row].value(&p.id).and_then(Value::as_date);
            match old {
                None if p.config.include_time == Some(true) => {
                    Value::Date(crate::model::DateValue {
                        start: day * crate::engine::DAY_MS - offset,
                        end: None,
                        include_time: true,
                    })
                }
                _ => ops::date_on(old, day, offset),
            }
        });
        self.set_value(value, window, cx);
        self.popover = None;
        self.date = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    // --- Columns ----------------------------------------------------------------------------

    /// Where a dragged header would land: the column whose place the pointer is over (never
    /// before the title's).
    pub(crate) fn drop_index(&self, x: Pixels) -> usize {
        let n = self.shown.columns.len();
        let mut left = self.column_left(1);
        for c in 1..n {
            let w = self.column_px(c);
            if x < left + w / 2. {
                return c;
            }
            left += w;
        }
        n.saturating_sub(1).max(1)
    }

    /// Ends a column drag: a resize or a move becomes a `viewChanged`; a header pressed and
    /// let go without moving opens its menu.
    pub(crate) fn end_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(drag) = self.drag.take() else {
            return;
        };
        self.ensure();
        let (Some(fields), Some(view)) = (self.database.database.clone(), self.shown.view.clone())
        else {
            return;
        };
        match drag {
            Drag::Resize {
                column,
                width,
                start,
                ..
            } => {
                if (width - start).abs() >= 0.5
                    && let Some(c) = self.shown.columns.get(column)
                {
                    let pid = c.property_id.clone();
                    self.change_view(ops::resize(&fields, &view, &pid, width), window, cx);
                }
            }
            Drag::Header {
                column, x, moved, ..
            } => {
                if moved {
                    let to = self.drop_index(x);
                    if to != column
                        && let Some(c) = self.shown.columns.get(column)
                    {
                        let pid = c.property_id.clone();
                        self.change_view(ops::move_column(&fields, &view, &pid, to), window, cx);
                    }
                } else {
                    self.open_header_menu(column, cx);
                }
            }
        }
        cx.notify();
    }
}

/// A day (days since 1970-01-01) as gpui-core's month takes it.
pub(crate) fn naive_day(days: i64) -> chrono::NaiveDate {
    let (y, m, d) = time::civil_from_days(days);
    i32::try_from(y)
        .ok()
        .and_then(|y| chrono::NaiveDate::from_ymd_opt(y, m, d))
        .unwrap_or_default()
}

/// A day picked in gpui-core's month, as days since 1970-01-01.
fn day_of(date: chrono::NaiveDate) -> i64 {
    use chrono::Datelike as _;
    time::days_from_civil(i64::from(date.year()), date.month(), date.day())
}

/// What the on-screen keyboard offers for a type: capitals and suggestions for words, none of
/// that for numbers, links, addresses and phone numbers.
fn keyboard_for(kind: &PropertyType) -> TextInputConfiguration {
    let words = matches!(kind, PropertyType::Title | PropertyType::Text);
    TextInputConfiguration {
        autocorrect: words,
        autocapitalize: if words {
            Autocapitalize::Sentences
        } else {
            Autocapitalize::None
        },
        suggestions: words,
        // Return commits, a textarea's too (Shift-Enter breaks its line).
        input_action: TextInputAction::Done,
    }
}

/// The icon of a property type, for headers and menus.
pub(crate) fn type_icon(kind: &PropertyType) -> gpui_core::IconName {
    use gpui_core::IconName as I;
    match kind {
        PropertyType::Title => I::Type,
        PropertyType::Text => I::TextAlignStart,
        PropertyType::Number => I::Hash,
        PropertyType::Select => I::CircleChevronDown,
        PropertyType::MultiSelect => I::Tags,
        PropertyType::Date => I::Calendar,
        PropertyType::Checkbox => I::SquareCheck,
        PropertyType::Url => I::Link,
        PropertyType::Email => I::AtSign,
        PropertyType::Phone => I::Phone,
        PropertyType::Person => I::User,
        PropertyType::Relation => I::ArrowUpRight,
        PropertyType::CreatedTime | PropertyType::EditedTime => I::Clock,
        PropertyType::CreatedBy => I::UserPlus,
        PropertyType::EditedBy => I::UserPen,
        PropertyType::Other(_) => I::Circle,
    }
}
