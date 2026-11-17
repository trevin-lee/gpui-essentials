//! Searching the shown view's rows from the bar of views, as Notion's search in a database: a
//! search button at the bar's right end that opens a field in its place (⌘F does too), and the
//! rows of the table, board or list narrowed as it's typed to those whose title or any shown
//! property's text contains it ([`engine::search`](crate::engine::search)), on top of the
//! view's own filter, sorts and groups.
//!
//! The search is this entity's alone: it's never saved in the view and never handed to the host
//! as an edit. Each row's words (its title and its shown cells as they read) are folded once,
//! the first time a search needs them after the docs or the view changed, so a keystroke is a
//! pass over them and the view's own rows aren't worked out again. Clearing it shows the view as
//! it was before, scrolled where it was, with its focus and its selection.

use std::cell::{Cell, RefCell};

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, ClickEvent, Context, Entity, FocusHandle, InteractiveElement,
    IntoElement, ParentElement, Pixels, Point, Role, ScrollStrategy, SharedString,
    StatefulInteractiveElement, Styled, Subscription, Window, div,
};
use gpui_core::{IconName, Input, InputEvent, InputState, Press};

use super::ops::{self, ColumnLayout};
use super::render::{icon, line};
use super::table::TableView;
use crate::engine::ViewRows;
use crate::engine::search::{Query, SearchText, search};
use crate::model::{DatabaseFields, Property, PropertyType, View, ViewType};

/// The search's state: what's typed, its field, and what it keeps between keystrokes.
#[derive(Default)]
pub(crate) struct Search {
    /// What's searched for, as typed (empty: nothing).
    pub text: String,
    /// Whether the field is open in the bar (it is while there's a query).
    pub open: bool,
    /// The field, made when it's first drawn (it needs the window), and its events.
    pub input: Option<(Entity<InputState>, Subscription)>,
    /// The field takes the keyboard when it's next drawn.
    pub focus_next: bool,
    /// The host set the query: the field is given it when it next draws. (Typing never is:
    /// keys typed before the field's change is heard would be overwritten.)
    pub push: bool,
    /// Whether the field had the keyboard when the bar last drew.
    pub focused: bool,
    /// The search button's focus.
    pub button: Option<FocusHandle>,
    /// Only the query changed since the rows were last worked out: the view's own rows from
    /// then are searched again, not worked out again.
    pub refilter: Cell<bool>,
    pub cache: RefCell<Cache>,
    /// The view as it was when the search began, shown again when it's cleared.
    pub saved: Option<Saved>,
}

/// What a search keeps between keystrokes.
#[derive(Default)]
pub(crate) struct Cache {
    /// The view's own rows, before searching (while the field's open).
    pub base: Option<ViewRows>,
    /// Each row's folded words, for the docs and the view `base` was worked out for.
    pub text: Option<SearchText>,
}

/// Where the view was when a search began.
pub(crate) struct Saved {
    view_id: Option<String>,
    list: Point<Pixels>,
    board_x: Point<Pixels>,
    columns: Vec<(String, Point<Pixels>)>,
    /// The focused cell's row and column.
    cell: Option<(String, usize)>,
    selected: Vec<String>,
    anchor: Option<String>,
}

impl TableView {
    // --- The host's part ----------------------------------------------------------------------

    /// Searches the shown view's rows for `query`, as typing in the bar's search field does:
    /// the table, board or list shows only the rows whose title or any shown property's text
    /// contains it, without regard to case or accents, within the view's own filter, in its
    /// order and groups (groups with no row found are left out). None, or an empty query,
    /// shows the view whole again. With the bar of views on, the field opens holding it (and
    /// None closes it).
    ///
    /// The search is never saved in the view nor emitted as an edit; it lasts while this
    /// entity does, across the views it shows.
    pub fn set_search(&mut self, query: Option<String>, cx: &mut Context<Self>) {
        let text = query.unwrap_or_default();
        self.search.open = !text.is_empty();
        self.search.focus_next = false;
        self.search.push = true;
        self.apply_search(text, cx);
    }

    /// What the rows are searched for, as typed, while there's something.
    pub fn search(&self) -> Option<&str> {
        (!self.search.text.is_empty()).then_some(self.search.text.as_str())
    }

    /// Opens the bar's search field and gives it the keyboard, as its button and ⌘F do (with
    /// the bar of views on). What's typed is selected, so typing replaces it.
    pub fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.bar.on {
            return;
        }
        self.close_popover(cx);
        self.search.open = true;
        match &self.search.input {
            Some((state, _)) => state.update(cx, |s, cx| {
                s.select_all(window, cx);
                s.focus(window, cx);
            }),
            None => self.search.focus_next = true,
        }
        cx.notify();
    }

    // --- What it shows ------------------------------------------------------------------------

    /// What the view shows with the search: `query` works out the view's own rows, which are
    /// reused while only the query changes, and searched while there's one.
    pub(crate) fn searched(
        &self,
        fields: &DatabaseFields,
        view: &View,
        columns: &[ColumnLayout],
        query: impl FnOnce() -> ViewRows,
    ) -> ViewRows {
        let mut cache = self.search.cache.borrow_mut();
        let base = match (self.search.refilter.replace(false), cache.base.take()) {
            (true, Some(base)) => base,
            _ => {
                cache.text = None;
                query()
            }
        };
        let Some(q) = Query::new(&self.search.text) else {
            // Kept for the first keystroke while the field's open.
            if self.search.open {
                cache.base = Some(base.clone());
            }
            return base;
        };
        let text = cache
            .text
            .get_or_insert_with(|| self.search_text(fields, view, columns, &base));
        let found = search(&base, text, &q);
        cache.base = Some(base);
        found
    }

    /// Each of the view's rows' words, folded: its title and its shown properties' text as
    /// their cells read (a table's columns, a board's cards' or a list's lines' properties).
    fn search_text(
        &self,
        fields: &DatabaseFields,
        view: &View,
        columns: &[ColumnLayout],
        base: &ViewRows,
    ) -> SearchText {
        let shown: Vec<&Property> = match view.kind {
            ViewType::Board | ViewType::List => ops::card_properties(fields, view)
                .iter()
                .filter_map(|id| fields.property(id))
                .collect(),
            _ => columns
                .iter()
                .filter_map(|c| fields.properties.get(c.property))
                .collect(),
        };
        // A checkbox shows no words; the title is the row's own.
        let shown: Vec<&Property> = shown
            .into_iter()
            .filter(|p| !matches!(p.kind, PropertyType::Title | PropertyType::Checkbox))
            .collect();
        let mut text = SearchText::new(self.rows.len());
        for &r in &base.rows {
            let row = &self.rows[r];
            let words = std::iter::once(row.title.clone())
                .chain(shown.iter().map(|p| self.cell_text(row, p)));
            text.set(r, words);
        }
        text
    }

    /// Whether a search is on and finds nothing.
    pub(crate) fn search_finds_nothing(&self) -> bool {
        Query::new(&self.search.text).is_some() && self.shown.rows.is_empty()
    }

    /// The query changed to `text`: the rows are searched again, without working the view's
    /// own out again. Starting a search remembers where the view was; clearing it goes back.
    fn apply_search(&mut self, text: String, cx: &mut Context<Self>) {
        let before = Query::new(&self.search.text);
        let after = Query::new(&text);
        if before == after {
            self.search.text = text;
            cx.notify();
            return;
        }
        // Whatever else changed is worked out first, so only the query is left.
        self.ensure();
        if before.is_none() {
            self.search.saved = Some(self.save_place());
        }
        self.search.text = text;
        self.search.refilter.set(true);
        self.dirty = true;
        if after.is_some() {
            // Each new query shows its first rows (or follows the focused one).
            self.set_offsets(Point::default(), Point::default(), &[]);
            self.ensure();
        } else {
            self.ensure();
            if let Some(saved) = self.search.saved.take() {
                self.restore_place(saved);
            }
        }
        cx.notify();
    }

    fn save_place(&self) -> Saved {
        let cell = self.cell.and_then(|c| {
            let row = self.focused_row()?;
            Some((row, c.column))
        });
        Saved {
            view_id: self.view().map(|v| v.id.clone()),
            list: self.list.0.borrow().base_handle.offset(),
            board_x: self.board.scroll.offset(),
            columns: self
                .board
                .lists
                .iter()
                .map(|(k, h)| (k.clone(), h.0.borrow().base_handle.offset()))
                .collect(),
            cell,
            selected: self.selected.clone(),
            anchor: self.anchor.clone(),
        }
    }

    /// Scrolls the table or list, the board sideways and the board's columns to these.
    fn set_offsets(
        &self,
        list: Point<Pixels>,
        board_x: Point<Pixels>,
        columns: &[(String, Point<Pixels>)],
    ) {
        {
            let mut l = self.list.0.borrow_mut();
            l.deferred_scroll_to_item = None;
            l.base_handle.set_offset(list);
        }
        self.board.scroll.set_offset(board_x);
        for (key, h) in &self.board.lists {
            let at = columns
                .iter()
                .find(|(k, _)| k == key)
                .map_or_else(Point::default, |(_, p)| *p);
            let mut s = h.0.borrow_mut();
            s.deferred_scroll_to_item = None;
            s.base_handle.set_offset(at);
        }
    }

    /// The view as it was before the search, if it's still the view shown: its scroll, its
    /// focused cell (unless another was focused since) and its selected rows.
    fn restore_place(&mut self, saved: Saved) {
        if saved.view_id != self.view().map(|v| v.id.clone()) {
            return;
        }
        let focus_now = self.cell.is_some();
        if !focus_now && let Some((row, column)) = &saved.cell {
            let item = self.shown.items.iter().position(
                |i| matches!(i, super::Item::Row { row: r, .. } if self.rows[*r].id == *row),
            );
            if let Some(item) = item {
                let column = (*column).min(self.shown.columns.len().saturating_sub(1));
                self.cell = Some(super::CellRef { item, column });
            }
        }
        if self.selected.is_empty() {
            let shown = |id: &str| self.shown.rows.iter().any(|&r| self.rows[r].id == id);
            self.selected = saved.selected.into_iter().filter(|id| shown(id)).collect();
            self.anchor = saved.anchor.filter(|a| shown(a));
        }
        self.set_offsets(saved.list, saved.board_x, &saved.columns);
        // A focus moved to while searching is kept in view.
        if focus_now && let Some(c) = self.cell {
            self.list.scroll_to_item(c.item, ScrollStrategy::Nearest);
        }
    }

    // --- The field ----------------------------------------------------------------------------

    fn search_input(&self) -> Option<Entity<InputState>> {
        self.search.input.as_ref().map(|(s, _)| s.clone())
    }

    /// Escape in the field: clears what's typed, then closes the field and goes back to the view.
    fn search_escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.search_input();
        if !self.search.text.is_empty() {
            if let Some(state) = state {
                state.update(cx, |s, cx| s.set_value("", window, cx));
            }
            self.apply_search(String::new(), cx);
            return;
        }
        self.close_search(window, cx);
    }

    /// Closes the empty field, the keyboard back with the view.
    fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search.open = false;
        self.search.input = None;
        // Its base is no longer kept up.
        self.search.cache.borrow_mut().base = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Before the bar draws: the field made (it needs the window) and focused when it opens,
    /// and holding what the host searched for.
    pub(crate) fn prepare_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.bar.on {
            return;
        }
        // A query always shows its field.
        if !self.search.text.is_empty() {
            self.search.open = true;
        }
        if !self.search.open {
            self.search.input = None;
            self.search.focused = false;
            return;
        }
        if self.search.input.is_none() {
            let text = self.search.text.clone();
            let state = cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("Search")
                    .default_value(text)
            });
            let events = cx.subscribe_in(
                &state,
                window,
                |this: &mut Self, state, event: &InputEvent, window, cx| match event {
                    InputEvent::Change => {
                        let text = state.read(cx).value().to_string();
                        this.apply_search(text, cx);
                    }
                    // Enter takes the keyboard to the rows found.
                    InputEvent::PressEnter { .. } => {
                        window.focus(&this.focus, cx);
                        cx.notify();
                    }
                    // Left empty, the field closes.
                    InputEvent::Blur => {
                        if this.search.text.is_empty() && this.search.open {
                            this.search.open = false;
                            this.search.input = None;
                            this.search.cache.borrow_mut().base = None;
                            cx.notify();
                        }
                    }
                    InputEvent::Focus => {}
                },
            );
            self.search.input = Some((state, events));
        }
        let Some(state) = self.search_input() else {
            return;
        };
        if std::mem::take(&mut self.search.push)
            && state.read(cx).value().as_ref() != self.search.text
        {
            let text = self.search.text.clone();
            state.update(cx, |s, cx| s.set_value(text, window, cx));
        }
        if std::mem::take(&mut self.search.focus_next) {
            state.update(cx, |s, cx| {
                s.select_all(window, cx);
                s.focus(window, cx);
            });
        }
        self.search.focused = state.read(cx).is_focused(window);
    }

    /// The bar's search: its button, or its field while it's open.
    pub(crate) fn render_search(&mut self, h: Pixels, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme.clone();
        let icon_size = t.px(t.sizes.icon);
        let Some(state) = self.search_input().filter(|_| self.search.open) else {
            let button = self
                .search
                .button
                .get_or_insert_with(|| cx.focus_handle())
                .clone();
            return Press::new("view-search")
                .accessibility_label("Search")
                .focus_on_press(false)
                .focus_ring_color(t.colors.focus)
                .debug_selector(|| "view-search".into())
                .track_focus(&button)
                .flex_none()
                .size(h)
                .flex()
                .items_center()
                .justify_center()
                .rounded(t.px(t.sizes.chip_radius))
                .hover(|s| s.bg(t.colors.hover))
                .child(icon(IconName::Search, icon_size, t.colors.text_muted))
                .on_click(
                    cx.listener(|this, _: &ClickEvent, window, cx| this.open_search(window, cx)),
                )
                .into_any_element();
        };
        let clear = (!self.search.text.is_empty()).then(|| {
            Press::new("view-search-clear")
                .accessibility_label("Clear search")
                .focus_on_press(false)
                .focus_ring_color(t.colors.focus)
                .debug_selector(|| "view-search-clear".into())
                .flex_none()
                .size(h)
                .flex()
                .items_center()
                .justify_center()
                .rounded(t.px(t.sizes.chip_radius))
                .hover(|s| s.bg(t.colors.hover))
                .child(icon(IconName::X, t.px(t.sizes.small), t.colors.text_muted))
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    if let Some(s) = this.search_input() {
                        s.update(cx, |s, cx| {
                            s.set_value("", window, cx);
                            s.focus(window, cx);
                        });
                    }
                    this.apply_search(String::new(), cx);
                }))
        });
        div()
            .id("gpui-tables-search")
            .debug_selector(|| "view-search-field".into())
            .role(Role::Search)
            .on_action(
                cx.listener(|this, _: &super::Cancel, window, cx| this.search_escape(window, cx)),
            )
            .flex_initial()
            .flex_shrink_1()
            .w(t.px(220.))
            .min_w(t.px(120.))
            .h(h)
            .pl(t.px(8.))
            .when(clear.is_none(), |d| d.pr(t.px(8.)))
            .flex()
            .items_center()
            .gap(t.px(6.))
            .rounded(t.px(t.sizes.chip_radius))
            .border_1()
            .border_color(if self.search.focused {
                t.colors.focus
            } else {
                t.colors.line
            })
            .bg(t.colors.background)
            .child(
                div()
                    .flex_none()
                    .child(icon(IconName::Search, icon_size, t.colors.text_muted)),
            )
            .child(
                Input::new(&state)
                    .appearance(false)
                    .accessibility_label("Search")
                    .caret_color(t.colors.accent)
                    .selection_color(t.colors.accent.opacity(0.3))
                    .placeholder_color(t.colors.text_faint)
                    .flex_1()
                    .min_w_0(),
            )
            .children(clear)
            .into_any_element()
    }

    /// "No results for “…”", where the rows would be, while a search finds nothing. It keeps
    /// the view's keys and focus, so Escape and ⌘F still work from it.
    pub(crate) fn render_no_results(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = &self.theme;
        div()
            .id("gpui-tables-no-results")
            .debug_selector(|| "no-results".into())
            .key_context(super::CONTEXT)
            .track_focus(&self.focus)
            .on_action(
                cx.listener(|this, _: &super::Up, window, cx| this.focus_view_bar(window, cx)),
            )
            .size_full()
            .p(t.px(24.))
            .flex()
            .justify_center()
            .bg(t.colors.background)
            .text_color(t.colors.text_muted)
            .font_family(t.fonts.body.clone())
            .text_size(t.px(t.sizes.text))
            .child(
                line(SharedString::from(format!(
                    "No results for “{}”",
                    self.search.text.trim()
                )))
                .max_w_full()
                .mt(t.px(24.)),
            )
            .into_any_element()
    }

    /// ⌘F: the field opens with the keyboard, unless a value is being typed in place; a host
    /// without the bar of views keeps the key.
    pub(crate) fn find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.bar.on || self.is_editing() {
            cx.propagate();
            return;
        }
        self.open_search(window, cx);
    }
}
