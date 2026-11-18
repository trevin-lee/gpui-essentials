//! The gallery view: the rows as cards in a grid, as many to a line as the view's width takes,
//! each the board's card (its title and the view's card properties). Pressing a card opens its
//! row (or peeks it); the arrows move between cards and Enter opens one; a card is dragged (the
//! board's gpui-dnd source) to another place in the manual order, and New at the end makes a
//! row. Grouping isn't drawn in a gallery yet: its rows are one grid.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, InteractiveElement, IntoElement, ParentElement, Pixels, Point,
    Styled, Window, canvas, div, point, px, uniform_list,
};
use gpui_core::{IconName, Press};

use super::board::BoardMetrics;
use super::render::{icon, line};
use super::table::{Item, TableView};

/// Where the gallery's grid was drawn: its top left (inside its padding), the cards a line, a
/// card's width and a line's height (a card and the gap under it), and the gap between cards.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct GalleryGrid {
    pub origin: Point<Pixels>,
    pub per_line: usize,
    pub card_width: Pixels,
    pub item: Pixels,
    pub gap: Pixels,
}

impl TableView {
    /// Whether the view is drawn as a gallery (its type is `gallery`).
    pub fn is_gallery(&mut self) -> bool {
        self.ensure();
        self.shown.gallery.is_some()
    }

    /// The gallery's cards, by place in the rows.
    pub(crate) fn gallery_rows(&self) -> Vec<usize> {
        self.shown
            .gallery
            .as_ref()
            .and_then(|lanes| lanes.first())
            .map(|l| l.rows.clone())
            .unwrap_or_default()
    }

    /// The grid's layout for the view's width now: cards of the board's column width less its
    /// inset, as many a line as fit (one at least).
    fn gallery_grid(&self, m: &BoardMetrics) -> GalleryGrid {
        let width = self.bounds.get().size.width;
        let card_width = m.column_width - m.inset * 2.;
        let room = (width - m.pad * 2. + m.gap).max(px(0.));
        let per_line = ((room / (card_width + m.gap)).floor() as usize).max(1);
        GalleryGrid {
            origin: self.bounds.get().origin + point(m.pad, m.pad),
            per_line,
            card_width,
            item: m.card + m.gap,
            gap: m.gap,
        }
    }

    pub(crate) fn render_gallery(&mut self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        self.forget_ended_drag(cx);
        let t = self.theme.clone();
        let m = self.board_metrics();
        let grid = self.gallery_grid(&m);
        self.gallery_grid.set(grid);
        let rows = self.gallery_rows();
        // Each card's line in the view's items (what its cells' element ids are made from).
        let item_of: Vec<usize> = rows
            .iter()
            .map(|&row| {
                self.shown
                    .items
                    .iter()
                    .position(|i| matches!(i, Item::Row { row: r, .. } if *r == row))
                    .unwrap_or(0)
            })
            .collect();
        let per = grid.per_line;
        // The lines of cards, then New on a line of its own after them.
        let lines = rows.len().div_ceil(per) + 1;
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
        let landing = self
            .board
            .drag
            .as_ref()
            .and_then(|d| d.target)
            .map(|(_, s)| s);
        let focused = self.gallery_focus;
        let entity = cx.entity().downgrade();
        let accent = t.colors.accent;
        let list = uniform_list("gpui-tables-gallery", lines, move |range, _, cx| {
            let Some(view) = entity.upgrade() else {
                return Vec::new();
            };
            view.update(cx, |this, cx| {
                range
                    .map(|line_ix| {
                        let start = line_ix * per;
                        if start >= rows.len() {
                            return this.render_gallery_new(&m, cx);
                        }
                        let end = (start + per).min(rows.len());
                        let mut cards: Vec<AnyElement> = Vec::new();
                        for at in start..end {
                            let row = rows[at];
                            let lifted_now = lifted.contains(&this.rows[row].id);
                            let card = this.render_card(
                                0,
                                item_of[at],
                                row,
                                "all",
                                focused == Some(at),
                                lifted_now,
                                &m,
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
                                    .when(
                                        at + 1 == rows.len() && landing == Some(rows.len()),
                                        |d| d.child(drop_line(accent, grid.gap, true)),
                                    )
                                    .into_any_element(),
                            );
                        }
                        div()
                            .h(grid.item)
                            .px(m.pad)
                            .flex()
                            .gap(grid.gap)
                            .children(cards)
                            .into_any_element()
                    })
                    .collect()
            })
        })
        .track_scroll(&self.list)
        .pt(m.pad)
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
                let rows = this.gallery_rows();
                if let Some(&row) = this.gallery_focus.and_then(|at| rows.get(at)) {
                    this.open_row(row, window, cx);
                }
            }))
            .on_action(
                cx.listener(|this, _: &super::Cancel, window, cx| this.board_cancel(window, cx)),
            )
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(t.colors.background)
            .text_color(t.colors.text)
            .font_family(t.fonts.body.clone())
            .text_size(t.px(t.sizes.text))
            .child(list)
            .child(probe);
        self.card_drop_target(root, cx)
    }

    /// New, after the cards: a row made at the end of the manual order.
    fn render_gallery_new(&mut self, m: &BoardMetrics, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme.clone();
        let shown = self.shown_serial;
        div()
            .h(m.new)
            .px(m.pad)
            .flex()
            .items_center()
            .child(
                Press::new("gallery-new")
                    .accessibility_label("New")
                    .debug_selector(|| "gallery-new".into())
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
                            this.new_row(None, window, cx);
                        }
                    })),
            )
            .into_any_element()
    }

    /// The arrows between the gallery's cards: across a line, or a line up or down.
    pub(crate) fn gallery_step(&mut self, across: isize, down: isize, cx: &mut Context<Self>) {
        let n = self.gallery_rows().len();
        if n == 0 {
            return;
        }
        let per = self.gallery_grid.get().per_line.max(1) as isize;
        let at = match self.gallery_focus {
            None => 0,
            Some(at) => (at as isize + across + down * per).clamp(0, n as isize - 1) as usize,
        };
        self.gallery_focus = Some(at);
        self.list
            .scroll_to_item(at / per as usize, gpui::ScrollStrategy::Nearest);
        cx.notify();
    }

    /// Where a card dragged to `at` would land among the gallery's cards: before the card under
    /// it (after it, past its middle), or at the end. Only while the view has no sort, as a
    /// board's and a list's manual order.
    pub(crate) fn drag_card_in_gallery(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        let moving = self.dragged_rows();
        let n = self.gallery_rows().len();
        let grid = self.gallery_grid.get();
        let scrolled = self.list.0.borrow().base_handle.offset().y;
        let target = (!moving.is_empty() && self.can_move_all(&moving, 0)).then(|| {
            let x = at.x - grid.origin.x;
            let y = at.y - grid.origin.y - scrolled;
            let line = (y / grid.item).floor().max(0.) as usize;
            let step = grid.card_width + grid.gap;
            let col = ((x / step).floor().max(0.) as usize).min(grid.per_line - 1);
            let past = x - step * col as f32 > grid.card_width / 2.;
            (0, (line * grid.per_line + col + usize::from(past)).min(n))
        });
        if let Some(d) = &mut self.board.drag {
            d.at = at;
            d.target = target;
        }
        cx.notify();
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
