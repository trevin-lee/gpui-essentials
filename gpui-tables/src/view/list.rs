//! The list view's state and geometry: a line per row (the title, then the view's card
//! properties to the right), under group headers that fold, with a New line at the end of each
//! group. The drawing is in `list_render`.
//!
//! A view of type `list` draws through the same [`TableView`] entity as a table or a board. Its
//! lines are the table's [`Item`]s (group headers, rows and New lines, all one height, so
//! `uniform_list` draws only those on screen), its focus is the table's focused line, and its
//! drags are the board's: the same [`TableView::move_card`], drop and long press, with its
//! groups as the board's columns. Only where a pointer is (which line, and which slot in which
//! group) is its own.

use gpui::{Bounds, Context, Pixels, Point, Window, point, px, size};

use super::board::BoardColumn;
use super::table::{CellRef, Item, TableView};
use crate::model::{Doc, PropertyType, Value};

/// The sizes a list is drawn at, in window pixels.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ListMetrics {
    /// A line's height: a row's, a group header's, a New line's.
    pub line: Pixels,
    /// Around the lines, left and right.
    pub pad: Pixels,
    /// Inside a line, left and right.
    pub inset: Pixels,
    /// Between the title and the values, and between values.
    pub gap: Pixels,
    /// The icon's square before a title.
    pub icon: Pixels,
    /// The widest one value is drawn.
    pub value: Pixels,
    /// The least room a value is drawn in, cut short, rather than left out.
    pub least: Pixels,
}

/// How wide a text of `size` points is likely to be, in points, a little over rather than under
/// (wide characters, emoji and the system font's capitals), so a value thought to fit does.
pub(crate) fn text_width(text: &str, size: f32) -> f32 {
    text.chars()
        .map(|c| match c as u32 {
            // Joiners, variation selectors and combining marks take no room of their own.
            0x200B..=0x200D | 0xFE00..=0xFE0F | 0x0300..=0x036F => 0.,
            0..=0x7F => 0.62,
            // Wide: Hangul, CJK, fullwidth forms, emoji and symbols.
            0x1100..=0x115F | 0x2E80..=0xA4CF | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF => 1.05,
            0xFF00..=0xFF60 | 0x1F000..=0x1FAFF | 0x2600..=0x27BF => 1.15,
            _ => 0.7,
        })
        .sum::<f32>()
        * size
}

impl TableView {
    // --- What it shows ----------------------------------------------------------------------

    /// Whether the view is drawn as a list (its type is `list`).
    pub fn is_list(&mut self) -> bool {
        self.ensure();
        self.shown.list.is_some()
    }

    /// A list's groups, in order (empty when the view isn't a list): one per group of the
    /// view's, folded or not, or one of every row when it doesn't group. Their places are what
    /// [`TableView::move_card`] moves rows between.
    pub fn list_groups(&mut self) -> &[BoardColumn] {
        self.ensure();
        self.shown.list.as_deref().unwrap_or(&[])
    }

    /// Where a row's line is in the window (its first, for a row in several groups), as the
    /// list last drew it: for a host's popover by a row, say. None when it isn't on screen, and
    /// while the rows changed since (until the list draws them).
    pub fn line_bounds(&self, row_id: &str) -> Option<Bounds<Pixels>> {
        self.shown.list.as_ref()?;
        let ix = self.shown.items.iter().position(|i| {
            matches!(*i, Item::Row { row, .. }
                if self.shown_row(row).is_some_and(|r| r.id == row_id))
        })?;
        let b = self.item_bounds(ix)?;
        let list = self.lines_bounds()?;
        (b.bottom() > list.top() && b.top() < list.bottom()).then_some(b)
    }

    pub(crate) fn list_metrics(&self) -> ListMetrics {
        let t = &self.theme;
        let p = |points: f32| t.pixels(points, self.rem);
        ListMetrics {
            line: t.hit(t.sizes.row_height, self.rem),
            pad: p(8.),
            inset: p(t.sizes.cell_padding),
            gap: p(12.),
            icon: p(t.sizes.text + 7.),
            value: p(240.),
            least: p(64.),
        }
    }

    /// A line's width and height, for a dragged row's picture.
    pub(crate) fn line_size(&self) -> (Pixels, Pixels) {
        let m = self.list_metrics();
        let width = (self.bounds.get().size.width - m.pad * 2.).max(m.least);
        (width, m.line)
    }

    /// The lines' bounds (the list's, without its padding), as last drawn.
    pub(crate) fn lines_bounds(&self) -> Option<Bounds<Pixels>> {
        let b = self.list.0.borrow().base_handle.bounds();
        (b.size.height > px(0.)).then_some(b)
    }

    /// How far the lines are scrolled (zero or less), in window pixels.
    fn list_offset(&self) -> Pixels {
        self.list.0.borrow().base_handle.offset().y
    }

    /// A line's height and the room left and right of the lines: a list's, or a table's rows
    /// (their lower borders included, and no room).
    fn line_geometry(&self) -> (Pixels, Pixels) {
        if self.shown.list.is_some() {
            let m = self.list_metrics();
            (m.line, m.pad)
        } else {
            (self.row_height(), px(0.))
        }
    }

    /// A line's bounds in the window, as the list (or a table's rows) is scrolled now.
    pub(crate) fn item_bounds(&self, ix: usize) -> Option<Bounds<Pixels>> {
        let list = self.lines_bounds()?;
        let (line, pad) = self.line_geometry();
        let top = list.top() + self.list_offset() + line * ix as f32;
        Some(Bounds::new(
            point(list.left() + pad, top),
            size(list.size.width - pad * 2., line),
        ))
    }

    /// The line under a point, and how far down it the point is (0 at its top, 1 at its
    /// bottom).
    fn item_at(&self, at: Point<Pixels>) -> Option<(usize, f32)> {
        let list = self.lines_bounds()?;
        if !list.contains(&at) {
            return None;
        }
        let (line, _) = self.line_geometry();
        let y = (at.y - list.top() - self.list_offset()) / line;
        let ix = y.floor();
        if ix < 0. || ix as usize >= self.shown.items.len() {
            return None;
        }
        Some((ix as usize, y - ix))
    }

    /// The row line under a point: its row's id, its group's key and its bounds.
    pub(crate) fn line_at(&self, at: Point<Pixels>) -> Option<(String, String, Bounds<Pixels>)> {
        let (ix, _) = self.item_at(at)?;
        let Item::Row { row, .. } = *self.shown.items.get(ix)? else {
            return None;
        };
        let lane = self.lane_of(ix)?;
        let key = self.move_lanes()?.get(lane)?.key_string();
        let b = self.item_bounds(ix)?;
        let id = self.shown_row(row)?.id.clone();
        b.contains(&at).then_some((id, key, b))
    }

    /// The group (a place in [`TableView::list_groups`]) a line is in.
    pub(crate) fn lane_of(&self, ix: usize) -> Option<usize> {
        Some(match *self.shown.items.get(ix)? {
            Item::Group(g) => g,
            Item::Row { group, .. } => group.unwrap_or(0),
            Item::New(g) | Item::Calc(g) => g.unwrap_or(0),
        })
    }

    /// A row line's place among its group's rows: the lines since its group's header (or since
    /// the top, in a list that doesn't group).
    pub(crate) fn slot_of(&self, ix: usize) -> usize {
        self.shown.items[..ix]
            .iter()
            .rev()
            .take_while(|i| !matches!(i, Item::Group(_)))
            .count()
    }

    // --- Keys -------------------------------------------------------------------------------

    /// Focuses a row's line (a table's row, in the focused column) in the group `key`, and keeps
    /// it there when the rows are worked out again (a row in several groups is focused in this
    /// one).
    pub(crate) fn focus_line(&mut self, row_id: &str, key: &str) {
        let Some(lanes) = self.move_lanes() else {
            return;
        };
        let item = self.shown.items.iter().position(|i| {
            matches!(*i, Item::Row { row, group }
                if self.shown_row(row).is_some_and(|r| r.id == row_id)
                    && lanes.get(group.unwrap_or(0)).is_some_and(|l| l.key_string() == key))
        });
        let item = item.or_else(|| {
            self.shown.items.iter().position(|i| {
                matches!(*i, Item::Row { row, .. }
                    if self.shown_row(row).is_some_and(|r| r.id == row_id))
            })
        });
        if let Some(item) = item {
            let column = self.cell.map_or(0, |c| c.column);
            self.cell = Some(CellRef { item, column });
        }
        self.focus_group = Some((row_id.to_owned(), key.to_owned()));
    }

    /// Enter: opens the focused row, folds or unfolds a group, makes a row from New.
    pub(crate) fn list_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ensure();
        let Some(c) = self.cell else {
            self.move_cell(0, 0, cx);
            return;
        };
        match self.shown.items.get(c.item).copied() {
            Some(Item::Row { row, .. }) => self.open_row(row, window, cx),
            Some(Item::Group(g)) => self.toggle_group(g, window, cx),
            Some(Item::New(g)) => self.new_row(g, window, cx),
            Some(Item::Calc(_)) | None => {}
        }
    }

    /// Page Up and Page Down: a screenful of lines.
    pub(crate) fn list_page(&mut self, d: isize, cx: &mut Context<Self>) {
        let m = self.list_metrics();
        let lines = (self.bounds.get().size.height / m.line).floor().max(1.);
        self.move_cell(d * lines as isize, 0, cx);
    }

    // --- Dragging ---------------------------------------------------------------------------

    /// Where a row dragged to `at` would land, in a list or a table: a group it can go to and
    /// a slot among its rows (0 is above the first; the count, below the last). Over a row's
    /// upper half it's above that row, over its lower half below; over a group's header it's
    /// the group's top, over its New line its end. Near the top or bottom each move scrolls a
    /// step.
    pub(crate) fn drag_row_to(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(drag) = &self.board.drag else {
            return;
        };
        let was = drag.at;
        self.auto_scroll_list(at, was);
        let moving = self.dragged_rows();
        let target = (|| {
            let lanes = self.move_lanes()?;
            if moving.is_empty() || !self.bounds.get().contains(&at) {
                return None;
            }
            let list = self.lines_bounds()?;
            let n = self.shown.items.len();
            // Above the first line (over a table's header) is the top of the first group; past
            // the last, the end of the last.
            let probe = point(at.x.clamp(list.left(), list.right() - px(1.)), at.y);
            let last = n.checked_sub(1)?;
            let (ix, frac) = if at.y < list.top() {
                (0, 0.)
            } else {
                self.item_at(probe).unwrap_or((last, 1.))
            };
            let lane = self.lane_of(ix)?;
            let slot = match self.shown.items[ix] {
                Item::Row { .. } => self.slot_of(ix) + usize::from(frac >= 0.5),
                Item::Group(_) => 0,
                Item::New(_) | Item::Calc(_) => lanes.get(lane)?.rows.len(),
            };
            self.can_move_all(&moving, lane).then_some((lane, slot))
        })();
        if let Some(d) = &mut self.board.drag {
            d.at = at;
            d.target = target;
        }
        cx.notify();
    }

    /// Scrolls the list (or a table's rows) a step while a dragged row is near its top or bottom (a step for each
    /// move of the pointer; there's no timer, so it stops when the pointer does).
    fn auto_scroll_list(&mut self, at: Point<Pixels>, was: Point<Pixels>) {
        let Some(list) = self.lines_bounds() else {
            return;
        };
        if at == was {
            return;
        }
        let edge = self.theme.pixels(40., self.rem);
        let step = self.theme.pixels(16., self.rem);
        let base = self.list.0.borrow().base_handle.clone();
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

    // --- Values -----------------------------------------------------------------------------

    /// The card properties a row's line shows, each with the most room it's drawn in: those
    /// that fit beside the title, in order, after the title has its room (as much as it needs,
    /// up to three fifths of the line). Empty values are left out; the first that doesn't fit
    /// is cut short when there's still some room, and the rest are left off.
    pub(crate) fn line_values(&self, r: &Doc, width: Pixels) -> Vec<(usize, Pixels)> {
        let Some(fields) = self.database.database.as_ref() else {
            return Vec::new();
        };
        let t = &self.theme;
        let m = self.list_metrics();
        let p = |points: f32| t.pixels(points, self.rem);
        let mut inner = width - m.inset * 2.;
        if r.icon().is_some() {
            inner -= m.icon + p(6.);
        }
        let title = p(text_width(&self.card_title(r), t.sizes.text) + 4.);
        let mut room = inner - title.min(inner * 0.6) - m.gap;
        let mut out = Vec::new();
        for &pi in &self.shown.card_properties {
            let Some(prop) = fields.properties.get(pi) else {
                continue;
            };
            let small = t.sizes.small;
            let chip = |name: &str| text_width(name, small) + 14.;
            let need = match prop.kind {
                PropertyType::Checkbox => t.sizes.text + 1. + 6. + text_width(&prop.name, small),
                PropertyType::Select => match r
                    .value(&prop.id)
                    .and_then(Value::option_id)
                    .and_then(|id| prop.option(id))
                {
                    Some(o) => chip(&o.name),
                    None => continue,
                },
                PropertyType::MultiSelect => {
                    let ids = r.value(&prop.id).and_then(Value::option_ids).unwrap_or(&[]);
                    let chips: Vec<f32> = ids
                        .iter()
                        .filter_map(|id| prop.option(id))
                        .map(|o| chip(&o.name))
                        .collect();
                    if chips.is_empty() {
                        continue;
                    }
                    chips.iter().sum::<f32>() + 4. * (chips.len() - 1) as f32
                }
                _ => {
                    let text = self.cell_text(r, prop);
                    if text.is_empty() {
                        continue;
                    }
                    text_width(&text, small)
                }
            };
            let need = p(need).min(m.value);
            let gap = if out.is_empty() { px(0.) } else { m.gap };
            if need + gap <= room {
                out.push((pi, need));
                room -= need + gap;
            } else {
                if room - gap >= m.least {
                    out.push((pi, room - gap));
                }
                break;
            }
        }
        out
    }
}
