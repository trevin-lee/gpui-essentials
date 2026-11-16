//! Drawing the table: the header, the rows (only those on screen, through `uniform_list`), the
//! cells by type, and the menus, pickers and day grid over them.

use std::ops::Range;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    Anchor, AnyElement, BoxShadow, ClickEvent, Context, DispatchPhase, ElementId, FontWeight, Hsla,
    InteractiveElement, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ParentElement, Pixels, Render, Role, ScrollEvent, SharedString, StatefulInteractiveElement,
    Styled, TouchDragEvent, TouchPhase, Window, anchored, canvas, deferred, div, point, px,
    uniform_list,
};
use gpui_core::{Icon, IconName, Input, Press, Selectable as _};

use super::format;
use super::host::person_label;
use super::table::{Drag, Entry, Item, Popover, TableView, type_icon};
use crate::engine::{GroupKey, time};
use crate::model::{Color, Doc, Person, Property, PropertyType, Value};

impl Render for TableView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.theme = self.host.theme(cx);
        self.rem = window.rem_size();
        self.ensure();
        // A board view draws as a board, a list view as a list, any other as a table: one
        // entity and one host.
        if self.shown.board.is_some() {
            return self.render_board(window, cx);
        }
        if self.shown.list.is_some() {
            return self.render_list(window, cx);
        }
        self.clamp_scroll();
        let t = self.theme.clone();
        let n = self.shown.items.len();
        let mut list = uniform_list(
            "gpui-tables-rows",
            n,
            cx.processor(|this, range: Range<usize>, window, cx| {
                this.render_items(range, window, cx)
            }),
        )
        .track_scroll(&self.list)
        .flex_1()
        .w_full();
        // Sideways pans and scrolls are the table's own (the title column stays put), so the
        // list takes only up and down.
        list.interactivity().base_style.restrict_scroll_to_axis = Some(true);

        let bounds = self.bounds.clone();
        let entity = cx.entity().downgrade();
        let dragging = self.drag.is_some();
        let touch_handles = if t.touch {
            self.handle_hits()
        } else {
            Vec::new()
        };
        let probe = canvas(
            move |b, _, _| bounds.set(b),
            move |_, _, window, _| {
                if dragging {
                    let e = entity.clone();
                    window.on_mouse_event(move |ev: &MouseMoveEvent, phase, _, cx| {
                        if phase == DispatchPhase::Capture {
                            let _ = e.update(cx, |this, cx| this.drag_to(ev.position, cx));
                        }
                    });
                    let e = entity.clone();
                    window.on_mouse_event(move |_: &MouseUpEvent, phase, window, cx| {
                        if phase == DispatchPhase::Capture {
                            let _ = e.update(cx, |this, cx| this.end_drag(window, cx));
                        }
                    });
                }
                if !touch_handles.is_empty() || dragging {
                    let e = entity.clone();
                    window.on_mouse_event(move |ev: &TouchDragEvent, phase, window, cx| {
                        if phase != DispatchPhase::Bubble {
                            return;
                        }
                        match ev.phase {
                            TouchPhase::Started => {
                                if let Some((column, _)) = touch_handles
                                    .iter()
                                    .find(|(_, b)| b.contains(&ev.start_position))
                                {
                                    let column = *column;
                                    window.prevent_default();
                                    let _ = e.update(cx, |this, cx| {
                                        this.begin_resize(column, ev.start_position.x, true, cx)
                                    });
                                }
                            }
                            TouchPhase::Moved => {
                                let _ = e.update(cx, |this, cx| this.drag_to(ev.position, cx));
                            }
                            TouchPhase::Ended | TouchPhase::Cancelled => {
                                let _ = e.update(cx, |this, cx| this.end_drag(window, cx));
                            }
                        }
                    });
                }
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();

        let popover = self.render_popover(window, cx);
        div()
            .id("gpui-tables")
            .debug_selector(|| "table".into())
            .key_context(super::CONTEXT)
            .track_focus(&self.focus)
            .role(Role::Grid)
            .aria_label(SharedString::from(
                self.shown
                    .view
                    .as_ref()
                    .map_or(self.database.title.clone(), |v| v.name.clone()),
            ))
            .on_action(cx.listener(|this, _: &super::Up, _, cx| this.on_vertical(-1, cx)))
            .on_action(cx.listener(|this, _: &super::Down, _, cx| this.on_vertical(1, cx)))
            .on_action(cx.listener(|this, _: &super::Left, _, cx| this.on_horizontal(-1, cx)))
            .on_action(cx.listener(|this, _: &super::Right, _, cx| this.on_horizontal(1, cx)))
            .on_action(
                cx.listener(|this, _: &super::NextCell, window, cx| this.on_tab(false, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &super::PrevCell, window, cx| this.on_tab(true, window, cx)),
            )
            .on_action(cx.listener(|this, _: &super::Confirm, window, cx| {
                if this.is_editing() {
                    // The field takes Enter itself; nothing else does while it's open.
                } else if this.popover.is_some() {
                    this.activate_highlight(false, window, cx)
                } else {
                    this.confirm(window, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &super::ConfirmAdd, window, cx| {
                if this.is_editing() {
                } else if this.popover.is_some() {
                    this.activate_highlight(true, window, cx)
                } else {
                    this.confirm(window, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &super::Cancel, window, cx| this.cancel(window, cx)))
            .on_action(cx.listener(|this, _: &super::Clear, window, cx| {
                if this.popover.is_none() && !this.is_editing() {
                    this.clear_cell(window, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &super::Edit, window, cx| {
                if this.popover.is_none()
                    && !this.is_editing()
                    && let Some(c) = this.cell
                {
                    this.edit_cell(c, window, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &super::ColumnMenu, _, cx| {
                if !this.is_editing()
                    && let Some(c) = this.cell
                {
                    this.open_header_menu(c.column, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &super::PageUp, _, cx| this.on_page(-1, cx)))
            .on_action(cx.listener(|this, _: &super::PageDown, _, cx| this.on_page(1, cx)))
            .on_scroll(cx.listener(|this, e: &ScrollEvent, window, cx| {
                let d = e.delta.pixel_delta(window.line_height());
                if d.x == px(0.) || d.y.abs() > d.x.abs() {
                    return;
                }
                let before = this.scroll_x;
                this.scroll_x = (this.scroll_x - d.x).clamp(px(0.), this.max_scroll_x());
                if this.scroll_x != before {
                    cx.notify();
                }
            }))
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(t.colors.background)
            .text_color(t.colors.text)
            .font_family(t.fonts.body.clone())
            .text_size(t.px(t.sizes.text))
            .child(self.render_header(cx))
            .child(list)
            .child(probe)
            .children(popover)
            .into_any_element()
    }
}

/// A one-line text that ends in an ellipsis when it doesn't fit.
pub(crate) fn line(text: impl Into<SharedString>) -> gpui::Div {
    div().min_w_0().truncate().child(text.into())
}

pub(crate) fn icon(name: IconName, size: gpui::Rems, color: Hsla) -> Icon {
    Icon::new(name).size(size).color(color)
}

impl TableView {
    // --- Keys -------------------------------------------------------------------------------

    // While a value is typed in place, the arrows (and Page Up and Down) are the field's: they
    // never move the focused cell.

    fn on_vertical(&mut self, d: isize, cx: &mut Context<Self>) {
        if self.is_editing() {
            return;
        }
        match &self.popover {
            Some(Popover::Date { .. }) => self.move_day(7 * d as i64, cx),
            Some(_) => self.move_highlight(d, cx),
            None => self.move_cell(d, 0, cx),
        }
    }

    fn on_horizontal(&mut self, d: isize, cx: &mut Context<Self>) {
        if self.is_editing() {
            return;
        }
        match &self.popover {
            Some(Popover::Date { .. }) => self.move_day(d as i64, cx),
            Some(_) => {}
            None => self.move_cell(0, d, cx),
        }
    }

    fn on_page(&mut self, d: isize, cx: &mut Context<Self>) {
        if self.is_editing() {
            return;
        }
        match &self.popover {
            Some(Popover::Date { .. }) => self.turn_month(d as i64, cx),
            Some(_) => {}
            None => {
                let rows = (self.bounds.get().size.height / self.row_height())
                    .floor()
                    .max(1.);
                self.move_cell(d * rows as isize, 0, cx)
            }
        }
    }

    /// Tab: commits a value being typed (a cell's moves on to the next cell), else moves the
    /// focused cell when no menu is open.
    fn on_tab(&mut self, back: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(e) = &self.editor {
            let cell = matches!(e.target, super::TextTarget::Cell { .. });
            self.finish_editor(true, true, window, cx);
            if cell {
                self.tab(back, cx);
            }
            return;
        }
        if self.popover.is_none() {
            self.tab(back, cx)
        }
    }

    // --- Drags ------------------------------------------------------------------------------

    pub(crate) fn begin_resize(
        &mut self,
        column: usize,
        x: Pixels,
        touch: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(c) = self.shown.columns.get(column) else {
            return;
        };
        self.popover = None;
        self.drag = Some(Drag::Resize {
            column,
            start_x: x,
            start: c.width,
            width: c.width,
            touch,
        });
        cx.notify();
    }

    pub(crate) fn drag_to(&mut self, at: gpui::Point<Pixels>, cx: &mut Context<Self>) {
        let (min, max) = (
            self.theme.sizes.min_column_width,
            self.theme.sizes.max_column_width,
        );
        let rem_base = self.theme.rem_base;
        let rem = f32::from(self.rem).max(1.);
        match &mut self.drag {
            Some(Drag::Resize {
                start_x,
                start,
                width,
                ..
            }) => {
                let d = f32::from(at.x - *start_x) * rem_base / rem;
                *width = (*start + d).clamp(min, max.max(min)).round();
                cx.notify();
            }
            Some(Drag::Header {
                start_x, x, moved, ..
            }) => {
                *x = at.x;
                if (at.x - *start_x).abs() > px(4.) {
                    *moved = true;
                }
                cx.notify();
            }
            None => {}
        }
    }

    /// Where each column's edge takes a drag, in the window: the touch minimum wide, centred
    /// on the edge.
    pub(crate) fn handle_hits(&self) -> Vec<(usize, gpui::Bounds<Pixels>)> {
        let hit = self.theme.hit(8., self.rem);
        let title_right = self.bounds.get().origin.x + self.column_px(0);
        (0..self.shown.columns.len())
            .filter_map(|c| {
                let b = self.header_bounds(c);
                let x = b.right();
                if c > 0 && x < title_right {
                    return None;
                }
                Some((
                    c,
                    gpui::Bounds::new(point(x - hit / 2., b.top()), gpui::size(hit, b.size.height)),
                ))
            })
            .collect()
    }

    // --- Header -----------------------------------------------------------------------------

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = &self.theme;
        let h = self.header_height();
        let cols = self.shown.columns.len();
        let strip = (1..cols)
            .map(|c| self.render_header_cell(c, cx))
            .collect::<Vec<_>>();
        let more = Press::new("gpui-tables-columns")
            .tab_stop(false)
            .focus_ring_color(t.colors.focus)
            .accessibility_label("Show and hide properties")
            .debug_selector(|| "columns-button".into())
            .flex_none()
            .w(t.hit(32., self.rem))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(t.colors.text_muted)
            .hover(|s| s.bg(t.colors.hover))
            .child(icon(
                IconName::Plus,
                t.px(t.sizes.icon),
                t.colors.text_muted,
            ))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.open_columns_menu(cx);
                window.focus(&this.focus, cx);
            }));
        // Where a dragged column would land.
        let indicator = match &self.drag {
            Some(Drag::Header {
                column,
                x,
                moved: true,
                ..
            }) => {
                let to = self.drop_index(*x);
                let mut left = (1..to).fold(px(0.), |w, c| w + self.column_px(c));
                if to > *column {
                    left += self.column_px(to);
                }
                Some(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(left - px(1.))
                        .w(px(2.))
                        .bg(t.colors.accent),
                )
            }
            _ => None,
        };
        let handles = self.handle_hits();
        let origin = self.bounds.get().origin;
        let resizing = match &self.drag {
            Some(Drag::Resize { column, .. }) => Some(*column),
            _ => None,
        };
        div()
            .id("gpui-tables-header")
            .relative()
            .flex()
            .flex_none()
            .w_full()
            .h(h)
            .bg(t.colors.header)
            .border_b_1()
            .border_color(t.colors.line_strong)
            .text_size(t.px(t.sizes.small))
            .text_color(t.colors.text_muted)
            .when(cols > 0, |d| d.child(self.render_header_cell(0, cx)))
            .child(
                div()
                    .id("gpui-tables-strip")
                    .debug_selector(|| "strip".into())
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_hidden()
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .left(-self.scroll_x)
                            .h_full()
                            .flex()
                            .children(strip)
                            .child(more)
                            .children(indicator),
                    ),
            )
            .children(handles.into_iter().map(|(c, b)| {
                let name = self
                    .property_at(c)
                    .map_or_else(String::new, |p| p.name.clone());
                let active = resizing == Some(c);
                div()
                    .id(("resize", c))
                    .debug_selector(move || format!("resize:{name}"))
                    .absolute()
                    .top_0()
                    .left(b.left() - origin.x)
                    .w(b.size.width)
                    .h_full()
                    .flex()
                    .justify_center()
                    .cursor_col_resize()
                    .group("resize")
                    .child(
                        div()
                            .w(px(2.))
                            .h_full()
                            .when(active, |d| d.bg(t.colors.accent))
                            .when(!active, |d| {
                                d.group_hover("resize", |s| s.bg(t.colors.line_strong))
                            }),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            this.begin_resize(c, e.position.x, false, cx);
                        }),
                    )
                    // A press let go before the next frame (a tap, a quick click) ends here:
                    // the window-wide listeners only exist from the frame after the press.
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _: &MouseUpEvent, window, cx| this.end_drag(window, cx)),
                    )
            }))
    }

    fn property_at(&self, column: usize) -> Option<&Property> {
        let col = self.shown.columns.get(column)?;
        self.database
            .database
            .as_ref()?
            .properties
            .get(col.property)
    }

    fn render_header_cell(&self, column: usize, cx: &mut Context<Self>) -> AnyElement {
        let t = &self.theme;
        let Some(p) = self.property_at(column) else {
            return div().into_any_element();
        };
        let view = self.shown.view.as_ref();
        let sorts = view.map_or(&[][..], |v| v.sort.as_slice());
        let sort = sorts.iter().position(|s| s.property_id == p.id);
        let filtered = view.is_some_and(|v| super::ops::filters_by(v, &p.id));
        let name = p.name.clone();
        let dragged = match &self.drag {
            Some(Drag::Header {
                column: c,
                start_x,
                x,
                moved: true,
            }) if *c == column => Some(*x - *start_x),
            _ => None,
        };
        let menu_open =
            matches!(&self.popover, Some(Popover::Header { column: c, .. }) if *c == column);
        let sel_name = name.clone();
        // A narrow column gives its name the room: no type icon, one mark at most.
        let roomy = self.column_px(column) >= t.pixels(90., self.rem);
        let filtered = filtered && (roomy || sort.is_none());
        let sort_icon = sort.map(|i| {
            let dir = &sorts[i].direction;
            div()
                .flex_none()
                .flex()
                .items_center()
                .child(icon(
                    if *dir == crate::model::Direction::Ascending {
                        IconName::ArrowUp
                    } else {
                        IconName::ArrowDown
                    },
                    t.px(t.sizes.icon - 2.),
                    t.colors.accent,
                ))
                .when(roomy && sorts.len() > 1, |d| {
                    d.child(
                        div()
                            .text_color(t.colors.accent)
                            .child(format!("{}", i + 1)),
                    )
                })
        });
        div()
            .id(("header", column))
            .debug_selector(move || format!("header:{sel_name}"))
            .role(Role::ColumnHeader)
            .aria_label(SharedString::from(name.clone()))
            .relative()
            .flex_none()
            .w(self.column_px(column))
            .h_full()
            .flex()
            .items_center()
            .gap(t.px(6.))
            .px(t.px(t.sizes.cell_padding))
            .overflow_hidden()
            .border_r_1()
            .border_color(t.colors.line)
            .cursor_pointer()
            .hover(|s| s.bg(t.colors.hover))
            .when(menu_open, |d| d.bg(t.colors.hover))
            .when_some(dragged, |d, dx| {
                d.left(dx).bg(t.colors.popover).opacity(0.85)
            })
            .when(roomy, |d| {
                d.child(icon(
                    type_icon(&p.kind),
                    t.px(t.sizes.icon),
                    t.colors.text_muted,
                ))
            })
            .child(line(name).flex_1().font_weight(FontWeight(t.sizes.medium)))
            .children(sort_icon)
            .when(filtered, |d| {
                d.child(icon(
                    IconName::Funnel,
                    t.px(t.sizes.icon - 2.),
                    t.colors.accent,
                ))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                    this.popover = None;
                    this.drag = Some(Drag::Header {
                        column,
                        start_x: e.position.x,
                        x: e.position.x,
                        moved: false,
                    });
                    cx.notify();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, window, cx| this.end_drag(window, cx)),
            )
            .into_any_element()
    }

    // --- Rows -------------------------------------------------------------------------------

    fn render_items(
        &mut self,
        range: Range<usize>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        range
            .filter_map(|ix| {
                let item = *self.shown.items.get(ix)?;
                Some(match item {
                    Item::Row { row, .. } => self.render_row(ix, row, cx),
                    Item::Group(g) => self.render_group(ix, g, cx),
                    Item::New(g) => self.render_new(ix, g, cx),
                })
            })
            .collect()
    }

    fn render_row(&self, ix: usize, row: usize, cx: &mut Context<Self>) -> AnyElement {
        let t = &self.theme;
        let cols = self.shown.columns.len();
        let cells: Vec<AnyElement> = (1..cols)
            .map(|c| self.render_cell(ix, row, c, cx))
            .collect();
        div()
            .id(("row", ix))
            .group("gpui-tables-row")
            .role(Role::Row)
            .h(self.row_height())
            .w_full()
            .flex()
            .border_b_1()
            .border_color(t.colors.line)
            .hover(|s| s.bg(t.colors.hover))
            .when(cols > 0, |d| d.child(self.render_cell(ix, row, 0, cx)))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_hidden()
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .left(-self.scroll_x)
                            .h_full()
                            .flex()
                            .children(cells),
                    ),
            )
            .into_any_element()
    }

    fn render_cell(
        &self,
        ix: usize,
        row: usize,
        column: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = &self.theme;
        let Some(p) = self.property_at(column) else {
            return div().into_any_element();
        };
        let r = &self.rows[row];
        let focused = self
            .cell
            .is_some_and(|c| c.item == ix && c.column == column);
        let row_focused = self.cell.is_some_and(|c| c.item == ix);
        let (rid, pid) = (r.id.clone(), p.id.clone());
        let is_check = p.kind == PropertyType::Checkbox;
        let numeric = p.kind == PropertyType::Number;
        let editor = self.editor.as_ref().filter(|e| e.edits(&r.id, &p.id));
        let editing = editor.is_some();
        let content = match editor {
            Some(e) => self.render_editor(e, column),
            None => self.cell_content(ix, row, r, p, row_focused, cx),
        };
        let label = format!("{}: {}", p.name, self.cell_text(r, p));
        div()
            .id(("cell", column))
            .debug_selector(move || format!("cell:{rid}:{pid}"))
            .role(if is_check { Role::CheckBox } else { Role::Cell })
            .aria_label(SharedString::from(label))
            .relative()
            .flex_none()
            .w(self.column_px(column))
            .h_full()
            .px(t.px(t.sizes.cell_padding))
            .flex()
            .items_center()
            .when(numeric && !editing, |d| d.justify_end())
            .overflow_hidden()
            .border_r_1()
            .border_color(t.colors.line)
            .when(editing, |d| d.bg(t.colors.background))
            .when(focused, |d| {
                d.when(!editing, |d| d.bg(t.colors.selection)).child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .border_2()
                        .border_color(t.colors.focus),
                )
            })
            .child(content)
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                // A press in the field being typed in is the field's (placing the caret).
                if editing {
                    return;
                }
                let cell = super::CellRef { item: ix, column };
                window.focus(&this.focus, cx);
                this.popover = None;
                if is_check || this.cell == Some(cell) {
                    this.edit_cell(cell, window, cx);
                } else {
                    this.set_cell(cell, cx);
                }
            }))
            .into_any_element()
    }

    /// The field a cell's value is typed in: on one line inside the cell, or for a text value,
    /// a textarea over the cell that grows down over the rows below (up to eight lines, then it
    /// scrolls), at least a menu's width.
    fn render_editor(&self, e: &super::table::Editor, column: usize) -> AnyElement {
        let t = &self.theme;
        let input = Input::new(&e.state)
            .appearance(false)
            .accessibility_label(e.label.clone())
            .caret_color(t.colors.accent)
            .selection_color(t.colors.accent.opacity(0.3))
            .placeholder_color(t.colors.text_faint)
            .flex_1()
            .min_w_0();
        if !e.multiline() {
            return div()
                .debug_selector(|| "editor".into())
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .when(e.kind == PropertyType::Title, |d| {
                    d.font_weight(FontWeight(t.sizes.medium))
                })
                .child(input)
                .into_any_element();
        }
        let width = self
            .column_px(column)
            .max(t.pixels(t.sizes.menu_width, self.rem));
        // One line is the row's height (the 2-pixel edge included); more grow down.
        let row = self.row_height();
        let line_height = t.pixels(t.sizes.text + 6., self.rem);
        let pad_y = ((row - px(4.) - line_height) / 2.).max(px(0.));
        let shadow = BoxShadow {
            color: t.colors.shadow,
            offset: point(px(0.), px(4.)),
            blur_radius: px(12.),
            spread_radius: px(0.),
            inset: false,
        };
        // Its layout starts at the cell's corner (the box it's in fills the cell); it's drawn
        // after the rows, so it isn't cut off by them, and kept in the window.
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .child(
                deferred(
                    anchored().snap_to_window_with_margin(px(8.)).child(
                        div()
                            .id("gpui-tables-editor")
                            .debug_selector(|| "editor".into())
                            .occlude()
                            .w(width)
                            .min_h(row)
                            // The text where the cell's is, inside the 2-pixel edge.
                            .px((t.pixels(t.sizes.cell_padding, self.rem) - px(2.)).max(px(0.)))
                            .py(pad_y)
                            .line_height(line_height)
                            .flex()
                            .bg(t.colors.popover)
                            .border_2()
                            .border_color(t.colors.focus)
                            .shadow(vec![shadow])
                            .text_color(t.colors.text)
                            .font_family(t.fonts.body.clone())
                            .text_size(t.px(t.sizes.text))
                            .child(input),
                    ),
                )
                .with_priority(1),
            )
            .into_any_element()
    }

    /// A cell's value as words, for assistive technology.
    pub(crate) fn cell_text(&self, r: &Doc, p: &Property) -> String {
        let offset = i64::from(self.host.utc_offset_minutes()) * 60_000;
        let v = r.value(&p.id);
        match p.kind {
            PropertyType::Title => r.title.clone(),
            PropertyType::Number => v
                .and_then(Value::as_number)
                .map(|x| format::number(x, &p.config))
                .unwrap_or_default(),
            PropertyType::Select => v
                .and_then(Value::as_str)
                .and_then(|id| p.option(id))
                .map(|o| o.name.clone())
                .unwrap_or_default(),
            PropertyType::MultiSelect => v
                .and_then(Value::as_ids)
                .unwrap_or(&[])
                .iter()
                .filter_map(|id| p.option(id).map(|o| o.name.clone()))
                .collect::<Vec<_>>()
                .join(", "),
            PropertyType::Date => v
                .and_then(Value::as_date)
                .map(|d| format::date(d, offset))
                .unwrap_or_default(),
            PropertyType::Checkbox => if v.and_then(Value::as_bool).unwrap_or(false) {
                "checked"
            } else {
                "unchecked"
            }
            .into(),
            PropertyType::Person => self.people_text(v),
            PropertyType::Relation => self.relation_text(p, v),
            PropertyType::CreatedTime => format::instant(r.created, offset),
            PropertyType::EditedTime => format::instant(r.updated, offset),
            PropertyType::CreatedBy => self.person_text(r.created_by()),
            PropertyType::EditedBy => self.person_text(r.edited_by()),
            _ => v.and_then(Value::as_str).unwrap_or_default().to_owned(),
        }
    }

    fn people_text(&self, v: Option<&Value>) -> String {
        v.and_then(Value::as_people)
            .unwrap_or(&[])
            .iter()
            .filter(|p| !matches!(p, Person::Other(_)))
            .map(|p| person_label(self.host.as_ref(), p))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// A created-by or edited-by: one person, or empty (as a person value leaves out a kind
    /// it doesn't know).
    fn person_text(&self, person: Option<&Person>) -> String {
        person
            .filter(|p| !matches!(p, Person::Other(_)))
            .map(|p| person_label(self.host.as_ref(), p))
            .unwrap_or_default()
    }

    fn relation_text(&self, p: &Property, v: Option<&Value>) -> String {
        let db = p.relation().map(|t| t.database_id).unwrap_or_default();
        v.and_then(Value::as_ids)
            .unwrap_or(&[])
            .iter()
            .map(|id| {
                self.host
                    .row_title(db, id)
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "Untitled".into())
            })
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub(crate) fn chip(&self, name: impl Into<SharedString>, color: &Color) -> gpui::Div {
        let t = &self.theme;
        let c = t.colors.options.get(color);
        div()
            .flex_shrink(1.)
            .min_w_0()
            .h(t.px(t.sizes.small + 8.))
            .px(t.px(6.))
            .rounded(t.px(t.sizes.chip_radius))
            .bg(c.background)
            .text_color(c.text)
            .text_size(t.px(t.sizes.small))
            .flex()
            .items_center()
            .child(line(name))
    }

    pub(crate) fn cell_content(
        &self,
        ix: usize,
        row: usize,
        r: &Doc,
        p: &Property,
        row_focused: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = &self.theme;
        let offset = i64::from(self.host.utc_offset_minutes()) * 60_000;
        let v = r.value(&p.id);
        match p.kind {
            PropertyType::Title => {
                let empty = r.title.is_empty();
                let title = if empty {
                    "Untitled".to_owned()
                } else {
                    r.title.clone()
                };
                let rid = r.id.clone();
                let hit = t.hit(22., self.rem);
                let focus = self.focus.clone();
                let open = Press::new(("open", ix))
                    .tab_stop(false)
                    .focus_ring_color(t.colors.focus)
                    .accessibility_label("Open")
                    .debug_selector(move || format!("open:{rid}"))
                    .flex_none()
                    .size(hit)
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(!t.touch && !row_focused, |d| {
                        d.opacity(0.)
                            .group_hover("gpui-tables-row", |s| s.opacity(1.))
                    })
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(t.px(22.))
                            .rounded(t.px(t.sizes.chip_radius))
                            .border_1()
                            .border_color(t.colors.line_strong)
                            .bg(t.colors.background)
                            .hover(|s| s.bg(t.colors.hover))
                            .child(icon(
                                IconName::Maximize2,
                                t.px(t.sizes.icon - 2.),
                                t.colors.text_muted,
                            )),
                    )
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        window.focus(&focus, cx);
                        this.open_row(row, window, cx);
                    }));
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(t.px(4.))
                    .child(
                        line(title)
                            .flex_1()
                            .font_weight(FontWeight(t.sizes.medium))
                            .when(empty, |d| d.text_color(t.colors.text_faint)),
                    )
                    .child(open)
                    .into_any_element()
            }
            PropertyType::Select => match v.and_then(Value::as_str).and_then(|id| p.option(id)) {
                Some(o) => div()
                    .min_w_0()
                    .flex()
                    .child(self.chip(o.name.clone(), &o.color))
                    .into_any_element(),
                None => div().into_any_element(),
            },
            PropertyType::MultiSelect => {
                let ids = v.and_then(Value::as_ids).unwrap_or(&[]);
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .gap(t.px(4.))
                    .overflow_hidden()
                    .children(
                        ids.iter()
                            .filter_map(|id| p.option(id))
                            .map(|o| self.chip(o.name.clone(), &o.color)),
                    )
                    .into_any_element()
            }
            PropertyType::Checkbox => {
                let on = v.and_then(Value::as_bool).unwrap_or(false);
                let size = t.px(t.sizes.text + 3.);
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
                            t.px(t.sizes.text),
                            t.colors.on_accent,
                        ))
                    })
                    .into_any_element()
            }
            PropertyType::Url | PropertyType::Email => {
                line(v.and_then(Value::as_str).unwrap_or_default().to_owned())
                    .text_color(t.colors.text)
                    .underline()
                    .into_any_element()
            }
            PropertyType::CreatedTime | PropertyType::EditedTime => line(self.cell_text(r, p))
                .text_color(t.colors.text_muted)
                .into_any_element(),
            PropertyType::Date => line(
                v.and_then(Value::as_date)
                    .map(|d| format::date(d, offset))
                    .unwrap_or_default(),
            )
            .into_any_element(),
            _ => line(self.cell_text(r, p)).into_any_element(),
        }
    }

    pub(crate) fn group_label(&self, key: &GroupKey) -> AnyElement {
        let t = &self.theme;
        let p = self
            .shown
            .view
            .as_ref()
            .and_then(|v| v.group_by.as_deref())
            .and_then(|g| self.database.database.as_ref()?.property(g));
        let text = |s: String| {
            line(s)
                .font_weight(FontWeight(t.sizes.medium))
                .into_any_element()
        };
        match key {
            GroupKey::Empty => text(format!("No {}", p.map_or("value", |p| p.name.as_str()))),
            GroupKey::Option(id) => match p.and_then(|p| p.option(id)) {
                Some(o) => self.chip(o.name.clone(), &o.color).into_any_element(),
                None => text(id.clone()),
            },
            GroupKey::Checked(b) => text(if *b { "Checked" } else { "Unchecked" }.into()),
            GroupKey::Person(person) => text(person_label(self.host.as_ref(), person)),
            GroupKey::Date(g) => text(format::date_group(g)),
        }
    }

    fn render_group(&self, ix: usize, g: usize, cx: &mut Context<Self>) -> AnyElement {
        let t = &self.theme;
        let Some(group) = self.shown.groups.get(g) else {
            return div().into_any_element();
        };
        let folded = self.shown.collapsed.get(g).copied().unwrap_or(false);
        let focused = self.cell.is_some_and(|c| c.item == ix);
        let key = group.key.to_string();
        let count = group.rows.len();
        let focus = self.focus.clone();
        Press::new(("group", ix))
            .tab_stop(false)
            .focus_ring_color(t.colors.focus)
            .focus_ring(false)
            .selected(focused)
            .accessibility_label(format!(
                "{} rows, {}",
                count,
                if folded { "folded" } else { "open" }
            ))
            .debug_selector(move || format!("group:{key}"))
            .h(self.row_height())
            .w_full()
            .flex()
            .items_center()
            .gap(t.px(8.))
            .px(t.px(t.sizes.cell_padding))
            .overflow_hidden()
            .bg(t.colors.group)
            .border_b_1()
            .border_color(t.colors.line)
            .hover(|s| s.bg(t.colors.hover))
            .when(focused, |d| d.border_2().border_color(t.colors.focus))
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
                    .text_size(t.px(t.sizes.small))
                    .text_color(t.colors.text_muted)
                    .child(count.to_string()),
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                window.focus(&focus, cx);
                this.cell = Some(super::CellRef {
                    item: ix,
                    column: 0,
                });
                this.toggle_group(g, window, cx);
            }))
            .into_any_element()
    }

    fn render_new(&self, ix: usize, g: Option<usize>, cx: &mut Context<Self>) -> AnyElement {
        let t = &self.theme;
        let focused = self.cell.is_some_and(|c| c.item == ix);
        let key = g
            .and_then(|g| self.shown.groups.get(g))
            .map_or_else(|| "table".to_owned(), |g| g.key.to_string());
        let focus = self.focus.clone();
        Press::new(("new", ix))
            .tab_stop(false)
            .focus_ring_color(t.colors.focus)
            .focus_ring(false)
            .accessibility_label("New row")
            .debug_selector(move || format!("new:{key}"))
            .h(self.row_height())
            .w_full()
            .flex()
            .items_center()
            .gap(t.px(6.))
            .px(t.px(t.sizes.cell_padding))
            .text_color(t.colors.text_faint)
            .border_b_1()
            .border_color(t.colors.line)
            .hover(|s| s.bg(t.colors.hover).text_color(t.colors.text_muted))
            .when(focused, |d| d.border_2().border_color(t.colors.focus))
            .child(icon(
                IconName::Plus,
                t.px(t.sizes.icon),
                t.colors.text_faint,
            ))
            .child(line("New"))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                window.focus(&focus, cx);
                this.cell = Some(super::CellRef {
                    item: ix,
                    column: 0,
                });
                this.new_row(g, window, cx);
            }))
            .into_any_element()
    }

    // --- Menus ------------------------------------------------------------------------------

    fn render_popover(&self, _: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let t = &self.theme;
        let popover = self.popover.clone()?;
        let target = match &popover {
            Popover::Header { column, .. } => self.header_bounds(*column),
            Popover::Columns { .. } => {
                let b = self.bounds.get();
                let w = t.pixels(t.sizes.menu_width, self.rem);
                gpui::Bounds::new(
                    point((b.right() - w).max(b.left()), b.top()),
                    gpui::size(w, self.header_height()),
                )
            }
            Popover::Picker { cell, .. } | Popover::Date { cell, .. } => self.cell_bounds(*cell),
            Popover::FilterValue { column } => self.header_bounds(*column),
        };
        let filter_value = matches!(popover, Popover::FilterValue { .. });
        let body = match &popover {
            Popover::Date { month, day, cell } => self.render_day_grid(*month, *day, *cell, cx),
            Popover::FilterValue { column } => self.render_filter_value(*column),
            Popover::Header { highlight, .. }
            | Popover::Columns { highlight }
            | Popover::Picker { highlight, .. } => self.render_entries(*highlight, cx),
        };
        let shadow = BoxShadow {
            color: t.colors.shadow,
            offset: point(px(0.), px(6.)),
            blur_radius: px(18.),
            spread_radius: px(0.),
            inset: false,
        };
        let menu = div()
            .id("gpui-tables-menu")
            .debug_selector(|| "menu".into())
            .occlude()
            .w(t.px(t.sizes.menu_width))
            .max_h(t.px(520.))
            .overflow_y_scroll()
            .py(t.px(4.))
            .rounded(t.px(t.sizes.radius))
            .border_1()
            .border_color(t.colors.popover_border)
            .bg(t.colors.popover)
            .shadow(vec![shadow])
            .text_size(t.px(t.sizes.text))
            .text_color(t.colors.text)
            .font_family(t.fonts.body.clone())
            .on_mouse_down_out(cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                // A filter value typed is applied by pressing elsewhere, as a cell's is.
                if filter_value {
                    this.finish_editor(true, false, window, cx);
                }
                this.popover = None;
                cx.notify();
            }))
            .child(body);
        Some(
            deferred(
                anchored()
                    .anchor(Anchor::TopLeft)
                    .attach(target, Anchor::BottomLeft)
                    .snap_to_window_with_margin(px(8.))
                    .child(menu),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }

    /// A filter's value typed in place under the column's header: what it filters by, then the
    /// field. Enter, Tab or pressing elsewhere applies it, Escape closes it.
    fn render_filter_value(&self, column: usize) -> AnyElement {
        let t = &self.theme;
        let Some(e) = self.editor.as_ref() else {
            return div().into_any_element();
        };
        let name = self.property_at(column).map_or("", |p| p.name.as_str());
        let how = match &e.target {
            super::TextTarget::Filter { op, .. } => match op {
                crate::model::FilterOp::Contains => "contains",
                crate::model::FilterOp::Greater => "is greater than",
                crate::model::FilterOp::Less => "is less than",
                _ => "is",
            },
            super::TextTarget::Cell { .. } => "",
        };
        let label = format!("{name} {how}");
        div()
            .px(t.px(8.))
            .pb(t.px(4.))
            .flex()
            .flex_col()
            .gap(t.px(4.))
            .child(
                line(label.clone())
                    .px(t.px(4.))
                    .pt(t.px(4.))
                    .text_size(t.px(t.sizes.small))
                    .text_color(t.colors.text_muted),
            )
            .child(
                div()
                    .debug_selector(|| "editor".into())
                    .h(t.hit(30., self.rem))
                    .px(t.px(8.))
                    .flex()
                    .items_center()
                    .rounded(t.px(t.sizes.chip_radius))
                    .bg(t.colors.background)
                    .border_2()
                    .border_color(t.colors.focus)
                    .child(
                        Input::new(&e.state)
                            .appearance(false)
                            .accessibility_label(label)
                            .caret_color(t.colors.accent)
                            .selection_color(t.colors.accent.opacity(0.3))
                            .placeholder_color(t.colors.text_faint)
                            .flex_1()
                            .min_w_0(),
                    ),
            )
            .into_any_element()
    }

    fn render_entries(&self, highlight: usize, cx: &mut Context<Self>) -> AnyElement {
        let t = &self.theme;
        let entries = self.entries();
        let hit = t.hit(28., self.rem);
        let any_check = entries.iter().any(|e| e.checked.is_some());
        div()
            .flex()
            .flex_col()
            .children(entries.into_iter().enumerate().map(|(i, e)| {
                let Entry {
                    label,
                    action,
                    icon: ic,
                    checked,
                    chip,
                } = e;
                let Some(action) = action else {
                    return div()
                        .px(t.px(12.))
                        .pt(t.px(8.))
                        .pb(t.px(2.))
                        .text_size(t.px(t.sizes.small))
                        .text_color(t.colors.text_muted)
                        .child(line(label))
                        .into_any_element();
                };
                let sel = label.clone();
                let mark = checked.unwrap_or(false);
                Press::new(ElementId::NamedInteger("entry".into(), i as u64))
                    .tab_stop(false)
                    .focus_ring_color(t.colors.focus)
                    .role(Role::MenuItem)
                    .selected(mark)
                    .debug_selector(move || format!("entry:{sel}"))
                    .mx(t.px(4.))
                    .h(hit)
                    .px(t.px(8.))
                    .flex()
                    .items_center()
                    .gap(t.px(8.))
                    .rounded(t.px(t.sizes.chip_radius))
                    .overflow_hidden()
                    .when(i == highlight, |d| d.bg(t.colors.hover))
                    .hover(|s| s.bg(t.colors.hover))
                    .when(any_check, |d| {
                        d.child(
                            div()
                                .flex_none()
                                .w(t.px(t.sizes.icon))
                                .flex()
                                .items_center()
                                .when(mark, |d| {
                                    d.child(icon(
                                        IconName::Check,
                                        t.px(t.sizes.icon),
                                        t.colors.accent,
                                    ))
                                }),
                        )
                    })
                    .when_some(ic, |d, ic| {
                        d.child(icon(ic, t.px(t.sizes.icon), t.colors.text_muted))
                    })
                    .map(|d| match &chip {
                        Some(c) => {
                            d.child(div().min_w_0().flex().child(self.chip(label.clone(), c)))
                        }
                        None => d.child(line(label.clone()).flex_1()),
                    })
                    .on_click(cx.listener(move |this, e: &ClickEvent, window, cx| {
                        this.activate(action.clone(), e.modifiers().shift, window, cx);
                    }))
                    .into_any_element()
            }))
            .into_any_element()
    }

    fn render_day_grid(
        &self,
        month: i64,
        day: i64,
        cell: super::CellRef,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = &self.theme;
        let (y, m, _) = time::civil_from_days(month);
        let monday = self.host.week_starts_monday();
        let first_weekday = time::weekday(month) as i64; // 0 = Monday
        let lead = if monday {
            first_weekday
        } else {
            (first_weekday + 1) % 7
        };
        let start = month - lead;
        let offset = i64::from(self.host.utc_offset_minutes()) * 60_000;
        let current = self
            .cell_target(cell)
            .and_then(|(r, p)| self.rows[r].value(&p.id).and_then(Value::as_date).copied())
            .map(|d| time::days_of(&d, offset).0);
        let today = self.shown.today;
        let hit = t.hit(28., self.rem);
        let names: [&str; 7] = if monday {
            ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"]
        } else {
            ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"]
        };
        let turn = |id: &'static str,
                    label: &'static str,
                    ic: IconName,
                    d: i64,
                    cx: &mut Context<Self>| {
            Press::new(id)
                .tab_stop(false)
                .focus_ring_color(t.colors.focus)
                .accessibility_label(label)
                .debug_selector(move || id.into())
                .size(hit)
                .flex()
                .items_center()
                .justify_center()
                .rounded(t.px(t.sizes.chip_radius))
                .hover(|s| s.bg(t.colors.hover))
                .child(icon(ic, t.px(t.sizes.icon), t.colors.text_muted))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.turn_month(d, cx);
                    window.focus(&this.focus, cx);
                }))
        };
        let weeks = (0..6)
            .map(|w| {
                div().flex().justify_between().children((0..7).map(|d| {
                    let dd = start + w * 7 + d;
                    let in_month = time::civil_from_days(dd).1 == m;
                    let is_current = current == Some(dd);
                    let label = time::civil_from_days(dd).2.to_string();
                    let ymd = time::format_ymd(dd);
                    Press::new(ElementId::NamedInteger("day".into(), (w * 7 + d) as u64))
                        .tab_stop(false)
                        .focus_ring_color(t.colors.focus)
                        .role(Role::Button)
                        .accessibility_label(format::day(dd))
                        .selected(is_current)
                        .debug_selector(move || format!("day:{ymd}"))
                        .size(hit)
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(t.px(t.sizes.chip_radius))
                        .text_size(t.px(t.sizes.small))
                        .when(!in_month, |s| s.text_color(t.colors.text_faint))
                        .when(dd == today, |s| {
                            s.border_1().border_color(t.colors.line_strong)
                        })
                        .when(dd == day, |s| s.bg(t.colors.hover))
                        .when(is_current, |s| {
                            s.bg(t.colors.accent).text_color(t.colors.on_accent)
                        })
                        .hover(|s| s.bg(t.colors.hover))
                        .child(label)
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            this.pick_day(Some(dd), window, cx);
                        }))
                }))
            })
            .collect::<Vec<_>>();
        let footer_button =
            |id: &'static str, label: &'static str, cx: &mut Context<Self>, day: Option<i64>| {
                Press::new(id)
                    .tab_stop(false)
                    .focus_ring_color(t.colors.focus)
                    .debug_selector(move || id.into())
                    .h(hit)
                    .px(t.px(8.))
                    .flex()
                    .items_center()
                    .rounded(t.px(t.sizes.chip_radius))
                    .text_color(t.colors.text_muted)
                    .hover(|s| s.bg(t.colors.hover))
                    .child(label)
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.pick_day(day, window, cx);
                    }))
            };
        div()
            .px(t.px(8.))
            .flex()
            .flex_col()
            .gap(t.px(2.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(turn(
                        "month-previous",
                        "Previous month",
                        IconName::ChevronLeft,
                        -1,
                        cx,
                    ))
                    .child(line(format::month(y, m)).font_weight(FontWeight(t.sizes.medium)))
                    .child(turn(
                        "month-next",
                        "Next month",
                        IconName::ChevronRight,
                        1,
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .justify_between()
                    .children(names.iter().map(|n| {
                        div()
                            .w(hit)
                            .flex()
                            .justify_center()
                            .text_size(t.px(t.sizes.small))
                            .text_color(t.colors.text_muted)
                            .child(*n)
                    })),
            )
            .children(weeks)
            .child(
                div()
                    .flex()
                    .justify_between()
                    .child(footer_button("day-today", "Today", cx, Some(today)))
                    .child(footer_button("day-clear", "Clear", cx, None)),
            )
            .into_any_element()
    }
}
