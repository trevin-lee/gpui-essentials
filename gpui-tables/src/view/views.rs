//! Changing a database's views, as in Notion: a bar of the views as tabs, "+" to add one (a name
//! and a type), and each view's menu to rename, duplicate, change type, move and delete it, with
//! its settings (grouping, a date's buckets, the properties on cards and lines, a table's shown
//! properties).
//!
//! Every change is a batch of edits for the host: `viewCreated` with the whole new view,
//! `viewChanged` with the whole changed view (several, when reordering renumbers them), and
//! `viewDeleted`. The table shows each change at once, as it does a header's, and keeps it until
//! the host hands it new docs. The crate keeps which view is shown; the host may hear of it
//! through [`TableHost::view_shown`](super::TableHost::view_shown).
//!
//! The menus are gpui-core's `PopupMenu` in a `Popover` hanging from the tab (or the bar's
//! settings button), a page at a time, as the property menus are: gpui-core has no submenus.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui::{App, Bounds, Context, FocusHandle, Pixels, ScrollHandle, Window};

use super::ops;
use super::properties::FieldFor;
use super::table::{Action, Entry, Popover, TableView, type_icon};
use crate::model::{DateBucket, Edit, PropertyType, View, ViewType, new_id, truncate_chars};

/// The types a view can be made as or changed to (galleries aren't drawn in v1).
pub(crate) const VIEW_TYPES: &[ViewType] = &[ViewType::Table, ViewType::Board, ViewType::List];

/// A page of a view's menu.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ViewPage {
    /// Rename, change type, settings, duplicate, move, delete.
    Menu,
    /// Its name, typed.
    Rename,
    /// Table, board or list.
    Type,
    /// A board needs a property to group by, and the view has none that can: which one.
    BoardGroup,
    /// Deleting it, confirmed.
    Delete,
    /// Its settings: type, grouping, a date's buckets, the properties shown, sort and filter.
    Settings,
    /// What it groups by.
    Group,
    /// How a date grouping buckets.
    GroupDate,
    /// The properties a board's cards or a list's lines show, in order.
    Properties,
    /// One of those: hide it, or move it up or down.
    Property(String),
}

impl ViewPage {
    pub(crate) fn is_field(&self) -> bool {
        *self == Self::Rename
    }
}

/// What a view's menu hangs from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ViewAnchor {
    /// The view's tab.
    Tab,
    /// The bar's settings button (or its sort and filter summary).
    Settings,
}

/// A page of a view's menu, which [`TableView::open_view_menu`] opens as the bar does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMenu {
    /// Rename, change type, settings, duplicate, move left or right, delete.
    Menu,
    /// Its name, in a field.
    Rename,
    /// Table, board or list.
    Type,
    /// Its settings.
    Settings,
    /// What it groups by.
    GroupBy,
    /// How it groups by a date (only while it does).
    DateGrouping,
    /// The properties its cards or lines show (a table's columns: the Properties menu).
    Properties,
    /// Deleting it, confirmed.
    Delete,
}

impl From<ViewMenu> for ViewPage {
    fn from(m: ViewMenu) -> Self {
        match m {
            ViewMenu::Menu => Self::Menu,
            ViewMenu::Rename => Self::Rename,
            ViewMenu::Type => Self::Type,
            ViewMenu::Settings => Self::Settings,
            ViewMenu::GroupBy => Self::Group,
            ViewMenu::DateGrouping => Self::GroupDate,
            ViewMenu::Properties => Self::Properties,
            ViewMenu::Delete => Self::Delete,
        }
    }
}

/// What an entry of a view's menu does.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ViewAction {
    Page(ViewPage),
    Back,
    /// A new view of this type, named as typed (the + menu).
    Add(ViewType),
    /// The new board grouped by this property (after the + menu asked).
    AddBoard(String),
    SetType(ViewType),
    /// Changed to a board grouped by this property.
    Board(String),
    Duplicate,
    /// Moved this many places (left is negative).
    Move(isize),
    /// Deleted (confirmed).
    Delete,
    GroupBy(Option<String>),
    GroupDate(DateBucket),
    /// Shown on cards or lines, or not.
    ToggleCard(String),
    /// The card property page's property moved up or down.
    MoveCard(isize),
    /// A table's columns: the Properties menu.
    Columns,
    ClearSorts,
    ClearFilter,
}

/// A board being added that waits to be told what to group by: its name, as typed.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NewView {
    pub name: String,
}

/// The bar's state: its controls' focus, where they were drawn, and its scroll.
#[derive(Default)]
pub(crate) struct Bar {
    pub on: bool,
    /// Each tab's focus, by view id.
    pub tabs: HashMap<String, FocusHandle>,
    pub add: Option<FocusHandle>,
    pub settings: Option<FocusHandle>,
    /// Where each control was drawn, for the menus to hang from.
    pub geometry: Rc<RefCell<BarGeometry>>,
    /// What had the keyboard when a view's menu opened from the bar, to have it back.
    pub return_to: Option<FocusHandle>,
    /// The tabs scroll sideways when they don't fit.
    pub scroll: ScrollHandle,
    /// The view whose tab was last scrolled into view.
    pub scrolled_to: Option<String>,
}

/// Where the bar's controls were last drawn, in the window.
#[derive(Clone, Debug, Default)]
pub(crate) struct BarGeometry {
    pub tabs: Vec<(String, Bounds<Pixels>)>,
    pub add: Option<Bounds<Pixels>>,
    pub settings: Option<Bounds<Pixels>>,
}

/// A date bucket's name, as the settings list it.
pub(crate) fn bucket_label(b: &DateBucket) -> &str {
    match b {
        DateBucket::Relative => "Relative",
        DateBucket::Day => "Day",
        DateBucket::Week => "Week",
        DateBucket::Month => "Month",
        DateBucket::Year => "Year",
        DateBucket::Other(s) => s,
    }
}

/// The icon of a view's type, for its tab and the menus.
pub(crate) fn view_icon(kind: &ViewType) -> gpui_core::IconName {
    use gpui_core::IconName as I;
    match kind {
        ViewType::Board => I::SquareKanban,
        ViewType::List => I::List,
        ViewType::Gallery => I::LayoutGrid,
        ViewType::Table | ViewType::Other(_) => I::Table,
    }
}

/// A menu entry's hint cut to a few words: gpui-core draws a hint (its shortcut text) whole,
/// so a long property name would crowd out the entry's label.
fn hint(text: &str) -> String {
    const MAX: usize = 16;
    if text.chars().count() <= MAX {
        return text.to_owned();
    }
    let cut: String = text.chars().take(MAX - 1).collect();
    format!("{}…", cut.trim_end())
}

fn untitled(title: &str) -> String {
    if title.trim().is_empty() {
        "Untitled".into()
    } else {
        title.to_owned()
    }
}

/// Why a board can't be made: nothing to group by.
const NO_GROUP: &str = "A board groups cards by a select, multi-select, checkbox, person or \
                        date property, and there's none yet: add one in a table first";

impl TableView {
    // --- The bar ------------------------------------------------------------------------------

    /// Draws the bar of views over the table, board or list: the database's views as tabs
    /// (each with its type's icon, the one shown marked), "+" to add a view, and the view's
    /// settings. Off by default, for a host that draws its own tabs and calls
    /// [`TableView::set_view`].
    pub fn with_view_bar(mut self, on: bool) -> Self {
        self.bar.on = on;
        self
    }

    /// Shows or hides the bar of views.
    pub fn set_view_bar(&mut self, on: bool, cx: &mut Context<Self>) {
        self.bar.on = on;
        cx.notify();
    }

    /// Whether the bar of views is drawn.
    pub fn has_view_bar(&self) -> bool {
        self.bar.on
    }

    /// Gives the keyboard to the shown view's tab in the bar (Up from the top line does too):
    /// Left and Right move along the tabs, Enter shows one (or opens the shown one's menu), ⌥↓
    /// opens its menu, Down or Escape goes back to the view.
    pub fn focus_view_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.bar.on {
            return;
        }
        let id = self.view().map(|v| v.id.clone());
        if let Some(h) = id.and_then(|id| self.bar.tabs.get(&id)).cloned() {
            window.focus(&h, cx);
        }
        cx.notify();
    }

    /// Opens the + menu, as pressing the bar's + does: a field for the new view's name over the
    /// types it can have (table, board, list). Typing names it; Up and Down move through the
    /// types; Enter, or a press on a type, makes it and shows it.
    pub fn open_add_view_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.remember_bar_focus(window, cx);
        self.open_add_view(String::new(), window, cx);
    }

    /// Opens a page of a view's menu, hanging from its tab, as pressing the shown view's tab
    /// (or right-clicking any) does: to rename it, change its type, see its settings, and so
    /// on. A page that doesn't apply (a date's buckets for a view not grouped by a date)
    /// doesn't open.
    pub fn open_view_menu(
        &mut self,
        view_id: &str,
        menu: ViewMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.remember_bar_focus(window, cx);
        let page = ViewPage::from(menu);
        if page == ViewPage::Properties && self.is_table_like(view_id) {
            if self.view().is_some_and(|v| v.id == view_id) {
                self.date = None;
                self.close_property_menu();
                self.open_columns_menu(cx);
            }
            return;
        }
        self.open_view_page(view_id.to_owned(), page, ViewAnchor::Tab, window, cx);
    }

    /// The views as the bar shows them, in order.
    pub(crate) fn bar_views(&self) -> Vec<View> {
        self.database
            .database
            .as_ref()
            .map(|f| f.views_in_order().into_iter().cloned().collect())
            .unwrap_or_default()
    }

    fn view_by_id(&self, id: &str) -> Option<&View> {
        self.database.database.as_ref()?.view(id)
    }

    fn is_table_like(&self, view_id: &str) -> bool {
        self.view_by_id(view_id)
            .is_some_and(|v| !matches!(v.kind, ViewType::Board | ViewType::List))
    }

    /// Notes which of the bar's controls has the keyboard, so a menu opened from it gives it
    /// back there.
    pub(crate) fn remember_bar_focus(&mut self, window: &Window, _: &App) {
        let handles = self
            .bar
            .tabs
            .values()
            .chain(self.bar.add.iter())
            .chain(self.bar.settings.iter());
        self.bar.return_to = handles.into_iter().find(|h| h.is_focused(window)).cloned();
    }

    /// Shows another view because the person asked (a tab, a new view, a delete), and tells the
    /// host.
    pub(crate) fn show_view(
        &mut self,
        view_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.view().map(|v| v.id.as_str()) == Some(view_id.as_str()) {
            return;
        }
        self.set_view(view_id.clone(), cx);
        let host = self.host.clone();
        window.defer(cx, move |window, cx| host.view_shown(&view_id, window, cx));
    }

    /// A tab pressed: another view is shown; the shown view's menu opens.
    pub(crate) fn press_tab(
        &mut self,
        view_id: String,
        keyboard: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.view().is_some_and(|v| v.id == view_id) {
            self.open_view_menu(&view_id, ViewMenu::Menu, window, cx);
            return;
        }
        self.show_view(view_id.clone(), window, cx);
        // From the keyboard it stays on the tab; a pointer's press leaves it with the view.
        if keyboard {
            if let Some(h) = self.bar.tabs.get(&view_id).cloned() {
                window.focus(&h, cx);
            }
        } else {
            window.focus(&self.focus, cx);
        }
    }

    /// Left or Right in the bar: the tab before or after the focused one.
    pub(crate) fn bar_step(&mut self, by: isize, window: &mut Window, cx: &mut Context<Self>) {
        let views = self.bar_views();
        let at = views.iter().position(|v| {
            self.bar
                .tabs
                .get(&v.id)
                .is_some_and(|h| h.is_focused(window))
        });
        let Some(at) = at else { return };
        let to = (at as isize + by).clamp(0, views.len() as isize - 1) as usize;
        if let Some(h) = self.bar.tabs.get(&views[to].id).cloned() {
            window.focus(&h, cx);
            cx.notify();
        }
    }

    /// ⌥↓ in the bar: the focused tab's menu.
    pub(crate) fn bar_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let id = self
            .bar
            .tabs
            .iter()
            .find(|(_, h)| h.is_focused(window))
            .map(|(id, _)| id.clone());
        if let Some(id) = id {
            self.open_view_menu(&id, ViewMenu::Menu, window, cx);
        }
    }

    /// Down or Escape in the bar: back to the view.
    pub(crate) fn leave_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Up from the top line (a board's top card) to the bar, when it's drawn and nothing's
    /// open. Whether it went.
    pub(crate) fn up_to_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.popover.is_some() || self.is_editing() || !self.at_top() {
            return false;
        }
        self.focus_view_bar(window, cx);
        true
    }

    /// Whether Up goes from the view to the bar: the bar's drawn and the top line (a board's
    /// top card) has the focus.
    pub(crate) fn at_top(&mut self) -> bool {
        if !self.bar.on {
            return false;
        }
        self.ensure();
        if self.shown.board.is_some() {
            return self
                .board_spot()
                .and_then(|s| self.spot_place(s))
                .is_some_and(|(_, place)| place == 0);
        }
        self.cell.is_some_and(|c| c.item == 0)
    }

    // --- Pages ----------------------------------------------------------------------------------

    fn open_add_view(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        self.ensure();
        self.date = None;
        self.close_property_menu();
        self.menu_serial += 1;
        let kind = self.view_kind.0;
        self.view_kind = (kind.min(VIEW_TYPES.len() - 1), self.view_kind.1 + 1);
        self.popover = Some(Popover::AddView);
        let max = self.host.limits().name_chars;
        self.open_field(FieldFor::NewView, name, max, "View name", window, cx);
        cx.notify();
    }

    pub(crate) fn open_view_page(
        &mut self,
        view_id: String,
        page: ViewPage,
        anchor: ViewAnchor,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ensure();
        let Some(view) = self.view_by_id(&view_id).cloned() else {
            return;
        };
        if page == ViewPage::GroupDate && !self.groups_by_date(&view) {
            return;
        }
        self.date = None;
        self.close_property_menu();
        self.menu_serial += 1;
        if page == ViewPage::Rename {
            let max = self.host.limits().name_chars;
            self.open_field(
                FieldFor::RenameView(view_id.clone()),
                view.name.clone(),
                max,
                "View name",
                window,
                cx,
            );
        } else {
            // A menu's page takes the keyboard when it first draws (gpui-core's PopupMenu).
            window.focus(&self.focus, cx);
        }
        self.popover = Some(Popover::View {
            view_id,
            page,
            anchor,
        });
        cx.notify();
    }

    fn groups_by_date(&self, view: &View) -> bool {
        let Some(fields) = self.database.database.as_ref() else {
            return false;
        };
        view.group_by
            .as_deref()
            .and_then(|g| fields.property(g))
            .is_some_and(|p| p.kind.is_date())
    }

    /// The view the open view menu is about.
    pub(crate) fn menu_view(&self) -> Option<&View> {
        match &self.popover {
            Some(Popover::View { view_id, .. }) => self.view_by_id(view_id),
            _ => None,
        }
    }

    /// Closes a view's menu, the keyboard going back where it was (a tab), else to the view.
    pub(crate) fn close_view_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.popover = None;
        self.close_property_menu();
        match self.bar.return_to.take() {
            Some(h) => window.focus(&h, cx),
            None => window.focus(&self.focus, cx),
        }
        cx.notify();
    }

    /// Up or Down in the + menu's field: the type above or below.
    pub(crate) fn move_view_kind(&mut self, by: isize, cx: &mut Context<Self>) {
        let n = VIEW_TYPES.len() as isize;
        let at = (self.view_kind.0 as isize + by).rem_euclid(n) as usize;
        self.view_kind = (at, self.view_kind.1 + 1);
        cx.notify();
    }

    /// Why a view's menu can't do something now: a note it refused with, or a limit reached.
    pub(crate) fn view_note(&self, cx: &App) -> Option<String> {
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
            Some(Popover::AddView) if fields.views.len() >= limits.views => {
                Some(self.full_views_note())
            }
            Some(Popover::AddView | Popover::View { .. }) if at_max => Some(format!(
                "Names are at most {} characters",
                self.field.as_ref().map_or(0, |f| f.max_chars)
            )),
            Some(Popover::NewBoard) => Some("Group the board by".into()),
            Some(Popover::View {
                page: ViewPage::BoardGroup,
                ..
            }) => Some("A board needs a property to group its cards by".into()),
            _ => None,
        }
    }

    fn full_views_note(&self) -> String {
        format!(
            "{} has {} views, the most it can hold",
            untitled(&self.database.title),
            self.host.limits().views
        )
    }

    // --- Entries ----------------------------------------------------------------------------

    /// The entries of a view's menu, or of the new board's group-by question.
    pub(crate) fn view_entries(&self) -> Vec<Entry> {
        use ViewAction as A;
        use gpui_core::IconName as I;
        let act = |label: &str, a: A| Entry::act(label, Action::View(a));
        let back = || act("Back", A::Back).icon(I::ArrowLeft);
        let Some(fields) = self.database.database.as_ref() else {
            return Vec::new();
        };
        if let Some(Popover::NewBoard) = &self.popover {
            let mut out = vec![Entry::heading("Group by")];
            for p in ops::groupable(fields) {
                out.push(act(&p.name, A::AddBoard(p.id.clone())).icon(type_icon(&p.kind)));
            }
            out.push(back());
            return out;
        }
        let Some(Popover::View { page, anchor, .. }) = &self.popover else {
            return Vec::new();
        };
        let Some(v) = self.menu_view() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        match page {
            ViewPage::Menu => {
                let views = fields.views_in_order();
                let at = views.iter().position(|x| x.id == v.id).unwrap_or(0);
                out.push(Entry::heading(v.name.clone()));
                out.push(act("Rename", A::Page(ViewPage::Rename)).icon(I::Pencil));
                out.push(
                    act("Change type", A::Page(ViewPage::Type))
                        .icon(view_icon(&v.kind))
                        .hint(ops::view_type_label(&v.kind)),
                );
                out.push(
                    act("View settings", A::Page(ViewPage::Settings)).icon(I::SlidersHorizontal),
                );
                out.push(act("Duplicate", A::Duplicate).icon(I::Copy));
                if at > 0 {
                    out.push(act("Move left", A::Move(-1)).icon(I::ArrowLeft));
                }
                if at + 1 < views.len() {
                    out.push(act("Move right", A::Move(1)).icon(I::ArrowRight));
                }
                out.push(act("Delete view", A::Page(ViewPage::Delete)).icon(I::Trash));
                return out;
            }
            ViewPage::Type => {
                out.push(Entry::heading("Type"));
                for k in VIEW_TYPES {
                    let on = *k == v.kind;
                    out.push(
                        act(ops::view_type_label(k), A::SetType(k.clone()))
                            .icon(view_icon(k))
                            .checked(on)
                            .highlighted(on),
                    );
                }
            }
            ViewPage::BoardGroup => {
                out.push(Entry::heading("Group by"));
                for p in ops::groupable(fields) {
                    out.push(act(&p.name, A::Board(p.id.clone())).icon(type_icon(&p.kind)));
                }
            }
            ViewPage::Delete => {
                out.push(Entry::heading(format!("Delete {}?", v.name)));
                out.push(act("Delete view", A::Delete).icon(I::Trash));
                out.push(act("Cancel", A::Back).icon(I::X).highlighted(true));
                return out;
            }
            ViewPage::Settings => {
                let group = v.group_by.as_deref().and_then(|g| fields.property(g));
                out.push(Entry::heading("View settings"));
                out.push(
                    act("Change type", A::Page(ViewPage::Type))
                        .icon(view_icon(&v.kind))
                        .hint(ops::view_type_label(&v.kind)),
                );
                out.push(
                    act("Group by", A::Page(ViewPage::Group))
                        .icon(I::Group)
                        .hint(hint(group.map_or("None", |p| p.name.as_str()))),
                );
                if group.is_some_and(|p| p.kind.is_date()) {
                    out.push(
                        act("Date grouping", A::Page(ViewPage::GroupDate))
                            .icon(I::CalendarDays)
                            .hint(bucket_label(
                                v.group_date.as_ref().unwrap_or(&DateBucket::Relative),
                            )),
                    );
                }
                match v.kind {
                    ViewType::Board | ViewType::List => {
                        let n = ops::card_properties(fields, v).len();
                        let label = if v.kind == ViewType::Board {
                            "Card properties"
                        } else {
                            "Properties"
                        };
                        out.push(
                            act(label, A::Page(ViewPage::Properties))
                                .icon(I::Rows3)
                                .hint(format!("{n} shown")),
                        );
                    }
                    _ => {
                        let n = ops::all_columns(fields, v)
                            .iter()
                            .filter(|c| !c.hidden)
                            .count();
                        out.push(
                            act("Properties", A::Columns)
                                .icon(I::Columns3)
                                .hint(format!("{n} shown")),
                        );
                    }
                }
                if !v.sort.is_empty() || v.filter.is_some() {
                    out.push(Entry::heading("Sort and filter"));
                }
                if !v.sort.is_empty() {
                    out.push(
                        act("Remove sorts", A::ClearSorts)
                            .icon(I::ArrowUpDown)
                            .hint(hint(&self.sort_summary(v))),
                    );
                }
                if v.filter.is_some() {
                    out.push(
                        act("Clear filter", A::ClearFilter)
                            .icon(I::FunnelX)
                            .hint(hint(&self.filter_summary(v))),
                    );
                }
                if *anchor == ViewAnchor::Settings {
                    return out;
                }
            }
            ViewPage::Group => {
                out.push(Entry::heading("Group by"));
                if v.kind != ViewType::Board {
                    let on = v.group_by.is_none();
                    out.push(
                        act("None", A::GroupBy(None))
                            .icon(I::X)
                            .checked(on)
                            .highlighted(on),
                    );
                }
                for p in ops::groupable(fields) {
                    let on = v.group_by.as_deref() == Some(p.id.as_str());
                    out.push(
                        act(&p.name, A::GroupBy(Some(p.id.clone())))
                            .icon(type_icon(&p.kind))
                            .checked(on)
                            .highlighted(on),
                    );
                }
            }
            ViewPage::GroupDate => {
                out.push(Entry::heading("Date grouping"));
                let current = v.group_date.clone().unwrap_or(DateBucket::Relative);
                for b in DateBucket::ALL {
                    let on = *b == current;
                    out.push(
                        act(bucket_label(b), A::GroupDate(b.clone()))
                            .checked(on)
                            .highlighted(on),
                    );
                }
            }
            ViewPage::Properties => {
                let shown = ops::card_properties(fields, v);
                out.push(Entry::heading("Shown"));
                for id in &shown {
                    if let Some(p) = fields.property(id) {
                        out.push(
                            act(&p.name, A::Page(ViewPage::Property(id.clone())))
                                .icon(type_icon(&p.kind))
                                .checked(true),
                        );
                    }
                }
                let hidden: Vec<_> = fields
                    .properties_in_order()
                    .into_iter()
                    .filter(|p| p.kind != PropertyType::Title && !shown.contains(&p.id))
                    .collect();
                if !hidden.is_empty() {
                    out.push(Entry::heading("Hidden"));
                }
                for p in hidden {
                    out.push(
                        act(&p.name, A::ToggleCard(p.id.clone()))
                            .icon(type_icon(&p.kind))
                            .checked(false),
                    );
                }
            }
            ViewPage::Property(pid) => {
                let shown = ops::card_properties(fields, v);
                let Some(i) = shown.iter().position(|x| x == pid) else {
                    out.push(back());
                    return out;
                };
                let name = fields.property(pid).map_or("", |p| p.name.as_str());
                out.push(Entry::heading(name.to_owned()));
                out.push(act("Hide", A::ToggleCard(pid.clone())).icon(I::EyeOff));
                if i > 0 {
                    out.push(act("Move up", A::MoveCard(-1)).icon(I::ArrowUp));
                }
                if i + 1 < shown.len() {
                    out.push(act("Move down", A::MoveCard(1)).icon(I::ArrowDown));
                }
            }
            ViewPage::Rename => {}
        }
        out.push(back());
        out
    }

    /// The view's sorts in a few words: "Name", "Stage, Name".
    pub(crate) fn sort_summary(&self, v: &View) -> String {
        let Some(fields) = self.database.database.as_ref() else {
            return String::new();
        };
        let names: Vec<&str> = v
            .sort
            .iter()
            .filter_map(|s| fields.property(&s.property_id).map(|p| p.name.as_str()))
            .collect();
        match names.as_slice() {
            [] => String::new(),
            [one] => (*one).to_owned(),
            [one, rest @ ..] => format!("{one} +{}", rest.len()),
        }
    }

    /// The view's filter in a few words: the property it tests, or how many rules.
    pub(crate) fn filter_summary(&self, v: &View) -> String {
        let Some(f) = &v.filter else {
            return String::new();
        };
        let conditions: Vec<&str> = f
            .walk()
            .filter_map(|(f, _)| match f {
                crate::model::Filter::Condition(c) => Some(c.property_id.as_str()),
                _ => None,
            })
            .collect();
        match conditions.as_slice() {
            [one] => self
                .database
                .database
                .as_ref()
                .and_then(|d| d.property(one))
                .map_or_else(|| "1 rule".into(), |p| p.name.clone()),
            many => format!("{} rules", many.len()),
        }
    }

    // --- Doing ------------------------------------------------------------------------------

    /// Does what a view menu's entry says.
    pub(crate) fn activate_view(
        &mut self,
        action: ViewAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(fields) = self.database.database.clone() else {
            return;
        };
        let limits = self.host.limits();
        let current = self.menu_view().cloned();
        let anchor = match &self.popover {
            Some(Popover::View { anchor, .. }) => *anchor,
            _ => ViewAnchor::Tab,
        };
        match action {
            ViewAction::Page(page) => {
                let Some(v) = current else { return };
                if page == ViewPage::Delete && fields.views.len() <= 1 {
                    self.refuse("A database keeps at least one view".into(), cx);
                    return;
                }
                self.open_view_page(v.id, page, anchor, window, cx);
            }
            ViewAction::Back => self.view_back(window, cx),
            ViewAction::Add(kind) => self.add_view(kind, window, cx),
            ViewAction::AddBoard(group) => {
                let name = self.new_view.take().map(|n| n.name).unwrap_or_default();
                self.create_view(ViewType::Board, name, Some(group), window, cx);
            }
            ViewAction::SetType(kind) => {
                let Some(v) = current else { return };
                if kind == v.kind {
                    self.view_back(window, cx);
                    return;
                }
                if kind == ViewType::Board {
                    match ops::board_group(&fields, Some(&v)) {
                        Some(g) => {
                            let changed = ops::retype_view(&v, kind, Some(g));
                            self.emit_views(vec![self.view_changed(changed)], None, window, cx);
                            self.close_view_menu(window, cx);
                        }
                        None if ops::groupable(&fields).is_empty() => {
                            self.refuse(NO_GROUP.into(), cx)
                        }
                        None => self.open_view_page(v.id, ViewPage::BoardGroup, anchor, window, cx),
                    }
                    return;
                }
                let changed = ops::retype_view(&v, kind, None);
                self.emit_views(vec![self.view_changed(changed)], None, window, cx);
                self.close_view_menu(window, cx);
            }
            ViewAction::Board(group) => {
                let Some(v) = current else { return };
                let changed = ops::retype_view(&v, ViewType::Board, Some(group));
                self.emit_views(vec![self.view_changed(changed)], None, window, cx);
                self.close_view_menu(window, cx);
            }
            ViewAction::Duplicate => {
                let Some(v) = current else { return };
                if fields.views.len() >= limits.views {
                    self.refuse(self.full_views_note(), cx);
                    return;
                }
                let copy = ops::duplicate_view(&fields, &v, new_id(), limits.name_chars);
                let id = copy.id.clone();
                let edit = Edit::ViewCreated {
                    database_id: self.database_id(),
                    view: copy,
                };
                self.emit_views(vec![edit], Some(id), window, cx);
                self.bar.return_to = None;
                self.close_view_menu(window, cx);
            }
            ViewAction::Move(by) => {
                let Some(v) = current else { return };
                let views = fields.views_in_order();
                let Some(at) = views.iter().position(|x| x.id == v.id) else {
                    return;
                };
                let to = (at as isize + by).max(0) as usize;
                self.move_view_to(&v.id, to, window, cx);
                // The menu stays open, on the view's new place.
                self.open_view_page(v.id, ViewPage::Menu, anchor, window, cx);
            }
            ViewAction::Delete => {
                let Some(v) = current else { return };
                self.delete_view(&v.id, window, cx);
            }
            ViewAction::GroupBy(g) => {
                let Some(v) = current else { return };
                if g.is_none() && v.kind == ViewType::Board {
                    return;
                }
                let changed = ops::set_group_by(&fields, &v, g.as_deref());
                let date = g
                    .as_deref()
                    .and_then(|g| fields.property(g))
                    .is_some_and(|p| p.kind.is_date());
                self.emit_views(vec![self.view_changed(changed)], None, window, cx);
                // Grouped by a date, its buckets are the next question; else back to settings.
                let next = if date {
                    ViewPage::GroupDate
                } else {
                    ViewPage::Settings
                };
                self.open_view_page(v.id, next, anchor, window, cx);
            }
            ViewAction::GroupDate(b) => {
                let Some(mut v) = current else { return };
                v.group_date = (b != DateBucket::Relative).then_some(b);
                let id = v.id.clone();
                self.emit_views(vec![self.view_changed(v)], None, window, cx);
                self.open_view_page(id, ViewPage::Settings, anchor, window, cx);
            }
            ViewAction::ToggleCard(pid) => {
                let Some(v) = current else { return };
                let changed = ops::toggle_card_property(&fields, &v, &pid, &limits);
                if changed == v {
                    self.refuse(
                        format!("A view shows at most {} properties", limits.properties),
                        cx,
                    );
                    return;
                }
                let id = v.id.clone();
                self.emit_views(vec![self.view_changed(changed)], None, window, cx);
                self.open_view_page(id, ViewPage::Properties, anchor, window, cx);
            }
            ViewAction::MoveCard(by) => {
                let Some(v) = current else { return };
                let Some(Popover::View {
                    page: ViewPage::Property(pid),
                    ..
                }) = self.popover.clone()
                else {
                    return;
                };
                let changed = ops::move_card_property(&fields, &v, &pid, by);
                let id = v.id.clone();
                self.emit_views(vec![self.view_changed(changed)], None, window, cx);
                self.open_view_page(id, ViewPage::Property(pid), anchor, window, cx);
            }
            ViewAction::Columns => {
                let Some(v) = current else { return };
                self.show_view(v.id, window, cx);
                self.close_property_menu();
                self.open_columns_menu(cx);
            }
            ViewAction::ClearSorts => {
                let Some(mut v) = current else { return };
                v.sort.clear();
                let id = v.id.clone();
                self.emit_views(vec![self.view_changed(v)], None, window, cx);
                self.open_view_page(id, ViewPage::Settings, anchor, window, cx);
            }
            ViewAction::ClearFilter => {
                let Some(mut v) = current else { return };
                v.filter = None;
                let id = v.id.clone();
                self.emit_views(vec![self.view_changed(v)], None, window, cx);
                self.open_view_page(id, ViewPage::Settings, anchor, window, cx);
            }
        }
    }

    /// Back a page: from a card property to the properties, from grouping and properties to
    /// the settings, from a board's group to the types, else to the view's menu (or closed,
    /// for the settings opened from the bar's button).
    fn view_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.popover, Some(Popover::NewBoard)) {
            let name = self.new_view.take().map(|n| n.name).unwrap_or_default();
            self.open_add_view(name, window, cx);
            if let Some(i) = VIEW_TYPES.iter().position(|k| *k == ViewType::Board) {
                self.view_kind = (i, self.view_kind.1 + 1);
            }
            return;
        }
        let Some(Popover::View {
            view_id,
            page,
            anchor,
        }) = self.popover.clone()
        else {
            self.close_view_menu(window, cx);
            return;
        };
        let to = match page {
            ViewPage::Property(_) => Some(ViewPage::Properties),
            ViewPage::Group | ViewPage::GroupDate | ViewPage::Properties => {
                Some(ViewPage::Settings)
            }
            ViewPage::BoardGroup => Some(ViewPage::Type),
            ViewPage::Menu => None,
            _ if anchor == ViewAnchor::Settings => None,
            _ => Some(ViewPage::Menu),
        };
        match to {
            Some(page) => self.open_view_page(view_id, page, anchor, window, cx),
            None => self.close_view_menu(window, cx),
        }
    }

    fn view_changed(&self, view: View) -> Edit {
        Edit::ViewChanged {
            database_id: self.database_id(),
            view,
        }
    }

    /// Shows view edits at once (as a header's change is), hands them to the host, and shows
    /// `show` (a view just made) when given.
    fn emit_views(
        &mut self,
        edits: Vec<Edit>,
        show: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if edits.is_empty() {
            return;
        }
        self.note = None;
        let id = self.database.id.clone();
        if let Some(fields) = self.database.database.as_mut() {
            for e in &edits {
                match e {
                    Edit::ViewCreated { database_id, view }
                    | Edit::ViewChanged { database_id, view }
                        if *database_id == id =>
                    {
                        match fields.views.iter_mut().find(|v| v.id == view.id) {
                            Some(v) => *v = view.clone(),
                            None => fields.views.push(view.clone()),
                        }
                    }
                    Edit::ViewDeleted {
                        database_id,
                        view_id,
                    } if *database_id == id => fields.views.retain(|v| v.id != *view_id),
                    _ => {}
                }
            }
        }
        self.emit_edits(edits, window, cx);
        self.dirty = true;
        if let Some(show) = show {
            self.show_view(show, window, cx);
        }
        cx.notify();
    }

    /// The + menu's type picked (or Enter on the highlighted one): a new view named as typed,
    /// or for its type; a board asks what to group by when there's no select.
    fn add_view(&mut self, kind: ViewType, window: &mut Window, cx: &mut Context<Self>) {
        let Some(fields) = self.database.database.clone() else {
            return;
        };
        let limits = self.host.limits();
        if fields.views.len() >= limits.views {
            self.refuse(self.full_views_note(), cx);
            return;
        }
        let typed = truncate_chars(&self.typed_name(cx), limits.name_chars).to_owned();
        if kind == ViewType::Board {
            match ops::board_group(&fields, None) {
                Some(g) => self.create_view(kind, typed, Some(g), window, cx),
                None if ops::groupable(&fields).is_empty() => self.refuse(NO_GROUP.into(), cx),
                None => {
                    self.field = None;
                    self.note = None;
                    self.new_view = Some(NewView { name: typed });
                    self.menu_serial += 1;
                    self.popover = Some(Popover::NewBoard);
                    window.focus(&self.focus, cx);
                    cx.notify();
                }
            }
            return;
        }
        self.create_view(kind, typed, None, window, cx);
    }

    /// Emits a new view (named for its type when `name` is empty) after the others, and shows
    /// it.
    fn create_view(
        &mut self,
        kind: ViewType,
        name: String,
        group_by: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(fields) = self.database.database.clone() else {
            return;
        };
        let name = if name.trim().is_empty() {
            ops::unique_view_name(&fields, ops::view_type_label(&kind))
        } else {
            name
        };
        let view = ops::new_view(&fields, new_id(), kind, name, group_by);
        let id = view.id.clone();
        let edit = Edit::ViewCreated {
            database_id: self.database_id(),
            view,
        };
        self.emit_views(vec![edit], Some(id), window, cx);
        self.new_view = None;
        self.bar.return_to = None;
        self.close_view_menu(window, cx);
    }

    /// What's typed in the open menu's field, trimmed.
    fn typed_name(&self, cx: &App) -> String {
        self.field
            .as_ref()
            .map(|f| f.state.read(cx).value().trim().to_owned())
            .unwrap_or_default()
    }

    /// Enter in the + menu's field: the highlighted type.
    pub(crate) fn commit_new_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let kind = VIEW_TYPES
            .get(self.view_kind.0)
            .cloned()
            .unwrap_or(ViewType::Table);
        self.add_view(kind, window, cx);
    }

    /// Enter (or Save) on a view's name.
    pub(crate) fn rename_view(
        &mut self,
        view_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(mut v) = self.view_by_id(view_id).cloned() else {
            return;
        };
        let name = truncate_chars(&self.typed_name(cx), self.host.limits().name_chars).to_owned();
        if name.is_empty() {
            self.refuse("A view needs a name".into(), cx);
            return;
        }
        if name != v.name {
            v.name = name;
            self.emit_views(vec![self.view_changed(v)], None, window, cx);
        }
        self.close_view_menu(window, cx);
    }

    /// Deletes a view, unless it's the last; the view shown goes to its neighbour (the next,
    /// else the one before).
    pub(crate) fn delete_view(
        &mut self,
        view_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let views = self.bar_views();
        if views.len() <= 1 {
            self.refuse("A database keeps at least one view".into(), cx);
            return;
        }
        let Some(at) = views.iter().position(|v| v.id == view_id) else {
            return;
        };
        let shown = self.view().is_some_and(|v| v.id == view_id);
        let neighbour = views
            .get(at + 1)
            .or_else(|| at.checked_sub(1).and_then(|i| views.get(i)))
            .map(|v| v.id.clone());
        let edit = Edit::ViewDeleted {
            database_id: self.database_id(),
            view_id: view_id.to_owned(),
        };
        self.emit_views(vec![edit], if shown { neighbour } else { None }, window, cx);
        self.bar.return_to = None;
        self.close_view_menu(window, cx);
    }

    /// Moves a view to place `to` among the views (a tab dragged, Move left or right).
    pub(crate) fn move_view_to(
        &mut self,
        view_id: &str,
        to: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(fields) = self.database.database.as_ref() else {
            return;
        };
        let changed = ops::move_view(fields, view_id, to);
        let edits = changed.into_iter().map(|v| self.view_changed(v)).collect();
        self.emit_views(edits, None, window, cx);
    }

    /// A tab dropped on another: it takes that one's place.
    pub(crate) fn drop_tab(
        &mut self,
        dragged: &str,
        onto: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let views = self.bar_views();
        if let Some(to) = views.iter().position(|v| v.id == onto) {
            self.move_view_to(dragged, to, window, cx);
        }
    }
}
