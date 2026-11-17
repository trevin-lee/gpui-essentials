//! The board view's state and what its controls do: its columns (the view's groups), the
//! focused card, a card being dragged, and the edits a drop, a New card or a fold make. The
//! drawing is in `board_render`.
//!
//! A view of type `board` draws through the same [`TableView`] entity as a table: the host
//! keeps one entity and one [`TableHost`](super::TableHost) whatever the view's type, and
//! [`TableView::set_view`] switches between them.
//!
//! A list and a table move their rows by the same rules: their groups are [`BoardColumn`]s too,
//! and the drag state, [`TableView::move_rows`], the drop and the finger's long press here serve
//! all three. Only where a pointer is (which column and slot, or which line) and what picks a row
//! up (a card, a line, a table row's handle) are each view's own.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui::{
    AppContext as _, Bounds, Context, Entity, Pixels, Point, ScrollHandle, ScrollStrategy,
    UniformListScrollHandle, Window, point, px,
};

use super::host::person_label;
use super::ops;
use super::table::TableView;
use crate::engine::{self, Group, GroupKey};
use crate::model::{Doc, Edit, PropertyType, Value, View, new_id};
use crate::order;

/// A column of a board, or a group of a list: one of the view's groups (or, for a view whose
/// `groupBy` is missing or can't group, every row in one).
#[derive(Clone, Debug, PartialEq)]
pub struct BoardColumn {
    /// The group's key; None for the one column of a board that doesn't group.
    pub key: Option<GroupKey>,
    /// Its cards: indexes into [`TableView::rows`], in the view's order.
    pub rows: Vec<usize>,
    /// Folded shut (its key is in the view's `collapsedGroups`): drawn as a line in the
    /// "Collapsed" column at the board's end rather than as a column of cards.
    pub folded: bool,
}

impl BoardColumn {
    /// The column's key as `collapsedGroups` writes it, or `all` for a board that doesn't
    /// group.
    pub fn key_string(&self) -> String {
        self.key
            .as_ref()
            .map_or_else(|| "all".to_owned(), GroupKey::to_string)
    }
}

/// Where the board's keyboard focus is, by column (an index into
/// [`TableView::board_columns`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoardSpot {
    /// A card, by its place in its column.
    Card { column: usize, index: usize },
    /// A column's New button.
    New { column: usize },
    /// A folded column's line in the "Collapsed" column.
    Folded { column: usize },
}

/// The focus by what it's on rather than where, so it follows its card when the rows are
/// worked out again (a drop into another column, the host's new docs).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum SpotId {
    Card(String),
    New,
    Folded,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct BoardFocus {
    pub column: String,
    pub spot: SpotId,
}

/// What GPUI carries while a mouse drags a card.
#[derive(Clone, Debug)]
pub(crate) struct DraggedCard {
    pub row_id: String,
    pub from: String,
}

/// A card being dragged, by the mouse (GPUI's drag and drop) or a finger (a long press).
#[derive(Clone, Debug)]
pub(crate) struct CardDrag {
    pub row_id: String,
    /// The column it was picked up from, by key.
    pub from: String,
    /// The other rows carried with it (a table's selected rows), each with the key of the
    /// group it's in, in the view's order.
    pub others: Vec<(String, String)>,
    /// Where in the card it was taken, from its top left corner.
    pub offset: Point<Pixels>,
    /// The pointer or finger, in the window.
    pub at: Point<Pixels>,
    /// Where it would land: a column and a slot among its cards as drawn (0 is above the first
    /// card, the count is below the last), or None over nowhere it can go.
    pub target: Option<(usize, usize)>,
    pub touch: bool,
    /// What's drawn under a finger (the mouse's is drawn by GPUI).
    pub ghost: Entity<CardGhost>,
}

/// The board's own state, kept across views.
#[derive(Default)]
pub(crate) struct Board {
    /// Each column's scroll, by its key.
    pub lists: HashMap<String, UniformListScrollHandle>,
    /// Sideways.
    pub scroll: ScrollHandle,
    pub focus: Option<BoardFocus>,
    /// The focus as places, worked out once a frame for the columns' lists.
    pub spot: Option<BoardSpot>,
    pub drag: Option<CardDrag>,
    /// Each column's bounds as last drawn, by column (None for a folded one).
    pub columns: Rc<RefCell<Vec<Option<Bounds<Pixels>>>>>,
}

impl Board {
    pub fn list(&mut self, key: &str) -> UniformListScrollHandle {
        self.lists.entry(key.to_owned()).or_default().clone()
    }
}

/// The sizes a board is drawn at, in window pixels.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BoardMetrics {
    pub column_width: Pixels,
    /// Around the board's columns.
    pub pad: Pixels,
    /// Between columns.
    pub gap: Pixels,
    /// Inside a column, around its cards.
    pub inset: Pixels,
    pub header: Pixels,
    pub card: Pixels,
    /// A card and the gap after it: a line of a column's list.
    pub item: Pixels,
    pub card_pad: Pixels,
    pub title: Pixels,
    pub line: Pixels,
    pub line_gap: Pixels,
    pub new: Pixels,
}

/// A card's (or a list's line's) picture while it's dragged: its title and the words of its
/// values.
pub struct CardGhost {
    pub(crate) title: String,
    pub(crate) lines: Vec<String>,
    pub(crate) theme: super::TableTheme,
    pub(crate) width: Pixels,
    pub(crate) height: Pixels,
    /// A list's line: the values beside the title rather than under it.
    pub(crate) inline: bool,
}

/// What a board's columns or a list's groups are: the view's groups, each folded or not, or
/// one of every row when the view doesn't group (or groups by a property that can't).
pub(crate) fn lanes_of(view: &View, groups: &[Group], rows: &[usize]) -> Vec<BoardColumn> {
    if view.group_by.is_some() && !groups.is_empty() {
        groups
            .iter()
            .map(|g| BoardColumn {
                folded: view.collapsed_groups.contains(&g.key.to_string()),
                key: Some(g.key.clone()),
                rows: g.rows.clone(),
            })
            .collect()
    } else {
        vec![BoardColumn {
            key: None,
            rows: rows.to_vec(),
            folded: false,
        }]
    }
}

impl TableView {
    // --- What it shows ----------------------------------------------------------------------

    /// Whether the view is drawn as a board (its type is `board`).
    pub fn is_board(&mut self) -> bool {
        self.ensure();
        self.shown.board.is_some()
    }

    /// The board's columns, in order (empty when the view isn't a board): the empty group's,
    /// then each option's (or person's, or date bucket's) as the engine orders them.
    pub fn board_columns(&mut self) -> &[BoardColumn] {
        self.ensure();
        self.shown.board.as_deref().unwrap_or(&[])
    }

    /// The board's focused card, New button or folded column.
    pub fn focused_card(&mut self) -> Option<BoardSpot> {
        self.ensure();
        self.board_spot()
    }

    /// Focuses a row's card (in the first column it's in), scrolling it into view.
    pub fn focus_card(&mut self, row_id: &str, cx: &mut Context<Self>) {
        self.ensure();
        let Some(cols) = &self.shown.board else {
            return;
        };
        let found = cols.iter().enumerate().find_map(|(ci, c)| {
            (!c.folded)
                .then(|| c.rows.iter().position(|&r| self.rows[r].id == row_id))
                .flatten()
                .map(|index| BoardSpot::Card { column: ci, index })
        });
        if let Some(spot) = found {
            self.set_board_spot(spot, cx);
        }
    }

    /// How far the board is scrolled sideways, in window pixels.
    pub fn board_scroll_x(&self) -> Pixels {
        -self.board.scroll.offset().x
    }

    /// Scrolls the board sideways to `x` window pixels (within what it can).
    pub fn set_board_scroll_x(&mut self, x: Pixels, cx: &mut Context<Self>) {
        let max = self.board.scroll.max_offset().x;
        let y = self.board.scroll.offset().y;
        self.board
            .scroll
            .set_offset(point(-x.clamp(px(0.), max.max(px(0.))), y));
        cx.notify();
    }

    /// Where a row's card is in the window (in the first open column it's in), as the board
    /// last drew it: for a host's popover by a card, say. None when it isn't drawn, and while
    /// the rows changed since (until the board draws them).
    pub fn card_bounds(&self, row_id: &str) -> Option<Bounds<Pixels>> {
        let cols = self.shown.board.as_ref()?;
        let m = self.board_metrics();
        cols.iter().enumerate().find_map(|(ci, c)| {
            let index = c
                .rows
                .iter()
                .position(|&r| self.shown_row(r).is_some_and(|r| r.id == row_id))?;
            let list = self.list_bounds(ci)?;
            let scrolled = self
                .board
                .lists
                .get(&c.key_string())
                .map_or(px(0.), |l| l.0.borrow().base_handle.offset().y);
            let top = list.top() + scrolled + m.item * index as f32 + (m.item - m.card) / 2.;
            let left = list.left() + m.inset;
            Some(Bounds::new(
                point(left, top),
                gpui::size(list.size.width - m.inset * 2., m.card),
            ))
        })
    }

    /// Whether a card is being dragged.
    pub fn is_dragging_card(&self) -> bool {
        self.board.drag.is_some()
    }

    pub(crate) fn board_metrics(&self) -> BoardMetrics {
        let t = &self.theme;
        let rem = self.rem;
        let p = |points: f32| t.pixels(points, rem);
        let card_pad = p(10.);
        let title = p(20.);
        let line = p(t.sizes.small + 10.);
        let line_gap = p(4.);
        let lines = self.shown.card_properties.len() as f32;
        let card = (card_pad * 2. + title + (line_gap + line) * lines + px(2.)).max(px(t.min_hit));
        let gap = p(8.);
        BoardMetrics {
            column_width: p(t.sizes.board_column_width),
            pad: p(12.),
            gap: p(12.),
            inset: p(6.),
            header: t.hit(t.sizes.header_height + 4., rem),
            card,
            item: card + gap,
            card_pad,
            title,
            line,
            line_gap,
            new: t.hit(t.sizes.row_height, rem),
        }
    }

    // --- Focus and keys ---------------------------------------------------------------------

    /// The focus as places, worked out from what it's on: a card that left its column is
    /// followed to the first column it's in now.
    pub(crate) fn board_spot(&self) -> Option<BoardSpot> {
        let cols = self.shown.board.as_ref()?;
        let f = self.board.focus.as_ref()?;
        let ci = cols.iter().position(|c| c.key_string() == f.column);
        match &f.spot {
            SpotId::Card(id) => {
                let find = |c: &BoardColumn| {
                    c.rows
                        .iter()
                        .position(|&r| self.shown_row(r).is_some_and(|r| r.id == *id))
                };
                if let Some(ci) = ci
                    && !cols[ci].folded
                    && let Some(index) = find(&cols[ci])
                {
                    return Some(BoardSpot::Card { column: ci, index });
                }
                cols.iter().enumerate().find_map(|(ci, c)| {
                    (!c.folded)
                        .then(|| find(c))
                        .flatten()
                        .map(|index| BoardSpot::Card { column: ci, index })
                })
            }
            SpotId::New => ci
                .filter(|&ci| !cols[ci].folded)
                .map(|column| BoardSpot::New { column }),
            SpotId::Folded => ci
                .filter(|&ci| cols[ci].folded)
                .map(|column| BoardSpot::Folded { column }),
        }
    }

    fn focus_of(&self, spot: BoardSpot) -> Option<BoardFocus> {
        let cols = self.shown.board.as_ref()?;
        Some(match spot {
            BoardSpot::Card { column, index } => {
                let c = cols.get(column)?;
                BoardFocus {
                    column: c.key_string(),
                    spot: SpotId::Card(self.shown_row(*c.rows.get(index)?)?.id.clone()),
                }
            }
            BoardSpot::New { column } => BoardFocus {
                column: cols.get(column)?.key_string(),
                spot: SpotId::New,
            },
            BoardSpot::Folded { column } => BoardFocus {
                column: cols.get(column)?.key_string(),
                spot: SpotId::Folded,
            },
        })
    }

    pub(crate) fn set_board_spot(&mut self, spot: BoardSpot, cx: &mut Context<Self>) {
        let Some(focus) = self.focus_of(spot) else {
            return;
        };
        self.board.focus = Some(focus);
        self.reveal_spot(spot);
        cx.notify();
    }

    /// The board's lanes, left to right: each open column, then the "Collapsed" column if any
    /// column is folded.
    pub(crate) fn lanes(&self) -> Vec<Option<usize>> {
        let Some(cols) = &self.shown.board else {
            return Vec::new();
        };
        let mut lanes: Vec<Option<usize>> = cols
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.folded)
            .map(|(i, _)| Some(i))
            .collect();
        if cols.iter().any(|c| c.folded) {
            lanes.push(None);
        }
        lanes
    }

    fn folded_columns(&self) -> Vec<usize> {
        self.shown.board.as_ref().map_or_else(Vec::new, |cols| {
            cols.iter()
                .enumerate()
                .filter(|(_, c)| c.folded)
                .map(|(i, _)| i)
                .collect()
        })
    }

    /// A spot as a lane and a place in it (a column's cards then its New button; the folded
    /// columns in the last lane).
    pub(crate) fn spot_place(&self, spot: BoardSpot) -> Option<(usize, usize)> {
        let lanes = self.lanes();
        match spot {
            BoardSpot::Card { column, index } => {
                Some((lanes.iter().position(|l| *l == Some(column))?, index))
            }
            BoardSpot::New { column } => {
                let n = self.shown.board.as_ref()?.get(column)?.rows.len();
                Some((lanes.iter().position(|l| *l == Some(column))?, n))
            }
            BoardSpot::Folded { column } => Some((
                lanes.iter().position(Option::is_none)?,
                self.folded_columns().iter().position(|&c| c == column)?,
            )),
        }
    }

    fn lane_len(&self, lane: Option<usize>) -> usize {
        match lane {
            Some(c) => self
                .shown
                .board
                .as_ref()
                .and_then(|cols| cols.get(c))
                .map_or(0, |c| c.rows.len() + 1),
            None => self.folded_columns().len(),
        }
    }

    fn spot_at(&self, lane: Option<usize>, place: usize) -> Option<BoardSpot> {
        match lane {
            Some(column) => {
                let n = self.shown.board.as_ref()?.get(column)?.rows.len();
                Some(if place < n {
                    BoardSpot::Card {
                        column,
                        index: place,
                    }
                } else {
                    BoardSpot::New { column }
                })
            }
            None => self
                .folded_columns()
                .get(place)
                .map(|&column| BoardSpot::Folded { column }),
        }
    }

    /// The arrows: up and down within a column (its cards, then New), left and right between
    /// columns at the same height.
    pub(crate) fn board_move(&mut self, d_lane: isize, d_place: isize, cx: &mut Context<Self>) {
        self.ensure();
        let lanes = self.lanes();
        if lanes.is_empty() {
            return;
        }
        let Some((lane, place)) = self.board_spot().and_then(|s| self.spot_place(s)) else {
            if let Some(spot) = self.spot_at(lanes[0], 0) {
                self.set_board_spot(spot, cx);
            }
            return;
        };
        let lane = (lane as isize + d_lane).clamp(0, lanes.len() as isize - 1) as usize;
        let len = self.lane_len(lanes[lane]);
        if len == 0 {
            return;
        }
        let place = (place as isize + d_place).clamp(0, len as isize - 1) as usize;
        if let Some(spot) = self.spot_at(lanes[lane], place) {
            self.set_board_spot(spot, cx);
        }
    }

    /// Tab: down the column, on to the top of the next.
    pub(crate) fn board_tab(&mut self, back: bool, cx: &mut Context<Self>) {
        self.ensure();
        let lanes = self.lanes();
        let spots: Vec<BoardSpot> = lanes
            .iter()
            .flat_map(|&l| (0..self.lane_len(l)).map(move |p| (l, p)))
            .filter_map(|(l, p)| self.spot_at(l, p))
            .collect();
        if spots.is_empty() {
            return;
        }
        let now = self
            .board_spot()
            .and_then(|s| spots.iter().position(|x| *x == s));
        let next = match (now, back) {
            (None, _) => 0,
            (Some(i), false) => (i + 1).min(spots.len() - 1),
            (Some(i), true) => i.saturating_sub(1),
        };
        self.set_board_spot(spots[next], cx);
    }

    /// Scrolls a spot into view: its card in its column, its column on the board.
    fn reveal_spot(&mut self, spot: BoardSpot) {
        let Some((lane, _)) = self.spot_place(spot) else {
            return;
        };
        if let BoardSpot::Card { column, index } = spot
            && let Some(key) = self
                .shown
                .board
                .as_ref()
                .and_then(|c| c.get(column))
                .map(BoardColumn::key_string)
        {
            self.board
                .list(&key)
                .scroll_to_item(index, ScrollStrategy::Nearest);
        }
        let m = self.board_metrics();
        let width = self.bounds.get().size.width;
        if width <= px(0.) {
            return;
        }
        let left = m.pad + (m.column_width + m.gap) * lane as f32;
        let right = left + m.column_width + m.pad;
        let x = self.board_scroll_x();
        let to = if left - m.pad < x {
            (left - m.pad).max(px(0.))
        } else if right > x + width {
            right - width
        } else {
            return;
        };
        let y = self.board.scroll.offset().y;
        self.board.scroll.set_offset(point(-to, y));
    }

    /// Enter: opens the focused card, makes a card from New, unfolds a folded column.
    pub(crate) fn board_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ensure();
        match self.board_spot() {
            None => self.board_move(0, 0, cx),
            Some(BoardSpot::Card { column, index }) => {
                if let Some(&row) = self
                    .shown
                    .board
                    .as_ref()
                    .and_then(|c| c.get(column))
                    .and_then(|c| c.rows.get(index))
                {
                    self.open_row(row, window, cx);
                }
            }
            Some(BoardSpot::New { column }) => self.new_card(column, window, cx),
            Some(BoardSpot::Folded { column }) => self.toggle_column(column, window, cx),
        }
    }

    // --- Edits ------------------------------------------------------------------------------

    /// Folds a column (or unfolds it): a `viewChanged` with its key in `collapsedGroups`.
    pub fn toggle_column(&mut self, column: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.ensure();
        let (Some(view), Some(key)) = (
            self.shown.view.clone(),
            self.shown
                .board
                .as_ref()
                .and_then(|c| c.get(column))
                .and_then(|c| c.key.clone()),
        ) else {
            return;
        };
        let folding = !self.shown.board.as_ref().is_some_and(|c| c[column].folded);
        let view = ops::toggle_group(&view, &key, &self.host.limits());
        // The focus goes with the column: to its line among the folded, or back to its New.
        self.board.focus = Some(BoardFocus {
            column: key.to_string(),
            spot: if folding { SpotId::Folded } else { SpotId::New },
        });
        self.change_view(view, window, cx);
    }

    /// Asks for a new card at the end of a column: a `rowCreated` with the column's value (and
    /// what the filter asks for) and an order key after every row's.
    pub fn new_card(&mut self, column: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.ensure();
        let (Some(fields), Some(view), Some(col)) = (
            self.database.database.as_ref(),
            self.shown.view.as_ref(),
            self.shown.board.as_ref().and_then(|c| c.get(column)),
        ) else {
            return;
        };
        let values = ops::prefill(fields, view, col.key.as_ref(), self.shown.today);
        let order = ops::append_order(self.rows.iter().map(|r| r.order.as_str()));
        let row_id = new_id();
        self.board.focus = Some(BoardFocus {
            column: col.key_string(),
            spot: SpotId::Card(row_id.clone()),
        });
        let edit = Edit::RowCreated {
            row_id,
            database_id: self.database.id.clone(),
            order,
            title: String::new(),
            values,
        };
        self.emit_edits(vec![edit], window, cx);
        cx.notify();
    }

    /// What rows move between: a board's columns, a list's groups or a table's.
    pub(crate) fn move_lanes(&self) -> Option<&Vec<BoardColumn>> {
        self.shown
            .board
            .as_ref()
            .or(self.shown.list.as_ref())
            .or(self.shown.table.as_ref())
    }

    /// The groups rows move between, in order: a board's columns, a list's groups or a table's
    /// (one of every row when the view doesn't group). Their places are what
    /// [`TableView::move_card`] and [`TableView::move_rows`] take.
    pub fn row_lanes(&mut self) -> &[BoardColumn] {
        self.ensure();
        self.move_lanes().map_or(&[], Vec::as_slice)
    }

    /// Whether a card (or a list's or table's row) can go from column (or group) `from` to
    /// `to`: within one only when the view has no sort (the sort places it), and across only
    /// when the group property can take the other's value.
    pub(crate) fn can_move(&self, row: usize, from: usize, to: usize) -> bool {
        let Some(cols) = self.move_lanes() else {
            return false;
        };
        let (Some(f), Some(t)) = (cols.get(from), cols.get(to)) else {
            return false;
        };
        if t.folded {
            return false;
        }
        if from == to {
            return !self.is_sorted();
        }
        self.moved_value(row, f.key.as_ref(), t.key.as_ref())
            .is_some()
    }

    /// Whether the view has a sort, which places its rows: then none are ordered by hand.
    pub(crate) fn is_sorted(&self) -> bool {
        self.shown.view.as_ref().is_some_and(|v| !v.sort.is_empty())
    }

    /// Whether rows (each with the group it's in) can go to `to` together: each one can, or
    /// in a sorted view is already there (the sort places it), and at least one moves.
    pub(crate) fn can_move_all(&self, rows: &[(usize, usize)], to: usize) -> bool {
        let sorted = self.is_sorted();
        let mut any = false;
        for &(row, from) in rows {
            if self.can_move(row, from, to) {
                any = true;
            } else if !(sorted && from == to) {
                return false;
            }
        }
        any
    }

    /// The group property's value for a row moved between columns (see [`ops::moved_value`]).
    fn moved_value(
        &self,
        row: usize,
        from: Option<&GroupKey>,
        to: Option<&GroupKey>,
    ) -> Option<(String, Option<Value>)> {
        let (from, to) = (from?, to?);
        let p = self
            .shown
            .view
            .as_ref()
            .and_then(|v| v.group_by.as_deref())
            .and_then(|g| self.database.database.as_ref()?.property(g))?;
        let offset = i64::from(self.host.utc_offset_minutes()) * 60_000;
        let r = self.rows.get(row)?;
        let value = ops::moved_value(p, r.value(&p.id), from, to, self.shown.today, offset)?;
        let value = value.map(|v| self.host.limits().clamp_value(p, v));
        Some((p.id.clone(), value))
    }

    /// Moves a row's card from column `from` to column `to`, to be the `index`th of that
    /// column's other cards (0 is the top), as a drop does. One batch: a `rowsMoved` with an
    /// order key between its new neighbours' (when the view has no sort; a sorted view places
    /// cards itself, as Notion's does), and a `rowValueChanged` setting the group property, when
    /// the column changes. Keys that can't be put between (an older scheme's, two the same)
    /// renumber the whole manual order.
    ///
    /// In a list or a table it moves a row between its groups, by their places in
    /// [`TableView::row_lanes`] (0 and 0 for one that doesn't group), by the same rules.
    pub fn move_card(
        &mut self,
        row_id: &str,
        from: usize,
        to: usize,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_rows(&[(row_id, from)], to, index, window, cx);
    }

    /// Moves several rows, each from the group it's in (by its place in
    /// [`TableView::row_lanes`]), to group `to`, together and in the view's order, to be the
    /// `index`th of that group's other rows: [`TableView::move_card`] for a table's selected
    /// rows. One batch: a `rowsMoved` with keys between their new neighbours'
    /// (`order::n_between`, or the manual order renumbered when those can't be put between),
    /// and a `rowValueChanged` for each row that changes group. In a sorted view only the
    /// group values change, and rows already in `to` stay where the sort puts them.
    pub fn move_rows(
        &mut self,
        rows: &[(&str, usize)],
        to: usize,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ensure();
        let moving = self.moving_rows(rows);
        if moving.is_empty() || !self.can_move_all(&moving, to) {
            return;
        }
        let list = self.shown.board.is_none();
        let Some(cols) = self.move_lanes() else {
            return;
        };
        let sorted = self.is_sorted();
        let ids: Vec<usize> = moving.iter().map(|&(r, _)| r).collect();
        let others: Vec<usize> = cols[to]
            .rows
            .iter()
            .copied()
            .filter(|r| !ids.contains(r))
            .collect();
        let index = index.min(others.len());
        let mut edits = Vec::new();
        // Where they'd be is where they are: nothing to order.
        let mut after: Vec<usize> = others[..index].to_vec();
        after.extend(&ids);
        after.extend(&others[index..]);
        let unmoved = after == cols[to].rows;
        if !sorted && !unmoved && !others.is_empty() {
            let prev = index.checked_sub(1).map(|i| others[i]);
            let next = others.get(index).copied();
            let keys = order::n_between(
                prev.map(|i| self.rows[i].order.as_str()),
                next.map(|i| self.rows[i].order.as_str()),
                ids.len(),
            );
            match keys {
                Ok(keys) => {
                    let (mut row_ids, mut orders) = (Vec::new(), Vec::new());
                    for (&row, key) in ids.iter().zip(keys) {
                        if key != self.rows[row].order {
                            row_ids.push(self.rows[row].id.clone());
                            orders.push(key);
                        }
                    }
                    if !row_ids.is_empty() {
                        edits.push(Edit::RowsMoved { row_ids, orders });
                    }
                }
                Err(_) => edits.extend(self.renumber(&ids, prev, next)),
            }
        }
        for &(row, from) in &moving {
            if from == to {
                continue;
            }
            let (f, t) = (cols[from].key.clone(), cols[to].key.clone());
            if let Some((property_id, value)) = self.moved_value(row, f.as_ref(), t.as_ref())
                && self.rows[row].value(&property_id) != value.as_ref()
            {
                edits.push(Edit::RowValueChanged {
                    row_id: self.rows[row].id.clone(),
                    property_id,
                    value,
                });
            }
        }
        let key = cols[to].key_string();
        let first = self.rows[moving[0].0].id.clone();
        let moved_across = moving[0].1 != to;
        if list {
            // The focus goes with the row, to the group it went to.
            self.focus_line(&first, &key);
        } else if moved_across {
            self.board.focus = Some(BoardFocus {
                column: key,
                spot: SpotId::Card(first),
            });
        }
        self.emit_edits(edits, window, cx);
        cx.notify();
    }

    /// Rows by id, each with the group it's in, in the view's order (the first given first
    /// among equals); those the view hasn't are left out.
    pub(crate) fn moving_rows(&self, rows: &[(&str, usize)]) -> Vec<(usize, usize)> {
        let mut out: Vec<(usize, usize)> = Vec::new();
        for &(id, from) in rows {
            if let Some(row) = self.rows.iter().position(|r| r.id == id)
                && !out.iter().any(|&(r, _)| r == row)
            {
                out.push((row, from));
            }
        }
        if out.len() > 1 {
            let place = |row: usize| self.shown.rows.iter().position(|&r| r == row);
            out.sort_by_key(|&(row, _)| place(row));
        }
        out
    }

    /// The whole manual order renumbered (`order::spread`) with `rows` after `prev` (or
    /// before `next`): a `rowsMoved` of every row whose key changes.
    fn renumber(&self, rows: &[usize], prev: Option<usize>, next: Option<usize>) -> Vec<Edit> {
        let mut all: Vec<usize> = (0..self.rows.len())
            .filter(|&i| !self.rows[i].is_deleted() && !rows.contains(&i))
            .collect();
        engine::sort::manual(&self.rows, &mut all);
        let at = match (prev, next) {
            (Some(p), _) => all
                .iter()
                .position(|&i| i == p)
                .map_or(all.len(), |i| i + 1),
            (None, Some(n)) => all.iter().position(|&i| i == n).unwrap_or(0),
            (None, None) => all.len(),
        };
        all.splice(at..at, rows.iter().copied());
        let keys = order::spread(all.len());
        let (mut row_ids, mut orders) = (Vec::new(), Vec::new());
        for (i, k) in all.into_iter().zip(keys) {
            if self.rows[i].order != k {
                row_ids.push(self.rows[i].id.clone());
                orders.push(k);
            }
        }
        if row_ids.is_empty() {
            Vec::new()
        } else {
            vec![Edit::RowsMoved { row_ids, orders }]
        }
    }

    // --- Dragging ---------------------------------------------------------------------------

    /// Picks a card (or a list's or table's row) up, with `others` (a table's other selected
    /// rows, each with its group's key) carried along: by the mouse (GPUI's drag has begun) or
    /// a finger (a long press). Returns the picture to draw under the pointer.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn begin_card_drag(
        &mut self,
        row_id: &str,
        from: &str,
        others: Vec<(String, String)>,
        offset: Point<Pixels>,
        at: Point<Pixels>,
        touch: bool,
        cx: &mut Context<Self>,
    ) -> Entity<CardGhost> {
        self.ensure();
        let (title, mut lines) = match self.rows.iter().find(|r| r.id == row_id) {
            Some(r) => (self.card_title(r), self.card_lines(r)),
            None => (String::new(), Vec::new()),
        };
        if !others.is_empty() {
            lines = vec![format!("{} rows", others.len() + 1)];
        }
        let inline = self.shown.board.is_none();
        let (width, height) = if self.shown.table.is_some() {
            // A table's row: as wide as its title column, or a menu if that's narrower.
            let menu = self.theme.pixels(self.theme.sizes.menu_width, self.rem);
            (self.column_px(0).max(menu), self.row_height())
        } else if inline {
            self.line_size()
        } else {
            let m = self.board_metrics();
            (m.column_width - m.inset * 2., m.card)
        };
        let ghost = cx.new(|_| CardGhost {
            title,
            lines,
            theme: self.theme.clone(),
            width,
            height,
            inline,
        });
        self.popover = None;
        self.board.drag = Some(CardDrag {
            row_id: row_id.to_owned(),
            from: from.to_owned(),
            others,
            offset,
            at,
            target: None,
            touch,
            ghost: ghost.clone(),
        });
        self.drag_card_to(at, cx);
        ghost
    }

    /// The rows a drag carries, each with the group it was picked up from (by place), the
    /// picked one first.
    pub(crate) fn dragged_rows(&self) -> Vec<(usize, usize)> {
        let (Some(drag), Some(cols)) = (&self.board.drag, self.move_lanes()) else {
            return Vec::new();
        };
        std::iter::once((&drag.row_id, &drag.from))
            .chain(drag.others.iter().map(|(r, k)| (r, k)))
            .filter_map(|(id, key)| {
                let row = self.rows.iter().position(|r| r.id == *id)?;
                let from = cols.iter().position(|c| c.key_string() == *key)?;
                Some((row, from))
            })
            .collect()
    }

    /// The card's title as a card shows it.
    pub(crate) fn card_title(&self, r: &Doc) -> String {
        if r.title.is_empty() {
            "Untitled".into()
        } else {
            r.title.clone()
        }
    }

    /// The card's values as words, those it shows (the empty ones aren't).
    pub(crate) fn card_lines(&self, r: &Doc) -> Vec<String> {
        let Some(fields) = self.database.database.as_ref() else {
            return Vec::new();
        };
        self.shown
            .card_properties
            .iter()
            .filter_map(|&i| fields.properties.get(i))
            .filter_map(|p| {
                let text = self.cell_text(r, p);
                match p.kind {
                    PropertyType::Checkbox => Some(format!(
                        "{} {}",
                        if text == "checked" { "☑" } else { "☐" },
                        p.name
                    )),
                    _ if text.is_empty() => None,
                    _ => Some(text),
                }
            })
            .collect()
    }

    /// The column under a point, by the columns' bounds as last drawn.
    pub(crate) fn column_at(&self, at: Point<Pixels>) -> Option<usize> {
        if !self.bounds.get().contains(&at) {
            return None;
        }
        let cols = self.board.columns.borrow();
        cols.iter()
            .position(|b| b.is_some_and(|b| at.x >= b.left() && at.x < b.right()))
    }

    /// The card under a point: its column and place.
    pub(crate) fn card_at(&self, at: Point<Pixels>) -> Option<(usize, usize, Bounds<Pixels>)> {
        let column = self.column_at(at)?;
        let col = self.shown.board.as_ref()?.get(column)?;
        let list = self.list_bounds(column)?;
        if !list.contains(&at) {
            return None;
        }
        let m = self.board_metrics();
        let scrolled = self
            .board
            .lists
            .get(&col.key_string())
            .map_or(px(0.), |l| l.0.borrow().base_handle.offset().y);
        let y = at.y - list.top() - scrolled;
        let index = (y / m.item).floor();
        if index < 0. || index as usize >= col.rows.len() {
            return None;
        }
        let index = index as usize;
        let top = list.top() + scrolled + m.item * index as f32 + (m.item - m.card) / 2.;
        let card = Bounds::new(
            point(list.left() + m.inset, top),
            gpui::size(list.size.width - m.inset * 2., m.card),
        );
        card.contains(&at).then_some((column, index, card))
    }

    /// Where a card dragged to `at` would land: a column it can go to and a slot among the cards
    /// drawn there. Near the board's edges, or a column's top or bottom, it scrolls them. (A
    /// list's row: the group and slot of the line under it.)
    pub(crate) fn drag_card_to(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        // The rows may have changed during the drag: their places are worked out again first.
        self.ensure();
        if self.shown.board.is_none() {
            self.drag_row_to(at, cx);
            return;
        }
        let Some(drag) = &self.board.drag else {
            return;
        };
        self.auto_scroll(at, drag.at);
        let moving = self.dragged_rows();
        let target = (|| {
            let cols = self.shown.board.as_ref()?;
            let to = self.column_at(at)?;
            if moving.is_empty() || !self.can_move_all(&moving, to) {
                return None;
            }
            let n = cols[to].rows.len();
            let Some(list) = self.list_bounds(to) else {
                return Some((to, n));
            };
            let m = self.board_metrics();
            let scrolled = self
                .board
                .lists
                .get(&cols[to].key_string())
                .map_or(px(0.), |l| l.0.borrow().base_handle.offset().y);
            let y = at.y.clamp(list.top(), list.bottom()) - list.top() - scrolled;
            let slot = (y / m.item).round().max(0.) as usize;
            Some((to, slot.min(n)))
        })();
        if let Some(d) = &mut self.board.drag {
            d.at = at;
            d.target = target;
        }
        cx.notify();
    }

    /// Scrolls the board sideways while a dragged card is near its left or right edge, and a
    /// column while it's near that column's top or bottom: a step for each move of the pointer
    /// toward the edge (there's no timer, so it stops when the pointer does).
    fn auto_scroll(&mut self, at: Point<Pixels>, was: Point<Pixels>) {
        let edge = self.theme.pixels(40., self.rem);
        let step = self.theme.pixels(16., self.rem);
        let b = self.bounds.get();
        if b.size.width <= px(0.) || at == was {
            return;
        }
        let x = self.board_scroll_x();
        let max = self.board.scroll.max_offset().x.max(px(0.));
        let to = if at.x < b.left() + edge {
            (x - step).max(px(0.))
        } else if at.x > b.right() - edge {
            (x + step).min(max)
        } else {
            x
        };
        if to != x {
            let y = self.board.scroll.offset().y;
            self.board.scroll.set_offset(point(-to, y));
        }
        let Some(column) = self.column_at(at) else {
            return;
        };
        let Some(list) = self.list_bounds(column) else {
            return;
        };
        let Some(key) = self
            .shown
            .board
            .as_ref()
            .and_then(|c| c.get(column))
            .map(BoardColumn::key_string)
        else {
            return;
        };
        if let Some(handle) = self.board.lists.get(&key) {
            let base = handle.0.borrow().base_handle.clone();
            let off = base.offset();
            let max = base.max_offset().y.max(px(0.));
            let y = if at.y < list.top() + edge && at.y >= list.top() - edge {
                (off.y + step).min(px(0.))
            } else if at.y > list.bottom() - edge && at.y <= list.bottom() + edge {
                (off.y - step).max(-max)
            } else {
                off.y
            };
            if y != off.y {
                base.set_offset(point(off.x, y));
            }
        }
    }

    /// Lets go of a dragged card (or list's row) where it is: a move, if it's over somewhere it
    /// can go.
    pub(crate) fn drop_card(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // The rows may have changed during the drag: their places are worked out again first.
        self.ensure();
        let moving = self.dragged_rows();
        let Some(drag) = self.board.drag.take() else {
            return;
        };
        cx.notify();
        let Some((to, slot)) = drag.target else {
            return;
        };
        let list = self.shown.board.is_none();
        let Some(cols) = self.move_lanes() else {
            return;
        };
        // The column it was over may be gone, if the rows changed during the drag.
        let Some(dest) = cols.get(to) else {
            return;
        };
        if moving.is_empty() {
            return;
        }
        // The slot counts the moving rows above it when they're in that column; the index
        // doesn't.
        let above = dest
            .rows
            .iter()
            .take(slot)
            .filter(|r| moving.iter().any(|(m, _)| m == *r))
            .count();
        let index = slot.saturating_sub(above);
        let key = dest.key_string();
        let ids: Vec<(String, usize)> = moving
            .iter()
            .map(|&(r, from)| (self.rows[r].id.clone(), from))
            .collect();
        let refs: Vec<(&str, usize)> = ids.iter().map(|(id, f)| (id.as_str(), *f)).collect();
        self.move_rows(&refs, to, index, window, cx);
        if list {
            self.focus_line(&drag.row_id, &key);
        } else {
            self.board.focus = Some(BoardFocus {
                column: key,
                spot: SpotId::Card(drag.row_id),
            });
        }
    }

    /// Escape: puts a dragged card (or list's row) back, else lets go of the focus.
    pub(crate) fn board_cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(d) = self.board.drag.take() {
            if !d.touch {
                cx.stop_active_drag(window);
            }
        } else {
            self.board.focus = None;
            self.cell = None;
        }
        cx.notify();
    }

    /// The words of a card for assistive technology: its title and values.
    pub(crate) fn card_words(&self, r: &Doc) -> String {
        let mut out = self.card_title(r);
        if let Some(fields) = self.database.database.as_ref() {
            for p in self
                .shown
                .card_properties
                .iter()
                .filter_map(|&i| fields.properties.get(i))
            {
                let text = self.cell_text(r, p);
                if !text.is_empty() {
                    out.push_str(&format!(", {}: {text}", p.name));
                }
            }
        }
        out
    }

    /// A column's name as words: the option's, "No Stage", a person's, a date bucket's.
    pub(crate) fn column_name(&self, col: &BoardColumn) -> String {
        let p = self
            .shown
            .view
            .as_ref()
            .and_then(|v| v.group_by.as_deref())
            .and_then(|g| self.database.database.as_ref()?.property(g));
        match &col.key {
            None => "All".into(),
            Some(GroupKey::Empty) => format!("No {}", p.map_or("value", |p| p.name.as_str())),
            Some(GroupKey::Option(id)) => p
                .and_then(|p| p.option(id))
                .map_or_else(|| id.clone(), |o| o.name.clone()),
            Some(GroupKey::Checked(b)) => if *b { "Checked" } else { "Unchecked" }.into(),
            Some(GroupKey::Person(person)) => person_label(self.host.as_ref(), person),
            Some(GroupKey::Date(g)) => super::format::date_group(g),
        }
    }
}
