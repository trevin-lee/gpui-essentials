//! A table, as shadcn/ui draws one (its `table.tsx`): a header row, body rows, an optional
//! footer and an optional caption under it. It's a simple display table, not a data grid
//! (sorting, editing and resizing are gpui-tables').
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::{ColumnAlign, Table, TableColumn, TableRow};
//! # struct Invoices { rows: Vec<(SharedString, SharedString)>, selected: Option<usize> }
//! # impl Render for Invoices {
//! fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
//!     let rows = self.rows.clone();
//!     Table::new("invoices")
//!         .h(px(320.))
//!         .column(TableColumn::new("Invoice").fixed(px(100.)))
//!         .column(TableColumn::new("Amount").align(ColumnAlign::End))
//!         .rows(rows.len(), move |range, _, _| {
//!             range
//!                 .map(|i| TableRow::new().cell(rows[i].0.clone()).cell(rows[i].1.clone()))
//!                 .collect()
//!         })
//!         .selected(self.selected)
//!         .on_select(cx.listener(|this, row: &usize, _, cx| {
//!             this.selected = Some(*row);
//!             cx.notify();
//!         }))
//!         .caption("A list of your recent invoices.")
//! }
//! # }
//! ```
//!
//! **Many rows.** The body scrolls by itself and asks the host only for the rows in view, so a
//! table of thousands of rows costs what a screenful does. Give the table a height (or a
//! `max_h`, or a flex parent that bounds it) for that: without one it's as tall as all its
//! rows, and draws them all. Every body row is one line tall (cells cut their text short), so
//! the rows are measured once.
//!
//! **Columns.** Each is [`fixed`](TableColumn::fixed) (shrinking only if the table is narrower
//! than its fixed columns), a [`fraction`](TableColumn::fraction) of what fixed columns leave,
//! or [`Auto`](ColumnWidth::Auto): an equal share of it, the same as a fraction of 1. A table
//! that draws only the rows in view can't size a column to the widest of all its rows, so no
//! width depends on what's in the cells, and the columns line up in every row.
//!
//! **Selection.** The host says which row is selected ([`selected`](Table::selected)). With
//! [`on_select`](Table::on_select) a click or a tap on a row asks to select it, and the body is
//! a Tab stop where Up and Down, Home and End, and Page Up and Page Down ask for another row
//! and scroll it into view. Rows light up under the pointer.
//!
//! To assistive technology it's a table of rows and cells, with column headers, counts and
//! indices, named by its caption.

use std::ops::Range;
use std::rc::Rc;

use gpui::{
    AbsoluteLength, AnyElement, App, ElementId, FontWeight, InteractiveElement as _, IntoElement,
    KeyBinding, ListSizingBehavior, ParentElement as _, Refineable as _, Rems, RenderOnce, Role,
    ScrollStrategy, SharedString, StatefulInteractiveElement as _, StyleRefinement, Styled,
    UniformListScrollHandle, Window, actions, div, prelude::FluentBuilder as _, px, rems,
    uniform_list,
};

use crate::base::{ActiveTheme as _, Theme, focus_ring};

actions!(
    gpui_core_table,
    [
        /// The row above the selected one.
        SelectPrevious,
        /// The row below it.
        SelectNext,
        SelectFirst,
        SelectLast,
        /// A page of rows up.
        SelectPageUp,
        /// A page of rows down.
        SelectPageDown
    ]
);

const CONTEXT: &str = "GpuiCoreTable";

pub(crate) fn init(cx: &mut App) {
    let c = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("up", SelectPrevious, c),
        KeyBinding::new("down", SelectNext, c),
        KeyBinding::new("home", SelectFirst, c),
        KeyBinding::new("end", SelectLast, c),
        KeyBinding::new("pageup", SelectPageUp, c),
        KeyBinding::new("pagedown", SelectPageDown, c),
    ]);
}

/// A size in the web's CSS pixels, as rems of its 15 pixel text.
fn css(v: f32) -> Rems {
    rems(v / 15.)
}

/// How wide a column is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ColumnWidth {
    /// This wide, unless the table is narrower than its fixed columns (then they shrink
    /// together).
    Fixed(AbsoluteLength),
    /// This share of what the fixed columns leave, against the other fractions.
    Fraction(f32),
    /// An equal share of what the fixed columns leave: a fraction of 1.
    Auto,
}

/// Where a column's cells sit across it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ColumnAlign {
    #[default]
    Start,
    Center,
    /// At the end: numbers, amounts.
    End,
}

/// A column: its header's title, its width and its alignment.
#[derive(Clone, Debug)]
pub struct TableColumn {
    title: SharedString,
    width: ColumnWidth,
    align: ColumnAlign,
}

impl TableColumn {
    /// A column with this header, an [`Auto`](ColumnWidth::Auto) width and its cells at the
    /// start.
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            width: ColumnWidth::Auto,
            align: ColumnAlign::Start,
        }
    }

    pub fn width(mut self, width: ColumnWidth) -> Self {
        self.width = width;
        self
    }

    /// [`ColumnWidth::Fixed`]: `px(120.)` or `rems(8.)`.
    pub fn fixed(self, width: impl Into<AbsoluteLength>) -> Self {
        self.width(ColumnWidth::Fixed(width.into()))
    }

    /// [`ColumnWidth::Fraction`].
    pub fn fraction(self, share: f32) -> Self {
        self.width(ColumnWidth::Fraction(share))
    }

    pub fn align(mut self, align: ColumnAlign) -> Self {
        self.align = align;
        self
    }
}

/// A row's cells, one for each column in order: text or any element. Text is cut short with
/// an ellipsis; an element is clipped at its cell.
#[derive(Default)]
pub struct TableRow {
    cells: Vec<AnyElement>,
}

impl TableRow {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cell(mut self, cell: impl IntoElement) -> Self {
        self.cells.push(cell.into_any_element());
        self
    }

    pub fn cells<E: IntoElement>(mut self, cells: impl IntoIterator<Item = E>) -> Self {
        self.cells
            .extend(cells.into_iter().map(IntoElement::into_any_element));
        self
    }
}

type Rows = Rc<dyn Fn(Range<usize>, &mut Window, &mut App) -> Vec<TableRow>>;
type OnSelect = Rc<dyn Fn(&usize, &mut Window, &mut App)>;

/// A table. See the module's documentation for how it behaves.
#[derive(IntoElement)]
pub struct Table {
    id: ElementId,
    columns: Vec<TableColumn>,
    count: usize,
    rows: Option<Rows>,
    footer: Option<TableRow>,
    caption: Option<SharedString>,
    label: Option<SharedString>,
    selected: Option<usize>,
    on_select: Option<OnSelect>,
    style: StyleRefinement,
}

impl Table {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            columns: Vec::new(),
            count: 0,
            rows: None,
            footer: None,
            caption: None,
            label: None,
            selected: None,
            on_select: None,
            style: StyleRefinement::default(),
        }
    }

    pub fn column(mut self, column: TableColumn) -> Self {
        self.columns.push(column);
        self
    }

    pub fn columns(mut self, columns: impl IntoIterator<Item = TableColumn>) -> Self {
        self.columns.extend(columns);
        self
    }

    /// `count` body rows, made by `rows` when they come into view: it's given the indices to
    /// make and returns their rows, in order.
    pub fn rows(
        mut self,
        count: usize,
        rows: impl Fn(Range<usize>, &mut Window, &mut App) -> Vec<TableRow> + 'static,
    ) -> Self {
        self.count = count;
        self.rows = Some(Rc::new(rows));
        self
    }

    /// A row under the body, on a quieter fill, in the stronger weight: totals.
    pub fn footer(mut self, footer: TableRow) -> Self {
        self.footer = Some(footer);
        self
    }

    /// Quiet text under the table saying what it is; it names the table to assistive
    /// technology too, unless [`accessibility_label`](Self::accessibility_label) does.
    pub fn caption(mut self, caption: impl Into<SharedString>) -> Self {
        self.caption = Some(caption.into());
        self
    }

    /// What the table is, to assistive technology.
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The selected body row, drawn on the muted fill.
    pub fn selected(mut self, row: Option<usize>) -> Self {
        self.selected = row;
        self
    }

    /// Told the row a click, a tap or a key asks to select. The host makes it the
    /// [`selected`](Self::selected) one.
    pub fn on_select(mut self, f: impl Fn(&usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }
}

impl Styled for Table {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// A cell's place in its row: its width and alignment, and its padding.
fn cell(column: &TableColumn, theme: &Theme) -> gpui::Div {
    let el = div()
        .flex()
        .items_center()
        .min_w_0()
        .overflow_hidden()
        .text_size(theme.text.sm)
        .whitespace_nowrap();
    let el = match column.width {
        ColumnWidth::Fixed(w) => el
            .flex_grow_0()
            .flex_shrink_1()
            .flex_basis(gpui::DefiniteLength::from(w)),
        ColumnWidth::Fraction(f) => el.flex_basis(px(0.)).flex_grow(f.max(0.)),
        ColumnWidth::Auto => el.flex_basis(px(0.)).flex_grow(1.),
    };
    match column.align {
        ColumnAlign::Start => el.justify_start(),
        ColumnAlign::Center => el.justify_center(),
        ColumnAlign::End => el.justify_end(),
    }
}

/// A cell's content, padded and cut short with an ellipsis. The padding is the content's, not
/// the cell's, so the cells' widths are exactly their columns' shares.
fn content(el: impl IntoElement) -> gpui::Div {
    div()
        .min_w_0()
        .overflow_hidden()
        .px(css(8.))
        .text_ellipsis()
        .child(el)
}

/// A row as assistive technology gets it: its role and index (counting the header as the
/// first), and whether it's selected when rows can be.
fn row_node(
    id: impl Into<ElementId>,
    index: usize,
    selected: Option<bool>,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .role(Role::Row)
        .aria_row_index(index)
        .when_some(selected, |d, s| d.aria_selected(s))
}

/// A cell as assistive technology gets it.
fn cell_node(el: gpui::Div, row: usize, column: usize, header: bool) -> gpui::Stateful<gpui::Div> {
    el.id(("cell", column))
        .role(if header {
            Role::ColumnHeader
        } else {
            Role::Cell
        })
        .aria_row_index(row)
        .aria_column_index(column + 1)
}

/// The table's own node: a table with its counts and its name.
fn table_node(
    id: ElementId,
    rows: usize,
    columns: usize,
    label: Option<SharedString>,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .role(Role::Table)
        .aria_row_count(rows)
        .aria_column_count(columns)
        .when_some(label, |d, l| d.aria_label(l))
}

/// The row a key asks for, from the selected one (or none), in a table of `count` rows with
/// `page` rows in view.
fn target(key: Key, from: Option<usize>, count: usize, page: usize) -> Option<usize> {
    let last = count.checked_sub(1)?;
    Some(match (key, from) {
        (Key::First, _) => 0,
        (Key::Last, _) => last,
        (Key::Previous | Key::PageUp, None) => last,
        (Key::Next | Key::PageDown, None) => 0,
        (Key::Previous, Some(i)) => i.saturating_sub(1).min(last),
        (Key::Next, Some(i)) => (i + 1).min(last),
        (Key::PageUp, Some(i)) => i.saturating_sub(page.max(1)).min(last),
        (Key::PageDown, Some(i)) => (i + page.max(1)).min(last),
    })
}

#[derive(Clone, Copy)]
enum Key {
    Previous,
    Next,
    First,
    Last,
    PageUp,
    PageDown,
}

impl RenderOnce for Table {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::base::init_soon(cx);
        let theme = cx.theme().clone();
        let columns: Rc<Vec<TableColumn>> = Rc::new(self.columns);
        let count = self.count;
        let selected = self.selected;
        let on_select = self.on_select.clone();
        let selectable = on_select.is_some();
        let (focus, scroll) = window
            .use_keyed_state(
                ElementId::NamedChild(std::sync::Arc::new(self.id.clone()), "state".into()),
                cx,
                |_, cx| (cx.focus_handle(), UniformListScrollHandle::new()),
            )
            .read(cx)
            .clone();
        let has_footer = self.footer.is_some();
        let row_count = count + 1 + usize::from(has_footer);

        let header = row_node("table-header", 1, None)
            .debug_selector(|| "table-header".into())
            .flex()
            .flex_none()
            .w_full()
            .h(css(40.))
            .border_b_1()
            .border_color(theme.border)
            .font_weight(FontWeight::MEDIUM)
            .text_color(theme.foreground)
            .children(columns.iter().enumerate().map(|(j, c)| {
                cell_node(cell(c, &theme).h_full(), 1, j, true)
                    .debug_selector(move || format!("table-head-{j}"))
                    .child(content(c.title.clone()))
            }));

        let body = self.rows.map(|rows| {
            let columns = columns.clone();
            let on_select = on_select.clone();
            let theme = theme.clone();
            uniform_list(
                ElementId::NamedChild(std::sync::Arc::new(self.id.clone()), "body".into()),
                count,
                move |range: Range<usize>, window, cx| {
                    let start = range.start;
                    let made = rows(range, window, cx);
                    made.into_iter()
                        .enumerate()
                        .map(|(k, row)| {
                            let i = start + k;
                            let is_selected = selected == Some(i);
                            let on_select = on_select.clone();
                            row_node(("row", i), i + 2, selectable.then_some(is_selected))
                                .debug_selector(move || format!("table-row-{i}"))
                                .flex()
                                .w_full()
                                // Every row the same height (the list measures one), the
                                // last one's rule unseen.
                                .border_b_1()
                                .border_color(if i + 1 < count {
                                    theme.border
                                } else {
                                    gpui::transparent_black()
                                })
                                .when(is_selected, |d| d.bg(theme.muted))
                                .when(!is_selected, |d| {
                                    d.hover(|s| s.bg(theme.muted.opacity(0.5)))
                                })
                                .when_some(on_select, |d, f| {
                                    d.cursor_pointer()
                                        .on_click(move |_, window, cx| f(&i, window, cx))
                                })
                                .children(
                                    row.cells.into_iter().zip(columns.iter()).enumerate().map(
                                        |(j, (el, c))| {
                                            cell_node(cell(c, &theme).py(css(8.)), i + 2, j, false)
                                                .debug_selector(move || {
                                                    format!("table-cell-{i}-{j}")
                                                })
                                                .child(content(el))
                                        },
                                    ),
                                )
                        })
                        .collect::<Vec<_>>()
                },
            )
            .with_sizing_behavior(ListSizingBehavior::Infer)
            .track_scroll(&scroll)
            .w_full()
            .min_h_0()
            .flex_shrink_1()
        });

        // The body's keys: each asks for a row and scrolls it into view.
        let key = |k: Key| {
            let (on_select, scroll) = (on_select.clone(), scroll.clone());
            move |window: &mut Window, cx: &mut App| {
                let Some(f) = &on_select else { return };
                // A page: the rows that fit in the body, by its height and its rows' (all
                // rows are as tall as the first).
                let page = scroll
                    .0
                    .borrow()
                    .last_item_size
                    .map(|s| {
                        let row = s.contents.height / count.max(1) as f32;
                        if row > px(0.) {
                            (s.item.height / row).floor() as usize
                        } else {
                            1
                        }
                    })
                    .unwrap_or(1);
                if let Some(i) = target(k, selected, count, page.saturating_sub(1)) {
                    scroll.scroll_to_item(i, ScrollStrategy::Nearest);
                    f(&i, window, cx);
                }
            }
        };
        let (previous, next, first, last, page_up, page_down) = (
            key(Key::Previous),
            key(Key::Next),
            key(Key::First),
            key(Key::Last),
            key(Key::PageUp),
            key(Key::PageDown),
        );
        let body = body.map(|list| {
            let ring = focus_ring(cx);
            div()
                .id("table-body")
                .debug_selector(|| "table-body".into())
                .flex()
                .flex_col()
                .w_full()
                .min_h_0()
                .flex_shrink_1()
                .when(selectable, |d| {
                    d.track_focus(&focus.clone().tab_stop(true))
                        .key_context(CONTEXT)
                        .focus_visible(move |s| s.shadow(vec![crate::base::ring_inside(ring)]))
                        .on_action(move |_: &SelectPrevious, w, cx| previous(w, cx))
                        .on_action(move |_: &SelectNext, w, cx| next(w, cx))
                        .on_action(move |_: &SelectFirst, w, cx| first(w, cx))
                        .on_action(move |_: &SelectLast, w, cx| last(w, cx))
                        .on_action(move |_: &SelectPageUp, w, cx| page_up(w, cx))
                        .on_action(move |_: &SelectPageDown, w, cx| page_down(w, cx))
                })
                .child(list)
        });

        let footer = self.footer.map(|row| {
            row_node("table-footer", row_count, None)
                .debug_selector(|| "table-footer".into())
                .flex()
                .flex_none()
                .w_full()
                .border_t_1()
                .border_color(theme.border)
                .bg(theme.muted.opacity(0.5))
                .font_weight(FontWeight::MEDIUM)
                .children(row.cells.into_iter().zip(columns.iter()).enumerate().map(
                    |(j, (el, c))| {
                        cell_node(cell(c, &theme).py(css(8.)), row_count, j, false)
                            .child(content(el))
                    },
                ))
        });

        let caption = self.caption.clone().map(|c| {
            div()
                .debug_selector(|| "table-caption".into())
                .flex_none()
                .mt(css(16.))
                .min_w_0()
                .text_size(theme.text.sm)
                .text_color(theme.muted_foreground)
                .child(c)
        });

        let mut el = table_node(
            self.id,
            row_count,
            columns.len(),
            self.label.or(self.caption),
        )
        .debug_selector(|| "table".into())
        .flex()
        .flex_col()
        .w_full()
        .min_w_0()
        .text_size(theme.text.sm)
        .child(header)
        .children(body)
        .children(footer)
        .children(caption);
        el.style().refine(&self.style);
        el
    }
}

#[cfg(test)]
mod tests {
    use gpui::{TestAppContext, accesskit};

    use super::*;

    fn node(el: &impl gpui::Element) -> accesskit::Node {
        let mut node = accesskit::Node::new(el.a11y_role().expect("a role"));
        el.write_a11y_info(&mut node);
        node
    }

    #[test]
    fn the_keys_ask_for_rows_within_the_table() {
        assert_eq!(target(Key::Next, None, 10, 4), Some(0));
        assert_eq!(target(Key::Previous, None, 10, 4), Some(9));
        assert_eq!(target(Key::Next, Some(9), 10, 4), Some(9));
        assert_eq!(target(Key::Previous, Some(0), 10, 4), Some(0));
        assert_eq!(target(Key::PageDown, Some(3), 10, 4), Some(7));
        assert_eq!(target(Key::PageDown, Some(8), 10, 4), Some(9));
        assert_eq!(target(Key::PageUp, Some(3), 10, 4), Some(0));
        assert_eq!(target(Key::Last, Some(3), 10, 4), Some(9));
        assert_eq!(target(Key::First, None, 10, 4), Some(0));
        assert_eq!(target(Key::Next, Some(3), 0, 4), None, "no rows");
    }

    #[gpui::test]
    fn rows_and_cells_say_what_they_are(cx: &mut TestAppContext) {
        let vcx = cx.add_empty_window();
        vcx.update(|_, _| {
            let t = node(&table_node("t".into(), 12, 3, Some("Invoices".into())).into_element());
            assert_eq!(t.role(), Role::Table);
            assert_eq!(t.row_count(), Some(12));
            assert_eq!(t.column_count(), Some(3));
            assert_eq!(t.label(), Some("Invoices"));

            let r = node(&row_node("r", 4, Some(true)).into_element());
            assert_eq!(r.role(), Role::Row);
            assert_eq!(r.row_index(), Some(4));
            assert_eq!(r.is_selected(), Some(true));
            assert_eq!(
                node(&row_node("r", 4, None).into_element()).is_selected(),
                None
            );

            let h = node(&cell_node(div(), 1, 0, true).into_element());
            assert_eq!(h.role(), Role::ColumnHeader);
            assert_eq!(h.column_index(), Some(1));
            let c = node(&cell_node(div(), 4, 2, false).into_element());
            assert_eq!(c.role(), Role::Cell);
            assert_eq!((c.row_index(), c.column_index()), (Some(4), Some(3)));
        });
    }
}
