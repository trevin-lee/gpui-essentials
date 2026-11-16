//! Tables: a bordered box of one-line cells that scrolls sideways when it's wider than the
//! card, every other row tinted; and the editable table, whose cells become inputs.

use std::rc::Rc;

use gpui_core::Disableable as _;
use gpui_kit::component::input::Input as KitInput;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::{Draw, pressable};
use crate::openui::{Cell, ColumnKind, Component, EditableTable, Table};

/// A column's width in CSS pixels, from the longest text it holds: each column is as wide as
/// its widest cell up to a cap (past which the cell ends in an ellipsis), and the room left
/// over is shared out.
fn width_for(chars: usize, font: f32, pad: f32) -> f32 {
    (chars as f32 * font * 0.56 + pad * 2.).clamp(64., 320.)
}

fn chars(s: &str) -> usize {
    s.chars().count()
}

/// How wide a cell's components ask to be, roughly (a tag, a button).
fn component_chars(c: &Component) -> usize {
    match c {
        Component::Tag(t) => chars(&t.text) + 4,
        Component::Button(b) => chars(&b.label) + 4,
        Component::Plain(s) => chars(s),
        _ => 12,
    }
}

impl Draw<'_, '_> {
    pub fn table(&mut self, table: &Table, path: &str) -> AnyElement {
        let t = self.t;
        let font = t.text.sm;
        let pad = t.space.cell;
        let limit = self.v.limits.table_rows;
        let rows = &table.rows[..table.rows.len().min(limit)];
        let widths: Vec<f32> = table
            .columns
            .iter()
            .enumerate()
            .map(|(c, col)| {
                let longest = rows
                    .iter()
                    .map(|r| match r.get(c) {
                        Some(Cell::Text(s)) => chars(s),
                        Some(Cell::Components(v)) => v.iter().map(component_chars).sum(),
                        _ => 0,
                    })
                    .chain(std::iter::once(chars(&col.label)))
                    .max()
                    .unwrap_or(0);
                width_for(longest, font, pad)
            })
            .collect();
        let total: f32 = widths.iter().sum();
        let cell = |d: &Self, c: usize| {
            let right = table
                .columns
                .get(c)
                .is_some_and(|col| col.kind == ColumnKind::Number);
            div()
                .flex_basis(d.px(widths[c]))
                .flex_grow(1.)
                .flex_shrink_0()
                .min_w(d.px(widths[c]))
                .px(d.px(pad))
                .py(d.px(pad))
                .flex()
                .flex_row()
                .items_center()
                .when(right, |x| x.justify_end())
        };
        let mut head = div()
            .w_full()
            .min_w(self.px(total))
            .flex()
            .flex_row()
            .when(!rows.is_empty(), |d| {
                d.border_b_1().border_color(t.colors.line)
            });
        for (c, col) in table.columns.iter().enumerate() {
            head = head.child(cell(self, c).child(self.body_line(
                col.label.clone(),
                font,
                t.colors.text_muted,
                self.regular(),
            )));
        }
        // As wide as its columns: what a scroller can scroll is what its children take.
        let mut body = div()
            .w_full()
            .min_w(self.px(total))
            .flex()
            .flex_col()
            .child(head);
        for (r, row) in rows.iter().enumerate() {
            let mut line = div()
                .w_full()
                .min_w(self.px(total))
                .flex()
                .flex_row()
                .when(r % 2 == 1, |d| d.bg(t.colors.highlight))
                .when(r + 1 < rows.len(), |d| {
                    d.border_b_1().border_color(t.colors.line)
                });
            for c in 0..table.columns.len() {
                let content =
                    match row.get(c) {
                        Some(Cell::Text(s)) => self
                            .line(s.clone(), font, t.colors.text, self.regular())
                            .into_any_element(),
                        Some(Cell::Components(v)) => {
                            let mut inner = div()
                                .min_w_0()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap(self.px(6.));
                            for (k, comp) in v.iter().enumerate() {
                                inner =
                                    inner.child(div().min_w_0().flex_shrink(1.).child(
                                        self.component(comp, &format!("{path}.t{r}.{c}.{k}")),
                                    ));
                            }
                            inner.into_any_element()
                        }
                        _ => div().into_any_element(),
                    };
                line = line.child(cell(self, c).child(content));
            }
            body = body.child(line);
        }
        let (scroller, handle) = self.scroll_x(&format!("{path}.scroll"), &format!("{path}.table"));
        let scroller = scroller.child(body);
        let boxed = div()
            .w_full()
            .min_w_0()
            .rounded(self.px(t.radii.xl))
            .border_1()
            .border_color(t.colors.line)
            .overflow_hidden()
            .child(scroller);
        let boxed = self.keep_sideways(boxed, handle);
        let more = table.rows.len().saturating_sub(rows.len());
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(self.px(6.))
            .child(boxed)
            .when(more > 0, |d| {
                d.child(self.text(
                    format!("Showing {} of {} rows.", rows.len(), table.rows.len()),
                    t.text.sm,
                    t.colors.text_muted,
                    self.regular(),
                ))
            })
            .into_any_element()
    }

    /// Cells that become inputs when pressed (a select column's become choices), and Save
    /// Changes to send them.
    pub fn editable_table(&mut self, table: &EditableTable, path: &str) -> AnyElement {
        let t = self.t;
        let font = t.text.sm;
        let pad = t.space.cell;
        let Some(state) = self.v.edits.get(path) else {
            return div().into_any_element();
        };
        let limit = self.v.limits.editable_rows;
        let rows = &state.rows[..state.rows.len().min(limit)];
        let widths: Vec<f32> = table
            .columns
            .iter()
            .enumerate()
            .map(|(c, col)| {
                // A select column shows its options' labels with a chevron beside them.
                let options = col
                    .options
                    .iter()
                    .map(|o| chars(&o.label) + 6)
                    .max()
                    .unwrap_or(0);
                let longest = rows
                    .iter()
                    .map(|r| r.get(c).map_or(0, |s| chars(s)))
                    .chain([chars(&col.header), options])
                    .max()
                    .unwrap_or(0);
                col.width.filter(|w| w.is_finite() && *w > 0.).map_or_else(
                    || width_for(longest, font, pad),
                    |w| (w as f32).clamp(48., 480.),
                )
            })
            .collect();
        let total: f32 = widths.iter().sum();
        let cell = |d: &Self, c: usize| {
            div()
                .flex_basis(d.px(widths[c]))
                .flex_grow(1.)
                .flex_shrink_0()
                .min_w(d.px(widths[c]))
                .flex()
                .flex_row()
                .items_center()
        };
        let mut head = div()
            .w_full()
            .min_w(self.px(total))
            .flex()
            .flex_row()
            .border_b_1()
            .border_color(t.colors.line);
        for (c, col) in table.columns.iter().enumerate() {
            let label = if col.header.is_empty() {
                col.key.clone()
            } else {
                col.header.clone()
            };
            head = head.child(
                cell(self, c)
                    .px(self.px(pad))
                    .py(self.px(pad))
                    .child(self.body_line(label, font, t.colors.text_muted, self.regular())),
            );
        }
        // As wide as its columns: what a scroller can scroll is what its children take.
        let mut body = div()
            .w_full()
            .min_w(self.px(total))
            .flex()
            .flex_col()
            .child(head);
        let editing = state
            .editing
            .as_ref()
            .map(|(r, c, s, _)| (*r, *c, s.clone()));
        for (r, row) in rows.iter().enumerate() {
            let mut line = div()
                .w_full()
                .min_w(self.px(total))
                .flex()
                .flex_row()
                .when(r + 1 < rows.len(), |d| {
                    d.border_b_1().border_color(t.colors.line)
                });
            for (c, col) in table.columns.iter().enumerate() {
                let text = row.get(c).cloned().unwrap_or_default();
                let key = path.to_string();
                let content = match (&editing, col.kind.as_str()) {
                    (Some((er, ec, s)), _) if *er == r && *ec == c => self
                        .sideways(
                            div()
                                .id(self.id(&format!("{path}.e{r}.{c}")))
                                .w_full()
                                .min_w_0()
                                .px(self.px(4.)),
                            &format!("{path}.e{r}.{c}"),
                        )
                        .child(KitInput::new(s).w_full())
                        .into_any_element(),
                    (_, "select") => {
                        let opts: Vec<(String, String)> = col
                            .options
                            .iter()
                            .map(|o| (o.value.clone(), o.label.clone()))
                            .collect();
                        let pick: super::form::Pick =
                            Rc::new(move |this, v, _, cx| this.set_cell(&key, r, c, v, cx));
                        div()
                            .w_full()
                            .min_w_0()
                            .px(self.px(4.))
                            .child(self.dropdown(
                                &format!("{path}.e{r}.{c}"),
                                &opts,
                                Some(&text),
                                "Choose",
                                pick,
                            ))
                            .into_any_element()
                    }
                    _ => {
                        let shown = if text.is_empty() {
                            "—".to_string()
                        } else {
                            text.clone()
                        };
                        let edit = self.cx.listener(move |this, _, window, cx| {
                            this.edit_cell(&key, r, c, window, cx)
                        });
                        let press = pressable(self.id(&format!("{path}.c{r}.{c}")))
                            .accessibility_label(format!("{}: {text}", col.header))
                            .w_full()
                            .min_w_0()
                            .justify_start()
                            .px(self.px(pad))
                            .py(self.px(pad))
                            .hover(|s| s.bg(t.colors.highlight))
                            .debug_selector({
                                let sel = self.selector(&format!("cell:{r}:{c}"));
                                move || sel
                            })
                            .child(self.body_line(
                                shown,
                                font,
                                if text.is_empty() {
                                    t.colors.text_faint
                                } else {
                                    t.colors.text
                                },
                                self.regular(),
                            ));
                        self.on_press(press, true, edit).into_any_element()
                    }
                };
                line = line.child(cell(self, c).child(content));
            }
            body = body.child(line);
        }
        let (scroller, handle) = self.scroll_x(&format!("{path}.scroll"), &format!("{path}.table"));
        let scroller = scroller.child(body);
        let boxed = div()
            .w_full()
            .min_w_0()
            .rounded(self.px(t.radii.xl))
            .border_1()
            .border_color(t.colors.line)
            .overflow_hidden()
            .child(scroller);
        let boxed = self.keep_sideways(boxed, handle);
        let dirty = state.dirty;
        let (key, tbl) = (path.to_string(), table.clone());
        let save_it = self
            .cx
            .listener(move |this, _, window, cx| this.save_table(&key, &tbl, window, cx));
        let save = pressable(self.id(&format!("{path}.save")))
            .accessibility_label("Save Changes")
            .disabled(!dirty)
            .flex_none()
            .py(self.px(7.))
            .px(self.px(10.))
            .rounded(self.px(t.radii.md))
            .bg(if dirty {
                t.colors.accent
            } else {
                t.colors.sunk
            })
            .border_1()
            .border_color(if dirty {
                t.colors.accent
            } else {
                t.colors.line
            })
            .debug_selector({
                let sel = self.selector("button:Save Changes");
                move || sel
            })
            .child(self.line(
                "Save Changes",
                t.text.md,
                if dirty {
                    t.colors.on_accent
                } else {
                    t.colors.text_faint
                },
                self.regular(),
            ));
        let save = self.on_press(save, dirty, save_it);
        let more = state.rows.len().saturating_sub(rows.len());
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(self.px(t.space.s))
            .child(boxed)
            .when(more > 0, |d| {
                d.child(self.text(
                    format!("Showing {} of {} rows.", rows.len(), state.rows.len()),
                    t.text.sm,
                    t.colors.text_muted,
                    self.regular(),
                ))
            })
            .child(div().w_full().flex().flex_row().justify_end().child(save))
            .into_any_element()
    }
}
