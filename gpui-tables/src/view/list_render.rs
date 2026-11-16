//! Drawing the list: its lines (only those on screen, through `uniform_list`), each row's
//! title with its icon and its values to the right, group headers that fold, New lines, and a
//! dragged row with where it would land.

use std::ops::Range;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, ClickEvent, Context, DispatchPhase, DragMoveEvent, ElementId,
    FontWeight, InteractiveElement, IntoElement, LongPressEvent, ParentElement, Pixels, Point,
    Role, SharedString, StatefulInteractiveElement, Styled, Window, canvas, div, px, uniform_list,
};
use gpui_core::{IconName, Press, Selectable as _};

use super::board::{CardGhost, DraggedCard};
use super::list::ListMetrics;
use super::render::{icon, line};
use super::table::{CellRef, Item, TableView};
use crate::model::{PropertyType, Value};

/// A dragged row's place to land, as each line needs it.
#[derive(Clone, Copy)]
struct Landing {
    lane: usize,
    slot: usize,
    /// The view is sorted: the group is marked rather than a place in it.
    sorted: bool,
}

impl TableView {
    pub(crate) fn render_list(&mut self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        // A mouse drag GPUI ended somewhere else (let go outside the list) is over.
        if self.board.drag.as_ref().is_some_and(|d| !d.touch) && !cx.has_active_drag() {
            self.board.drag = None;
        }
        let t = self.theme.clone();
        let m = self.list_metrics();
        let n = self.shown.items.len();
        let list = uniform_list(
            "gpui-tables-list",
            n,
            cx.processor(|this, range: Range<usize>, _, cx| this.render_lines(range, cx)),
        )
        .track_scroll(&self.list)
        .flex_1()
        .w_full();

        let bounds = self.bounds.clone();
        let entity = cx.entity().downgrade();
        let probe = canvas(
            move |b, _, _| bounds.set(b),
            move |_, _, window, _| {
                // A finger's long press picks a row up; the finger then drags it.
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

        // A finger's row follows it (the mouse's is drawn by GPUI).
        let origin = self.bounds.get().origin;
        let ghost = self.board.drag.as_ref().filter(|d| d.touch).map(|d| {
            let at = d.at - origin - d.offset;
            div().absolute().left(at.x).top(at.y).child(d.ghost.clone())
        });

        div()
            .id("gpui-tables")
            .debug_selector(|| "list".into())
            .key_context(super::CONTEXT)
            .track_focus(&self.focus)
            .role(Role::List)
            .aria_label(SharedString::from(
                self.shown
                    .view
                    .as_ref()
                    .map_or(self.database.title.clone(), |v| v.name.clone()),
            ))
            .on_action(cx.listener(|this, _: &super::Up, window, cx| {
                if !this.up_to_bar(window, cx) {
                    this.move_cell(-1, 0, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &super::Down, _, cx| this.move_cell(1, 0, cx)))
            .on_action(cx.listener(|this, _: &super::NextCell, _, cx| this.move_cell(1, 0, cx)))
            .on_action(cx.listener(|this, _: &super::PrevCell, _, cx| this.move_cell(-1, 0, cx)))
            .on_action(cx.listener(|this, _: &super::PageUp, _, cx| this.list_page(-1, cx)))
            .on_action(cx.listener(|this, _: &super::PageDown, _, cx| this.list_page(1, cx)))
            .on_action(
                cx.listener(|this, _: &super::Confirm, window, cx| this.list_confirm(window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &super::ConfirmAdd, window, cx| {
                    this.list_confirm(window, cx)
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
            .flex()
            .flex_col()
            .overflow_hidden()
            .pt(m.pad / 2.)
            .bg(t.colors.background)
            .text_color(t.colors.text)
            .font_family(t.fonts.body.clone())
            .text_size(t.px(t.sizes.text))
            .child(list)
            .child(probe)
            .children(ghost)
            .into_any_element()
    }

    /// The lines on screen.
    fn render_lines(&mut self, range: Range<usize>, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let m = self.list_metrics();
        // The list's width this frame (the list has laid itself out before it asks for the
        // lines it shows), or the view's from the last frame while it measures a line.
        let width = self
            .list
            .0
            .borrow()
            .last_item_size
            .map_or(self.bounds.get().size.width, |s| s.item.width);
        let width = (width - m.pad * 2.).max(px(0.));
        let sorted = self.shown.view.as_ref().is_some_and(|v| !v.sort.is_empty());
        let landing = self
            .board
            .drag
            .as_ref()
            .and_then(|d| d.target)
            .map(|(lane, slot)| Landing { lane, slot, sorted });
        let dragged = self.board.drag.as_ref().map(|d| d.row_id.clone());
        let mut slot = range.clone().next().map_or(0, |start| self.slot_of(start));
        let mut out = Vec::with_capacity(range.len());
        for ix in range {
            let Some(item) = self.shown.items.get(ix).copied() else {
                break;
            };
            // A rule above each group but the first.
            let rule = matches!(item, Item::Group(_)) && ix > 0;
            let el = match item {
                Item::Row { row, group } => {
                    let lane = group.unwrap_or(0);
                    let el = self.render_line(
                        ix,
                        row,
                        lane,
                        slot,
                        width,
                        landing,
                        dragged.as_deref(),
                        &m,
                        cx,
                    );
                    slot += 1;
                    el
                }
                Item::Group(g) => {
                    slot = 0;
                    self.render_list_group(ix, g, landing, &m, cx)
                }
                Item::New(g) => self.render_list_new(ix, g, landing, &m, cx),
            };
            out.push(
                div()
                    .h(m.line)
                    .w_full()
                    .relative()
                    .px(m.pad)
                    .child(el)
                    .when(rule, |d| {
                        d.child(
                            div()
                                .absolute()
                                .top_0()
                                .left(m.pad)
                                .right(m.pad)
                                .h(px(1.))
                                .bg(self.theme.colors.line),
                        )
                    })
                    .into_any_element(),
            );
        }
        out
    }

    /// A line showing where a dragged row would land: above or below a line.
    fn landing_bar(&self, top: bool) -> gpui::Div {
        div()
            .absolute()
            .left_0()
            .right_0()
            .h(px(2.))
            .bg(self.theme.colors.accent)
            .rounded(px(1.))
            .when(top, |d| d.top_0())
            .when(!top, |d| d.bottom_0())
    }

    #[allow(clippy::too_many_arguments)]
    fn render_line(
        &mut self,
        ix: usize,
        row: usize,
        lane: usize,
        slot: usize,
        width: Pixels,
        landing: Option<Landing>,
        dragged: Option<&str>,
        m: &ListMetrics,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme.clone();
        let Some(lanes) = self.shown.list.as_ref() else {
            return div().into_any_element();
        };
        let (key, count) = lanes
            .get(lane)
            .map_or_else(|| ("all".to_owned(), 0), |l| (l.key_string(), l.rows.len()));
        let r = &self.rows[row];
        let rid = r.id.clone();
        let focused = self.cell.is_some_and(|c| c.item == ix);
        let lifted = dragged == Some(rid.as_str());
        let empty = r.title.is_empty();
        let title = self.card_title(r);
        let words = self.card_words(r);
        // An emoji icon only: an image icon (`asset:`) or an invalid one draws nothing for now.
        let glyph = r.icon().map(str::to_owned);
        let values: Vec<AnyElement> = self
            .line_values(r, width)
            .into_iter()
            .filter_map(|(pi, max)| {
                let p = self.database.database.as_ref()?.properties.get(pi)?;
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
                            .child(line(p.name.clone()))
                            .into_any_element()
                    }
                    _ => self.cell_content(ix, row, r, p, false, cx),
                };
                Some(
                    div()
                        .flex_shrink(1.)
                        .min_w_0()
                        .max_w(max)
                        .flex()
                        .items_center()
                        .overflow_hidden()
                        .child(content)
                        .into_any_element(),
                )
            })
            .collect();
        let weak = cx.entity().downgrade();
        let from = key.clone();
        let sel = format!("line:{rid}:{key}");
        let bar = landing
            .filter(|l| !l.sorted && l.lane == lane)
            .and_then(|l| {
                if l.slot == slot {
                    Some(true)
                } else if l.slot == count && slot + 1 == count {
                    Some(false)
                } else {
                    None
                }
            });
        Press::new(ElementId::NamedInteger("line".into(), ix as u64))
            .tab_stop(false)
            .focus_on_press(false)
            .focus_ring(false)
            .min_hit(px(t.min_hit))
            .selected(focused)
            .role(Role::ListItem)
            .accessibility_label(words)
            .debug_selector(move || sel.clone())
            .relative()
            .size_full()
            .px(m.inset)
            .flex()
            .items_center()
            .gap(m.gap)
            .overflow_hidden()
            .rounded(t.px(t.sizes.chip_radius))
            .hover(|s| s.bg(t.colors.hover))
            .when(focused, |d| {
                d.bg(t.colors.selection)
                    .border_1()
                    .border_color(t.colors.focus)
            })
            .when(lifted, |d| d.opacity(0.35))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(t.px(6.))
                    .when_some(glyph, |d, g| {
                        let sel = format!("icon:{rid}");
                        d.child(
                            div()
                                .debug_selector(move || sel)
                                .flex_none()
                                .size(m.icon)
                                .flex()
                                .items_center()
                                .justify_center()
                                .overflow_hidden()
                                .child(line(g)),
                        )
                    })
                    .child(
                        line(title)
                            .flex_1()
                            .font_weight(FontWeight(t.sizes.medium))
                            .when(empty, |d| d.text_color(t.colors.text_faint)),
                    ),
            )
            .when(!values.is_empty(), |d| {
                d.child(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_end()
                        .gap(m.gap)
                        .text_size(t.px(t.sizes.small))
                        .text_color(t.colors.text_muted)
                        .children(values),
                )
            })
            .when_some(bar, |d, top| d.child(self.landing_bar(top)))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                window.focus(&this.focus, cx);
                this.cell = Some(CellRef {
                    item: ix,
                    column: 0,
                });
                this.open_row(row, window, cx);
                cx.notify();
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

    fn render_list_group(
        &self,
        ix: usize,
        g: usize,
        landing: Option<Landing>,
        m: &ListMetrics,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = &self.theme;
        let Some(group) = self.shown.groups.get(g) else {
            return div().into_any_element();
        };
        let folded = self.shown.collapsed.get(g).copied().unwrap_or(false);
        let focused = self.cell.is_some_and(|c| c.item == ix);
        let key = group.key.to_string();
        let count = group.rows.len();
        // In a sorted view a row dropped in another group goes where the sort puts it: the
        // group is marked rather than a place in it.
        let target = landing.is_some_and(|l| l.sorted && l.lane == g);
        let name = self
            .shown
            .list
            .as_ref()
            .and_then(|l| l.get(g))
            .map(|c| self.column_name(c))
            .unwrap_or_default();
        Press::new(ElementId::NamedInteger("list-group".into(), ix as u64))
            .tab_stop(false)
            .focus_on_press(false)
            .focus_ring(false)
            .min_hit(px(t.min_hit))
            .selected(focused)
            .accessibility_label(format!(
                "{name}, {count} rows, {}",
                if folded { "collapsed" } else { "expanded" }
            ))
            .debug_selector(move || format!("group:{key}"))
            .size_full()
            .px(m.inset - t.pixels(4., self.rem))
            .flex()
            .items_center()
            .gap(t.px(8.))
            .overflow_hidden()
            .rounded(t.px(t.sizes.chip_radius))
            .text_size(t.px(t.sizes.small))
            .hover(|s| s.bg(t.colors.hover))
            .when(focused, |d| d.border_1().border_color(t.colors.focus))
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
            .child(div().min_w_0().flex().child(self.group_label(&group.key)))
            .child(
                div()
                    .flex_none()
                    .text_color(t.colors.text_muted)
                    .child(count.to_string()),
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                window.focus(&this.focus, cx);
                this.cell = Some(CellRef {
                    item: ix,
                    column: 0,
                });
                this.toggle_group(g, window, cx);
            }))
            .into_any_element()
    }

    fn render_list_new(
        &self,
        ix: usize,
        g: Option<usize>,
        landing: Option<Landing>,
        m: &ListMetrics,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = &self.theme;
        let focused = self.cell.is_some_and(|c| c.item == ix);
        let lane = g.unwrap_or(0);
        let lanes = self.shown.list.as_deref().unwrap_or(&[]);
        let key = lanes
            .get(lane)
            .map_or_else(|| "all".to_owned(), |l| l.key_string());
        // An empty group's place to land is above its New line.
        let empty = lanes.get(lane).is_some_and(|l| l.rows.is_empty());
        let bar = empty && landing.is_some_and(|l| !l.sorted && l.lane == lane && l.slot == 0);
        let name = lanes
            .get(lane)
            .filter(|l| l.key.is_some())
            .map(|l| format!("New row in {}", self.column_name(l)))
            .unwrap_or_else(|| "New row".into());
        Press::new(ElementId::NamedInteger("list-new".into(), ix as u64))
            .tab_stop(false)
            .focus_on_press(false)
            .focus_ring(false)
            .min_hit(px(t.min_hit))
            .selected(focused)
            .accessibility_label(name)
            .debug_selector(move || format!("new:{key}"))
            .relative()
            .size_full()
            .px(m.inset)
            .flex()
            .items_center()
            .gap(t.px(6.))
            .rounded(t.px(t.sizes.chip_radius))
            .text_color(t.colors.text_faint)
            .hover(|s| s.bg(t.colors.hover).text_color(t.colors.text_muted))
            .when(focused, |d| d.border_1().border_color(t.colors.focus))
            .child(icon(
                IconName::Plus,
                t.px(t.sizes.icon),
                t.colors.text_faint,
            ))
            .child(line("New"))
            .when(bar, |d| d.child(self.landing_bar(true)))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                window.focus(&this.focus, cx);
                this.cell = Some(CellRef {
                    item: ix,
                    column: 0,
                });
                this.new_row(g, window, cx);
            }))
            .into_any_element()
    }
}
