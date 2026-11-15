//! What pressing something in a card does, and what it sends: the contract of OpenUI's React
//! renderer (react-lang's triggerAction, and the message text a web host builds from it). A button's steps run in
//! order: @Set and @Reset change the card's $variables, @OpenUrl opens a link (only http and
//! https ones), and @ToAssistant sends the agent a message as the user: the words, then any
//! context and the form's values as JSON, exactly as a web host writes it, so the agent reads a
//! press the same from either.

use super::Budget;
use super::eval::Eval;
use super::syntax::Ast;
use super::value::{
    Obj, V, from_serde, js_trim, nests_deeper, number, parse_float, to_json, to_serde,
};
use regex::RegexBuilder;
use std::sync::Arc;

/// An http or https address from a card, kept as data: the host opens it through its own check.
/// One with a user name or password in it is refused, since it reads as a different site.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link(String);

impl Link {
    pub fn parse(s: &str) -> Option<Link> {
        if s.len() > 4096 {
            return None;
        }
        let u = url::Url::parse(js_trim(s)).ok()?;
        let ok = matches!(u.scheme(), "http" | "https")
            && u.host_str().is_some_and(|h| !h.is_empty())
            && u.username().is_empty()
            && u.password().is_none();
        ok.then(|| Link(u.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A value from a card (a step's context) as the agent will get it.
#[derive(Clone, Debug)]
pub struct Data(pub(crate) V);

impl Data {
    /// The value as JSON, written the way the web's JSON.stringify writes it.
    pub fn json(&self) -> String {
        to_json(&self.0, 0)
    }

    pub fn to_value(&self) -> serde_json::Value {
        to_serde(&self.0)
    }
}

impl PartialEq for Data {
    fn eq(&self, other: &Data) -> bool {
        self.json() == other.json()
    }
}

/// An expression kept for when it runs: the value a @Set step gives its $variable.
#[derive(Clone, Debug)]
pub struct Expr(pub(crate) Arc<Ast>);

impl PartialEq for Expr {
    fn eq(&self, other: &Expr) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
            || to_json(&self.0.to_value(), 0) == to_json(&other.0.to_value(), 0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    /// A message to the agent from the user. Without a message the press's words are sent (a
    /// button with no action sends its label).
    ToAssistant {
        message: Option<String>,
        context: Option<Data>,
    },
    OpenUrl(Link),
    /// A $variable (its name with the $) set to the expression's value at the time.
    Set {
        var: String,
        value: Expr,
    },
    /// $variables back to their declared defaults.
    Reset(Vec<String>),
}

/// What a press does: its steps, in order.
#[derive(Clone, Debug, PartialEq)]
pub struct Action {
    pub steps: Vec<Step>,
    /// Written as steps (@ToAssistant, Action([...])) rather than as no action or the legacy
    /// { type } object, which matters for checking a form first.
    plan: bool,
}

impl Action {
    /// What a press without an action does: send its words.
    pub fn default_words() -> Action {
        Action {
            steps: vec![Step::ToAssistant {
                message: None,
                context: None,
            }],
            plan: false,
        }
    }

    /// Whether a primary button in a form checks the form's rules before this runs: always,
    /// unless it's written as steps none of which reaches the agent (the web's Button).
    pub fn checks_form(&self) -> bool {
        !self.plan
            || self
                .steps
                .iter()
                .any(|s| matches!(s, Step::ToAssistant { .. }))
    }

    /// The same action with a card's details added to each message's context, as the web adds
    /// them when a card in a card block is pressed (react-ui's withItemContext).
    pub(crate) fn with_item(&self, item: &Obj) -> Action {
        if !self.plan {
            // The legacy and default forms carry the details as event params, which the web
            // doesn't put in the message.
            return self.clone();
        }
        let mut details = Obj::new();
        for (k, v) in item.iter().filter(|(_, v)| !matches!(v, V::Undef)) {
            details.set(k.clone(), v.clone());
        }
        if details.is_empty() {
            return self.clone();
        }
        let suffix = format!("Selected item: {}", to_json(&V::obj(details), 0));
        let steps = self
            .steps
            .iter()
            .map(|s| match s {
                Step::ToAssistant { message, context } => {
                    let context = match context.as_ref().map(|c| &c.0) {
                        None | Some(V::Null | V::Undef) => suffix.clone(),
                        Some(V::Str(s)) => format!("{s}\n{suffix}"),
                        Some(other) => format!("{}\n{suffix}", to_json(other, 0)),
                    };
                    Step::ToAssistant {
                        message: message.clone(),
                        context: Some(Data(V::str(&context))),
                    }
                }
                other => other.clone(),
            })
            .collect();
        Action { steps, plan: true }
    }
}

/// An evaluated action prop as an Action, or None when there's none.
pub(crate) fn action(v: &V) -> Option<Action> {
    match v {
        V::Undef | V::Null => None,
        V::Plan(steps) => Some(Action {
            steps: steps.iter().filter_map(step).collect(),
            plan: true,
        }),
        V::Obj(o) => {
            // The legacy { type, url, context, params } object: the web opens its url or sends
            // the words with its context.
            let truthy = |k: &str| o.get(k).filter(|v| v.truthy()).cloned();
            let param = |k: &str| {
                o.get("params")
                    .map(|p| p.field(k))
                    .filter(|v| !matches!(v, V::Undef))
            };
            let kind = truthy("type")
                .map(|t| super::value::to_string(&t))
                .unwrap_or_else(|| "continue_conversation".into());
            let steps = match kind.as_str() {
                "open_url" => truthy("url")
                    .or_else(|| param("url"))
                    .and_then(|u| u.as_str().and_then(Link::parse))
                    .map(Step::OpenUrl)
                    .into_iter()
                    .collect(),
                "continue_conversation" => {
                    let context = truthy("context")
                        .or_else(|| param("context"))
                        .filter(|c| !c.is_nullish())
                        .map(Data);
                    vec![Step::ToAssistant {
                        message: None,
                        context,
                    }]
                }
                _ => Vec::new(),
            };
            Some(Action { steps, plan: false })
        }
        // Anything else throws in the web's handler, so the press does nothing.
        _ => Some(Action {
            steps: Vec::new(),
            plan: false,
        }),
    }
}

fn step(v: &V) -> Option<Step> {
    let V::Obj(o) = v else { return None };
    match o.get("type").and_then(V::as_str)? {
        "continue_conversation" => {
            let message = match o.get("message") {
                Some(V::Str(s)) => s.to_string(),
                Some(m) if m.truthy() => return None,
                _ => String::new(),
            };
            let context = o
                .get("context")
                .filter(|c| !matches!(c, V::Undef))
                .cloned()
                .map(Data);
            Some(Step::ToAssistant {
                message: Some(message),
                context,
            })
        }
        "open_url" => o
            .get("url")
            .and_then(V::as_str)
            .and_then(Link::parse)
            .map(Step::OpenUrl),
        "set" => match (o.get("target").and_then(V::as_str), o.get("valueAST")) {
            (Some(var), Some(V::Ast(a))) => Some(Step::Set {
                var: var.to_string(),
                value: Expr(a.clone()),
            }),
            _ => None,
        },
        "reset" => {
            let vars: Vec<String> = o
                .get("targets")?
                .as_arr()?
                .iter()
                .filter_map(|t| t.as_str().map(str::to_string))
                .collect();
            (!vars.is_empty()).then_some(Step::Reset(vars))
        }
        _ => None,
    }
}

/// What a press leaves for the host to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// Send this to the agent as the user's message.
    Send(String),
    /// Open this link (after the host's own check).
    Open(Link),
}

/// A card's state: its $variables (by name, with the $) and the values of its form fields, kept
/// as react-lang's store keeps them, since a message to the agent carries them as they are.
#[derive(Clone, Debug, Default)]
pub struct Store {
    entries: Obj,
}

/// The value inside a field's { value, componentType } entry.
fn unwrap(v: V) -> V {
    match &v {
        V::Obj(o) if o.has("value") => o.get("value").cloned().unwrap_or(V::Undef),
        _ => v,
    }
}

impl Store {
    pub(crate) fn new(defaults: &Obj) -> Store {
        let mut s = Store::default();
        for (k, v) in defaults.iter() {
            s.entries.set(k.clone(), v.clone());
        }
        s
    }

    /// What an expression reads for a $variable.
    pub(crate) fn state(&self, name: &str) -> V {
        unwrap(self.entries.get(name).cloned().unwrap_or(V::Undef))
    }

    pub(crate) fn set(&mut self, name: &str, v: V) {
        self.entries.set(name, v);
    }

    /// A $variable's value (name with the $).
    pub fn var(&self, name: &str) -> Option<serde_json::Value> {
        self.entries.get(name).map(|v| to_serde(&unwrap(v.clone())))
    }

    /// Sets a $variable: what a field bound to it writes, or a modal closing.
    pub fn set_var(&mut self, name: &str, value: serde_json::Value) {
        self.entries.set(name, from_serde(&value));
    }

    /// A field's value: in its form when it has one, else at the top (react-lang's getFieldValue).
    pub fn field(&self, form: Option<&str>, name: &str) -> Option<serde_json::Value> {
        let v = match named(form) {
            None => self.entries.get(name).cloned(),
            Some(f) => match self.entries.get(f) {
                Some(V::Obj(o)) => o.get(name).cloned(),
                _ => None,
            },
        }?;
        match unwrap(v) {
            V::Undef => None,
            v => Some(to_serde(&v)),
        }
    }

    /// Sets a field that isn't bound to a $variable, as { value, componentType } in its form
    /// (react-lang's setFieldValue). `kind` is the component that names itself there (Chips,
    /// OptionCards, EditableTable); the other inputs leave it out, as on the web.
    pub fn set_field(
        &mut self,
        form: Option<&str>,
        kind: Option<&str>,
        name: &str,
        value: serde_json::Value,
    ) {
        let mut entry = Obj::new();
        entry.set("value", from_serde(&value));
        if let Some(k) = kind {
            entry.set("componentType", V::str(k));
        }
        match named(form) {
            None => self.entries.set(name, V::obj(entry)),
            Some(f) => {
                let mut data = match self.entries.get(f) {
                    Some(V::Obj(o)) => (**o).clone(),
                    _ => Obj::new(),
                };
                data.set(name, V::obj(entry));
                self.entries.set(f, V::obj(data));
            }
        }
    }

    /// The form state a press carries: its form's fields when there are any, otherwise the
    /// whole store (react-lang's getFormPayload).
    fn payload(&self, form: Option<&str>) -> Obj {
        if let Some(f) = named(form)
            && let Some(V::Obj(o)) = self.entries.get(f)
        {
            let mut out = Obj::new();
            out.set(f, V::Obj(o.clone()));
            return out;
        }
        self.entries.clone()
    }
}

/// A form name as the web tests one: an empty name is no form.
fn named(form: Option<&str>) -> Option<&str> {
    form.filter(|f| !f.is_empty())
}

pub(crate) fn run(
    action: &Action,
    words: &str,
    form: Option<&str>,
    store: &mut Store,
    defaults: &Obj,
) -> Vec<Effect> {
    let form = named(form);
    let mut out = Vec::new();
    for s in &action.steps {
        match s {
            Step::ToAssistant { message, context } => {
                let text = message_text(
                    message.as_deref().unwrap_or(words),
                    context.as_ref().map(|c| &c.0),
                    form,
                    &store.payload(form),
                );
                if !text.is_empty() {
                    out.push(Effect::Send(text));
                }
            }
            Step::OpenUrl(link) => out.push(Effect::Open(link.clone())),
            Step::Set { var, value } => {
                let mut budget = Budget::new();
                let snapshot = store.clone();
                let state = |name: &str| snapshot.state(name);
                let v = Eval::new(&mut budget, &state).plain(&value.0);
                // A press that wraps a $variable in itself nests it one deeper each time; past
                // MAX_DEPTH it stays as it was.
                if budget.failed.is_none() && !nests_deeper(&v, super::MAX_DEPTH) {
                    store.set(var, v);
                }
            }
            Step::Reset(vars) => {
                for var in vars {
                    let v = defaults
                        .get(var)
                        .cloned()
                        .filter(|v| !matches!(v, V::Undef))
                        .unwrap_or(V::Null);
                    store.set(var, v);
                }
            }
        }
    }
    out
}

/// The web's uiActionText: the words, then the context and form values as JSON below them.
fn message_text(words: &str, context: Option<&V>, form: Option<&str>, state: &Obj) -> String {
    let words = js_trim(words);
    let mut extra = Obj::new();
    if let Some(c) = context.filter(|c| !c.is_nullish()) {
        extra.set("context", c.clone());
    }
    if !state.is_empty() {
        let key = match form {
            Some(f) if !f.is_empty() => format!("form {f}"),
            _ => "form".into(),
        };
        extra.set(key, V::obj(state.clone()));
    }
    if extra.is_empty() {
        return words.to_string();
    }
    let text = format!("{words}\n\n```json\n{}\n```", to_json(&V::obj(extra), 2));
    js_trim(&text).to_string()
}

/// One validation rule on a field, as written in its rules object.
#[derive(Clone, Debug, PartialEq)]
pub enum Rule {
    Required,
    Email,
    Url,
    Numeric,
    Min(f64),
    Max(f64),
    MinLength(f64),
    MaxLength(f64),
    Pattern(String),
}

/// A field's rules in the order written; the first that fails gives the message.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rules(pub Vec<Rule>);

impl Rules {
    /// react-ui's parseStructuredRules: true turns a rule on, a value gives its argument.
    pub(crate) fn from_value(v: &V) -> Rules {
        let V::Obj(o) = v else {
            return Rules::default();
        };
        let mut rules = Vec::new();
        for (k, val) in o.iter() {
            if matches!(val, V::Bool(false) | V::Undef | V::Null) {
                continue;
            }
            let num = || match val {
                V::Num(n) => *n,
                V::Str(s) => number(s),
                _ => f64::NAN,
            };
            let rule = match &**k {
                "required" => Rule::Required,
                "email" => Rule::Email,
                "url" => Rule::Url,
                "numeric" => Rule::Numeric,
                "min" => Rule::Min(num()),
                "max" => Rule::Max(num()),
                "minLength" => Rule::MinLength(num()),
                "maxLength" => Rule::MaxLength(num()),
                "pattern" => match val {
                    V::Str(s) if s.len() <= 1_000 => Rule::Pattern(s.to_string()),
                    _ => continue,
                },
                _ => continue,
            };
            rules.push(rule);
        }
        Rules(rules)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn required(&self) -> bool {
        self.0.contains(&Rule::Required)
    }

    /// The message for a value that breaks a rule, as lang-core's validators word it.
    pub fn check(&self, value: &serde_json::Value) -> Option<String> {
        use serde_json::Value as J;
        let empty = |v: &J| match v {
            J::Null => true,
            J::String(s) => s.is_empty(),
            J::Array(a) => a.is_empty(),
            J::Object(m) => m.is_empty(),
            _ => false,
        };
        let utf16 = |s: &str| s.encode_utf16().count() as f64;
        let as_number = |v: &J| match v {
            J::Number(n) => n.as_f64().unwrap_or(f64::NAN),
            J::String(s) => parse_float(s),
            J::Bool(b) => parse_float(if *b { "true" } else { "false" }),
            _ => f64::NAN,
        };
        let fmt = super::value::num_text;
        let below = |n: f64, m: f64| {
            n.partial_cmp(&m)
                .is_none_or(|o| o == std::cmp::Ordering::Less)
        };
        let above = |n: f64, m: f64| {
            n.partial_cmp(&m)
                .is_none_or(|o| o == std::cmp::Ordering::Greater)
        };
        for rule in &self.0 {
            if empty(value) {
                if *rule == Rule::Required {
                    return Some("This field is required".into());
                }
                continue;
            }
            let failed = match rule {
                Rule::Required => match value {
                    J::Object(m)
                        if m.values().all(J::is_boolean)
                            && !m.values().any(|b| b.as_bool() == Some(true)) =>
                    {
                        Some("At least one option is required".into())
                    }
                    _ => None,
                },
                Rule::Email => {
                    let ok = value.as_str().is_some_and(|s| {
                        regex::Regex::new(r"^[^\s@]+@[^\s@]+\.[^\s@]+$")
                            .is_ok_and(|r| r.is_match(s))
                    });
                    (!ok).then(|| "Please enter a valid email".into())
                }
                Rule::Url => (!value.as_str().is_some_and(|s| url::Url::parse(s).is_ok()))
                    .then(|| "Please enter a valid URL".into()),
                Rule::Numeric => {
                    let ok = match value {
                        J::Number(_) => true,
                        J::String(s) => !parse_float(s).is_nan() && !js_trim(s).is_empty(),
                        _ => false,
                    };
                    (!ok).then(|| "Must be a number".into())
                }
                // A bound that isn't a number fails every value, as `n >= NaN` is false in
                // JavaScript too: hence "below" and "above" include incomparable.
                Rule::Min(m) => {
                    let n = as_number(value);
                    (!n.is_nan() && below(n, *m)).then(|| format!("Must be at least {}", fmt(*m)))
                }
                Rule::Max(m) => {
                    let n = as_number(value);
                    (!n.is_nan() && above(n, *m))
                        .then(|| format!("Must be no more than {}", fmt(*m)))
                }
                Rule::MinLength(m) => value
                    .as_str()
                    .filter(|s| below(utf16(s), *m))
                    .map(|_| format!("Must be at least {} characters", fmt(*m))),
                Rule::MaxLength(m) => value
                    .as_str()
                    .filter(|s| above(utf16(s), *m))
                    .map(|_| format!("Must be no more than {} characters", fmt(*m))),
                // An agent's pattern runs in the regex crate, which takes linear time whatever
                // the pattern, and within a size limit; one it can't compile checks nothing.
                Rule::Pattern(p) => value.as_str().and_then(|s| {
                    let r = RegexBuilder::new(p)
                        .size_limit(1 << 20)
                        .dfa_size_limit(1 << 20)
                        .build()
                        .ok()?;
                    (!r.is_match(s)).then(|| "Invalid format".into())
                }),
            };
            if failed.is_some() {
                return failed;
            }
        }
        None
    }
}
