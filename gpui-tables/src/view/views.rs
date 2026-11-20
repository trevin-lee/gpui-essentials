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
//! settings button). A view's menu opens its types and its settings as submenus beside their
//! entries, and the settings open the grouping, a date's buckets and the properties on cards
//! and lines (each with its own submenu to hide or move it) the same way. A rename and a delete
//! take a step of their own, as the property menus' do. The + menu is gpui-core's `Picker`
//! naming rather than filtering: the new view's name typed over its types.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui::{App, Bounds, Context, FocusHandle, Pixels, ScrollHandle, Window};

use super::ops;
use super::properties::FieldFor;
use super::table::{Action, Entry, Popover, Reopen, TableView, type_icon};
use crate::model::{
    CardPreview, CardSize, DateBucket, Edit, PropertyType, View, ViewType, new_id, truncate_chars,
};

/// The types a view can be made as or changed to.
pub(crate) const VIEW_TYPES: &[ViewType] = &[
    ViewType::Table,
    ViewType::Board,
    ViewType::List,
    ViewType::Gallery,
];

/// A view's menu: its own, a step of it, or one of its submenus opened on its own
/// ([`ViewMenu`]).
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
    /// A filter rule's value, typed (its index among the rules).
    RuleValue(usize),
}

impl ViewPage {
    pub(crate) fn is_field(&self) -> bool {
        matches!(self, Self::Rename | Self::RuleValue(_))
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

/// A view's menu or a part of it, which [`TableView::open_view_menu`] opens on its own, hanging
/// from the view's tab: its submenus and steps too.
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
    /// A gallery's cards show their row's cover (true) or nothing above their title.
    CardPreview(bool),
    /// A gallery's cards' size.
    CardSize(CardSize),
    /// Shown on cards or lines, or not.
    ToggleCard(String),
    /// A table's columns: the Properties menu.
    Columns,
    ClearSorts,
    ClearFilter,
    /// The filter's rules: any of them, or all.
    FilterAny(bool),
    /// A rule added for this property.
    AddRule(String),
    /// Rule `0` deleted.
    RemoveRule(usize),
    /// Rule `0` made to test this property instead (a new rule for it).
    RuleProperty(usize, String),
    /// Rule `0`'s condition.
    RuleOp(usize, crate::model::FilterOp),
    /// Rule `0`'s value, picked.
    RuleValue(usize, serde_json::Value),
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

fn untitled(title: &str) -> String {
    if title.trim().is_empty() {
        "Untitled".into()
    } else {
        title.to_owned()
    }
}

/// The reorder group of the properties a board's cards or a list's lines show.
pub(crate) const CARD_ORDER: &str = "card-properties";

/// The settings' entry for a date grouping's buckets, highlighted as it appears.
const DATE_GROUPING: &str = "Date grouping";
/// A gallery's settings entry for what its cards show above their title, and its choice of
/// the row's cover.
const CARD_PREVIEW: &str = "Card preview";
const PAGE_COVER: &str = "Page cover";
const CARD_SIZE: &str = "Card size";

/// A card size as its settings entry names it.
fn size_label(size: &CardSize) -> &'static str {
    match size {
        CardSize::Small => "Small",
        CardSize::Large => "Large",
        _ => "Medium",
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
    /// types it can have (table, board, list), in gpui-core's `Picker` naming rather than
    /// filtering. Typing names it; Up and Down move through the types; Enter, or a press on a
    /// type, makes it and shows it.
    pub fn open_add_view_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.remember_bar_focus(window, cx);
        self.open_add_view(Reopen::default(), cx);
    }

    /// Opens a view's menu, hanging from its tab, as pressing the shown view's tab (or
    /// right-clicking any) does, or a part of it on its own: its name to type, its types, its
    /// settings, and so on, which its menu opens as submenus. A part that doesn't apply (a
    /// date's buckets for a view not grouped by a date) doesn't open.
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

    pub(crate) fn view_by_id(&self, id: &str) -> Option<&View> {
        self.database.database.as_ref()?.view(id)
    }

    fn is_table_like(&self, view_id: &str) -> bool {
        self.view_by_id(view_id).is_some_and(|v| {
            !matches!(v.kind, ViewType::Board | ViewType::List | ViewType::Gallery)
        })
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
        // Already shown as asked: the view asked for, or the fallback when none was. One asked
        // for that's gone (just deleted) shows the fallback, but the host hasn't been told.
        let asked = match &self.view_id {
            Some(id) => Some(id.as_str()),
            None => self.view().map(|v| v.id.as_str()),
        };
        if asked == Some(view_id.as_str()) {
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

    /// Opens the + menu, where `reopen` says (on Board with its name as typed, back from what a
    /// new board groups by).
    fn open_add_view(&mut self, reopen: Reopen, cx: &mut Context<Self>) {
        self.ensure();
        self.date = None;
        self.close_property_menu();
        self.menu_serial += 1;
        self.popover = Some(Popover::AddView);
        self.reopen = reopen;
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
        if let ViewPage::RuleValue(i) = page {
            self.open_rule_value(&view_id, i, window, cx);
        } else if page == ViewPage::Rename {
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
            Some(Popover::View { .. }) if at_max => Some(format!(
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

    /// The entries of a view's menu (or a part of it opened on its own), or of the new board's
    /// group-by question.
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
        let Some(Popover::View { page, .. }) = &self.popover else {
            return Vec::new();
        };
        let Some(v) = self.menu_view() else {
            return Vec::new();
        };
        match page {
            ViewPage::Menu => {
                let views = fields.views_in_order();
                let at = views.iter().position(|x| x.id == v.id).unwrap_or(0);
                let mut out = vec![
                    Entry::heading(v.name.clone()),
                    act("Rename", A::Page(ViewPage::Rename)).icon(I::Pencil),
                    Entry::sub("Change type", self.view_type_entries(v))
                        .icon(view_icon(&v.kind))
                        .hint(ops::view_type_label(&v.kind)),
                    Entry::sub("View settings", self.settings_entries(v, false))
                        .icon(I::SlidersHorizontal),
                    act("Duplicate", A::Duplicate).icon(I::Copy),
                ];
                if at > 0 {
                    out.push(act("Move left", A::Move(-1)).icon(I::ArrowLeft));
                }
                if at + 1 < views.len() {
                    out.push(act("Move right", A::Move(1)).icon(I::ArrowRight));
                }
                out.push(act("Delete view", A::Page(ViewPage::Delete)).icon(I::Trash));
                out
            }
            ViewPage::Type => self.view_type_entries(v),
            ViewPage::BoardGroup => {
                let mut out = vec![Entry::heading("Group by")];
                for p in ops::groupable(fields) {
                    out.push(act(&p.name, A::Board(p.id.clone())).icon(type_icon(&p.kind)));
                }
                out.push(back());
                out
            }
            ViewPage::Delete => vec![
                Entry::heading(format!("Delete {}?", v.name)),
                act("Delete view", A::Delete).icon(I::Trash),
                act("Cancel", A::Back).icon(I::X).highlighted(true),
            ],
            // On its own (the bar's settings button), it changes the type too.
            ViewPage::Settings => self.settings_entries(v, true),
            ViewPage::Group => self.group_entries(v),
            ViewPage::GroupDate => self.group_date_entries(v),
            ViewPage::Properties => self.card_entries(v),
            ViewPage::Rename | ViewPage::RuleValue(_) => Vec::new(),
        }
    }

    /// The types a view can change to, its own checked.
    fn view_type_entries(&self, v: &View) -> Vec<Entry> {
        let mut out = vec![Entry::heading("Type")];
        for k in VIEW_TYPES {
            let on = *k == v.kind;
            out.push(
                Entry::act(
                    ops::view_type_label(k),
                    Action::View(ViewAction::SetType(k.clone())),
                )
                .icon(view_icon(k))
                .checked(on)
                .highlighted(on),
            );
        }
        out
    }

    /// A view's settings: its type (`with_type`, unless its menu has Change type already), its
    /// grouping and a date's buckets, the properties shown, and its sorts and filter cleared.
    fn settings_entries(&self, v: &View, with_type: bool) -> Vec<Entry> {
        use ViewAction as A;
        use gpui_core::IconName as I;
        let act = |label: &str, a: A| Entry::act(label, Action::View(a));
        let Some(fields) = self.database.database.as_ref() else {
            return Vec::new();
        };
        let group = v.group_by.as_deref().and_then(|g| fields.property(g));
        let mut out = vec![Entry::heading("View settings")];
        if with_type {
            out.push(
                Entry::sub("Change type", self.view_type_entries(v))
                    .icon(view_icon(&v.kind))
                    .hint(ops::view_type_label(&v.kind)),
            );
        }
        out.push(
            Entry::sub("Group by", self.group_entries(v))
                .icon(I::Group)
                .hint(group.map_or("None", |p| p.name.as_str())),
        );
        if group.is_some_and(|p| p.kind.is_date()) {
            out.push(
                Entry::sub(DATE_GROUPING, self.group_date_entries(v))
                    .icon(I::CalendarDays)
                    .hint(bucket_label(
                        v.group_date.as_ref().unwrap_or(&DateBucket::Relative),
                    )),
            );
        }
        if v.kind == ViewType::Gallery {
            let cover = v.card_preview == Some(CardPreview::Cover);
            out.push(
                Entry::sub(CARD_PREVIEW, self.card_preview_entries(v))
                    .icon(I::PanelTop)
                    .hint(if cover { PAGE_COVER } else { "None" }),
            );
            let size = v.card_size.clone().unwrap_or(CardSize::Medium);
            out.push(
                Entry::sub(CARD_SIZE, self.card_size_entries(v))
                    .icon(I::Maximize2)
                    .hint(size_label(&size)),
            );
        }
        match v.kind {
            ViewType::Board | ViewType::List | ViewType::Gallery => {
                let n = ops::card_properties(fields, v).len();
                let label = if matches!(v.kind, ViewType::Board | ViewType::Gallery) {
                    "Card properties"
                } else {
                    "Properties"
                };
                out.push(
                    Entry::sub(label, self.card_entries(v))
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
        out.push(Entry::heading("Sort and filter"));
        let r = ops::rules(v);
        out.push(
            Entry::sub(super::filter_rules::FILTER, self.filter_rule_entries(v))
                .icon(I::Funnel)
                .hint(match r.rules.len() {
                    0 => "None".to_owned(),
                    _ => self.filter_summary(v),
                }),
        );
        if !v.sort.is_empty() {
            out.push(
                act("Remove sorts", A::ClearSorts)
                    .icon(I::ArrowUpDown)
                    .hint(self.sort_summary(v)),
            );
        }
        if v.filter.is_some() {
            out.push(
                act("Clear filter", A::ClearFilter)
                    .icon(I::FunnelX)
                    .hint(self.filter_summary(v)),
            );
        }
        out
    }

    /// What a view can group by, the current one checked (and None, but for a board).
    fn group_entries(&self, v: &View) -> Vec<Entry> {
        use ViewAction as A;
        let Some(fields) = self.database.database.as_ref() else {
            return Vec::new();
        };
        let mut out = vec![Entry::heading("Group by")];
        if v.kind != ViewType::Board {
            let on = v.group_by.is_none();
            out.push(
                Entry::act("None", Action::View(A::GroupBy(None)))
                    .icon(gpui_core::IconName::X)
                    .checked(on)
                    .highlighted(on),
            );
        }
        for p in ops::groupable(fields) {
            let on = v.group_by.as_deref() == Some(p.id.as_str());
            out.push(
                Entry::act(&p.name, Action::View(A::GroupBy(Some(p.id.clone()))))
                    .icon(type_icon(&p.kind))
                    .checked(on)
                    .highlighted(on),
            );
        }
        out
    }

    /// How a date grouping buckets, the current one checked.
    /// What a gallery's cards show above their title: nothing, or their row's cover.
    fn card_preview_entries(&self, v: &View) -> Vec<Entry> {
        let cover = v.card_preview == Some(CardPreview::Cover);
        let mut out = vec![Entry::heading(CARD_PREVIEW)];
        for (label, on, icon) in [
            ("None", !cover, gpui_core::IconName::X),
            (PAGE_COVER, cover, gpui_core::IconName::Image),
        ] {
            out.push(
                Entry::act(
                    label,
                    Action::View(ViewAction::CardPreview(label == PAGE_COVER)),
                )
                .icon(icon)
                .checked(on)
                .highlighted(on),
            );
        }
        out
    }

    /// How big a gallery's cards are, the current size checked.
    fn card_size_entries(&self, v: &View) -> Vec<Entry> {
        let current = v.card_size.clone().unwrap_or(CardSize::Medium);
        let mut out = vec![Entry::heading(CARD_SIZE)];
        for size in [CardSize::Small, CardSize::Medium, CardSize::Large] {
            let on = size == current;
            out.push(
                Entry::act(size_label(&size), Action::View(ViewAction::CardSize(size)))
                    .checked(on)
                    .highlighted(on),
            );
        }
        out
    }

    fn group_date_entries(&self, v: &View) -> Vec<Entry> {
        let mut out = vec![Entry::heading("Date grouping")];
        let current = v.group_date.clone().unwrap_or(DateBucket::Relative);
        for b in DateBucket::ALL {
            let on = *b == current;
            out.push(
                Entry::act(
                    bucket_label(b),
                    Action::View(ViewAction::GroupDate(b.clone())),
                )
                .checked(on)
                .highlighted(on),
            );
        }
        out
    }

    /// The properties a board's cards or a list's lines show, in order, each opening a submenu
    /// to hide it and dragged (or moved with Alt-Up and Alt-Down) to put them in another order;
    /// then the rest, to show.
    fn card_entries(&self, v: &View) -> Vec<Entry> {
        use ViewAction as A;
        use gpui_core::IconName as I;
        let Some(fields) = self.database.database.as_ref() else {
            return Vec::new();
        };
        let shown = ops::card_properties(fields, v);
        let mut out = vec![Entry::heading("Shown")];
        for id in &shown {
            let Some(p) = fields.property(id) else {
                continue;
            };
            let hide = Entry::act("Hide", Action::View(A::ToggleCard(id.clone()))).icon(I::EyeOff);
            out.push(
                Entry::sub(p.name.clone(), vec![hide])
                    .icon(type_icon(&p.kind))
                    .checked(true)
                    .reorder(CARD_ORDER),
            );
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
                Entry::act(&p.name, Action::View(A::ToggleCard(p.id.clone())))
                    .icon(type_icon(&p.kind))
                    .checked(false),
            );
        }
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
                // The menu stays open, hanging from the view's tab in its new place, on the
                // entry that moved it (gpui-core keeps the highlight on its entry).
                self.move_view_to(&v.id, to, window, cx);
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
                // Grouped by a date, its buckets are the next question: the settings' highlight
                // moves to Date grouping, beside Group by, so → opens it. Each time is a new
                // request, so a date picked again moves it once more.
                if date
                    && !self.groups_by_date(&v)
                    && let Some(path) = self.settings_path()
                {
                    self.move_requests += 1;
                    self.move_highlight = Some((path, DATE_GROUPING.into(), self.move_requests));
                }
                self.emit_views(vec![self.view_changed(changed)], None, window, cx);
            }
            ViewAction::GroupDate(b) => {
                let Some(mut v) = current else { return };
                v.group_date = (b != DateBucket::Relative).then_some(b);
                self.emit_views(vec![self.view_changed(v)], None, window, cx);
            }
            ViewAction::CardPreview(cover) => {
                let Some(mut v) = current else { return };
                // Only ever none or cover, written out.
                let preview = if cover {
                    CardPreview::Cover
                } else {
                    CardPreview::None
                };
                if v.card_preview.as_ref() == Some(&preview) {
                    return;
                }
                v.card_preview = Some(preview);
                self.emit_views(vec![self.view_changed(v)], None, window, cx);
            }
            ViewAction::CardSize(size) => {
                let Some(mut v) = current else { return };
                // Medium is the default: written as the field left out.
                let size = (size != CardSize::Medium).then_some(size);
                if v.card_size == size {
                    return;
                }
                v.card_size = size;
                self.emit_views(vec![self.view_changed(v)], None, window, cx);
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
                // The properties stay open in their new order, on the one shown or hidden
                // (gpui-core keeps the highlight on its entry).
                self.emit_views(vec![self.view_changed(changed)], None, window, cx);
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
                self.emit_views(vec![self.view_changed(v)], None, window, cx);
            }
            ViewAction::ClearFilter => {
                let Some(mut v) = current else { return };
                v.filter = None;
                self.emit_views(vec![self.view_changed(v)], None, window, cx);
            }
            a @ (ViewAction::FilterAny(_)
            | ViewAction::AddRule(_)
            | ViewAction::RemoveRule(_)
            | ViewAction::RuleProperty(..)
            | ViewAction::RuleOp(..)
            | ViewAction::RuleValue(..)) => {
                let Some(v) = current else { return };
                self.apply_rule_action(v, a, window, cx);
            }
        }
    }

    /// Back from a step: to the view's menu (or the settings, from the bar's button) on the
    /// entry it came from, or to the + menu, on Board, from a new board's group. Its menu, or a
    /// part of it opened on its own, closes.
    fn view_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.popover, Some(Popover::NewBoard)) {
            let typed = self.new_view.take().map(|n| n.name);
            let reopen = Reopen {
                name: typed,
                ..Reopen::on(ops::view_type_label(&ViewType::Board))
            };
            self.open_add_view(reopen, cx);
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
        // A board's group came from Board in Change type's submenu, open again.
        let board = || {
            Reopen::at(
                vec!["Change type".into()],
                Some(ops::view_type_label(&ViewType::Board).into()),
            )
        };
        if let ViewPage::RuleValue(i) = page {
            self.rule_value_back(view_id, i, window, cx);
            return;
        }
        let to = match (page, anchor) {
            (ViewPage::BoardGroup, ViewAnchor::Settings) => Some((ViewPage::Settings, board())),
            (ViewPage::BoardGroup, ViewAnchor::Tab) => Some((ViewPage::Menu, board())),
            (ViewPage::Rename, ViewAnchor::Tab) => Some((ViewPage::Menu, Reopen::on("Rename"))),
            (ViewPage::Delete, ViewAnchor::Tab) => {
                Some((ViewPage::Menu, Reopen::on("Delete view")))
            }
            _ => None,
        };
        match to {
            Some((page, from)) => {
                self.open_view_page(view_id, page, anchor, window, cx);
                self.reopen = from;
            }
            None => self.close_view_menu(window, cx),
        }
    }

    /// The submenus from the open view menu down to its settings: View settings in the view's
    /// menu, none in the settings opened on their own; None for any other part of it.
    fn settings_path(&self) -> Option<Vec<String>> {
        match &self.popover {
            Some(Popover::View {
                page: ViewPage::Menu,
                ..
            }) => Some(vec!["View settings".into()]),
            Some(Popover::View {
                page: ViewPage::Settings,
                ..
            }) => Some(Vec::new()),
            _ => None,
        }
    }

    /// A shown card property dragged to another place among them (or moved with Alt-Up or
    /// Alt-Down): a `viewChanged` with `cardProperties` in the new order, the menu open.
    pub(crate) fn reorder_card(
        &mut self,
        from: usize,
        to: usize,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(fields), Some(v)) = (self.database.database.clone(), self.menu_view().cloned())
        else {
            return;
        };
        // The property the entry was drawn for: at `from`, named as the entry is.
        let Some(pid) = ops::card_properties(&fields, &v)
            .get(from)
            .filter(|id| fields.property(id).is_some_and(|p| p.name == name))
            .cloned()
        else {
            return;
        };
        let changed = ops::move_card_property(&fields, &v, &pid, to as isize - from as isize);
        if changed != v {
            self.emit_views(vec![self.view_changed(changed)], None, window, cx);
        }
    }

    /// A view's change shown and handed to the host, as any setting's is.
    pub(crate) fn emit_view_change(
        &mut self,
        view: View,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.emit_views(vec![self.view_changed(view)], None, window, cx);
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

    /// The + menu's type picked (or Enter on the highlighted one): a new view named `typed`, or
    /// for its type; a board asks what to group by when there's no select.
    pub(crate) fn add_view(
        &mut self,
        kind: ViewType,
        typed: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(fields) = self.database.database.clone() else {
            return;
        };
        let limits = self.host.limits();
        if fields.views.len() >= limits.views {
            self.refuse(self.full_views_note(), cx);
            return;
        }
        let typed = truncate_chars(typed.trim(), limits.name_chars).to_owned();
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
