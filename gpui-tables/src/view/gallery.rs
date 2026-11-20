//! The gallery view: the rows as cards in a grid, as many to a line as the view's width takes,
//! each the board's card (its title and the view's card properties). Pressing a card opens its
//! row (or peeks it); the arrows move between cards and Enter opens one; a card is dragged (the
//! board's gpui-dnd source) to another place in the manual order, and New at the end makes a
//! row. With the view's card preview on its rows' covers (`cardPreview: "cover"`), each card
//! starts with a 16:9 band: the row's cover image (which the host gives, `TableHost::image`) or
//! its colour, and an empty band for a row with neither.
//!
//! Grouped by the view's `groupBy`, as a list is: each group is a header (its value and its
//! count, pressed to fold it) over its own grid of cards and a New that makes a row in it. A card
//! dragged into another group takes that group's value, as a board's card dragged to another
//! column does.
//!
//! The lines (headers, lines of cards and New) differ in height, so the gallery is a `list`
//! (only the lines on screen drawn) rather than a `uniform_list`.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, ElementId, Hsla, ImageSource, InteractiveElement, IntoElement,
    ListOffset, ObjectFit, ParentElement, Pixels, Point, Styled, StyledImage, Window, canvas, div,
    img, list, point, px,
};
use gpui_core::{IconName, Press};

use super::board::{BoardColumn, BoardMetrics};
use super::render::{icon, line};
use super::table::{Item, TableView};
use crate::model::{CardPreview, CardSize, Doc};

/// A small card's width and a large one's, in points (a medium one is the board's).
const CARD_SMALL: f32 = 172.;
const CARD_LARGE: f32 = 340.;

/// Where the gallery's grid was drawn: its top left (inside its padding), the cards a line, a
/// card's width and a line's height (a card and the gap under it), the gap between cards, and
/// the height of the cover band atop each card (zero without one).
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct GalleryGrid {
    pub origin: Point<Pixels>,
    pub per_line: usize,
    pub card_width: Pixels,
    pub item: Pixels,
    pub gap: Pixels,
    pub band: Pixels,
}

/// A line of the gallery, by its group's place in [`TableView::row_lanes`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GalleryLine {
    /// A group's header (only when the view groups).
    Group(usize),
    /// A line of a group's cards, from its `start`th.
    Cards { lane: usize, start: usize },
    /// A group's New (or the gallery's, when it doesn't group).
    New(usize),
}

/// The gallery's lines as last handed to its list, and where each group's first row is among
/// the view's items (what its cards' cells' element ids are made from).
#[derive(Default)]
pub(crate) struct GalleryLines {
    pub lines: Vec<GalleryLine>,
    pub first_item: Vec<usize>,
}

/// A row's cover as a card's band shows it.
pub(crate) enum Cover {
    /// `{"style": "image", "assetId": ..}`.
    Image(String),
    /// `{"style": "solid", "color": "#rrggbb" or "#rrggbbaa"}`.
    Color(Hsla),
}

impl Cover {
    /// A row doc's `cover`, if it's one the band can show.
    pub(crate) fn of(r: &Doc) -> Option<Self> {
        let c = r.cover.as_object()?;
        match c.get("style")?.as_str()? {
            "image" => Some(Self::Image(c.get("assetId")?.as_str()?.to_owned())),
            "solid" => color(c.get("color")?.as_str()?).map(Self::Color),
            _ => None,
        }
    }
}

/// `#rrggbb` or `#rrggbbaa` as a colour.
fn color(s: &str) -> Option<Hsla> {
    let hex = s.strip_prefix('#')?;
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let v = u32::from_str_radix(hex, 16).ok()?;
    match hex.len() {
        6 => Some(gpui::rgb(v).into()),
        8 => Some(gpui::rgba(v).into()),
        _ => None,
    }
}

impl TableView {
    /// Whether the view is drawn as a gallery (its type is `gallery`).
    pub fn is_gallery(&mut self) -> bool {
        self.ensure();
        self.shown.gallery.is_some()
    }

    /// The gallery's groups: the view's, folded or not, or one of every row when it doesn't
    /// group.
    pub(crate) fn gallery_lanes(&self) -> &[BoardColumn] {
        self.shown.gallery.as_deref().unwrap_or(&[])
    }

    /// The gallery's lines with `per` cards a line: each group's header (when it groups), then,
    /// unless it's folded, its lines of cards and its New.
    pub(crate) fn gallery_lines(&self, per: usize) -> Vec<GalleryLine> {
        let lanes = self.gallery_lanes();
        let grouped = lanes.first().is_some_and(|l| l.key.is_some());
        let mut out = Vec::new();
        for (lane, l) in lanes.iter().enumerate() {
            if grouped {
                out.push(GalleryLine::Group(lane));
            }
            if l.folded {
                continue;
            }
            out.extend(
                (0..l.rows.len())
                    .step_by(per.max(1))
                    .map(|start| GalleryLine::Cards { lane, start }),
            );
            out.push(GalleryLine::New(lane));
        }
        out
    }

    /// The card the keyboard is on: its group and its place there. None when it's gone (or
    /// its group is folded).
    pub(crate) fn gallery_spot(&self) -> Option<(usize, usize)> {
        let (id, key) = self.gallery_focus.as_ref()?;
        let lanes = self.gallery_lanes();
        let lane = lanes.iter().position(|l| l.key_string() == *key)?;
        if lanes[lane].folded {
            return None;
        }
        let at = lanes[lane]
            .rows
            .iter()
            .position(|&r| self.rows.get(r).is_some_and(|r| r.id == *id))?;
        Some((lane, at))
    }

    /// Puts the keyboard on the `at`th card of group `lane`: by its row and group, so it stays
    /// on the card as rows come and go.
    pub(crate) fn focus_gallery_card(&mut self, lane: usize, at: usize) {
        let lanes = self.gallery_lanes();
        let Some(l) = lanes.get(lane) else {
            return;
        };
        let Some(r) = l.rows.get(at).and_then(|&r| self.rows.get(r)) else {
            return;
        };
        self.gallery_focus = Some((r.id.clone(), l.key_string()));
    }

    /// The gallery's row the keyboard is on, by place in the rows.
    pub(crate) fn gallery_focused_row(&self) -> Option<usize> {
        let (lane, at) = self.gallery_spot()?;
        self.gallery_lanes()[lane].rows.get(at).copied()
    }

    /// The grid's layout for the view's width now: cards of the view's card size (medium is the
    /// board's column width less its inset), never wider than the view, as many a line as fit
    /// (one at least).
    fn gallery_grid(&self, m: &BoardMetrics) -> GalleryGrid {
        let width = self.bounds.get().size.width;
        let p = |points: f32| self.theme.pixels(points, self.rem);
        let size = self.view().and_then(|v| v.card_size.clone());
        let card_width = match size {
            Some(CardSize::Small) => p(CARD_SMALL),
            Some(CardSize::Large) => p(CARD_LARGE),
            _ => m.column_width - m.inset * 2.,
        }
        .min((width - m.pad * 2.).max(px(1.)));
        let room = (width - m.pad * 2. + m.gap).max(px(0.));
        let per_line = ((room / (card_width + m.gap)).floor() as usize).max(1);
        let band = if self.view().and_then(|v| v.card_preview.as_ref()) == Some(&CardPreview::Cover)
        {
            (card_width * (9. / 16.)).round()
        } else {
            px(0.)
        };
        GalleryGrid {
            origin: self.bounds.get().origin + point(m.pad, m.pad),
            per_line,
            card_width,
            item: m.card + band + m.gap,
            gap: m.gap,
            band,
        }
    }

    /// Hands the gallery's list its lines, when they changed (a row came or went, a group
    /// folded, the width took another number of cards a line), keeping where it was scrolled to.
    fn sync_gallery_lines(&mut self, lines: Vec<GalleryLine>) {
        // A grouped view's items are each group's header, then its rows (unless it's folded)
        // and its New; an ungrouped one's are the rows, then New.
        let grouped = self
            .gallery_lanes()
            .first()
            .is_some_and(|l| l.key.is_some());
        let first_item: Vec<usize> = if grouped {
            (self.shown.items.iter().enumerate())
                .filter_map(|(ix, i)| matches!(i, Item::Group(_)).then_some(ix + 1))
                .collect()
        } else {
            vec![0]
        };
        self.gallery_lines.first_item = first_item;
        if self.gallery_lines.lines != lines {
            let top = self.gallery_list.logical_scroll_top();
            self.gallery_list.reset(lines.len());
            self.gallery_list.scroll_to(ListOffset {
                item_ix: top.item_ix.min(lines.len().saturating_sub(1)),
                offset_in_item: top.offset_in_item,
            });
            self.gallery_lines.lines = lines;
        }
    }

    pub(crate) fn render_gallery(&mut self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        self.forget_ended_drag(cx);
        let t = self.theme.clone();
        let m = self.board_metrics();
        let grid = self.gallery_grid(&m);
        self.gallery_grid.set(grid);
        self.sync_gallery_lines(self.gallery_lines(grid.per_line));
        let entity = cx.entity().downgrade();
        let lines = list(self.gallery_list.clone(), move |ix, _, cx| {
            entity
                .upgrade()
                .map(|view| view.update(cx, |this, cx| this.render_gallery_line(ix, &grid, &m, cx)))
                .unwrap_or_else(|| div().into_any_element())
        })
        .size_full();
        let bounds = self.bounds.clone();
        let probe = canvas(move |b, _, _| bounds.set(b), |_, _, _, _| {})
            .absolute()
            .top_0()
            .left_0()
            .size_full();
        let root = div()
            .id("gpui-tables")
            .debug_selector(|| "gallery".into())
            .key_context(super::CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &super::Left, _, cx| this.gallery_step(-1, 0, cx)))
            .on_action(cx.listener(|this, _: &super::Right, _, cx| this.gallery_step(1, 0, cx)))
            .on_action(cx.listener(|this, _: &super::Up, _, cx| this.gallery_step(0, -1, cx)))
            .on_action(cx.listener(|this, _: &super::Down, _, cx| this.gallery_step(0, 1, cx)))
            .on_action(cx.listener(|this, _: &super::Confirm, window, cx| {
                this.ensure();
                if let Some(row) = this.gallery_focused_row() {
                    this.open_row(row, window, cx);
                }
            }))
            .on_action(
                cx.listener(|this, _: &super::Cancel, window, cx| this.board_cancel(window, cx)),
            )
            .map(|el| self.card_keys(el, cx))
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(t.colors.background)
            .text_color(t.colors.text)
            .font_family(t.fonts.body.clone())
            .text_size(t.px(t.sizes.text))
            .child(lines)
            .child(probe);
        self.card_drop_target(root, cx)
    }

    /// The `ix`th of the gallery's lines, at its height (the first with the gallery's padding
    /// above it).
    fn render_gallery_line(
        &mut self,
        ix: usize,
        grid: &GalleryGrid,
        m: &BoardMetrics,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(&line) = self.gallery_lines.lines.get(ix) else {
            return div().into_any_element();
        };
        let (height, el) = match line {
            GalleryLine::Group(lane) => (m.header, self.render_gallery_group(lane, cx)),
            GalleryLine::Cards { lane, start } => (
                grid.item,
                self.render_gallery_cards(lane, start, grid, m, cx),
            ),
            // The gap under New parts it from the next group.
            GalleryLine::New(lane) => (m.new + m.gap, self.render_gallery_new(lane, m, cx)),
        };
        let top = if ix == 0 { m.pad } else { px(0.) };
        div()
            .h(height + top)
            .pt(top)
            .px(m.pad)
            .child(el)
            .into_any_element()
    }

    /// A line of a group's cards, from its `start`th, with where a dragged card would land.
    fn render_gallery_cards(
        &mut self,
        lane: usize,
        start: usize,
        grid: &GalleryGrid,
        m: &BoardMetrics,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let accent = self.theme.colors.accent;
        let (rows, key) = match self.gallery_lanes().get(lane) {
            Some(l) => (l.rows.clone(), l.key_string()),
            None => return div().into_any_element(),
        };
        let first_item = self
            .gallery_lines
            .first_item
            .get(lane)
            .copied()
            .unwrap_or(0);
        let lifted: Vec<String> = self
            .board
            .drag
            .as_ref()
            .map(|d| {
                std::iter::once(d.row_id.clone())
                    .chain(d.others.iter().map(|(id, _)| id.clone()))
                    .collect()
            })
            .unwrap_or_default();
        // Where a dragged card would land in this group (a sorted view marks the group's
        // header instead).
        let landing = (!self.is_sorted())
            .then(|| self.board.drag.as_ref().and_then(|d| d.target))
            .flatten()
            .filter(|&(l, _)| l == lane)
            .map(|(_, s)| s);
        let focused = self
            .gallery_spot()
            .filter(|&(l, _)| l == lane)
            .map(|(_, at)| at);
        let n = rows.len();
        let end = (start + grid.per_line).min(n);
        let mut cards: Vec<AnyElement> = Vec::new();
        for (at, &row) in rows.iter().enumerate().take(end).skip(start) {
            let lifted_now = lifted.contains(&self.rows[row].id);
            let card = self.render_card(
                lane,
                first_item + at,
                row,
                &key,
                focused == Some(at),
                lifted_now,
                (grid.band > px(0.)).then_some(grid.band),
                m,
                cx,
            );
            cards.push(
                div()
                    .relative()
                    .flex_none()
                    .w(grid.card_width)
                    .child(card)
                    // Where a dragged card would land: a line before this one.
                    .when(landing == Some(at), |d| {
                        d.child(drop_line(accent, grid.gap, false))
                    })
                    // Or after the last.
                    .when(at + 1 == n && landing == Some(n), |d| {
                        d.child(drop_line(accent, grid.gap, true))
                    })
                    .into_any_element(),
            );
        }
        div()
            .size_full()
            .flex()
            .gap(grid.gap)
            .children(cards)
            .into_any_element()
    }

    /// A group's header: its fold, its value and how many rows it has. Pressing it folds or
    /// unfolds the group; while a dragged card would go to it in a sorted view (which places
    /// it), it's marked.
    fn render_gallery_group(&self, lane: usize, cx: &mut Context<Self>) -> AnyElement {
        let t = &self.theme;
        let Some((col, Some(group_key))) = self.gallery_lanes().get(lane).map(|c| (c, &c.key))
        else {
            return div().into_any_element();
        };
        let folded = col.folded;
        let key = col.key_string();
        let count = col.rows.len();
        let name = self.column_name(col);
        let shown = self.shown_serial;
        let target = self.is_sorted()
            && self
                .board
                .drag
                .as_ref()
                .and_then(|d| d.target)
                .is_some_and(|(l, _)| l == lane);
        Press::new(ElementId::NamedInteger("gallery-group".into(), lane as u64))
            .tab_stop(false)
            .focus_on_press(false)
            .focus_ring(false)
            .min_hit(px(t.min_hit))
            .accessibility_label(format!(
                "{name}, {count} rows, {}",
                if folded { "collapsed" } else { "expanded" }
            ))
            .debug_selector(move || format!("group:{key}"))
            .size_full()
            .px(t.pixels(4., self.rem))
            .flex()
            .items_center()
            .gap(t.px(8.))
            .overflow_hidden()
            .rounded(t.px(t.sizes.chip_radius))
            .text_size(t.px(t.sizes.small))
            .hover(|s| s.bg(t.colors.hover))
            .when(target, |d| {
                d.bg(t.colors.selection)
                    .border_1()
                    .border_color(t.colors.accent)
            })
            .child(icon(
                if folded {
                    IconName::ChevronRight
                } else {
                    IconName::ChevronDown
                },
                t.px(t.sizes.icon),
                t.colors.text_muted,
            ))
            .child(div().min_w_0().flex().child(self.group_label(group_key)))
            .child(
                div()
                    .flex_none()
                    .text_color(t.colors.text_muted)
                    .child(count.to_string()),
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                if this.still_shown(shown) {
                    window.focus(&this.focus, cx);
                    this.toggle_group(lane, window, cx);
                }
            }))
            .into_any_element()
    }

    /// New, after a group's cards (or all of them): a row made at the end of the manual order,
    /// with the group's value.
    fn render_gallery_new(
        &mut self,
        lane: usize,
        m: &BoardMetrics,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme.clone();
        let shown = self.shown_serial;
        let lanes = self.gallery_lanes();
        let grouped = lanes.get(lane).is_some_and(|l| l.key.is_some());
        let label = lanes.get(lane).filter(|_| grouped).map_or_else(
            || "New".to_owned(),
            |l| format!("New in {}", self.column_name(l)),
        );
        // An empty group's place to land is its New.
        let landing = !self.is_sorted()
            && lanes.get(lane).is_some_and(|l| l.rows.is_empty())
            && self
                .board
                .drag
                .as_ref()
                .and_then(|d| d.target)
                .is_some_and(|(l, _)| l == lane);
        // The ungrouped gallery's New keeps the id it always had.
        let (id, selector) = if grouped {
            let key = lanes[lane].key_string();
            (
                ElementId::NamedInteger("gallery-new".into(), lane as u64),
                format!("gallery-new:{key}"),
            )
        } else {
            (
                ElementId::Name("gallery-new".into()),
                "gallery-new".to_owned(),
            )
        };
        let group = grouped.then_some(lane);
        div()
            .h(m.new)
            .relative()
            .flex()
            .items_center()
            .child(
                Press::new(id)
                    .accessibility_label(label)
                    .debug_selector(move || selector.clone())
                    .focus_on_press(false)
                    .min_hit(px(t.min_hit))
                    .flex()
                    .items_center()
                    .gap(t.px(6.))
                    .px(t.px(8.))
                    .h_full()
                    .rounded(t.px(t.sizes.chip_radius))
                    .text_color(t.colors.text_muted)
                    .hover(|s| s.bg(t.colors.hover))
                    .child(icon(
                        IconName::Plus,
                        t.px(t.sizes.icon),
                        t.colors.text_muted,
                    ))
                    .child(line("New"))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        if this.still_shown(shown) {
                            this.new_row(group, window, cx);
                        }
                    })),
            )
            .when(landing, |d| d.child(self.landing_bar(true)))
            .into_any_element()
    }

    /// The card lines, each with its line, group, first card and number of cards.
    fn gallery_card_lines(&self) -> Vec<(usize, usize, usize, usize)> {
        let lanes = self.gallery_lanes();
        let per = self.gallery_grid.get().per_line.max(1);
        self.gallery_lines(per)
            .into_iter()
            .enumerate()
            .filter_map(|(ix, l)| match l {
                GalleryLine::Cards { lane, start } => {
                    let n = lanes[lane].rows.len();
                    Some((ix, lane, start, (n - start).min(per)))
                }
                _ => None,
            })
            .collect()
    }

    /// The arrows between the gallery's cards: the next or the one before (on to the next
    /// group's at a group's end), or a line up or down (into the next group's lines too). The
    /// first arrow comes to the first card.
    pub(crate) fn gallery_step(&mut self, across: isize, down: isize, cx: &mut Context<Self>) {
        self.ensure();
        let cards = self.gallery_card_lines();
        if cards.is_empty() {
            return;
        }
        let here = self.gallery_spot().and_then(|(lane, at)| {
            let k = cards
                .iter()
                .position(|&(_, l, start, n)| l == lane && at >= start && at < start + n)?;
            Some((k, at - cards[k].2))
        });
        let (k, col) = match here {
            None => (0, 0),
            Some((k, col)) if across > 0 => {
                if col + 1 < cards[k].3 {
                    (k, col + 1)
                } else if k + 1 < cards.len() {
                    (k + 1, 0)
                } else {
                    (k, col)
                }
            }
            Some((k, col)) if across < 0 => {
                if col > 0 {
                    (k, col - 1)
                } else if k > 0 {
                    (k - 1, cards[k - 1].3 - 1)
                } else {
                    (k, col)
                }
            }
            Some((k, col)) => {
                let k = (k as isize + down).clamp(0, cards.len() as isize - 1) as usize;
                (k, col.min(cards[k].3 - 1))
            }
        };
        let (line, lane, start, _) = cards[k];
        self.focus_gallery_card(lane, start + col);
        self.gallery_list.scroll_to_reveal_item(line);
        cx.notify();
    }

    /// Scrolls the gallery to show the focused card's line.
    pub(crate) fn reveal_gallery_focus(&self) {
        let Some((lane, at)) = self.gallery_spot() else {
            return;
        };
        if let Some(&(line, ..)) = self
            .gallery_card_lines()
            .iter()
            .find(|&&(_, l, start, n)| l == lane && at >= start && at < start + n)
        {
            self.gallery_list.scroll_to_reveal_item(line);
        }
    }

    /// Where a card dragged to `at` would land among the gallery's cards: in the group of the
    /// line under it, before the card under it (after it, past its middle), at a group's start
    /// over its header, or at its end over its New (or below the last line). Within a group only
    /// while the view has no sort, as a board's and a list's manual order; into another group
    /// when it can take that group's value. Near the gallery's top or bottom each move scrolls
    /// a step.
    pub(crate) fn drag_card_in_gallery(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        let was = self.board.drag.as_ref().map_or(at, |d| d.at);
        self.auto_scroll_gallery(at, was);
        let moving = self.dragged_rows();
        let grid = self.gallery_grid.get();
        let target = (|| {
            if moving.is_empty() || !self.bounds.get().contains(&at) {
                return None;
            }
            let lines = &self.gallery_lines.lines;
            let first = self.gallery_list.logical_scroll_top().item_ix;
            // The lines drawn, from the top one; the pointer is over one of them, or past the
            // last.
            let mut under = None;
            let mut last = None;
            for ix in first..lines.len() {
                let Some(b) = self.gallery_list.bounds_for_item(ix) else {
                    break;
                };
                last = Some(ix);
                if at.y < b.bottom() {
                    under = Some(ix);
                    break;
                }
            }
            let lanes = self.gallery_lanes();
            let (lane, slot) = match under {
                Some(ix) => match lines[ix] {
                    GalleryLine::Group(lane) => (lane, 0),
                    GalleryLine::New(lane) => (lane, lanes.get(lane)?.rows.len()),
                    GalleryLine::Cards { lane, start } => {
                        let x = at.x - grid.origin.x;
                        let step = grid.card_width + grid.gap;
                        let col = ((x / step).floor().max(0.) as usize).min(grid.per_line - 1);
                        let past = x - step * col as f32 > grid.card_width / 2.;
                        let n = lanes.get(lane)?.rows.len();
                        (lane, (start + col + usize::from(past)).min(n))
                    }
                },
                // Below the last line: the end of its group.
                None => {
                    let lane = match lines.get(last?)? {
                        GalleryLine::Group(l) | GalleryLine::New(l) => *l,
                        GalleryLine::Cards { lane, .. } => *lane,
                    };
                    (lane, lanes.get(lane)?.rows.len())
                }
            };
            self.can_move_all(&moving, lane).then_some((lane, slot))
        })();
        if let Some(d) = &mut self.board.drag {
            d.at = at;
            d.target = target;
        }
        cx.notify();
    }

    /// Scrolls the gallery a step while a dragged card is near its top or bottom (a step for
    /// each move of the pointer).
    fn auto_scroll_gallery(&mut self, at: Point<Pixels>, was: Point<Pixels>) {
        let b = self.gallery_list.viewport_bounds();
        if at == was || b.size.height <= px(0.) {
            return;
        }
        let edge = self.theme.pixels(40., self.rem);
        let step = self.theme.pixels(16., self.rem);
        if at.y < b.top() + edge && at.y >= b.top() - edge {
            self.gallery_list.scroll_by(-step);
        } else if at.y > b.bottom() - edge && at.y <= b.bottom() + edge {
            self.gallery_list.scroll_by(step);
        }
    }
}

/// The line where a dragged card would land: down the left of the card it goes before (or the
/// right of the last), in the gap.
fn drop_line(accent: gpui::Hsla, gap: Pixels, after: bool) -> impl IntoElement {
    div()
        .absolute()
        .top_0()
        .bottom_0()
        .when(!after, |d| d.left(-(gap / 2.) - px(1.)))
        .when(after, |d| d.right(-(gap / 2.) - px(1.)))
        .w(px(2.))
        .bg(accent)
        .debug_selector(|| "gallery-landing".into())
}

impl TableView {
    /// The band atop a gallery's card: the row's cover image or colour, else empty.
    pub(crate) fn cover_band(&self, r: &Doc, height: Pixels, cx: &gpui::App) -> AnyElement {
        let t = &self.theme;
        let rid = r.id.clone();
        let band = div()
            .debug_selector(move || format!("cover:{rid}"))
            .flex_none()
            .w_full()
            .h(height)
            .overflow_hidden()
            .bg(t.colors.hover);
        match Cover::of(r) {
            Some(Cover::Color(c)) => band.bg(c).into_any_element(),
            Some(Cover::Image(asset)) => match self.host.image(&asset, cx) {
                Some(image) => band
                    .child(
                        img(ImageSource::Image(image))
                            .size_full()
                            .object_fit(ObjectFit::Cover),
                    )
                    .into_any_element(),
                None => band.into_any_element(),
            },
            None => band.into_any_element(),
        }
    }
}
