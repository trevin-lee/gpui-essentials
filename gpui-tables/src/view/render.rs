//! Drawing the table: the header, the rows (only those on screen, through `uniform_list`), the
//! cells by type, and the menus, pickers and month over them (gpui-core's Popover, PopupMenu
//! and MonthGrid).

use std::collections::HashSet;
use std::ops::Range;
use std::rc::Rc;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, BoxShadow, ClickEvent, Context, DispatchPhase, ElementId, Entity, FontWeight,
    Hsla, InteractiveElement, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ParentElement, Pixels, Render, Role, ScrollEvent, SharedString,
    StatefulInteractiveElement, Styled, TextAlign, TouchDragEvent, TouchPhase, Window, anchored,
    canvas, deferred, div, point, px, uniform_list,
};
use gpui_core::{
    DatePickerColors, DismissReason, Icon, IconName, Input, InputState, MenuReorder, MenuSelect,
    MonthGrid, Picker, Popover as CorePopover, PopoverColors, PopupMenu, PopupMenuColors,
    PopupMenuItem, Press, Selectable as _,
};

use super::format;
use super::host::person_label;
use super::list_render::Landing;
use super::table::{Action, Drag, Entry, Item, Popover, Reopen, TableView, naive_day, type_icon};
use crate::engine::{GroupKey, time};
use crate::model::{Color, Doc, Person, Property, PropertyType, Value};
use gpui_dnd::{DndProvider, DragMonitor};

impl Render for TableView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.theme = self.host.theme(cx);
        self.rem = window.rem_size();
        self.ensure();
        // The month's state lives only while its popover is open.
        if !matches!(self.popover, Some(Popover::Date { .. })) {
            self.date = None;
        }
        // A board view draws as a board, a list view as a list, any other as a table: one
        // entity and one host.
        let body = if self.shown.board.is_some() {
            self.render_board(window, cx)
        } else if self.shown.list.is_some() {
            self.render_list(window, cx)
        } else {
            self.render_table(window, cx)
        };
        // The bar of views over it (when it's on), and the bar's menus. gpui-dnd follows a table
        // row's drag and draws its preview (a host's own provider round the window takes over).
        DndProvider::new(self.render_frame(body, window, cx))
    }
}

impl TableView {
    fn render_table(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        self.clamp_scroll();
        // A row's drag that gpui-dnd ended (dropped where it couldn't land, Escape, a lost
        // touch: its preview going back) is over for the table.
        if self.board.drag.is_some() {
            let m = DragMonitor::of(cx);
            if !m.is_dragging() || m.is_returning() {
                self.board.drag = None;
            }
        }
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
        let note = self.render_drag_note();
        let root = div()
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
            .on_action(cx.listener(|this, _: &super::Up, window, cx| {
                if !this.up_to_bar(window, cx) {
                    this.on_vertical(-1, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &super::Down, _, cx| this.on_vertical(1, cx)))
            .on_action(cx.listener(|this, _: &super::Left, _, cx| this.on_horizontal(-1, cx)))
            .on_action(cx.listener(|this, _: &super::Right, _, cx| this.on_horizontal(1, cx)))
            .on_action(
                cx.listener(|this, _: &super::NextCell, window, cx| this.on_tab(false, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &super::PrevCell, window, cx| this.on_tab(true, window, cx)),
            )
            // The field takes Enter itself, and an open menu or month has the keyboard (gpui-core's
            // PopupMenu and MonthGrid pick with Enter): nothing else does while they're open.
            .on_action(cx.listener(|this, _: &super::Confirm, window, cx| {
                if !this.is_editing() && this.popover.is_none() {
                    this.confirm(window, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &super::ConfirmAdd, window, cx| {
                if !this.is_editing() && this.popover.is_none() {
                    this.confirm(window, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &super::Cancel, window, cx| this.cancel(window, cx)))
            // Delete and Backspace delete selected rows, and otherwise empty the focused cell.
            .on_action(cx.listener(|this, _: &super::Clear, window, cx| {
                if this.popover.is_none() && !this.is_editing() {
                    this.ensure();
                    if this.selected.is_empty() {
                        this.clear_cell(window, cx)
                    } else {
                        let rows = this.acted_rows();
                        this.ask_delete(rows, window, cx)
                    }
                }
            }))
            .on_action(cx.listener(|this, _: &super::DeleteRows, window, cx| {
                if this.popover.is_none() && !this.is_editing() {
                    this.ensure();
                    let rows = this.acted_rows();
                    this.ask_delete(rows, window, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &super::DuplicateRows, window, cx| {
                if this.popover.is_none() && !this.is_editing() {
                    this.ensure();
                    let rows = this.acted_rows();
                    let ids: Vec<&str> = rows.iter().map(String::as_str).collect();
                    this.duplicate_rows(&ids, window, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &super::RowMenu, _, cx| {
                if this.popover.is_none() && !this.is_editing() {
                    this.open_row_menu(cx)
                }
            }))
            .on_action(cx.listener(|this, _: &super::MoveRowsUp, window, cx| {
                this.on_move_rows(-1, window, cx)
            }))
            .on_action(cx.listener(|this, _: &super::MoveRowsDown, window, cx| {
                this.on_move_rows(1, window, cx)
            }))
            .on_action(cx.listener(|this, _: &super::SelectRow, _, cx| {
                if this.popover.is_none() && !this.is_editing() {
                    this.toggle_focused_row(cx)
                }
            }))
            .on_action(cx.listener(|this, _: &super::SelectUp, _, cx| {
                if this.popover.is_none() && !this.is_editing() {
                    this.extend_selection(-1, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &super::SelectDown, _, cx| {
                if this.popover.is_none() && !this.is_editing() {
                    this.extend_selection(1, cx)
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
            // ⌥↓: the selected rows' menu while rows are selected, else the column's.
            .on_action(cx.listener(|this, _: &super::ColumnMenu, _, cx| {
                if this.is_editing() {
                    return;
                }
                if !this.selected.is_empty() {
                    this.open_row_menu(cx)
                } else if let Some(c) = this.cell {
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
            .children(note)
            .child(popover);
        self.row_drop_target(root, cx)
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

    // While a value is typed in place, the arrows (and Page Up and Down) are the field's, and
    // while a menu or month is open they're its own (gpui-core's PopupMenu and MonthGrid): they
    // never move the focused cell.

    // Selected rows follow Up and Down; Left, Right and Tab go back to cells, letting go of
    // them.

    fn on_vertical(&mut self, d: isize, cx: &mut Context<Self>) {
        if !self.is_editing() && self.popover.is_none() {
            self.move_cell(d, 0, cx);
            self.selection_follows_focus();
        }
    }

    fn on_horizontal(&mut self, d: isize, cx: &mut Context<Self>) {
        if !self.is_editing() && self.popover.is_none() {
            self.clear_selection();
            self.move_cell(0, d, cx)
        }
    }

    fn on_move_rows(&mut self, d: isize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_editing() && self.popover.is_none() {
            self.ensure();
            let rows = self.acted_rows();
            let ids: Vec<&str> = rows.iter().map(String::as_str).collect();
            self.move_rows_by(&ids, d, window, cx)
        }
    }

    fn on_page(&mut self, d: isize, cx: &mut Context<Self>) {
        if !self.is_editing() && self.popover.is_none() {
            let rows = (self.bounds.get().size.height / self.row_height())
                .floor()
                .max(1.);
            self.move_cell(d * rows as isize, 0, cx)
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
            self.clear_selection();
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
        let more = Press::new("gpui-tables-add-property")
            .tab_stop(false)
            .focus_ring_color(t.colors.focus)
            .accessibility_label("Add a property")
            .debug_selector(|| "add-property".into())
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
                window.focus(&this.focus, cx);
                this.open_add_property_menu(window, cx);
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

    /// A property's name, or nothing for one the database hasn't.
    fn property_name(&self, property_id: &str) -> &str {
        self.database
            .database
            .as_ref()
            .and_then(|f| f.property(property_id))
            .map_or("", |p| p.name.as_str())
    }

    pub(crate) fn property_at(&self, column: usize) -> Option<&Property> {
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
        let menu_open = matches!(&self.popover,
            Some(Popover::Header { property_id }) if *property_id == p.id);
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
            // The title's name over the rows' titles, after their handles (when there's room).
            .when(column == 0 && roomy, |d| d.pl(self.handle_width()))
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
        // While rows are dragged: where they'd land, and each row's place in its group.
        let landing = self
            .board
            .drag
            .as_ref()
            .and_then(|d| d.target)
            .map(|(lane, slot)| Landing {
                lane,
                slot,
                sorted: self.is_sorted(),
            });
        let mut slot = match (&self.board.drag, range.clone().next()) {
            (Some(_), Some(start)) => self.slot_of(start),
            _ => 0,
        };
        range
            .filter_map(|ix| {
                let item = *self.shown.items.get(ix)?;
                Some(match item {
                    Item::Row { row, group } => {
                        let lane = group.unwrap_or(0);
                        let el = self.render_row(ix, row, lane, slot, landing, cx);
                        slot += 1;
                        el
                    }
                    Item::Group(g) => {
                        slot = 0;
                        self.render_group(ix, g, landing, cx)
                    }
                    Item::New(g) => self.render_new(ix, g, landing, cx),
                    Item::Calc(g) => self.render_calc(ix, g, cx),
                })
            })
            .collect()
    }

    fn render_row(
        &self,
        ix: usize,
        row: usize,
        lane: usize,
        slot: usize,
        landing: Option<Landing>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = &self.theme;
        let cols = self.shown.columns.len();
        let cells: Vec<AnyElement> = (1..cols)
            .map(|c| self.render_cell(ix, row, c, cx))
            .collect();
        let rid = self.rows[row].id.clone();
        let selected = self.selected.contains(&rid);
        let lifted = self
            .board
            .drag
            .as_ref()
            .is_some_and(|d| d.row_id == rid || d.others.iter().any(|(id, _)| *id == rid));
        let count = self
            .move_lanes()
            .and_then(|l| l.get(lane))
            .map_or(0, |l| l.rows.len());
        let bar = Landing::bar(landing, lane, slot, count);
        let pressed = rid.clone();
        let hovered = rid.clone();
        let shown = self.shown_serial;
        let sel = format!("row:{rid}");
        div()
            .id(("row", ix))
            .debug_selector(move || sel)
            .group("gpui-tables-row")
            .role(Role::Row)
            .relative()
            .h(self.row_height())
            .w_full()
            .flex()
            .border_b_1()
            .border_color(t.colors.line)
            .hover(|s| s.bg(t.colors.hover))
            .when(selected, |d| d.bg(t.colors.selection))
            .when(lifted, |d| d.opacity(0.35))
            .on_hover(
                cx.listener(move |this, over: &bool, _, cx| this.hover_row(&hovered, *over, cx)),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    if this.still_shown(shown) {
                        this.context_menu(ix, &pressed, e.position, window, cx);
                    }
                }),
            )
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
            .when_some(bar, |d, top| d.child(self.landing_bar(top)))
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
        let shown = self.shown_serial;
        let is_check = p.kind == PropertyType::Checkbox;
        let numeric = p.kind == PropertyType::Number;
        let editor = self.editor.as_ref().filter(|e| e.edits(&r.id, &p.id));
        let editing = editor.is_some();
        let content = match editor {
            Some(e) => self.render_editor(e, column),
            None => self.cell_content(ix, row, r, p, row_focused, cx),
        };
        let label = format!("{}: {}", p.name, self.cell_text(r, p));
        // The first column starts with the row's handle, in its padding's place.
        let handle = (column == 0).then(|| {
            let lane = self.lane_of(ix).unwrap_or(0);
            let key = self
                .move_lanes()
                .and_then(|l| l.get(lane))
                .map_or_else(|| "all".to_owned(), |l| l.key_string());
            // With a pointer the handle is only made for the row under it, the focused row,
            // selected rows and dragged ones: the others keep its room empty.
            let dragged =
                self.board.drag.as_ref().is_some_and(|d| {
                    d.row_id == r.id || d.others.iter().any(|(id, _)| *id == r.id)
                });
            let built = t.touch
                || row_focused
                || dragged
                || self.hovered_row.as_deref() == Some(r.id.as_str())
                || self.selected.contains(&r.id);
            if built {
                self.render_handle(ix, row, key, cx)
            } else {
                div()
                    .flex_none()
                    .w(self.handle_width())
                    .h_full()
                    .into_any_element()
            }
        });
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
            .when(handle.is_some(), |d| d.pl_0())
            .children(handle)
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
                // A press in the field being typed in is the field's (placing the caret). One
                // on lines since worked out again may be on another row's place.
                if editing || !this.still_shown(shown) {
                    return;
                }
                let cell = super::CellRef { item: ix, column };
                window.focus(&this.focus, cx);
                this.popover = None;
                this.clear_selection();
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
        let field = FieldColors::of(t);
        // A number is typed at the right, where the cell shows it.
        let align = if e.kind == PropertyType::Number {
            TextAlign::Right
        } else {
            TextAlign::Left
        };
        let input = Input::new(&e.state)
            .text_align(align)
            .appearance(false)
            .accessibility_label(e.label.clone())
            .caret_color(field.caret)
            .selection_color(field.selection)
            .placeholder_color(field.placeholder)
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
        let v = r.value(&p.id);
        match p.kind {
            PropertyType::Person => self.people_text(v),
            PropertyType::Relation => self.relation_text(p, v),
            PropertyType::CreatedBy => self.person_text(r.created_by()),
            PropertyType::EditedBy => self.person_text(r.edited_by()),
            _ => plain_cell_text(r, p, i64::from(self.host.utc_offset_minutes()) * 60_000),
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
                let shown = self.shown_serial;
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
                        if !this.still_shown(shown) {
                            return;
                        }
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
            PropertyType::Select => {
                match v.and_then(Value::option_id).and_then(|id| p.option(id)) {
                    Some(o) => div()
                        .min_w_0()
                        .flex()
                        .child(self.chip(o.name.clone(), &o.color))
                        .into_any_element(),
                    None => div().into_any_element(),
                }
            }
            PropertyType::MultiSelect => {
                let ids = v.and_then(Value::option_ids).unwrap_or(&[]);
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
            PropertyType::Url | PropertyType::Email => line(
                p.shown_text(v.and_then(Value::as_str).unwrap_or_default())
                    .to_owned(),
            )
            .text_color(t.colors.text)
            .underline()
            .into_any_element(),
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

    fn render_group(
        &self,
        ix: usize,
        g: usize,
        landing: Option<Landing>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = &self.theme;
        let Some(group) = self.shown.groups.get(g) else {
            return div().into_any_element();
        };
        // In a sorted view rows dropped in another group go where the sort puts them: the
        // group is marked rather than a place in it.
        let target = landing.is_some_and(|l| l.sorted && l.lane == g);
        let folded = self.shown.collapsed.get(g).copied().unwrap_or(false);
        let focused = self.cell.is_some_and(|c| c.item == ix);
        let key = group.key.to_string();
        let count = group.rows.len();
        let focus = self.focus.clone();
        let shown = self.shown_serial;
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
            .when(target, |d| {
                d.bg(t.colors.selection)
                    .border_2()
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
                    .text_size(t.px(t.sizes.small))
                    .text_color(t.colors.text_muted)
                    .child(count.to_string()),
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                if !this.still_shown(shown) {
                    return;
                }
                window.focus(&focus, cx);
                this.cell = Some(super::CellRef {
                    item: ix,
                    column: 0,
                });
                this.toggle_group(g, window, cx);
            }))
            .into_any_element()
    }

    fn render_new(
        &self,
        ix: usize,
        g: Option<usize>,
        landing: Option<Landing>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = &self.theme;
        let focused = self.cell.is_some_and(|c| c.item == ix);
        // An empty group's place to land is above its New row.
        let lane = g.unwrap_or(0);
        let empty = self
            .move_lanes()
            .and_then(|l| l.get(lane))
            .is_some_and(|l| l.rows.is_empty());
        let bar = empty && landing.is_some_and(|l| !l.sorted && l.lane == lane && l.slot == 0);
        let key = g
            .and_then(|g| self.shown.groups.get(g))
            .map_or_else(|| "table".to_owned(), |g| g.key.to_string());
        let focus = self.focus.clone();
        let shown = self.shown_serial;
        Press::new(("new", ix))
            .tab_stop(false)
            .focus_ring_color(t.colors.focus)
            .focus_ring(false)
            .accessibility_label("New row")
            .debug_selector(move || format!("new:{key}"))
            .relative()
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
            .when(bar, |d| d.child(self.landing_bar(true)))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                if !this.still_shown(shown) {
                    return;
                }
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

    /// The open menu, picker, month or filter field, in gpui-core's Popover. It's drawn closed
    /// while none is open, so that on closing it hands the keyboard back to the table (its
    /// named handle). While what it hangs from is scrolled out of the window (a column's header
    /// off to the side), it stays open, held at the window's edge, as the table's own panel did.
    fn render_popover(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = &self.theme;
        let popover = CorePopover::new("gpui-tables-popover")
            .return_focus(&self.focus)
            .close_when_scrolled_away(false)
            .colors(PopoverColors {
                background: t.colors.popover,
                foreground: t.colors.text,
                border: t.colors.popover_border,
            })
            .max_height(t.px(520.))
            .on_dismiss(cx.listener(|this, reason: &DismissReason, window, cx| {
                this.dismiss(*reason, window, cx)
            }));
        // The bar's menus are drawn with the bar.
        let Some(open) = self.popover.clone().filter(|p| !p.is_view_menu()) else {
            return popover.into_any_element();
        };
        let target = match &open {
            Popover::Header { property_id } | Popover::FilterValue { property_id } => {
                match self.column_of(property_id) {
                    Some(column) => self.header_bounds(column),
                    None => return popover.into_any_element(),
                }
            }
            Popover::Columns => {
                let b = self.bounds.get();
                let w = t.pixels(t.sizes.menu_width, self.rem);
                gpui::Bounds::new(
                    point((b.right() - w).max(b.left()), b.top()),
                    gpui::size(w, self.header_height()),
                )
            }
            Popover::Picker { cell } | Popover::Date { cell } => self.cell_bounds(*cell),
            Popover::AddProperty | Popover::AddRelation => self.add_button_bounds(),
            Popover::Property { property_id, .. } => self
                .shown
                .columns
                .iter()
                .position(|c| c.property_id == *property_id)
                .map_or_else(|| self.add_button_bounds(), |c| self.header_bounds(c)),
            Popover::AddView | Popover::NewBoard | Popover::View { .. } => {
                return popover.into_any_element();
            }
            Popover::Calc { property_id, group } => {
                match self.calc_bounds(property_id, group.as_ref()) {
                    Some(b) => b,
                    None => return popover.into_any_element(),
                }
            }
            // From the point a right-click pressed, else the first row's handle (or the top of
            // the table, while it's scrolled away).
            Popover::Row { rows, at, .. } => match at {
                Some(at) => gpui::Bounds::new(*at, gpui::size(px(1.), px(1.))),
                None => rows
                    .first()
                    .and_then(|id| self.handle_bounds(id))
                    .unwrap_or_else(|| {
                        let b = self.bounds.get();
                        gpui::Bounds::new(b.origin, gpui::size(px(1.), self.header_height()))
                    }),
            },
        };
        // A menu's width, or what's seen of its anchor if that's wider (gpui-core's Popover
        // hangs it from the part of a header that's in the window), inside the window (its
        // edges, the popover's border and padding).
        let viewport = window.viewport_size();
        let seen = target.intersect(&gpui::Bounds::new(point(px(0.), px(0.)), viewport));
        let room = (viewport.width - px(26.)).max(px(0.));
        let width = t
            .pixels(t.sizes.menu_width, self.rem)
            .max(seen.size.width)
            .min(room);
        let body = div()
            .debug_selector(|| "menu".into())
            .font_family(t.fonts.body.clone())
            .text_size(t.px(t.sizes.text))
            .text_color(t.colors.text);
        let popover = popover.open(true).anchor(target);
        let name = |column: usize| self.property_at(column).map_or("", |p| p.name.as_str());
        match open {
            Popover::Header { property_id } => popover
                .accessibility_label(format!("{} column", self.property_name(&property_id)))
                .p(t.px(4.))
                .child(
                    body.w(width)
                        .children(self.render_note(cx))
                        .child(self.popup_menu(cx)),
                )
                .into_any_element(),
            Popover::Calc { property_id, .. } => popover
                .accessibility_label(format!("{} calculation", self.property_name(&property_id)))
                .p(t.px(4.))
                .child(body.w(width).child(self.popup_menu(cx)))
                .into_any_element(),
            Popover::Columns => popover
                .accessibility_label("Properties")
                .p(t.px(4.))
                .child(body.w(width).child(self.popup_menu(cx)))
                .into_any_element(),
            // A name is typed: the picker's field has the keyboard, and the popover leaves it
            // there.
            Popover::AddProperty => popover
                .take_focus(false)
                .accessibility_label("Add a property")
                .p(t.px(4.))
                .child(
                    body.w(width)
                        .children(self.render_note(cx))
                        .child(self.add_property_picker(cx)),
                )
                .into_any_element(),
            Popover::Property { page, property_id } if page.is_field() => {
                let label = self
                    .database
                    .database
                    .as_ref()
                    .and_then(|f| f.property(&property_id))
                    .map_or_else(String::new, |p| p.name.clone());
                popover
                    .take_focus(false)
                    .accessibility_label(label)
                    .p(t.px(4.))
                    .child(body.w(width).child(self.render_field_page(&page, cx)))
                    .into_any_element()
            }
            Popover::Property { property_id, .. } => {
                let label = self
                    .database
                    .database
                    .as_ref()
                    .and_then(|f| f.property(&property_id))
                    .map_or_else(String::new, |p| p.name.clone());
                popover
                    .accessibility_label(label)
                    .p(t.px(4.))
                    .child(
                        body.w(width)
                            .children(self.render_note(cx))
                            .child(self.popup_menu(cx)),
                    )
                    .into_any_element()
            }
            Popover::Row { rows, .. } => popover
                .accessibility_label(if rows.len() == 1 {
                    "Row".to_owned()
                } else {
                    format!("{} rows", rows.len())
                })
                .match_trigger_width(false)
                .p(t.px(4.))
                .child(
                    body.w(t.pixels(t.sizes.menu_width, self.rem).min(room))
                        .children(self.render_note(cx))
                        .child(self.popup_menu(cx)),
                )
                .into_any_element(),
            Popover::AddRelation => popover
                .accessibility_label("Add a relation")
                .p(t.px(4.))
                .child(
                    body.w(width)
                        .children(self.render_note(cx))
                        .child(self.popup_menu(cx)),
                )
                .into_any_element(),
            Popover::Picker { cell } => popover
                .accessibility_label(name(cell.column).to_owned())
                .p(t.px(4.))
                .child(body.w(width).child(self.picker(cell, cx)))
                .into_any_element(),
            // The field has the keyboard: the popover leaves it there.
            Popover::FilterValue { property_id } => popover
                .take_focus(false)
                .py(t.px(4.))
                .child(body.w(width).child(self.render_filter_value(&property_id)))
                .into_any_element(),
            Popover::AddView | Popover::NewBoard | Popover::View { .. } => {
                popover.open(false).into_any_element()
            }
            // The month took the keyboard when it opened; the popover leaves it there.
            Popover::Date { cell } => {
                let Some((state, _)) = &self.date else {
                    return popover.open(false).into_any_element();
                };
                let mut grid = MonthGrid::new(state)
                    .day_size(t.px(28.))
                    .colors(self.month_colors())
                    .footer(self.render_date_footer(cx));
                if t.touch {
                    grid = grid.touch();
                }
                // A range is drawn as one, its ends picked and the days between tinted; a pick
                // moves its start and keeps its length.
                let offset = i64::from(self.host.utc_offset_minutes()) * 60_000;
                let range = self
                    .cell_target(cell)
                    .and_then(|(r, p)| self.rows[r].value(&p.id).and_then(Value::as_date))
                    .filter(|d| d.end.is_some())
                    .map(|d| time::days_of(d, offset));
                if let Some((first, last)) = range {
                    grid = grid.shown_range(naive_day(first), naive_day(last));
                }
                popover
                    .take_focus(false)
                    .match_trigger_width(false)
                    .accessibility_label(name(cell.column).to_owned())
                    .p(t.px(4.))
                    .child(body.child(grid))
                    .into_any_element()
            }
        }
    }

    /// The popover asks to close: a press elsewhere, or Escape. A filter value being typed is
    /// applied by a press elsewhere, as a cell's is, and dropped by Escape.
    pub(crate) fn dismiss(
        &mut self,
        reason: DismissReason,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(self.popover, Some(Popover::FilterValue { .. })) {
            let escape = reason == DismissReason::Escape;
            self.finish_editor(!escape, escape, window, cx);
        }
        self.date = None;
        self.popover = None;
        self.close_property_menu();
        cx.notify();
    }

    /// Entries as gpui-core's, what each one's id does, and the one to open on. An entry's id
    /// is its label (numbered when two in the same menu are the same), so a test or a host finds
    /// it as `menu-item-<label>`. An option is drawn as the table draws it in a cell, a coloured
    /// pill (`label_element`), while its name stays the entry's label: what assistive tech
    /// reads and what a picker's search matches. An entry with a submenu comes with its
    /// entries, to be made into the submenu ([`TableView::menu_of`]).
    pub(crate) fn entries_of(
        &self,
        entries: Vec<Entry>,
    ) -> (Vec<MenuEntry>, PickActions, Option<SharedString>) {
        let ids = entry_ids(&entries);
        let mut actions = Vec::new();
        let mut out = Vec::new();
        let mut open_on = None;
        for (e, id) in entries.into_iter().zip(ids) {
            let Entry {
                label,
                action,
                icon,
                checked,
                chip,
                hint,
                disabled,
                highlight,
                submenu,
                reorder,
            } = e;
            let Some(id) = id else {
                out.push(MenuEntry::Heading(label.into()));
                continue;
            };
            let mut item = PopupMenuItem::new(id.clone(), label.clone());
            if let Some(icon) = icon {
                item = item.icon(icon);
            }
            if let Some(checked) = checked {
                item = item.checked(checked);
            }
            if let Some(color) = &chip {
                let pill = format!("pill:{label}");
                item = item.label_element(self.chip(label, color).debug_selector(move || pill));
            }
            if let Some(hint) = hint {
                item = item.shortcut(hint);
            }
            if disabled {
                item = item.disabled(true);
            }
            if let Some(group) = reorder {
                item = item.reorder_group(group);
            }
            if highlight && open_on.is_none() {
                open_on = Some(id.clone());
            }
            if let Some(action) = action {
                actions.push((id.clone(), action));
            }
            out.push(MenuEntry::Item(id, item, submenu));
        }
        (out, Rc::new(actions), open_on)
    }

    /// Entries as gpui-core's PopupMenu, their submenus made the same way. A submenu takes its
    /// rows' size and colours from the menu it opens from. gpui-core keeps the highlight and the
    /// open submenu on their entries (by id) while entries come, go and move, so a menu keeps
    /// its place as its entries change (an option moved, Date grouping added).
    ///
    /// It opens with the submenus of `open`'s entries open, one in another, and on `on` in the
    /// last of them (in the menu itself when `open` is empty), else on its highlighted entry.
    ///
    /// `at` is the menu's place: the entries whose submenus lead to it from the top menu. The
    /// open menu at [`TableView::move_highlight`]'s place has its highlight moved there.
    pub(crate) fn menu_of(
        &self,
        key: &str,
        entries: Vec<Entry>,
        at: &[String],
        open: &[String],
        on: Option<&str>,
        cx: &mut Context<Self>,
    ) -> PopupMenu {
        let (entries, actions, open_on) = self.entries_of(entries);
        let mut menu = PopupMenu::new(ElementId::NamedInteger(
            key.to_owned().into(),
            self.menu_serial,
        ));
        let mut highlight = open_on;
        let mut opened = None;
        for e in entries {
            menu = match e {
                MenuEntry::Heading(title) => menu.heading(title),
                MenuEntry::Item(id, item, sub) => {
                    if open.is_empty() && on == Some(id.as_ref()) {
                        highlight = Some(id.clone());
                    }
                    match sub {
                        None => menu.item(item),
                        Some(sub) => {
                            let here = opened.is_none() && open.first().is_some_and(|o| *o == *id);
                            let (path, deeper) = if here {
                                opened = Some(id.clone());
                                (&open[1..], on)
                            } else {
                                (&[][..], None)
                            };
                            let mut place = at.to_vec();
                            place.push(id.to_string());
                            let key = format!("{key}/{id}");
                            let sub = self.menu_of(&key, sub, &place, path, deeper, cx);
                            menu.item(item.submenu(sub))
                        }
                    }
                }
            };
        }
        menu = match (opened, highlight) {
            (Some(id), _) => menu.open_submenu(id),
            (None, Some(id)) => menu.highlight(id),
            (None, None) => menu,
        };
        // The highlight moved to an entry here, while the menu's open.
        if let Some((place, id, request)) = &self.move_highlight
            && place.as_slice() == at
        {
            menu = menu.move_highlight_request(id.clone(), *request);
        }
        // The top menu takes every reorder; its submenus send theirs to it.
        if at.is_empty() {
            menu =
                menu.on_reorder(cx.listener(|this, ev: &MenuReorder, window, cx| {
                    this.reorder_entry(ev, window, cx)
                }));
        }
        menu.on_select(self.on_pick(actions, cx))
    }

    /// An entry of a reorder group moved: an option among the options, a card property among
    /// those shown. The table hands the host the change and draws the menu in the new order.
    fn reorder_entry(&mut self, ev: &MenuReorder, window: &mut Window, cx: &mut Context<Self>) {
        match ev.group.as_ref() {
            super::properties::OPTION_ORDER => {
                self.reorder_option(ev.from, ev.to, &ev.id, window, cx)
            }
            super::views::CARD_ORDER => self.reorder_card(ev.from, ev.to, &ev.id, window, cx),
            _ => {}
        }
    }

    /// Runs a picked entry's action. Shift (Shift-Enter, or a Shift-click) adds a sort to the
    /// others rather than replacing them. (A change that takes away the submenu with the
    /// keyboard, an option deleted, say, gives it back to the menu around it: gpui-core's.)
    pub(crate) fn on_pick(
        &self,
        actions: PickActions,
        cx: &mut Context<Self>,
    ) -> impl Fn(&MenuSelect, &mut Window, &mut gpui::App) + 'static {
        cx.listener(move |this, pick: &MenuSelect, window, cx| {
            if let Some((_, action)) = actions.iter().find(|(id, _)| *id == pick.id) {
                this.activate(action.clone(), pick.modifiers.shift, window, cx);
            }
        })
    }

    /// A menu's and a picker's colours, from the table's theme: the hover fill for the
    /// highlight, the accent for checks, muted headings.
    #[allow(clippy::needless_update)]
    pub(crate) fn menu_colors(&self) -> PopupMenuColors {
        let t = &self.theme;
        PopupMenuColors {
            highlight: Some(t.colors.hover),
            highlight_text: Some(t.colors.text),
            muted: Some(t.colors.text_muted),
            separator: Some(t.colors.line),
            check: Some(t.colors.accent),
            heading: Some(t.colors.text_muted),
            ..Default::default()
        }
    }

    /// The open menu as gpui-core's PopupMenu: its sections under their headings (at the
    /// table's small size), rows 28 points tall at the table's text size (44 on touch), checks
    /// in a column before the labels as Notion's, and the deeper menus (a type, the options, a
    /// number's format) as submenus beside their entries. It opens on its highlighted entry, or
    /// where [`TableView::reopen`] says (the entry a step of it came back from, in its submenu).
    pub(crate) fn popup_menu(&self, cx: &mut Context<Self>) -> PopupMenu {
        let t = &self.theme;
        let entries = self.entries();
        let Reopen { path, on, .. } = &self.reopen;
        self.menu_of("gpui-tables-menu", entries, &[], path, on.as_deref(), cx)
            .touch(t.touch)
            .row_height(t.px(28.))
            .text_size(t.px(t.sizes.text))
            .heading_size(t.px(t.sizes.small))
            .check_leading(true)
            .colors(self.menu_colors())
    }

    /// A cell's picker as gpui-core's Picker: typing searches the options (people, rows), Enter
    /// picks the first match, and it opens on the current value. Checks lead the labels, as in
    /// the menus, and its search field is drawn as the table's other fields ([`FieldColors`]).
    /// A select's or multi-select's offers to create an option from what's typed
    /// ([`TableView::create_option`]), its field taking at most an option name's length.
    fn picker(&self, cell: super::CellRef, cx: &mut Context<Self>) -> Picker {
        let t = &self.theme;
        let field = FieldColors::of(t);
        let (entries, actions, _) = self.entries_of(self.entries());
        let kind = self.cell_target(cell).map(|(_, p)| p.kind.clone());
        let placeholder = match kind {
            Some(PropertyType::Select | PropertyType::MultiSelect) => "Search or create an option",
            Some(PropertyType::Person) => "Search for a person",
            _ => "Search",
        };
        let picker = Picker::new(ElementId::NamedInteger(
            "gpui-tables-picker".into(),
            self.menu_serial,
        ))
        .placeholder(placeholder)
        .items(entries.into_iter().filter_map(|e| match e {
            MenuEntry::Item(_, item, _) => Some(item),
            MenuEntry::Heading(_) => None,
        }))
        .touch(t.touch)
        .row_height(t.px(28.))
        .text_size(t.px(t.sizes.text))
        .check_leading(true)
        .caret_color(field.caret)
        .selection_color(field.selection)
        .placeholder_color(field.placeholder)
        .colors(self.menu_colors())
        .on_select(self.on_pick(actions, cx));
        if self.can_create_option() {
            // What's typed may name a new option: at most an option name's length.
            picker
                .max_chars(self.host.limits().option_name_chars)
                .on_create(
                    cx.listener(|this, text: &str, window, cx| {
                        this.create_option(text, window, cx)
                    }),
                )
        } else {
            picker
        }
    }

    /// The month's colours, from the table's theme: the accent for the picked day (or a
    /// range's ends) and the ring round today, the selection's tint between them, the hover
    /// fill.
    fn month_colors(&self) -> DatePickerColors {
        let t = &self.theme;
        DatePickerColors {
            selected: Some(t.colors.accent),
            selected_text: Some(t.colors.on_accent),
            range: Some(t.colors.selection),
            hover: Some(t.colors.hover),
            muted: Some(t.colors.text_muted),
            cursor: Some(t.colors.line_strong),
            focus_ring: Some(t.colors.focus.opacity(0.5)),
            today_ring: Some(t.colors.accent),
            ..Default::default()
        }
    }

    /// Under the month: Today, and Clear to empty the date.
    fn render_date_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = &self.theme;
        let hit = t.hit(28., self.rem);
        let today = self.shown.today;
        let button =
            |id: &'static str, label: &'static str, day: Option<i64>, cx: &mut Context<Self>| {
                Press::new(id)
                    .tab_stop(false)
                    .focus_ring_color(t.colors.focus)
                    .debug_selector(move || id.into())
                    .h(hit)
                    .px(t.px(8.))
                    .flex()
                    .items_center()
                    .rounded(t.px(t.sizes.chip_radius))
                    .text_size(t.px(t.sizes.text))
                    .text_color(t.colors.text_muted)
                    .hover(|s| s.bg(t.colors.hover))
                    .child(label)
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.pick_day(day, window, cx);
                    }))
            };
        div()
            .debug_selector(|| "month-footer".into())
            .flex()
            .justify_between()
            .child(button("day-today", "Today", Some(today), cx))
            .child(button("day-clear", "Clear", None, cx))
            .into_any_element()
    }

    /// A filter's value typed in place under the column's header: what it filters by, then the
    /// field. Enter, Tab or pressing elsewhere applies it, Escape closes it.
    fn render_filter_value(&self, property_id: &str) -> AnyElement {
        let t = &self.theme;
        let field = FieldColors::of(t);
        let Some(e) = self.editor.as_ref() else {
            return div().into_any_element();
        };
        let name = self.property_name(property_id);
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
                            .caret_color(field.caret)
                            .selection_color(field.selection)
                            .placeholder_color(field.placeholder)
                            .flex_1()
                            .min_w_0(),
                    ),
            )
            .into_any_element()
    }
}

/// The colours of every field the table types in (a cell's value in place, a filter's value, a
/// picker's search, a property's name), so they all look alike: the caret in the accent, the
/// selection the accent at 30%, the placeholder faint.
pub(crate) struct FieldColors {
    caret: Hsla,
    selection: Hsla,
    placeholder: Hsla,
}

impl FieldColors {
    pub(crate) fn of(t: &super::TableTheme) -> Self {
        Self {
            caret: t.colors.accent,
            selection: t.colors.accent.opacity(0.3),
            placeholder: t.colors.text_faint,
        }
    }
}

/// A menu's entry, before it's handed to gpui-core: a section's heading, or an entry with its
/// id and the entries of its submenu, if it opens one.
pub(crate) enum MenuEntry {
    Heading(SharedString),
    Item(SharedString, PopupMenuItem, Option<Vec<Entry>>),
}

/// Each entry's id, as its menu has it: its label, numbered when two in the same menu are the
/// same (None for a section's heading).
pub(crate) fn entry_ids(entries: &[Entry]) -> Vec<Option<SharedString>> {
    let mut used: HashSet<SharedString> = HashSet::new();
    entries
        .iter()
        .map(|e| {
            if e.action.is_none() && e.submenu.is_none() {
                return None;
            }
            let mut id = SharedString::from(e.label.clone());
            let mut n = 1;
            while used.contains(&id) {
                n += 1;
                id = format!("{} ({n})", e.label).into();
            }
            used.insert(id.clone());
            Some(id)
        })
        .collect()
}

/// What each entry's id does.
pub(crate) type PickActions = Rc<Vec<(SharedString, Action)>>;

// --- Property menus -------------------------------------------------------------------------

impl TableView {
    /// Where the + after the last column is, in the window (held inside the table).
    pub(crate) fn add_button_bounds(&self) -> gpui::Bounds<Pixels> {
        let t = &self.theme;
        let b = self.bounds.get();
        let n = self.shown.columns.len();
        let w = t.hit(32., self.rem);
        let left = if n <= 1 {
            b.origin.x + self.column_px(0)
        } else {
            self.column_left(n - 1) + self.column_px(n - 1)
        };
        let left = left.min(b.right() - w).max(b.left());
        gpui::Bounds::new(point(left, b.top()), gpui::size(w, self.header_height()))
    }

    /// Why the open menu refused something, or what limit it's at: muted, over its entries.
    pub(crate) fn render_note(&self, cx: &App) -> Option<AnyElement> {
        let t = &self.theme;
        let note = self.page_note(cx)?;
        Some(
            div()
                .id("gpui-tables-menu-note")
                .debug_selector(|| "menu-note".into())
                .role(Role::Status)
                .aria_label(SharedString::from(note.clone()))
                .px(t.px(8.))
                .py(t.px(4.))
                .overflow_hidden()
                .line_clamp(3)
                .text_size(t.px(t.sizes.small))
                .text_color(t.colors.text_muted)
                .child(note)
                .into_any_element(),
        )
    }

    /// A menu's field: gpui-core's Input in a box drawn as the table's other fields.
    pub(crate) fn field_box(&self, state: &Entity<InputState>, label: String) -> gpui::Div {
        let t = &self.theme;
        let field = FieldColors::of(t);
        div()
            .debug_selector(|| "menu-field".into())
            .mx(t.px(4.))
            .h(t.hit(30., self.rem))
            .px(t.px(8.))
            .flex()
            .items_center()
            .rounded(t.px(t.sizes.chip_radius))
            .bg(t.colors.background)
            .border_2()
            .border_color(t.colors.focus)
            .child(
                Input::new(state)
                    .appearance(false)
                    .accessibility_label(label)
                    .caret_color(field.caret)
                    .selection_color(field.selection)
                    .placeholder_color(field.placeholder)
                    .flex_1()
                    .min_w_0(),
            )
    }

    pub(crate) fn small_menu(&self, id: &'static str) -> PopupMenu {
        let t = &self.theme;
        PopupMenu::new(ElementId::NamedInteger(id.into(), self.menu_serial))
            .take_focus(false)
            .touch(t.touch)
            .row_height(t.px(28.))
            .text_size(t.px(t.sizes.text))
            .heading_size(t.px(t.sizes.small))
            .check_leading(true)
            .colors(self.menu_colors())
    }

    /// A name typed on a page of its own (a property's, an option's): what it's for, the
    /// field, a note, and Save and Back for a pointer or a finger (Enter saves, Escape closes).
    fn render_field_page(
        &self,
        page: &super::properties::Page,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        use super::properties::{Page, PropAction};
        let t = &self.theme;
        let Some(f) = &self.field else {
            return div().into_any_element();
        };
        let title = match page {
            Page::NewOption => "New option",
            Page::RenameOption(_) => "Rename option",
            _ => "Rename property",
        };
        let save = cx.listener(|this, pick: &MenuSelect, window, cx| {
            if pick.id.as_ref() == "Save" {
                this.commit_field(window, cx);
            } else {
                this.activate_property(PropAction::Back, window, cx);
            }
        });
        div()
            .flex()
            .flex_col()
            .gap(t.px(4.))
            .child(
                line(title)
                    .px(t.px(8.))
                    .pt(t.px(4.))
                    .text_size(t.px(t.sizes.small))
                    .text_color(t.colors.text_muted),
            )
            .child(self.field_box(&f.state, title.to_owned()))
            .children(self.render_note(cx))
            .child(
                self.small_menu("gpui-tables-field-menu")
                    .item(PopupMenuItem::new("Save", "Save").icon(IconName::Check))
                    .item(PopupMenuItem::new("Back", "Back").icon(IconName::ArrowLeft))
                    .on_select(save),
            )
            .into_any_element()
    }

    /// The + menu as gpui-core's Picker, naming rather than filtering: the new property's name
    /// typed in its field over the types it can have. Up and Down move through the types while
    /// typing stays in the field; Enter makes the highlighted one, a press any, named as typed.
    fn add_property_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let full = self
            .database
            .database
            .as_ref()
            .is_some_and(|d| d.properties.len() >= self.host.limits().properties);
        let types = PropertyType::ADDABLE.iter().map(|k| {
            PopupMenuItem::new(k.label().to_owned(), k.label().to_owned())
                .icon(type_icon(k))
                .disabled(full)
        });
        let pick = cx.listener(|this, pick: &MenuSelect, window, cx| {
            if let Some(k) = PropertyType::ADDABLE
                .iter()
                .find(|k| k.label() == pick.id.as_ref())
            {
                let name = pick.query.as_deref().unwrap_or_default();
                this.add_property(k.clone(), name, window, cx);
            }
        });
        self.naming_picker("gpui-tables-add-property", "Property name", types, pick)
    }

    /// A + menu's Picker: a name typed in its field (at most the limits' name length, 44 points
    /// tall on touch) over what to make (the types), which stay whole as it's typed. Back from a
    /// step of it, it opens on the entry the step came from, the name as it was typed.
    pub(crate) fn naming_picker(
        &self,
        id: &'static str,
        placeholder: &'static str,
        items: impl IntoIterator<Item = PopupMenuItem>,
        on_select: impl Fn(&MenuSelect, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let t = &self.theme;
        let field = FieldColors::of(t);
        let picker = Picker::new(ElementId::NamedInteger(id.into(), self.menu_serial))
            .filter(false)
            .placeholder(placeholder)
            .max_chars(self.host.limits().name_chars)
            .items(items)
            .touch(t.touch)
            .row_height(t.px(28.))
            .text_size(t.px(t.sizes.text))
            .check_leading(true)
            .caret_color(field.caret)
            .selection_color(field.selection)
            .placeholder_color(field.placeholder)
            .colors(self.menu_colors())
            .on_select(on_select);
        let Reopen { on, name, .. } = self.reopen.clone();
        let picker = match on {
            Some(on) => picker.highlight(on),
            None => picker,
        };
        let picker = match name {
            Some(name) => picker.query(name),
            None => picker,
        };
        div()
            .id(id)
            .debug_selector(|| "menu-picker".into())
            .child(picker)
            .into_any_element()
    }
}

/// Whether a property's cells read through the host: people's names and related rows' titles.
pub(crate) fn reads_host(kind: &PropertyType) -> bool {
    matches!(
        kind,
        PropertyType::Person
            | PropertyType::Relation
            | PropertyType::CreatedBy
            | PropertyType::EditedBy
    )
}

/// A cell's value as words, for a property whose cells don't read through the host
/// ([`reads_host`]; empty for one that does), its times `offset_ms` from UTC.
pub(crate) fn plain_cell_text(r: &Doc, p: &Property, offset_ms: i64) -> String {
    let v = r.value(&p.id);
    match p.kind {
        PropertyType::Title => r.title.clone(),
        PropertyType::Number => v
            .and_then(Value::as_number)
            .map(|x| format::number(x, &p.config))
            .unwrap_or_default(),
        PropertyType::Select => v
            .and_then(Value::option_id)
            .and_then(|id| p.option(id))
            .map(|o| o.name.clone())
            .unwrap_or_default(),
        PropertyType::MultiSelect => v
            .and_then(Value::option_ids)
            .unwrap_or(&[])
            .iter()
            .filter_map(|id| p.option(id).map(|o| o.name.clone()))
            .collect::<Vec<_>>()
            .join(", "),
        PropertyType::Date => v
            .and_then(Value::as_date)
            .map(|d| format::date(d, offset_ms))
            .unwrap_or_default(),
        PropertyType::Checkbox => if v.and_then(Value::as_bool).unwrap_or(false) {
            "checked"
        } else {
            "unchecked"
        }
        .into(),
        PropertyType::CreatedTime => format::instant(r.created, offset_ms),
        PropertyType::EditedTime => format::instant(r.updated, offset_ms),
        _ if reads_host(&p.kind) => String::new(),
        _ => p
            .shown_text(v.and_then(Value::as_str).unwrap_or_default())
            .to_owned(),
    }
}
