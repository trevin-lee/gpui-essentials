//! A table's rows as wholes: the handle (⋮⋮) at each row's left edge, selecting rows, their
//! menu (open, duplicate, move up and down, delete) and the keys for it all.
//!
//! A row's drag comes from gpui-dnd: the handle is its source (the mouse past a few pixels, a
//! finger's long press), the table its target, and gpui-dnd draws the preview and handles
//! Escape. Where the row lands and the move are the board's and the list's (`board.rs` and
//! `list.rs`): the same state, [`TableView::move_rows`] and edge scrolling, with the table's
//! groups as their lanes. A table carries its selected rows along with the one dragged.

use gpui::{
    AnyElement, Bounds, ClickEvent, Context, ElementId, InteractiveElement, IntoElement, Modifiers,
    ParentElement, Pixels, Point, SharedString, Styled, Window, div, point, px, size,
};
use gpui_core::{IconName, Press};
use gpui_dnd::dnd;

use super::a11y::Control;
use super::board::DraggedCard;
use super::render::icon;
use super::table::{Action, CellRef, Entry, Item, Popover, TableView};
use crate::engine::{self, relation};
use crate::model::{Edit, Values, new_id};
use crate::order;

/// What the rows' menu does.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum RowAction {
    Open,
    Duplicate,
    /// Up (-1) or down (1) a place in its group.
    MoveBy(isize),
    /// Deletes them: at once for one row, else asking first.
    Delete,
    /// Deletes them, confirmed.
    ConfirmDelete,
    /// Doesn't delete them after all.
    Cancel,
}

/// How a press changes the table's selected rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Select {
    /// The row alone (a plain press on its handle).
    Only,
    /// The row added, or taken out if it was in (⌘-press, Space).
    Toggle,
    /// Every row from the last one picked to this one (Shift-press).
    Extend,
}

impl TableView {
    // --- Selection --------------------------------------------------------------------------

    /// The selected rows, by id, in the order they were selected.
    pub fn selected_rows(&self) -> &[String] {
        &self.selected
    }

    /// Changes the selected rows as a press on a row's handle does: the row alone, toggled, or
    /// the rows from the last one picked to it.
    pub fn select_row(&mut self, row_id: &str, how: Select, cx: &mut Context<Self>) {
        self.ensure();
        if !self.rows.iter().any(|r| r.id == row_id) {
            return;
        }
        match how {
            Select::Only => {
                self.selected = vec![row_id.to_owned()];
                self.anchor = Some(row_id.to_owned());
            }
            Select::Toggle => {
                match self.selected.iter().position(|id| id == row_id) {
                    Some(i) => {
                        self.selected.remove(i);
                    }
                    None => self.selected.push(row_id.to_owned()),
                }
                self.anchor = Some(row_id.to_owned());
            }
            Select::Extend => {
                let anchor = self.anchor.clone().unwrap_or_else(|| row_id.to_owned());
                self.selected = self.rows_between(&anchor, row_id);
                self.anchor = Some(anchor);
            }
        }
        cx.notify();
    }

    /// Lets go of the selected rows.
    pub(crate) fn clear_selection(&mut self) {
        self.selected.clear();
        self.anchor = None;
    }

    /// The rows from `a` to `b` (either way round) as the table lists them, each once.
    fn rows_between(&self, a: &str, b: &str) -> Vec<String> {
        let place = |id: &str| {
            self.shown
                .items
                .iter()
                .position(|i| matches!(i, Item::Row { row, .. } if self.rows[*row].id == id))
        };
        let (Some(x), Some(y)) = (place(a), place(b)) else {
            return vec![b.to_owned()];
        };
        let mut out: Vec<String> = Vec::new();
        for item in &self.shown.items[x.min(y)..=x.max(y)] {
            if let Item::Row { row, .. } = item {
                let id = &self.rows[*row].id;
                if !out.contains(id) {
                    out.push(id.clone());
                }
            }
        }
        out
    }

    /// Lets go of selected rows the view no longer shows, and closes the rows' menu when one
    /// of its rows is gone.
    pub(crate) fn prune_selection(&mut self) {
        let shown = |s: &Self, id: &str| s.shown.rows.iter().any(|&r| s.rows[r].id == id);
        if !self.selected.is_empty() {
            let keep: Vec<String> = self
                .selected
                .iter()
                .filter(|id| shown(self, id))
                .cloned()
                .collect();
            self.selected = keep;
            if self.anchor.as_deref().is_some_and(|a| !shown(self, a)) {
                self.anchor = None;
            }
        }
        if let Some(Popover::Row { rows, .. }) = &self.popover
            && !rows.iter().all(|id| shown(self, id))
        {
            self.popover = None;
            self.note = None;
        }
    }

    /// The row of the focused line, if it's a row's.
    pub(crate) fn focused_row(&self) -> Option<String> {
        let c = self.cell?;
        match self.shown.items.get(c.item)? {
            Item::Row { row, .. } => Some(self.rows[*row].id.clone()),
            _ => None,
        }
    }

    /// What an action on rows acts on: the selected rows, in the view's order, or else the
    /// focused row.
    pub(crate) fn acted_rows(&self) -> Vec<String> {
        if self.selected.is_empty() {
            return self.focused_row().into_iter().collect();
        }
        let mut out: Vec<String> = Vec::new();
        for &r in &self.shown.rows {
            let id = &self.rows[r].id;
            if self.selected.contains(id) {
                out.push(id.clone());
            }
        }
        out
    }

    // --- Keys -------------------------------------------------------------------------------

    /// Space: selects the focused row, or lets go of it.
    pub(crate) fn toggle_focused_row(&mut self, cx: &mut Context<Self>) {
        self.ensure();
        if let Some(id) = self.focused_row() {
            self.select_row(&id, Select::Toggle, cx);
        }
    }

    /// Shift-Up and Shift-Down: the focus moves a line and the selection runs from where it
    /// started to there.
    pub(crate) fn extend_selection(&mut self, d: isize, cx: &mut Context<Self>) {
        self.ensure();
        let Some(start) = self.focused_row() else {
            self.move_cell(d, 0, cx);
            return;
        };
        if self.selected.is_empty() || self.anchor.is_none() {
            self.selected = vec![start.clone()];
            self.anchor = Some(start);
        }
        self.move_cell(d, 0, cx);
        if let (Some(to), Some(anchor)) = (self.focused_row(), self.anchor.clone()) {
            self.selected = self.rows_between(&anchor, &to);
        }
        cx.notify();
    }

    /// After Up or Down moved the focus, a selection follows it: the row it's on now, alone
    /// (or nothing, on a group's header or a New row).
    pub(crate) fn selection_follows_focus(&mut self) {
        if self.selected.is_empty() {
            return;
        }
        match self.focused_row() {
            Some(id) => {
                self.selected = vec![id.clone()];
                self.anchor = Some(id);
            }
            None => self.clear_selection(),
        }
    }

    /// Delete and Backspace with rows selected, and ⌘⌫: deletes the rows (one at once, more
    /// after asking).
    pub(crate) fn ask_delete(
        &mut self,
        rows: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match rows.len() {
            0 => {}
            1 => {
                let ids: Vec<&str> = rows.iter().map(String::as_str).collect();
                self.delete_rows(&ids, window, cx);
            }
            _ => self.open_rows_popover(rows, None, true, cx),
        }
    }

    // --- The menu ---------------------------------------------------------------------------

    /// Opens the menu of the selected rows (or the focused row), hanging from the first one's
    /// handle, as ⌘/ does.
    pub fn open_row_menu(&mut self, cx: &mut Context<Self>) {
        self.ensure();
        let rows = self.acted_rows();
        if !rows.is_empty() {
            self.open_rows_popover(rows, None, false, cx);
        }
    }

    pub(crate) fn open_rows_popover(
        &mut self,
        rows: Vec<String>,
        at: Option<Point<Pixels>>,
        confirm: bool,
        cx: &mut Context<Self>,
    ) {
        self.date = None;
        self.close_property_menu();
        self.menu_serial += 1;
        self.note = self.sorted_note();
        self.popover = Some(Popover::Row { rows, at, confirm });
        cx.notify();
    }

    /// Why rows can't be put in order by hand: the view sorts them.
    pub(crate) fn sorted_note(&self) -> Option<String> {
        let view = self.shown.view.as_ref()?;
        let first = view.sort.first()?;
        let name = self
            .database
            .database
            .as_ref()
            .and_then(|f| f.property(&first.property_id))
            .map_or("a property", |p| p.name.as_str());
        // A long name is cut short, so the note wraps in a menu's width.
        let short: String = name.chars().take(24).collect();
        let name = if short.len() < name.len() {
            format!("{short}…")
        } else {
            short
        };
        Some(format!("Sorted by {name}: the sort places rows"))
    }

    /// A press on a row's handle: the row selected (alone, toggled with ⌘, or a run with Shift)
    /// and, for a plain press, its menu opened under the handle.
    pub(crate) fn press_handle(
        &mut self,
        ix: usize,
        row_id: &str,
        modifiers: Modifiers,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus, cx);
        let column = self.cell.map_or(0, |c| c.column);
        self.cell = Some(CellRef { item: ix, column });
        if modifiers.shift {
            self.select_row(row_id, Select::Extend, cx);
        } else if modifiers.platform || modifiers.control {
            self.select_row(row_id, Select::Toggle, cx);
        } else {
            self.select_row(row_id, Select::Only, cx);
            self.open_row_menu(cx);
        }
    }

    /// A right-click on a row: its menu at the pointer, for the selected rows when it's one of
    /// them, else for it alone.
    pub(crate) fn context_menu(
        &mut self,
        ix: usize,
        row_id: &str,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus, cx);
        self.finish_editor(true, false, window, cx);
        if !self.selected.iter().any(|id| id == row_id) {
            self.select_row(row_id, Select::Only, cx);
        }
        let column = self.cell.map_or(0, |c| c.column);
        self.cell = Some(CellRef { item: ix, column });
        let rows = self.acted_rows();
        self.open_rows_popover(rows, Some(at), false, cx);
    }

    /// The rows' menu: Open (for one row), Duplicate, Move up and down (not in a sorted view,
    /// nor past their group's ends), Delete; or, deleting several, whether to.
    pub(crate) fn row_entries(&self) -> Vec<Entry> {
        use gpui_core::IconName as I;
        let Some(Popover::Row { rows, confirm, .. }) = &self.popover else {
            return Vec::new();
        };
        let act = |label: &str, a: RowAction| Entry::act(label, Action::Row(a));
        let n = rows.len();
        if *confirm {
            return vec![
                Entry::heading(format!("Delete {n} rows?")),
                act(&format!("Delete {n} rows"), RowAction::ConfirmDelete).icon(I::Trash),
                act("Cancel", RowAction::Cancel)
                    .icon(I::X)
                    .highlighted(true),
            ];
        }
        let mut out = Vec::new();
        if n > 1 {
            out.push(Entry::heading(format!("{n} rows")));
        } else {
            out.push(act("Open", RowAction::Open).icon(I::Maximize2));
        }
        out.push(
            act("Duplicate", RowAction::Duplicate)
                .icon(I::Copy)
                .hint("⌘D"),
        );
        let ids: Vec<&str> = rows.iter().map(String::as_str).collect();
        out.push(
            act("Move up", RowAction::MoveBy(-1))
                .icon(I::ArrowUp)
                .hint("⌘⇧↑")
                .disabled(self.place_by(&ids, -1).is_none()),
        );
        out.push(
            act("Move down", RowAction::MoveBy(1))
                .icon(I::ArrowDown)
                .hint("⌘⇧↓")
                .disabled(self.place_by(&ids, 1).is_none()),
        );
        out.push(act("Delete", RowAction::Delete).icon(I::Trash).hint("⌘⌫"));
        out
    }

    /// Does what the rows' menu says.
    pub(crate) fn activate_row(
        &mut self,
        action: RowAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(Popover::Row { rows, .. }) = self.popover.clone() else {
            return;
        };
        self.popover = None;
        self.note = None;
        window.focus(&self.focus, cx);
        let ids: Vec<&str> = rows.iter().map(String::as_str).collect();
        match action {
            RowAction::Open => {
                if let Some(row) = self.rows.iter().position(|r| r.id == rows[0]) {
                    self.open_row(row, window, cx);
                }
            }
            RowAction::Duplicate => self.duplicate_rows(&ids, window, cx),
            RowAction::MoveBy(d) => self.move_rows_by(&ids, d, window, cx),
            RowAction::Delete => self.ask_delete(rows.clone(), window, cx),
            RowAction::ConfirmDelete => self.delete_rows(&ids, window, cx),
            RowAction::Cancel => {}
        }
        cx.notify();
    }

    // --- Edits ------------------------------------------------------------------------------

    /// Deletes rows: one `rowsDeleted`, which the host applies by setting each one's `deleted`
    /// (and can undo by setting it back). The focus goes to the row after them (or before),
    /// and the selection is let go.
    pub fn delete_rows(&mut self, row_ids: &[&str], window: &mut Window, cx: &mut Context<Self>) {
        self.ensure();
        let ids: Vec<String> = row_ids
            .iter()
            .filter(|id| self.rows.iter().any(|r| r.id == **id && !r.is_deleted()))
            .map(|id| (*id).to_owned())
            .collect();
        if ids.is_empty() {
            return;
        }
        // The focus moves off the rows going: to the next row that stays, else the one before.
        if let Some(c) = self.cell
            && let Some(Item::Row { row, .. }) = self.shown.items.get(c.item)
            && ids.contains(&self.rows[*row].id)
        {
            let stays =
                |i: &Item| matches!(i, Item::Row { row, .. } if !ids.contains(&self.rows[*row].id));
            let next = self.shown.items[c.item..]
                .iter()
                .position(stays)
                .map(|p| c.item + p)
                .or_else(|| self.shown.items[..c.item].iter().rposition(stays));
            self.cell = next.map(|item| CellRef { item, ..c });
        }
        self.clear_selection();
        self.emit_edits(vec![Edit::RowsDeleted { row_ids: ids }], window, cx);
        cx.notify();
    }

    /// Duplicates rows, each copy right after its row: a `rowCreated` for each with a new id,
    /// the row's title and values (its derived ones aside) and an order key between the row's
    /// and the next one's (the manual order renumbered in a `rowsMoved` first, when those can't
    /// be put between). A two-way relation's values are copied too, with a `relationChanged`
    /// for each, so the rows on the other side list the copy. One batch; the first copy is
    /// focused when the host hands it back.
    pub fn duplicate_rows(
        &mut self,
        row_ids: &[&str],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ensure();
        let Some(fields) = self.database.database.clone() else {
            return;
        };
        let limits = self.host.limits();
        let mut originals: Vec<usize> = Vec::new();
        for id in row_ids {
            if let Some(r) = self
                .rows
                .iter()
                .position(|r| r.id == *id && !r.is_deleted())
                && !originals.contains(&r)
            {
                originals.push(r);
            }
        }
        if originals.is_empty() {
            return;
        }
        let mut manual: Vec<usize> = (0..self.rows.len())
            .filter(|&i| !self.rows[i].is_deleted())
            .collect();
        engine::sort::manual(&self.rows, &mut manual);
        let copies: Vec<String> = originals.iter().map(|_| new_id()).collect();
        // Each copy between its row and the row after it in the manual order.
        let keys: Result<Vec<String>, _> = originals
            .iter()
            .map(|&r| {
                let at = manual.iter().position(|&i| i == r);
                let next = at
                    .and_then(|p| manual.get(p + 1))
                    .map(|&i| self.rows[i].order.as_str());
                order::between(Some(self.rows[r].order.as_str()), next)
            })
            .collect();
        let mut edits = Vec::new();
        let keys = match keys {
            Ok(keys) => keys,
            Err(_) => {
                // Keys that can't be put between: the whole order renumbered, copies included.
                enum Slot {
                    Row(usize),
                    Copy(usize),
                }
                let mut all: Vec<Slot> = Vec::new();
                for &i in &manual {
                    all.push(Slot::Row(i));
                    all.extend(
                        originals
                            .iter()
                            .enumerate()
                            .filter(|&(_, &o)| o == i)
                            .map(|(k, _)| Slot::Copy(k)),
                    );
                }
                let fresh = order::spread(all.len());
                let mut keys = vec![String::new(); originals.len()];
                let (mut row_ids, mut orders) = (Vec::new(), Vec::new());
                for (slot, key) in all.into_iter().zip(fresh) {
                    match slot {
                        Slot::Row(i) if self.rows[i].order != key => {
                            row_ids.push(self.rows[i].id.clone());
                            orders.push(key);
                        }
                        Slot::Row(_) => {}
                        Slot::Copy(k) => keys[k] = key,
                    }
                }
                if !row_ids.is_empty() {
                    edits.push(Edit::RowsMoved { row_ids, orders });
                }
                keys
            }
        };
        let mut back = Vec::new();
        for ((&r, copy), order) in originals.iter().zip(&copies).zip(keys) {
            let row = &self.rows[r];
            let mut values = Values::new();
            for (pid, v) in row.values.iter().flatten() {
                if fields.property(pid).is_some_and(|p| p.kind.is_derived()) {
                    continue;
                }
                values.insert(pid.clone(), v.clone());
                // The other side of a two-way relation lists the copy too.
                if let Some(p) = fields.property(pid)
                    && let Some(ids) = v.as_ids()
                {
                    back.extend(
                        relation::set(copy, p, &[], ids.to_vec(), &limits)
                            .into_iter()
                            .filter(|e| matches!(e, Edit::RelationChanged { .. })),
                    );
                }
            }
            edits.push(Edit::RowCreated {
                row_id: copy.clone(),
                database_id: self.database.id.clone(),
                order,
                title: limits.clamp_title(&row.title).to_owned(),
                values,
            });
        }
        edits.extend(back);
        self.clear_selection();
        self.pending_row = copies.first().cloned();
        self.emit_edits(edits, window, cx);
        cx.notify();
    }

    /// Where rows moved a place up (-1) or down (1) in their group would go: the group and
    /// the index among its other rows, as [`TableView::move_rows`] takes them. None in a sorted
    /// view, for rows in different groups, or at the group's end.
    fn place_by(&self, row_ids: &[&str], d: isize) -> Option<(usize, usize)> {
        if self.is_sorted() || row_ids.is_empty() {
            return None;
        }
        let lanes = self.move_lanes()?;
        let rows: Vec<usize> = row_ids
            .iter()
            .filter_map(|id| self.rows.iter().position(|r| r.id == *id))
            .collect();
        // The group of the focused line when it's one of them, else the first that has them
        // all.
        let focused = self.cell.and_then(|c| match self.shown.items.get(c.item)? {
            Item::Row { row, group } if rows.contains(row) => Some(group.unwrap_or(0)),
            _ => None,
        });
        let has_all = |l: usize| rows.iter().all(|r| lanes[l].rows.contains(r));
        let lane = focused
            .filter(|&l| has_all(l))
            .or_else(|| (0..lanes.len()).find(|&l| has_all(l)))?;
        let places: Vec<usize> = lanes[lane]
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| rows.contains(r))
            .map(|(i, _)| i)
            .collect();
        let (first, last) = (*places.first()?, *places.last()?);
        let others = lanes[lane].rows.len() - places.len();
        let index = if d < 0 {
            first.checked_sub(1)?
        } else {
            // Past the next other row: the others before the last one, and that one.
            let before = last + 1 - places.len();
            (before < others).then_some(before + 1)?
        };
        Some((lane, index))
    }

    /// Moves rows a place up (-1) or down (1) in their group, as the menu's Move up and Move
    /// down and ⌘⇧↑ and ⌘⇧↓ do: a `rowsMoved`, as a drag there would make. Nothing in a sorted
    /// view or at the group's end.
    pub fn move_rows_by(
        &mut self,
        row_ids: &[&str],
        d: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ensure();
        let Some((lane, index)) = self.place_by(row_ids, d) else {
            return;
        };
        let pairs: Vec<(&str, usize)> = row_ids.iter().map(|id| (*id, lane)).collect();
        self.move_rows(&pairs, lane, index, window, cx);
    }

    // --- Dragging ---------------------------------------------------------------------------

    /// The rows a drag of `row_id` carries besides it: the other selected rows when it's one
    /// of them (each with its group's key), else none, the selection let go.
    pub(crate) fn carried_with(&mut self, row_id: &str) -> Vec<(String, String)> {
        self.ensure();
        if self.shown.table.is_none() {
            return Vec::new();
        }
        if !self.selected.iter().any(|id| id == row_id) {
            self.clear_selection();
            return Vec::new();
        }
        let Some(lanes) = self.move_lanes() else {
            return Vec::new();
        };
        self.acted_rows()
            .into_iter()
            .filter(|id| id != row_id)
            .filter_map(|id| {
                let row = self.rows.iter().position(|r| r.id == id)?;
                let lane = lanes.iter().find(|l| l.rows.contains(&row))?;
                Some((id, lane.key_string()))
            })
            .collect()
    }

    /// The handle's width: room for its grip, or a finger's 44 points.
    pub(crate) fn handle_width(&self) -> Pixels {
        self.theme.hit(22., self.rem)
    }

    /// Where a row's handle is in the window, as the table last drew it: for a host's popover
    /// by a row, say. None when the row isn't on screen.
    pub fn handle_bounds(&self, row_id: &str) -> Option<Bounds<Pixels>> {
        self.shown.table.as_ref()?;
        let ix = self
            .shown
            .items
            .iter()
            .position(|i| matches!(i, Item::Row { row, .. } if self.rows[*row].id == row_id))?;
        self.item_handle(ix)
    }

    fn item_handle(&self, ix: usize) -> Option<Bounds<Pixels>> {
        let b = self.item_bounds(ix)?;
        let list = self.lines_bounds()?;
        let handle = Bounds::new(
            point(self.bounds.get().origin.x, b.top()),
            size(self.handle_width(), self.row_height() - px(1.)),
        );
        (b.bottom() > list.top() && b.top() < list.bottom()).then_some(handle)
    }

    /// Notes the row a pointer is over, whose handle is made (and the one it left, whose
    /// handle goes): a redraw only when that changes.
    pub(crate) fn hover_row(&mut self, row_id: &str, over: bool, cx: &mut Context<Self>) {
        if over {
            if self.hovered_row.as_deref() != Some(row_id) {
                self.hovered_row = Some(row_id.to_owned());
                cx.notify();
            }
        } else if self.hovered_row.as_deref() == Some(row_id) {
            self.hovered_row = None;
            cx.notify();
        }
    }

    /// A row's handle: ⋮⋮ at its left edge, made for the row a pointer is over (always on
    /// touch, and on a focused, selected or dragged row). A press selects the row and opens its menu (⌘ and Shift add to the
    /// selection instead); a drag (a long press, by finger) moves it and the selected rows with
    /// it.
    pub(crate) fn render_handle(
        &self,
        ix: usize,
        row: usize,
        lane_key: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = &self.theme;
        let r = &self.rows[row];
        let rid = r.id.clone();
        let sel = format!("handle:{rid}");
        let label = format!("{}: select, move or open its menu", self.card_title(r));
        let pressed = rid.clone();
        let shown = self.shown_serial;
        let press = Press::new(ElementId::NamedInteger("handle".into(), ix as u64))
            .tab_stop(false)
            .focus_on_press(false)
            .focus_ring(false)
            .min_hit(px(t.min_hit))
            .accessibility_label(SharedString::from(label))
            .debug_selector(move || sel)
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .cursor_grab()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(t.px(18.))
                    .rounded(t.px(t.sizes.chip_radius))
                    .hover(|s| s.bg(t.colors.hover))
                    .child(icon(
                        IconName::GripVertical,
                        t.px(t.sizes.icon),
                        t.colors.text_muted,
                    )),
            )
            .on_click(cx.listener(move |this, e: &ClickEvent, window, cx| {
                cx.stop_propagation();
                // A press on lines since worked out again may be on another row's place.
                if this.still_shown(shown) {
                    this.press_handle(ix, &pressed, e.modifiers(), window, cx);
                }
            }));
        // The handle is the row's drag source (gpui-dnd): by the mouse past a few pixels, or a
        // finger's long press. The table takes it up as its own drag when it begins.
        dnd(ElementId::Name(
            format!("gpui-tables-row:{rid}:{lane_key}").into(),
        ))
        .flex_none()
        .w(self.handle_width())
        .h_full()
        .drop_indicator(false)
        .shared_source(
            &self.card_source(cx),
            DraggedCard {
                row_id: rid,
                from: lane_key,
            },
        )
        .child(press)
        .into_any_element()
    }

    /// While rows are dragged in a sorted view: that the sort places them, so they can only
    /// change group.
    pub(crate) fn render_drag_note(&self) -> Option<AnyElement> {
        self.board.drag.as_ref()?;
        let note = self.sorted_note()?;
        let t = &self.theme;
        Some(
            div()
                .absolute()
                .bottom(t.px(12.))
                .left_0()
                .right_0()
                .flex()
                .justify_center()
                .child(
                    div()
                        .id("gpui-tables-drag-note")
                        .debug_selector(|| "drag-note".into())
                        .control(
                            module_path!(),
                            gpui::Role::Status,
                            SharedString::from(note.clone()),
                        )
                        .max_w(t.px(480.))
                        .min_w_0()
                        .px(t.px(10.))
                        .py(t.px(6.))
                        .rounded(t.px(t.sizes.radius))
                        .bg(t.colors.popover)
                        .border_1()
                        .border_color(t.colors.popover_border)
                        .text_size(t.px(t.sizes.small))
                        .text_color(t.colors.text_muted)
                        .overflow_hidden()
                        .child(super::render::line(note)),
                )
                .into_any_element(),
        )
    }
}
