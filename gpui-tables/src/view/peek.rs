//! A row's peek (Notion's side peek): with [`TableView::set_peek`] on, opening a row (its Open
//! button, a card, a list's line, Enter) shows it in a panel at the view's right instead of
//! asking the host to open its page. The panel has the row's title and every property with its
//! value, buttons to go to the row above or below (in the view's order), Open as page (the
//! host's `OpenRow`, as opening did before), and Close (Escape too). Pressing the title or a
//! value edits it with its cell's own editor (a field, the options, people or rows, the month,
//! or a checkbox's toggle), hung from the panel; a property the view hides as a column is shown
//! and not edited.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, InteractiveElement, IntoElement, ParentElement, Pixels,
    SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};
use gpui_core::{IconName, Press};

use super::render::{icon, line};
use super::table::{Item, TableView, type_icon};
use crate::model::{Doc, PropertyType, TableEvent};

impl TableView {
    /// Whether opening a row peeks it ([`peek`](Self::peek)) rather than asking the host to open
    /// its page. Off by default.
    pub fn set_peek(&mut self, on: bool, cx: &mut Context<Self>) {
        self.peek_mode = on;
        if !on {
            self.peek = None;
        }
        cx.notify();
    }

    /// Whether opening a row peeks it.
    pub fn peeks(&self) -> bool {
        self.peek_mode
    }

    /// Shows row `row_id` in the peek panel (whether or not opening peeks).
    pub fn peek(&mut self, row_id: &str, cx: &mut Context<Self>) {
        if self.rows.iter().any(|r| r.id == row_id) {
            self.peek = Some(row_id.to_owned());
            cx.notify();
        }
    }

    /// Closes the peek panel.
    pub fn close_peek(&mut self, cx: &mut Context<Self>) {
        if self.peek.take().is_some() {
            cx.notify();
        }
    }

    /// The row the peek panel shows, if it's open.
    pub fn peeked_row(&self) -> Option<&str> {
        self.peek.as_deref()
    }

    /// The rows in the view's order, by id: what the peek's up and down go through.
    fn shown_row_ids(&self) -> Vec<String> {
        self.shown
            .items
            .iter()
            .filter_map(|item| match *item {
                Item::Row { row, .. } => self.shown_row(row).map(|r| r.id.clone()),
                _ => None,
            })
            .collect()
    }

    /// The row before (`by` = -1) or after (1) the peeked one, in the view's order.
    pub(crate) fn peek_step(&mut self, by: isize, cx: &mut Context<Self>) {
        let Some(id) = self.peek.clone() else { return };
        let ids = self.shown_row_ids();
        let Some(at) = ids.iter().position(|x| *x == id) else {
            return;
        };
        let to = at as isize + by;
        if to < 0 || to as usize >= ids.len() {
            return;
        }
        self.peek = Some(ids[to as usize].clone());
        cx.notify();
    }

    /// The peek panel, when a row is peeked and still there.
    pub(crate) fn render_peek(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let id = self.peek.clone()?;
        let Some(r) = self.rows.iter().find(|r| r.id == id).cloned() else {
            // The row went (deleted, or the host's docs changed): the panel goes with it.
            self.peek = None;
            return None;
        };
        let fields = self.database.database.clone()?;
        let t = self.theme.clone();
        let ids = self.shown_row_ids();
        let at = ids.iter().position(|x| *x == id);
        let has_prev = at.is_some_and(|i| i > 0);
        let has_next = at.is_some_and(|i| i + 1 < ids.len());
        let button = |sel: &'static str, label: &'static str, name: IconName, on: bool| {
            Press::new(sel)
                .accessibility_label(label)
                .debug_selector(move || sel.into())
                .focus_on_press(false)
                .flex_none()
                .size(t.hit(28., self.rem))
                .flex()
                .items_center()
                .justify_center()
                .rounded(t.px(t.sizes.chip_radius))
                .when(on, |p| p.hover(|s| s.bg(t.colors.hover)))
                .when(!on, |p| p.opacity(0.4))
                .child(icon(name, t.px(t.sizes.icon), t.colors.text_muted))
        };
        let row_id = id.clone();
        let header = div()
            .flex()
            .items_center()
            .gap(t.px(2.))
            .px(t.px(8.))
            .py(t.px(6.))
            .border_b_1()
            .border_color(t.colors.line)
            .child(
                button("peek-close", "Close", IconName::X, true)
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.close_peek(cx))),
            )
            .child(
                button("peek-up", "The row above", IconName::ChevronUp, has_prev).when(
                    has_prev,
                    |b| {
                        b.on_click(
                            cx.listener(|this, _: &ClickEvent, _, cx| this.peek_step(-1, cx)),
                        )
                    },
                ),
            )
            .child(
                button(
                    "peek-down",
                    "The row below",
                    IconName::ChevronDown,
                    has_next,
                )
                .when(has_next, |b| {
                    b.on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.peek_step(1, cx)))
                }),
            )
            .child(div().flex_1())
            .child(
                button("peek-open", "Open as page", IconName::Maximize2, true).on_click(
                    cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.open_peeked(&row_id, window, cx)
                    }),
                ),
            );
        let title = if r.title.is_empty() {
            "Untitled".to_owned()
        } else {
            r.title.clone()
        };
        let name_width = t.px(132.);
        let shown: Vec<crate::model::Property> = fields
            .properties_in_order()
            .into_iter()
            .filter(|p| p.kind != PropertyType::Title)
            .cloned()
            .collect();
        let properties: Vec<_> = shown
            .iter()
            .map(|p| {
                let text = self.peek_value(&r, p);
                let empty = text.is_empty();
                let editable = self.peek_cell(&id, Some(&p.id)).is_some();
                let sel = format!("peek-value:{}", p.id);
                div()
                    .flex()
                    .items_start()
                    .gap(t.px(8.))
                    .min_h(t.px(28.))
                    .py(t.px(4.))
                    .child(
                        div()
                            .flex_none()
                            .w(name_width)
                            .flex()
                            .items_center()
                            .gap(t.px(6.))
                            .text_color(t.colors.text_muted)
                            .child(icon(
                                type_icon(&p.kind),
                                t.px(t.sizes.icon),
                                t.colors.text_muted,
                            ))
                            .child(line(p.name.clone())),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("peek-press:{}", p.id)))
                            .debug_selector(move || sel)
                            .flex_1()
                            .min_w_0()
                            .relative()
                            .rounded(t.px(t.sizes.chip_radius))
                            .when(editable, |d| {
                                d.cursor_pointer()
                                    .hover(|s| s.bg(t.colors.hover))
                                    .child(self.peek_place(&p.id, cx))
                                    .on_click(cx.listener({
                                        let (id, pid) = (id.clone(), p.id.clone());
                                        move |this, _: &ClickEvent, window, cx| {
                                            this.edit_peeked(&id, Some(&pid), window, cx)
                                        }
                                    }))
                            })
                            .when(empty, |d| d.text_color(t.colors.text_muted))
                            .child(SharedString::from(if empty {
                                "Empty".to_owned()
                            } else {
                                text
                            })),
                    )
            })
            .collect();
        // An edit from the panel is over once its editor is: its anchor goes.
        if self.popover.is_none() && self.editor.is_none() {
            self.peek_anchor = None;
        }
        Some(
            div()
                .id("gpui-tables-peek")
                .debug_selector(|| "peek".into())
                .absolute()
                .top_0()
                .right_0()
                .h_full()
                .w(self.peek_width())
                .flex()
                .flex_col()
                .bg(t.colors.background)
                .border_l_1()
                .border_color(t.colors.line_strong)
                .shadow_lg()
                .occlude()
                .child(header)
                .child(
                    div()
                        .id("gpui-tables-peek-body")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .px(t.px(16.))
                        .py(t.px(12.))
                        .flex()
                        .flex_col()
                        .gap(t.px(4.))
                        .child(
                            div()
                                .id("peek-title-press")
                                .cursor_pointer()
                                .child(self.peek_place("title", cx))
                                .on_click(cx.listener({
                                    let id = id.clone();
                                    move |this, _: &ClickEvent, window, cx| {
                                        this.edit_peeked(&id, None, window, cx)
                                    }
                                }))
                                .debug_selector(|| "peek-title".into())
                                .pb(t.px(8.))
                                .text_size(t.px(t.sizes.text * 1.6))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(t.colors.text)
                                .child(SharedString::from(title)),
                        )
                        .children(properties),
                )
                .into_any_element(),
        )
    }

    /// A probe recording where a value (or the title) was drawn, for its editor to hang from.
    fn peek_place(&self, key: &str, _: &mut Context<Self>) -> impl IntoElement {
        let places = self.peek_places.clone();
        let key = key.to_owned();
        gpui::canvas(
            move |b, _, _| {
                places.borrow_mut().insert(key.clone(), b);
            },
            |_, _, _, _| {},
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
    }

    /// The cell of row `row_id`'s property `pid` (its title when None) in the view as shown, if
    /// it has one: a property the view hides as a column has none.
    fn peek_cell(&self, row_id: &str, pid: Option<&str>) -> Option<super::table::CellRef> {
        let fields = self.database.database.as_ref()?;
        let p = match pid {
            Some(pid) => fields.properties.iter().position(|p| p.id == pid)?,
            None => fields
                .properties
                .iter()
                .position(|p| p.kind == PropertyType::Title)?,
        };
        let item = self.shown.items.iter().position(|i| {
            matches!(*i, Item::Row { row, .. } if self.rows.get(row).is_some_and(|r| r.id == row_id))
        })?;
        let column = self.shown.columns.iter().position(|c| c.property == p)?;
        Some(super::table::CellRef { item, column })
    }

    /// Edits the peeked row's property `pid` (its title when None) with its cell's editor, hung
    /// from where the panel draws it.
    pub(crate) fn edit_peeked(
        &mut self,
        row_id: &str,
        pid: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ensure();
        let Some(cell) = self.peek_cell(row_id, pid) else {
            return;
        };
        let key = pid.unwrap_or("title");
        self.peek_anchor = self.peek_places.borrow().get(key).copied();
        self.edit_cell(cell, window, cx);
    }

    /// The panel's width: two fifths of the view, between 320 and 480 points (all of it when
    /// narrower than that).
    fn peek_width(&self) -> Pixels {
        let whole = self.bounds.get().size.width;
        let (min, max) = (
            self.theme.px(320.).to_pixels(self.rem),
            self.theme.px(480.).to_pixels(self.rem),
        );
        if whole <= min {
            return whole.max(px(0.));
        }
        (whole * 0.4).clamp(min, max)
    }

    /// A property's value in the peek, as its cell shows it in words (a checkbox says so).
    fn peek_value(&self, r: &Doc, p: &crate::model::Property) -> String {
        let text = self.cell_text(r, p);
        match p.kind {
            PropertyType::Checkbox => {
                if text == "checked" {
                    "Checked".into()
                } else {
                    "Unchecked".into()
                }
            }
            _ => text,
        }
    }

    /// Open as page: the host opens the peeked row's page, and the peek closes.
    fn open_peeked(&mut self, row_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.peek = None;
        self.emit(
            TableEvent::OpenRow {
                row_id: row_id.to_owned(),
            },
            window,
            cx,
        );
        cx.notify();
    }
}
