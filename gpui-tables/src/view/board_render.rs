//! Drawing the board: its columns side by side (scrolling sideways together), each column's
//! cards (only those on screen, through `uniform_list`, scrolling up and down on their own), a
//! New button under each, the folded columns gathered at the end, and a dragged card with
//! where it would land.

use std::ops::Range;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, BoxShadow, ClickEvent, Context, DispatchPhase, DragMoveEvent,
    ElementId, FontWeight, InteractiveElement, IntoElement, ListSizingBehavior, LongPressEvent,
    ParentElement, Pixels, Point, Render, Role, SharedString, StatefulInteractiveElement, Styled,
    TouchPhase, Window, canvas, div, point, px, uniform_list,
};
use gpui_core::{IconName, Press, Selectable as _};

use super::board::{BoardMetrics, BoardSpot, CardGhost, DraggedCard};
use super::render::{icon, line};
use super::table::TableView;
use crate::model::{PropertyType, Value};

/// What one column needs to draw, copied out so drawing can borrow the view.
struct ColumnInfo {
    index: usize,
    key: String,
    count: usize,
    grouped: bool,
}

impl TableView {
    pub(crate) fn render_board(&mut self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        // A mouse drag GPUI ended somewhere else (let go outside the board) is over.
        if self.board.drag.as_ref().is_some_and(|d| !d.touch) && !cx.has_active_drag() {
            self.board.drag = None;
        }
        let t = self.theme.clone();
        let m = self.board_metrics();
        let infos: Vec<ColumnInfo> = self
            .shown
            .board
            .as_ref()
            .map(|cols| {
                cols.iter()
                    .enumerate()
                    .map(|(index, c)| ColumnInfo {
                        index,
                        key: c.key_string(),
                        count: c.rows.len(),
                        grouped: c.key.is_some(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let folded: Vec<usize> = self
            .shown
            .board
            .as_ref()
            .map(|cols| {
                cols.iter()
                    .enumerate()
                    .filter(|(_, c)| c.folded)
                    .map(|(i, _)| i)
                    .collect()
            })
            .unwrap_or_default();
        *self.board.columns.borrow_mut() = vec![None; infos.len()];
        let spot = self.board_spot();
        self.board.spot = spot;
        let mut lanes: Vec<AnyElement> = Vec::new();
        for info in &infos {
            if !folded.contains(&info.index) {
                lanes.push(self.render_column(info, spot, &m, cx));
            }
        }
        if !folded.is_empty() {
            lanes.push(self.render_folded(&folded, &infos, spot, &m, cx));
        }

        let bounds = self.bounds.clone();
        let entity = cx.entity().downgrade();
        let probe = canvas(
            move |b, _, _| bounds.set(b),
            move |_, _, window, _| {
                // A finger's long press picks a card up; the finger then drags it.
                let e = entity.clone();
                window.on_mouse_event(move |ev: &LongPressEvent, phase, window, cx| {
                    if phase != DispatchPhase::Bubble {
                        return;
                    }
                    let claimed = e
                        .update(cx, |this, cx| this.long_press(ev, window, cx))
                        .unwrap_or(false);
                    if claimed {
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                });
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();

        // A finger's card follows it (the mouse's is drawn by GPUI).
        let origin = self.bounds.get().origin;
        let ghost = self.board.drag.as_ref().filter(|d| d.touch).map(|d| {
            let at = d.at - origin - d.offset;
            div().absolute().left(at.x).top(at.y).child(d.ghost.clone())
        });

        let mut scroller = div()
            .id("gpui-tables-board-scroll")
            .debug_selector(|| "board-scroll".into())
            .size_full()
            .overflow_x_scroll()
            .track_scroll(&self.board.scroll)
            .flex()
            .items_start()
            .gap(m.gap)
            .p(m.pad)
            .children(lanes);
        // Up and down belong to the columns; the board takes only sideways scrolls and pans.
        scroller.interactivity().base_style.restrict_scroll_to_axis = Some(true);

        div()
            .id("gpui-tables")
            .debug_selector(|| "board".into())
            .key_context(super::CONTEXT)
            .track_focus(&self.focus)
            .role(Role::Grid)
            .aria_label(SharedString::from(
                self.shown
                    .view
                    .as_ref()
                    .map_or(self.database.title.clone(), |v| v.name.clone()),
            ))
            .on_action(cx.listener(|this, _: &super::Up, _, cx| this.board_move(0, -1, cx)))
            .on_action(cx.listener(|this, _: &super::Down, _, cx| this.board_move(0, 1, cx)))
            .on_action(cx.listener(|this, _: &super::Left, _, cx| this.board_move(-1, 0, cx)))
            .on_action(cx.listener(|this, _: &super::Right, _, cx| this.board_move(1, 0, cx)))
            .on_action(cx.listener(|this, _: &super::NextCell, _, cx| this.board_tab(false, cx)))
            .on_action(cx.listener(|this, _: &super::PrevCell, _, cx| this.board_tab(true, cx)))
            .on_action(
                cx.listener(|this, _: &super::Confirm, window, cx| this.board_confirm(window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &super::ConfirmAdd, window, cx| {
                    this.board_confirm(window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &super::Cancel, window, cx| this.board_cancel(window, cx)),
            )
            .on_drag_move(cx.listener(|this, e: &DragMoveEvent<DraggedCard>, _, cx| {
                this.drag_card_to(e.event.position, cx)
            }))
            .on_drop(cx.listener(|this, _: &DraggedCard, window, cx| this.drop_card(window, cx)))
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(t.colors.background)
            .text_color(t.colors.text)
            .font_family(t.fonts.body.clone())
            .text_size(t.px(t.sizes.text))
            .child(scroller)
            .child(probe)
            .children(ghost)
            .into_any_element()
    }

    /// A finger's long press: on a card it picks the card up, and the finger then moves it.
    /// Returns whether it took the press (a plain pan, which never becomes a long press,
    /// scrolls instead).
    fn long_press(
        &mut self,
        ev: &LongPressEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match ev.phase {
            TouchPhase::Started => {
                if self.board.drag.is_some() || self.shown.board.is_none() {
                    return false;
                }
                let Some((column, index, card)) = self.card_at(ev.start_position) else {
                    return false;
                };
                let Some(col) = self.shown.board.as_ref().and_then(|c| c.get(column)) else {
                    return false;
                };
                let (row_id, key) = (self.rows[col.rows[index]].id.clone(), col.key_string());
                window.focus(&self.focus, cx);
                let offset = ev.start_position - card.origin;
                self.begin_card_drag(&row_id, &key, offset, ev.start_position, true, cx);
                true
            }
            TouchPhase::Moved => {
                if !self.board.drag.as_ref().is_some_and(|d| d.touch) {
                    return false;
                }
                self.drag_card_to(ev.position, cx);
                true
            }
            TouchPhase::Ended => {
                if !self.board.drag.as_ref().is_some_and(|d| d.touch) {
                    return false;
                }
                self.drag_card_to(ev.position, cx);
                self.drop_card(window, cx);
                true
            }
            TouchPhase::Cancelled => {
                if self.board.drag.take().is_some() {
                    cx.notify();
                }
                false
            }
        }
    }

    /// The bounds of a column's cards (its list's), as last drawn.
    pub(crate) fn list_bounds(&self, column: usize) -> Option<gpui::Bounds<Pixels>> {
        let col = self.shown.board.as_ref()?.get(column)?;
        if col.folded {
            return None;
        }
        let handle = self.board.lists.get(&col.key_string())?;
        let b = handle.0.borrow().base_handle.bounds();
        (b.size.height > px(0.) || col.rows.is_empty()).then_some(b)
    }

    fn render_column(
        &mut self,
        info: &ColumnInfo,
        spot: Option<BoardSpot>,
        m: &BoardMetrics,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme.clone();
        let ci = info.index;
        let key = info.key.clone();
        let handle = self.board.list(&key);
        let target = self.board.drag.as_ref().and_then(|d| d.target);
        let is_target = target.is_some_and(|(c, _)| c == ci);
        let name = self
            .shown
            .board
            .as_ref()
            .and_then(|c| c.get(ci))
            .map(|c| self.column_name(c))
            .unwrap_or_default();
        let label = match self
            .shown
            .board
            .as_ref()
            .and_then(|c| c.get(ci))
            .and_then(|c| c.key.clone())
        {
            Some(k) => self.group_label(&k),
            None => line(name.clone())
                .font_weight(FontWeight(t.sizes.medium))
                .into_any_element(),
        };
        let fold = info.grouped.then(|| {
            let sel = key.clone();
            Press::new(("fold", ci))
                .tab_stop(false)
                .focus_on_press(false)
                .focus_ring_color(t.colors.focus)
                .min_hit(px(t.min_hit))
                .accessibility_label(format!("Collapse {name}"))
                .debug_selector(move || format!("fold:{sel}"))
                .flex_none()
                .size(t.px(24.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(t.px(t.sizes.chip_radius))
                .hover(|s| s.bg(t.colors.hover))
                .child(icon(
                    IconName::ChevronsLeft,
                    t.px(t.sizes.icon),
                    t.colors.text_muted,
                ))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    window.focus(&this.focus, cx);
                    this.toggle_column(ci, window, cx);
                }))
        });
        let header = div()
            .flex_none()
            .h(m.header)
            .px(m.inset + t.pixels(4., self.rem))
            .flex()
            .items_center()
            .gap(t.px(8.))
            .overflow_hidden()
            .text_size(t.px(t.sizes.small))
            .child(div().min_w_0().flex().child(label))
            .child(
                div()
                    .flex_none()
                    .text_color(t.colors.text_muted)
                    .child(info.count.to_string()),
            )
            .child(div().flex_1())
            .children(fold);

        let mut list = uniform_list(
            ("gpui-tables-cards", ci),
            info.count,
            cx.processor(move |this, range: Range<usize>, _, cx| this.render_cards(ci, range, cx)),
        )
        .with_sizing_behavior(ListSizingBehavior::Infer)
        .track_scroll(&handle)
        .flex_shrink(1.)
        .min_h_0()
        .w_full()
        .px(m.inset);
        list.interactivity().base_style.restrict_scroll_to_axis = Some(true);

        let new_focused = spot == Some(BoardSpot::New { column: ci });
        let sel = key.clone();
        let new = Press::new(("board-new", ci))
            .tab_stop(false)
            .focus_on_press(false)
            .focus_ring(false)
            .min_hit(px(t.min_hit))
            .accessibility_label(format!("New card in {name}"))
            .debug_selector(move || format!("board-new:{sel}"))
            .flex_none()
            .h(m.new)
            .mx(m.inset)
            .mb(m.inset)
            .px(t.px(8.))
            .flex()
            .items_center()
            .gap(t.px(6.))
            .rounded(t.px(t.sizes.chip_radius))
            .text_color(t.colors.text_faint)
            .hover(|s| s.bg(t.colors.hover).text_color(t.colors.text_muted))
            .when(new_focused, |d| d.border_2().border_color(t.colors.focus))
            .child(icon(
                IconName::Plus,
                t.px(t.sizes.icon),
                t.colors.text_faint,
            ))
            .child(line("New"))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                window.focus(&this.focus, cx);
                this.new_card(ci, window, cx);
            }));

        let columns = self.board.columns.clone();
        let probe = canvas(
            move |b, _, _| {
                if let Some(slot) = columns.borrow_mut().get_mut(ci) {
                    *slot = Some(b);
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();

        let sel = key.clone();
        div()
            .id(("board-column", ci))
            .debug_selector(move || format!("column:{sel}"))
            .role(Role::List)
            .aria_label(SharedString::from(format!("{name}, {} cards", info.count)))
            .relative()
            .flex_none()
            .w(m.column_width)
            .max_h_full()
            .flex()
            .flex_col()
            .rounded(t.px(t.sizes.radius))
            .bg(t.colors.group)
            .border_1()
            .border_color(if is_target {
                t.colors.accent
            } else {
                t.colors.group
            })
            .when(is_target, |d| d.bg(t.colors.selection))
            .child(header)
            .child(list)
            .child(new)
            .child(probe)
            .into_any_element()
    }

    /// The folded columns, each a line to press to unfold it, in a column at the board's end.
    fn render_folded(
        &mut self,
        folded: &[usize],
        infos: &[ColumnInfo],
        spot: Option<BoardSpot>,
        m: &BoardMetrics,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme.clone();
        let entries: Vec<AnyElement> = folded
            .iter()
            .filter_map(|&ci| {
                let info = infos.get(ci)?;
                let col = self.shown.board.as_ref()?.get(ci)?;
                let key = col.key.clone()?;
                let name = self.column_name(col);
                let focused = spot == Some(BoardSpot::Folded { column: ci });
                let sel = info.key.clone();
                Some(
                    Press::new(("folded", ci))
                        .tab_stop(false)
                        .focus_on_press(false)
                        .focus_ring(false)
                        .min_hit(px(t.min_hit))
                        .selected(focused)
                        .accessibility_label(format!(
                            "{name}, {} cards, collapsed; expand",
                            info.count
                        ))
                        .debug_selector(move || format!("folded:{sel}"))
                        .flex_none()
                        .h(m.new)
                        .px(t.px(8.))
                        .flex()
                        .items_center()
                        .gap(t.px(8.))
                        .overflow_hidden()
                        .rounded(t.px(t.sizes.chip_radius))
                        .hover(|s| s.bg(t.colors.hover))
                        .when(focused, |d| d.border_2().border_color(t.colors.focus))
                        .child(icon(
                            IconName::ChevronRight,
                            t.px(t.sizes.icon),
                            t.colors.text_muted,
                        ))
                        .child(div().min_w_0().flex().child(self.group_label(&key)))
                        .child(
                            div()
                                .flex_none()
                                .text_size(t.px(t.sizes.small))
                                .text_color(t.colors.text_muted)
                                .child(info.count.to_string()),
                        )
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            window.focus(&this.focus, cx);
                            this.toggle_column(ci, window, cx);
                        }))
                        .into_any_element(),
                )
            })
            .collect();
        div()
            .id("board-collapsed")
            .debug_selector(|| "collapsed".into())
            .role(Role::List)
            .aria_label("Collapsed")
            .flex_none()
            .w(m.column_width)
            .max_h_full()
            .flex()
            .flex_col()
            .rounded(t.px(t.sizes.radius))
            .bg(t.colors.group)
            .child(
                div()
                    .flex_none()
                    .h(m.header)
                    .px(m.inset + t.pixels(4., self.rem))
                    .flex()
                    .items_center()
                    .text_size(t.px(t.sizes.small))
                    .text_color(t.colors.text_muted)
                    .child(line("Collapsed").font_weight(FontWeight(t.sizes.medium))),
            )
            .child(
                div()
                    .id("board-collapsed-list")
                    .flex_shrink(1.)
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(m.inset)
                    .pb(m.inset)
                    .flex()
                    .flex_col()
                    .gap(t.px(2.))
                    .children(entries),
            )
            .into_any_element()
    }

    /// A column's cards on screen, each a line of the list with half the gap above and below,
    /// so where a dragged card would land is the edge between two lines.
    fn render_cards(
        &mut self,
        column: usize,
        range: Range<usize>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let m = self.board_metrics();
        let spot = self.board.spot;
        let Some(rows) = self
            .shown
            .board
            .as_ref()
            .and_then(|c| c.get(column))
            .map(|c| (c.rows[range.clone()].to_vec(), c.rows.len(), c.key_string()))
        else {
            return Vec::new();
        };
        let (rows, n, key) = rows;
        let sorted = self.shown.view.as_ref().is_some_and(|v| !v.sort.is_empty());
        let slot = self
            .board
            .drag
            .as_ref()
            .and_then(|d| d.target)
            .filter(|(c, _)| *c == column && !sorted)
            .map(|(_, s)| s);
        let dragged = self.board.drag.as_ref().map(|d| d.row_id.clone());
        let accent = self.theme.colors.accent;
        range
            .zip(rows)
            .map(|(ix, row)| {
                let focused = spot == Some(BoardSpot::Card { column, index: ix });
                let lifted = dragged.as_deref() == Some(self.rows[row].id.as_str());
                let card = self.render_card(column, ix, row, &key, focused, lifted, &m, cx);
                let bar = |top: bool| {
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .h(px(2.))
                        .bg(accent)
                        .rounded(px(1.))
                        .when(top, |d| d.top_0())
                        .when(!top, |d| d.bottom_0())
                };
                div()
                    .relative()
                    .h(m.item)
                    .w_full()
                    .py((m.item - m.card) / 2.)
                    .child(card)
                    .when(slot == Some(ix), |d| d.child(bar(true)))
                    .when(slot == Some(n) && ix + 1 == n, |d| d.child(bar(false)))
                    .into_any_element()
            })
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_card(
        &mut self,
        column: usize,
        ix: usize,
        row: usize,
        key: &str,
        focused: bool,
        lifted: bool,
        m: &BoardMetrics,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme.clone();
        let r = &self.rows[row];
        let title = self.card_title(r);
        let empty = r.title.is_empty();
        let words = self.card_words(r);
        let rid = r.id.clone();
        let mut lines: Vec<AnyElement> = Vec::new();
        if let Some(fields) = self.database.database.as_ref() {
            for &pi in &self.shown.card_properties {
                let Some(p) = fields.properties.get(pi) else {
                    continue;
                };
                let content = match p.kind {
                    PropertyType::Checkbox => {
                        let on = r.value(&p.id).and_then(Value::as_bool).unwrap_or(false);
                        let size = t.px(t.sizes.text + 1.);
                        div()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(t.px(6.))
                            .child(
                                div()
                                    .flex_none()
                                    .size(size)
                                    .rounded(t.px(3.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .when(on, |d| d.bg(t.colors.accent))
                                    .when(!on, |d| d.border_1().border_color(t.colors.line_strong))
                                    .when(on, |d| {
                                        d.child(icon(
                                            IconName::Check,
                                            t.px(t.sizes.small),
                                            t.colors.on_accent,
                                        ))
                                    }),
                            )
                            .child(line(p.name.clone()).text_color(t.colors.text_muted))
                            .into_any_element()
                    }
                    _ if self.cell_text(r, p).is_empty() => continue,
                    _ => self.cell_content(ix, row, r, p, false, cx),
                };
                lines.push(
                    div()
                        .h(m.line)
                        .w_full()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .overflow_hidden()
                        .text_size(t.px(t.sizes.small))
                        .child(content)
                        .into_any_element(),
                );
            }
        }
        let weak = cx.entity().downgrade();
        let from = key.to_owned();
        let sel = format!("card:{rid}:{key}");
        let shadow = BoxShadow {
            color: t.colors.shadow.opacity(0.25),
            offset: point(px(0.), px(1.)),
            blur_radius: px(2.),
            spread_radius: px(0.),
            inset: false,
        };
        Press::new(ElementId::NamedInteger("card".into(), row as u64))
            .tab_stop(false)
            .focus_on_press(false)
            .focus_ring(false)
            .selected(focused)
            .accessibility_label(words)
            .debug_selector(move || sel.clone())
            .w_full()
            .h(m.card)
            .px(m.card_pad)
            .py(m.card_pad)
            .flex()
            .flex_col()
            .gap(m.line_gap)
            .overflow_hidden()
            .rounded(t.px(t.sizes.radius))
            .bg(t.colors.card)
            .border_1()
            .border_color(t.colors.line)
            .shadow(vec![shadow])
            .hover(|s| s.border_color(t.colors.line_strong))
            .when(focused, |d| d.border_2().border_color(t.colors.focus))
            .when(lifted, |d| d.opacity(0.35))
            .child(
                div()
                    .h(m.title)
                    .w_full()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .child(
                        line(title)
                            .flex_1()
                            .font_weight(FontWeight(t.sizes.medium))
                            .when(empty, |d| d.text_color(t.colors.text_faint)),
                    ),
            )
            .children(lines)
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                window.focus(&this.focus, cx);
                this.set_board_spot(BoardSpot::Card { column, index: ix }, cx);
                this.open_row(row, window, cx);
            }))
            .on_drag(
                DraggedCard { row_id: rid, from },
                move |d: &DraggedCard, offset: Point<Pixels>, window, cx| {
                    let at = window.mouse_position();
                    match weak.upgrade() {
                        Some(view) => view.update(cx, |this, cx| {
                            this.begin_card_drag(&d.row_id, &d.from, offset, at, false, cx)
                        }),
                        None => cx.new(|_| CardGhost::empty()),
                    }
                },
            )
            .into_any_element()
    }
}

impl CardGhost {
    fn empty() -> Self {
        Self {
            title: String::new(),
            lines: Vec::new(),
            theme: super::TableTheme::default(),
            width: px(0.),
            height: px(0.),
        }
    }
}

impl Render for CardGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let t = &self.theme;
        let shadow = BoxShadow {
            color: t.colors.shadow,
            offset: point(px(0.), px(8.)),
            blur_radius: px(20.),
            spread_radius: px(0.),
            inset: false,
        };
        div()
            .w(self.width)
            .h(self.height)
            .px(t.px(10.))
            .py(t.px(10.))
            .flex()
            .flex_col()
            .gap(t.px(4.))
            .overflow_hidden()
            .rounded(t.px(t.sizes.radius))
            .bg(t.colors.card)
            .border_1()
            .border_color(t.colors.line_strong)
            // Opaque, so the card it's carried over doesn't show through it.
            .shadow(vec![shadow])
            .font_family(t.fonts.body.clone())
            .text_size(t.px(t.sizes.text))
            .text_color(t.colors.text)
            .child(
                div()
                    .h(t.px(20.))
                    .flex()
                    .items_center()
                    .child(line(self.title.clone()).font_weight(FontWeight(t.sizes.medium))),
            )
            .children(self.lines.iter().map(|l| {
                div()
                    .h(t.px(t.sizes.small + 10.))
                    .flex()
                    .items_center()
                    .text_size(t.px(t.sizes.small))
                    .text_color(t.colors.text_muted)
                    .child(line(l.clone()))
            }))
    }
}
