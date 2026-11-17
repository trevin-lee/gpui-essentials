//! The table's calculation footer: a line under the table's rows (and under each open group's)
//! with a cell per column. A cell without a calculation is blank until a pointer is over it (or
//! it's focused, or always on touch), when it says "Calculate"; pressing it opens a menu of the
//! calculations that fit its column's type. A cell with one shows its result at the right, after
//! a short muted label ("Sum 1,240"), formatted as its column's cells are.
//!
//! The results are worked out with the rows ([`crate::engine::calc`]), when the docs or the view
//! change, never while drawing.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, HighlightStyle, InteractiveElement, IntoElement,
    ParentElement, Role, SharedString, StatefulInteractiveElement, Styled, StyledText, Window, div,
};
use gpui_core::Press;

use super::format;
use super::ops::ColumnLayout;
use super::render::line;
use super::table::{Action, CellRef, Entry, Item, Popover, TableView};
use crate::engine::{Calculated, DAY_MS, Group, calculate};
use crate::model::{Calculation, DatabaseFields, Doc, Property, PropertyConfig, View};

/// Each footer's results, a place per column (None where a column has no calculation or it
/// works out as nothing): the table's one footer, or one per group when it groups (a folded
/// group's empty, as it shows none). Nothing at all when the view has no calculations.
pub(crate) fn footers(
    fields: &DatabaseFields,
    view: &View,
    columns: &[ColumnLayout],
    groups: Option<(&[Group], &[bool])>,
    shown: &[usize],
    rows: &[Doc],
) -> Vec<Vec<Option<Calculated>>> {
    if view.calculations.is_empty() {
        return Vec::new();
    }
    let footer = |indexes: &[usize]| -> Vec<Option<Calculated>> {
        columns
            .iter()
            .map(|c| {
                let calc = view.calculations.get(&c.property_id)?;
                let p = fields.properties.get(c.property)?;
                calculate(p, calc, rows, indexes)
            })
            .collect()
    };
    match groups {
        Some((groups, folded)) => groups
            .iter()
            .enumerate()
            .map(|(i, g)| {
                if folded.get(i).copied().unwrap_or(false) {
                    Vec::new()
                } else {
                    footer(&g.rows)
                }
            })
            .collect(),
        None => vec![footer(shown)],
    }
}

/// A result as its column's cells show its kind of value: a count with its digits grouped, a
/// percent to a tenth, a number in the column's format (its currency, its precision), a date
/// as its cells show dates (in the viewer's zone, with the time when it has one), and a length
/// of time in its largest unit.
pub fn result_text(property: &Property, result: Calculated, offset_ms: i64) -> String {
    let plain = PropertyConfig::default();
    match result {
        Calculated::Count(n) => format::number(n as f64, &plain),
        Calculated::Percent(x) => {
            format!("{}%", format::number((x * 1000.).round() / 10., &plain))
        }
        Calculated::Number(x) => format::number(x, &property.config),
        Calculated::Date { ms, include_time } => {
            if include_time {
                format::instant(ms, offset_ms)
            } else {
                format::day(ms.div_euclid(DAY_MS))
            }
        }
        Calculated::Duration(ms) => duration(ms),
    }
}

/// A length of time in its largest unit, rounded: "40 minutes", "3 hours", "12 days",
/// "8 months", "2 years".
pub fn duration(ms: i64) -> String {
    let minutes = ms.max(0) as f64 / 60_000.;
    let hours = minutes / 60.;
    let days = hours / 24.;
    let (n, unit) = if minutes < 59.5 {
        (minutes, "minute")
    } else if hours < 23.5 {
        (hours, "hour")
    } else if days < 59.5 {
        (days, "day")
    } else if days < 730. {
        (days * 12. / 365.25, "month")
    } else {
        (days / 365.25, "year")
    };
    let n = n.round();
    let plain = PropertyConfig::default();
    let s = if n == 1. { "" } else { "s" };
    format!("{} {unit}{s}", format::number(n, &plain))
}

/// The entries of a column's calculation menu: None, the counts and percents every type has,
/// then its type's own, the current one checked and opened on.
pub(crate) fn entries(p: &Property, current: Option<&Calculation>) -> Vec<Entry> {
    let current = current.filter(|c| c.applies_to(&p.kind));
    let pick = |c: &Calculation| {
        Entry::act(c.label(), Action::Calc(Some(c.clone())))
            .checked(current == Some(c))
            .highlighted(current == Some(c))
    };
    let mut out = vec![
        Entry::act("None", Action::Calc(None))
            .checked(current.is_none())
            .highlighted(current.is_none()),
    ];
    let (counts, percents) = Calculation::ANY.split_at(5);
    out.push(Entry::heading("Count"));
    out.extend(counts.iter().map(pick));
    out.push(Entry::heading("Percent"));
    out.extend(percents.iter().map(pick));
    let own = Calculation::of_type(&p.kind);
    if !own.is_empty() {
        out.push(Entry::heading(p.kind.label().to_owned()));
        out.extend(own.iter().map(pick));
    }
    out
}

impl TableView {
    /// Opens the calculation menu of a property's column in a footer: the table's (`group`
    /// None), or a group's, by its key ([`crate::engine::GroupKey`]'s string), as pressing the
    /// footer's cell does. Nothing opens for a footer or column the table doesn't show.
    pub fn open_calculation_menu(
        &mut self,
        property_id: &str,
        group: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        self.ensure();
        if let (Some(item), Some(column)) = (
            self.calc_item(group.map(str::to_owned).as_ref()),
            self.column_of(property_id),
        ) {
            self.cell = Some(CellRef { item, column });
            self.open_calc_menu(column, group.map(str::to_owned), cx);
        }
    }

    /// What a footer's cell says, as its label and result ("Sum $1,240"): the table's footer
    /// (`group` None) or a group's, by its key. None where the column has no calculation (or
    /// one that doesn't fit its type), or the table shows no such footer.
    pub fn calculation_text(&mut self, property_id: &str, group: Option<&str>) -> Option<String> {
        self.ensure();
        let column = self.column_of(property_id)?;
        let item = self.calc_item(group.map(str::to_owned).as_ref())?;
        let Item::Calc(g) = self.shown.items[item] else {
            return None;
        };
        let (label, value) = self.calc_words(g, column)?;
        Some(match value {
            Some(v) => format!("{label} {v}"),
            None => label,
        })
    }

    /// The footer line of a group (by key), or the table's.
    fn calc_item(&self, group: Option<&String>) -> Option<usize> {
        self.shown.items.iter().position(|i| match (i, group) {
            (Item::Calc(None), None) => true,
            (Item::Calc(Some(g)), Some(key)) => self
                .shown
                .groups
                .get(*g)
                .is_some_and(|g| g.key.to_string() == *key),
            _ => false,
        })
    }

    pub(crate) fn open_calc_menu(
        &mut self,
        column: usize,
        group: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(col) = self.shown.columns.get(column) else {
            return;
        };
        self.date = None;
        self.menu_serial += 1;
        self.popover = Some(Popover::Calc {
            property_id: col.property_id.clone(),
            group,
        });
        cx.notify();
    }

    /// Opens the menu of a footer line's cell in `column` (Enter on it).
    pub(crate) fn open_calc_menu_on(
        &mut self,
        column: usize,
        group: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        let key = group.and_then(|g| self.shown.groups.get(g).map(|g| g.key.to_string()));
        self.open_calc_menu(column, key, cx);
    }

    /// The bounds of the open calculation menu's cell, which it hangs from.
    pub(crate) fn calc_bounds(
        &self,
        property_id: &str,
        group: Option<&String>,
    ) -> Option<gpui::Bounds<gpui::Pixels>> {
        let item = self.calc_item(group)?;
        let column = self.column_of(property_id)?;
        Some(self.cell_bounds(CellRef { item, column }))
    }

    /// Closes the calculation menu when its footer or column is no longer shown (its group
    /// folded or gone, its column hidden).
    pub(crate) fn close_gone_calc_menu(&mut self) {
        if let Some(Popover::Calc { property_id, group }) = &self.popover
            && (self.calc_item(group.as_ref()).is_none() || self.column_of(property_id).is_none())
        {
            self.popover = None;
        }
    }

    /// The open calculation menu's entries.
    pub(crate) fn calc_entries(&self) -> Vec<Entry> {
        let Some(Popover::Calc { property_id, .. }) = &self.popover else {
            return Vec::new();
        };
        let (Some(fields), Some(view)) =
            (self.database.database.as_ref(), self.shown.view.as_ref())
        else {
            return Vec::new();
        };
        match fields.property(property_id) {
            Some(p) => entries(p, view.calculations.get(property_id)),
            None => Vec::new(),
        }
    }

    /// A pick in the calculation menu: the view's `calculations` with the column's set (or
    /// removed, for None), as a `viewChanged`. Every footer shows it, as Notion's do.
    pub(crate) fn activate_calc(
        &mut self,
        calc: Option<Calculation>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(Popover::Calc { property_id, .. }) = self.popover.take() else {
            return;
        };
        window.focus(&self.focus, cx);
        cx.notify();
        let Some(mut view) = self.shown.view.clone() else {
            return;
        };
        match calc {
            Some(c) => view.calculations.insert(property_id, c),
            None => view.calculations.remove(&property_id),
        };
        self.change_view(view, window, cx);
    }

    /// A footer cell's label and result: the label alone when it works out as nothing (an
    /// average of no numbers). None without a calculation that fits the column.
    fn calc_words(&self, group: Option<usize>, column: usize) -> Option<(String, Option<String>)> {
        let p = self.property_at(column)?;
        let calc = self
            .shown
            .view
            .as_ref()?
            .calculations
            .get(&p.id)
            .filter(|c| c.applies_to(&p.kind))?;
        let offset = i64::from(self.host.utc_offset_minutes()) * 60_000;
        let result = self
            .shown
            .calcs
            .get(group.unwrap_or(0))
            .and_then(|f| f.get(column).copied().flatten());
        Some((
            calc.short_label().to_owned(),
            result.map(|r| result_text(p, r, offset)),
        ))
    }

    // --- Drawing ----------------------------------------------------------------------------

    /// A footer line: its title column's cell in place, the others in the strip that scrolls
    /// sideways, as a row's.
    pub(crate) fn render_calc(
        &self,
        ix: usize,
        group: Option<usize>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = &self.theme;
        let cols = self.shown.columns.len();
        let cells: Vec<AnyElement> = (1..cols)
            .map(|c| self.render_calc_cell(ix, group, c, cx))
            .collect();
        let key = group
            .and_then(|g| self.shown.groups.get(g))
            .map_or_else(|| "table".to_owned(), |g| g.key.to_string());
        div()
            .id(("calc", ix))
            .debug_selector(move || format!("calc:{key}"))
            .role(Role::Row)
            .h(self.row_height())
            .w_full()
            .flex()
            .border_b_1()
            .border_color(t.colors.line)
            .when(cols > 0, |d| {
                d.child(self.render_calc_cell(ix, group, 0, cx))
            })
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

    fn render_calc_cell(
        &self,
        ix: usize,
        group: Option<usize>,
        column: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = &self.theme;
        let Some(p) = self.property_at(column) else {
            return div().into_any_element();
        };
        let focused = self.cell == Some(CellRef { item: ix, column });
        let open = matches!(&self.popover, Some(Popover::Calc { property_id, .. })
            if *property_id == p.id && self.cell.is_some_and(|c| c.item == ix));
        let words = self.calc_words(group, column);
        let label = match &words {
            Some((l, Some(v))) => format!("{}: {l} {v}", p.name),
            Some((l, None)) => format!("{}: {l}", p.name),
            None => format!("Calculate {}", p.name),
        };
        // One line, the label muted before the result, so a narrow column ends it in one
        // ellipsis.
        let content = match words {
            Some((l, value)) => {
                let text = match value {
                    Some(v) => format!("{l} {v}"),
                    None => l.clone(),
                };
                let muted = HighlightStyle {
                    color: Some(t.colors.text_muted),
                    ..Default::default()
                };
                div()
                    .min_w_0()
                    .truncate()
                    .child(StyledText::new(text).with_highlights([(0..l.len(), muted)]))
                    .into_any_element()
            }
            // Blank until a pointer is over it, it's focused or its menu is open; always on
            // touch, where nothing hovers.
            None => line("Calculate")
                .text_color(t.colors.text_faint)
                .when(!(t.touch || focused || open), |d| {
                    d.invisible()
                        .group_hover("gpui-tables-calc", |s| s.visible())
                })
                .into_any_element(),
        };
        let pid = p.id.clone();
        let key = group
            .and_then(|g| self.shown.groups.get(g))
            .map_or_else(|| "table".to_owned(), |g| g.key.to_string());
        let shown = self.shown_serial;
        let focus = self.focus.clone();
        Press::new(("calc-cell", column))
            .tab_stop(false)
            .focus_ring(false)
            .accessibility_label(SharedString::from(label))
            .debug_selector(move || format!("calc:{key}:{pid}"))
            .group("gpui-tables-calc")
            .relative()
            .flex_none()
            .w(self.column_px(column))
            .h_full()
            .px(t.px(t.sizes.cell_padding))
            .flex()
            .items_center()
            .justify_end()
            .overflow_hidden()
            .border_r_1()
            .border_color(t.colors.line)
            .hover(|s| s.bg(t.colors.hover))
            .when(open, |d| d.bg(t.colors.hover))
            .when(focused, |d| {
                d.child(
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
                if !this.still_shown(shown) {
                    return;
                }
                window.focus(&focus, cx);
                this.clear_selection();
                this.cell = Some(CellRef { item: ix, column });
                this.open_calc_menu_on(column, group, cx);
            }))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{NumberFormat, PropertyType};

    #[test]
    fn results_as_their_columns_show_them() {
        let money =
            Property::new("p", "P", PropertyType::Number, "a0").with_config(PropertyConfig {
                format: Some(NumberFormat::Currency),
                currency: Some("USD".into()),
                precision: Some(0),
                ..PropertyConfig::default()
            });
        assert_eq!(
            result_text(&money, Calculated::Number(1_240_000.4), 0),
            "$1,240,000"
        );
        assert_eq!(result_text(&money, Calculated::Count(1240), 0), "1,240");
        assert_eq!(
            result_text(&money, Calculated::Percent(1. / 3.), 0),
            "33.3%"
        );
        assert_eq!(result_text(&money, Calculated::Percent(1.), 0), "100%");
        let date = Property::new("d", "D", PropertyType::Date, "a0");
        let nov15 = crate::engine::time::days_from_civil(2026, 11, 15);
        assert_eq!(
            result_text(
                &date,
                Calculated::Date {
                    ms: nov15 * DAY_MS,
                    include_time: false
                },
                -8 * 3_600_000
            ),
            "Nov 15, 2026"
        );
        assert_eq!(
            result_text(
                &date,
                Calculated::Date {
                    ms: nov15 * DAY_MS + 15 * 3_600_000,
                    include_time: true
                },
                0
            ),
            "Nov 15, 2026 3:00 PM"
        );
    }

    #[test]
    fn durations() {
        assert_eq!(duration(0), "0 minutes");
        assert_eq!(duration(60_000), "1 minute");
        assert_eq!(duration(3 * 3_600_000 + 29 * 60_000), "3 hours");
        assert_eq!(duration(3 * 3_600_000 + 59 * 60_000), "4 hours");
        assert_eq!(duration(DAY_MS), "1 day");
        assert_eq!(duration(59 * DAY_MS), "59 days");
        assert_eq!(duration(243 * DAY_MS), "8 months");
        assert_eq!(duration(400 * DAY_MS), "13 months");
        assert_eq!(duration(3 * 365 * DAY_MS + DAY_MS), "3 years");
        assert_eq!(duration(-5), "0 minutes");
        assert!(duration(i64::MAX).ends_with(" years"));
    }

    #[test]
    fn menus_fit_the_type() {
        let labels = |kind: PropertyType, current: Option<Calculation>| -> Vec<String> {
            let p = Property::new("p", "P", kind, "a0");
            entries(&p, current.as_ref())
                .into_iter()
                .map(|e| match e.checked {
                    Some(true) => format!("[{}]", e.label),
                    _ => e.label,
                })
                .collect()
        };
        let number = labels(PropertyType::Number, Some(Calculation::Sum));
        assert_eq!(number[0], "None");
        assert!(number.contains(&"[Sum]".to_owned()));
        assert!(number.contains(&"Median".to_owned()));
        assert!(!number.contains(&"Earliest".to_owned()));
        let text = labels(PropertyType::Text, None);
        assert_eq!(text[0], "[None]");
        assert_eq!(text.len(), 10, "None, two headings and seven: {text:?}");
        // One left from another type checks None.
        let date = labels(PropertyType::Date, Some(Calculation::Sum));
        assert_eq!(date[0], "[None]");
        assert!(date.contains(&"Latest".to_owned()));
        let check = labels(PropertyType::Checkbox, None);
        assert!(check.contains(&"Percent unchecked".to_owned()));
    }
}
