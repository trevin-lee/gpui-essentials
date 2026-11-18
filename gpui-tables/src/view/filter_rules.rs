//! The filter builder (Notion's "Advanced filter"): a view's filter as a list of rules, in
//! View settings' Filter submenu (and opened from the bar's filter summary).
//!
//! The submenu lists the rules as Rule 1, Rule 2... (each saying what it tests beside it),
//! with Match all and Match any once there are two, then Add a filter rule (the properties) and
//! Delete filter. Each rule's own submenu changes its property, its condition and its value, or
//! deletes it: options, people and related rows are picked from a list, a span from Today to
//! Next year, and text, numbers and dates are typed on a step of their own. A rule made of a
//! group (from the JSON, or another app) stays as it is, and can be deleted.
//!
//! Every change is the `viewChanged` of [`ops::rules`] written back, as any other view setting.
//! A rule without a value yet filters nothing (the engine leaves it out), as in Notion.

use gpui::{Context, Window};
use serde_json::{Value as Json, json};

use super::format;
use super::host::person_label;
use super::ops;
use super::properties::FieldFor;
use super::table::{Action, Entry, Popover, Reopen, TableView, type_icon};
use super::views::{ViewAction, ViewAnchor, ViewPage};
use crate::model::{Condition, Filter, FilterOp, Property, PropertyType, Value, View, Within};

/// The submenu's entry in View settings, and what the bar's filter summary opens.
pub(crate) const FILTER: &str = "Filter";

/// A rule's entry: "Rule 1", from 0.
pub(crate) fn rule_label(index: usize) -> String {
    format!("Rule {}", index + 1)
}

/// An operator in a rule's words.
pub(crate) fn op_label(op: &FilterOp) -> &'static str {
    use FilterOp as O;
    match op {
        O::Is => "is",
        O::IsNot => "is not",
        O::Contains => "contains",
        O::DoesNotContain => "does not contain",
        O::StartsWith => "starts with",
        O::IsEmpty => "is empty",
        O::IsNotEmpty => "is not empty",
        O::Greater => "is greater than",
        O::GreaterOrEqual => "is at least",
        O::Less => "is less than",
        O::LessOrEqual => "is at most",
        O::Before => "is before",
        O::After => "is after",
        O::OnOrBefore => "is on or before",
        O::OnOrAfter => "is on or after",
        O::Within => "is within",
        O::IsChecked => "is checked",
        O::IsNotChecked => "is not checked",
        O::Other(_) => "tests",
    }
}

/// A span's name in the menu.
fn within_label(w: &Within) -> &'static str {
    match w {
        Within::Today => "Today",
        Within::PastWeek => "Past week",
        Within::PastMonth => "Past month",
        Within::PastYear => "Past year",
        Within::NextWeek => "Next week",
        Within::NextMonth => "Next month",
        Within::NextYear => "Next year",
        Within::Other(_) => "Some time",
    }
}

/// Whether the operator takes no value.
fn needs_no_value(op: &FilterOp) -> bool {
    matches!(
        op,
        FilterOp::IsEmpty | FilterOp::IsNotEmpty | FilterOp::IsChecked | FilterOp::IsNotChecked
    )
}

/// How a rule's value is given: picked from a list, a span, typed, or none.
#[derive(Clone, Copy, Debug, PartialEq)]
enum ValueKind {
    None,
    Pick,
    Span,
    Typed,
}

fn value_kind(p: &Property, op: &FilterOp) -> ValueKind {
    if needs_no_value(op) {
        return ValueKind::None;
    }
    if *op == FilterOp::Within {
        return ValueKind::Span;
    }
    match p.kind {
        PropertyType::Select
        | PropertyType::MultiSelect
        | PropertyType::Person
        | PropertyType::Relation
        | PropertyType::CreatedBy
        | PropertyType::EditedBy => ValueKind::Pick,
        PropertyType::Checkbox => ValueKind::None,
        _ => ValueKind::Typed,
    }
}

impl TableView {
    /// The Filter submenu's entries for view `v`.
    pub(crate) fn filter_rule_entries(&self, v: &View) -> Vec<Entry> {
        use gpui_core::IconName as I;
        let act = |label: &str, a: ViewAction| Entry::act(label, Action::View(a));
        let Some(fields) = self.database.database.as_ref() else {
            return Vec::new();
        };
        let r = ops::rules(v);
        let mut out = vec![Entry::heading(FILTER)];
        if r.rules.len() > 1 {
            out.push(
                act("Match all", ViewAction::FilterAny(false))
                    .checked(!r.any)
                    .highlighted(!r.any),
            );
            out.push(act("Match any", ViewAction::FilterAny(true)).checked(r.any));
            out.push(Entry::heading("Rules"));
        }
        for (i, rule) in r.rules.iter().enumerate() {
            let entry = match rule {
                Filter::Condition(c) => match fields.property(&c.property_id) {
                    Some(p) => Entry::sub(rule_label(i), self.rule_entries(i, p, c))
                        .icon(type_icon(&p.kind))
                        .hint(self.rule_summary(p, c)),
                    None => Entry::sub(
                        rule_label(i),
                        vec![act("Delete rule", ViewAction::RemoveRule(i)).icon(I::Trash)],
                    )
                    .icon(I::Funnel)
                    .hint("A deleted property"),
                },
                group => {
                    let n = group
                        .walk()
                        .filter(|(f, _)| matches!(f, Filter::Condition(_)))
                        .count();
                    Entry::sub(
                        rule_label(i),
                        vec![
                            Entry::heading(format!("A group of {n} rules")),
                            act("Delete rule", ViewAction::RemoveRule(i)).icon(I::Trash),
                        ],
                    )
                    .icon(I::Group)
                    .hint(format!("A group of {n} rules"))
                }
            };
            out.push(entry);
        }
        let add: Vec<Entry> =
            std::iter::once(Entry::heading("Filter by"))
                .chain(filterable(fields).map(|p| {
                    act(&p.name, ViewAction::AddRule(p.id.clone())).icon(type_icon(&p.kind))
                }))
                .collect();
        out.push(Entry::sub("Add a filter rule", add).icon(I::Plus));
        if v.filter.is_some() {
            out.push(act("Delete filter", ViewAction::ClearFilter).icon(I::FunnelX));
        }
        out
    }

    /// Rule `index`'s own submenu: its property, its condition, its value, and Delete rule.
    fn rule_entries(&self, index: usize, p: &Property, c: &Condition) -> Vec<Entry> {
        use gpui_core::IconName as I;
        let act = |label: &str, a: ViewAction| Entry::act(label, Action::View(a));
        let Some(fields) = self.database.database.as_ref() else {
            return Vec::new();
        };
        let properties: Vec<Entry> = filterable(fields)
            .map(|q| {
                let on = q.id == p.id;
                act(&q.name, ViewAction::RuleProperty(index, q.id.clone()))
                    .icon(type_icon(&q.kind))
                    .checked(on)
                    .highlighted(on)
            })
            .collect();
        let conditions: Vec<Entry> = FilterOp::ALL
            .iter()
            .filter(|op| op.applies_to(&p.kind))
            .map(|op| {
                let on = *op == c.op;
                act(op_label(op), ViewAction::RuleOp(index, op.clone()))
                    .checked(on)
                    .highlighted(on)
            })
            .collect();
        let mut out = vec![
            Entry::sub("Property", properties)
                .icon(type_icon(&p.kind))
                .hint(p.name.clone()),
            Entry::sub("Condition", conditions)
                .icon(I::Equal)
                .hint(op_label(&c.op)),
        ];
        let value = self.value_text(p, c);
        match value_kind(p, &c.op) {
            ValueKind::None => {}
            ValueKind::Pick => {
                out.push(
                    Entry::sub("Value", self.pick_entries(index, p, c))
                        .icon(I::ListFilter)
                        .hint(value.unwrap_or_else(|| "None".into())),
                );
            }
            ValueKind::Span => {
                let spans = Within::ALL
                    .iter()
                    .map(|w| {
                        let on = c.value.as_str() == Some(w.as_str());
                        act(
                            within_label(w),
                            ViewAction::RuleValue(index, json!(w.as_str())),
                        )
                        .icon(I::Calendar)
                        .checked(on)
                        .highlighted(on)
                    })
                    .collect();
                out.push(
                    Entry::sub("Value", spans)
                        .icon(I::Calendar)
                        .hint(value.unwrap_or_else(|| "None".into())),
                );
            }
            ValueKind::Typed => {
                out.push(
                    act("Value", ViewAction::Page(ViewPage::RuleValue(index)))
                        .icon(I::Type)
                        .hint(value.unwrap_or_else(|| "None".into())),
                );
            }
        }
        out.push(act("Delete rule", ViewAction::RemoveRule(index)).icon(I::Trash));
        out
    }

    /// What a rule picks its value from: options (as their pills), people or related rows.
    fn pick_entries(&self, index: usize, p: &Property, c: &Condition) -> Vec<Entry> {
        let pick = |label: String, value: Json| {
            let on = c.value == value;
            Entry::act(label, Action::View(ViewAction::RuleValue(index, value)))
                .checked(on)
                .highlighted(on)
        };
        let mut out = Vec::new();
        match &p.kind {
            PropertyType::Select | PropertyType::MultiSelect => {
                for o in p.options() {
                    out.push(pick(o.name.clone(), json!(o.id)).chip(o.color.clone()));
                }
            }
            PropertyType::Person | PropertyType::CreatedBy | PropertyType::EditedBy => {
                let kinds = (p.kind == PropertyType::Person).then(|| p.person_kinds());
                for person in self.host.people(p) {
                    if let Some(kinds) = &kinds
                        && !person.kind().is_some_and(|k| kinds.contains(&k))
                    {
                        continue;
                    }
                    let Some(key) = person.key() else { continue };
                    out.push(
                        pick(person_label(self.host.as_ref(), &person), json!(key))
                            .icon(gpui_core::IconName::User),
                    );
                }
            }
            PropertyType::Relation => {
                if let Some(t) = p.relation() {
                    for r in self.host.relation_rows(t.database_id).into_iter().take(50) {
                        let title = if r.title.is_empty() {
                            "Untitled".into()
                        } else {
                            r.title
                        };
                        out.push(pick(title, json!(r.id)).icon(gpui_core::IconName::FileText));
                    }
                }
            }
            _ => {}
        }
        out
    }

    /// A rule in a few words: "Stage is Lead", "Role contains eng", "Warmth is greater than …".
    pub(crate) fn rule_summary(&self, p: &Property, c: &Condition) -> String {
        let op = op_label(&c.op);
        if value_kind(p, &c.op) == ValueKind::None {
            return format!("{} {op}", p.name);
        }
        let value = self.value_text(p, c).unwrap_or_else(|| "…".into());
        format!("{} {op} {value}", p.name)
    }

    /// A rule's value as a person reads it: an option's name, a person, a related row's title, a
    /// span, or what was typed. None when it has none.
    fn value_text(&self, p: &Property, c: &Condition) -> Option<String> {
        if c.value.is_null() {
            return None;
        }
        if c.op == FilterOp::Within {
            let w = c.value.as_str().map(Within::parse)?;
            return Some(within_label(&w).to_owned());
        }
        let s = c.value.as_str();
        Some(match &p.kind {
            PropertyType::Select | PropertyType::MultiSelect => {
                let o = s.and_then(|id| p.option(id))?;
                o.name.clone()
            }
            PropertyType::Person | PropertyType::CreatedBy | PropertyType::EditedBy => self
                .host
                .people(p)
                .into_iter()
                .find(|x| x.key().as_deref() == s)
                .map_or_else(
                    || "Someone".to_owned(),
                    |x| person_label(self.host.as_ref(), &x),
                ),
            PropertyType::Relation => p
                .relation()
                .and_then(|t| {
                    self.host
                        .relation_rows(t.database_id)
                        .into_iter()
                        .find(|r| Some(r.id.as_str()) == s)
                })
                .map_or_else(
                    || "A row".to_owned(),
                    |r| {
                        if r.title.is_empty() {
                            "Untitled".to_owned()
                        } else {
                            r.title
                        }
                    },
                ),
            _ => match &c.value {
                Json::String(t) => t.clone(),
                Json::Number(n) => n.to_string(),
                other => other.to_string(),
            },
        })
    }

    /// Does what a Filter submenu's entry says to view `v`: the changed view, or a note.
    pub(crate) fn apply_rule_action(
        &mut self,
        v: View,
        action: ViewAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(fields) = self.database.database.clone() else {
            return;
        };
        let changed = match action {
            ViewAction::FilterAny(any) => ops::set_any(&v, any),
            ViewAction::AddRule(pid) => {
                let Some(p) = fields.property(&pid) else {
                    return;
                };
                ops::add_rule(&v, Filter::Condition(ops::new_rule(p)))
            }
            ViewAction::RemoveRule(i) => ops::remove_rule(&v, i),
            ViewAction::RuleProperty(i, pid) => {
                let Some(p) = fields.property(&pid) else {
                    return;
                };
                let same = matches!(
                    ops::rules(&v).rules.get(i),
                    Some(Filter::Condition(c)) if c.property_id == pid
                );
                if same {
                    return;
                }
                ops::set_rule(&v, i, Filter::Condition(ops::new_rule(p)))
            }
            ViewAction::RuleOp(i, op) => {
                let Some(Filter::Condition(c)) = ops::rules(&v).rules.get(i).cloned() else {
                    return;
                };
                let Some(p) = fields.property(&c.property_id) else {
                    return;
                };
                // The value stays while it means the same to the new condition.
                let keep = value_kind(p, &op) == value_kind(p, &c.op);
                let value = if keep { c.value.clone() } else { Json::Null };
                ops::set_rule(
                    &v,
                    i,
                    Filter::Condition(Condition {
                        property_id: c.property_id,
                        op,
                        value,
                    }),
                )
            }
            ViewAction::RuleValue(i, value) => {
                let Some(Filter::Condition(c)) = ops::rules(&v).rules.get(i).cloned() else {
                    return;
                };
                ops::set_rule(&v, i, Filter::Condition(Condition { value, ..c }))
            }
            _ => return,
        };
        let limits = self.host.limits();
        if changed
            .filter
            .as_ref()
            .is_some_and(|f| f.nodes() > limits.filter_nodes)
        {
            self.refuse(
                format!("A filter has at most {} rules", limits.filter_nodes),
                cx,
            );
            return;
        }
        if changed != v {
            self.emit_view_change(changed, window, cx);
        }
    }

    /// A rule's value typed: what it is now, as someone would type it.
    pub(crate) fn rule_value_text(&self, v: &View, index: usize) -> String {
        match ops::rules(v).rules.get(index) {
            Some(Filter::Condition(c)) => match &c.value {
                Json::String(t) => t.clone(),
                Json::Number(n) => n.to_string(),
                _ => String::new(),
            },
            _ => String::new(),
        }
    }

    /// Enter on a rule's typed value: a number read as the property's format reads it, a date
    /// as 2026-11-15, text cut to its limit. An empty field takes the value away.
    pub(crate) fn commit_rule_value(
        &mut self,
        view_id: &str,
        index: usize,
        typed: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(fields), Some(v)) = (
            self.database.database.clone(),
            self.view_by_id(view_id).cloned(),
        ) else {
            return;
        };
        let Some(Filter::Condition(c)) = ops::rules(&v).rules.get(index).cloned() else {
            return;
        };
        let Some(p) = fields.property(&c.property_id) else {
            return;
        };
        let t = typed.trim();
        let value = if t.is_empty() {
            Json::Null
        } else if p.kind == PropertyType::Number {
            match format::parse_number(t, &p.config) {
                Some(x) => Value::number(x).to_json(),
                None => {
                    self.refuse("Type a number".into(), cx);
                    return;
                }
            }
        } else if p.kind.is_date() {
            if !is_day(t) {
                self.refuse("Type a date as 2026-11-15".into(), cx);
                return;
            }
            json!(t)
        } else {
            match self.host.limits().clamp_value(p, Value::text(t)) {
                Value::Text(t) => json!(t),
                _ => json!(t),
            }
        };
        let changed = ops::set_rule(&v, index, Filter::Condition(Condition { value, ..c }));
        if changed != v {
            self.emit_view_change(changed, window, cx);
        }
        self.rule_value_back(view_id.to_owned(), index, window, cx);
    }

    /// Back from a rule's typed value: the Filter submenu open again on its rule, on Value.
    pub(crate) fn rule_value_back(
        &mut self,
        view_id: String,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let anchor = match &self.popover {
            Some(Popover::View { anchor, .. }) => *anchor,
            _ => ViewAnchor::Settings,
        };
        let (page, mut path) = match anchor {
            ViewAnchor::Settings => (ViewPage::Settings, Vec::new()),
            _ => (ViewPage::Menu, vec!["View settings".to_owned()]),
        };
        path.extend([FILTER.to_owned(), rule_label(index)]);
        self.open_view_page(view_id, page, anchor, window, cx);
        self.reopen = Reopen::at(path, Some("Value".into()));
    }

    /// Opens a rule's typed value on a step of its own.
    pub(crate) fn open_rule_value(
        &mut self,
        view_id: &str,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(v) = self.view_by_id(view_id).cloned() else {
            return;
        };
        let text = self.rule_value_text(&v, index);
        let max = self.host.limits().text_chars;
        self.open_field(
            FieldFor::RuleValue(view_id.to_owned(), index),
            text,
            max,
            "Value",
            window,
            cx,
        );
    }
}

/// The properties a rule can test: every one some operator applies to.
fn filterable(fields: &crate::model::DatabaseFields) -> impl Iterator<Item = &Property> {
    fields
        .properties_in_order()
        .into_iter()
        .filter(|p| FilterOp::ALL.iter().any(|op| op.applies_to(&p.kind)))
}

/// Whether `t` is a day as 2026-11-15.
fn is_day(t: &str) -> bool {
    let b = t.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b.iter()
            .enumerate()
            .all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}
