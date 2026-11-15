//! Statements to a tree of components: lang-core 1.1.0's materialize.ts, validation.ts and
//! buildResult (parser.ts), followed step by step. References resolve (hoisting included),
//! positional arguments become named props by the library's parameter order, props are checked
//! against the library's schemas, and a component missing what it needs is dropped with an issue,
//! exactly where lang-core drops it, so a card shows the same parts here as on the web.

use super::eval::Eval;
use super::schema::{LIBRARY, Library, Schema, json_value};
use super::syntax::{self, A, Ast, Comp, is_builtin, is_reserved};
use super::value::{El, Obj, V, nests_deeper, same_value_zero, to_json};
use super::{Budget, CardError};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Why part of a card was dropped or read differently from what was written (lang-core's
/// ValidationError). The card still shows what was left.
#[derive(Clone, Debug, PartialEq)]
pub struct Issue {
    /// lang-core's code: missing-required, null-required, unknown-component, inline-reserved,
    /// excess-args, type-mismatch, unknown-function or no-root.
    pub code: &'static str,
    pub component: String,
    /// The prop, as a JSON pointer ("/title", "/rows/0/left").
    pub path: String,
    /// The statement it was found in.
    pub statement: Option<String>,
    /// The card still renders as written (a missing root statement with a fallback).
    pub warning: bool,
    pub message: String,
}

#[derive(Clone, Debug)]
pub(crate) enum Kind {
    Value(A),
    State(A),
    Query(A),
    Mutation(A),
}

impl Kind {
    fn expr(&self) -> &A {
        match self {
            Kind::Value(a) | Kind::State(a) | Kind::Query(a) | Kind::Mutation(a) => a,
        }
    }
}

/// What lang-core's parse gives the renderer. The app needs the root, the state and the issues;
/// the rest is what the golden tests compare with lang-core's.
pub(crate) struct Parsed {
    pub root: Option<V>,
    #[cfg_attr(not(test), allow(dead_code))]
    pub root_expression: Option<A>,
    pub state: Obj,
    pub issues: Vec<Issue>,
    #[cfg_attr(not(test), allow(dead_code))]
    pub unresolved: Vec<String>,
    #[cfg_attr(not(test), allow(dead_code))]
    pub orphaned: Vec<String>,
    #[cfg_attr(not(test), allow(dead_code))]
    pub incomplete: bool,
    #[cfg_attr(not(test), allow(dead_code))]
    pub statements: usize,
}

/// The parameters of the built-in calls, by position (their zod objects' key order).
fn builtin_params(name: &str) -> &'static [&'static str] {
    match name {
        "Count" | "First" | "Last" => &["array"],
        "Sum" | "Avg" | "Min" | "Max" => &["numbers"],
        "Sort" => &["array", "field", "direction"],
        "Filter" => &["array", "field", "operator", "value"],
        "Round" => &["number", "decimals"],
        "Abs" | "Floor" | "Ceil" => &["number"],
        "Each" => &["array", "varName", "template"],
        "Run" => &["ref"],
        "ToAssistant" => &["message", "context"],
        "OpenUrl" => &["url"],
        "Set" => &["variable", "value"],
        "Reset" => &["variable"],
        "Action" => &["steps"],
        _ => &[],
    }
}

/// Built-ins whose arguments the runtime reads unevaluated.
fn lazy(name: &str) -> bool {
    matches!(name, "Each" | "Run" | "Set" | "Reset" | "Action")
}

pub(crate) fn is_component_call(a: &Ast) -> bool {
    matches!(a, Ast::Comp(c) if !c.func && !is_builtin(&c.name) && !is_reserved(&c.name))
}

/// A memoized statement, in the mode it was read in.
#[derive(Clone)]
enum Memo {
    Value(V),
    Expr(A),
}

struct Ctx<'a> {
    lib: &'a Library,
    syms: HashMap<Arc<str>, A>,
    issues: Vec<Issue>,
    /// The names lang-core reports unresolved (the first MAX_ISSUES of them), and how many
    /// there were, which decides what's memoized.
    unres: Vec<Arc<str>>,
    unres_count: usize,
    visited: HashSet<Arc<str>>,
    partial: bool,
    current: Option<Arc<str>>,
    reached: HashSet<Arc<str>>,
    memo: HashMap<(bool, Arc<str>), Memo>,
    budget: &'a mut Budget,
}

/// A value that failed its schema and has no default to stand in.
struct Invalid;

/// What checking a value against its schema gave: the value as it was (None), a cleaned copy,
/// or V::Undef for a prop to leave out.
type Checked = Result<Option<V>, Invalid>;

impl Ctx<'_> {
    fn issue(&mut self, code: &'static str, component: &str, path: &str, message: String) {
        if self.issues.len() >= super::MAX_ISSUES {
            return;
        }
        self.issues.push(Issue {
            code,
            component: component.to_string(),
            path: path.to_string(),
            statement: self.current.as_deref().map(str::to_string),
            warning: false,
            message,
        });
    }

    fn with_statement<T>(&mut self, id: Arc<str>, f: impl FnOnce(&mut Self) -> T) -> T {
        let prev = self.current.replace(id);
        let out = f(self);
        self.current = prev;
        out
    }

    /// One step of work and one level deeper; false once the card has used its budget.
    fn enter(&mut self) -> bool {
        self.budget.enter()
    }

    fn leave(&mut self) {
        self.budget.leave();
    }

    fn signature(&self, name: &str) -> String {
        let params = self
            .lib
            .get(name)
            .map(|c| {
                c.params
                    .iter()
                    .map(|p| {
                        if p.required {
                            format!("{}*", p.name)
                        } else {
                            p.name.to_string()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        format!("{name}({params})")
    }

    fn resolve_value(&mut self, name: &Arc<str>) -> V {
        match self.resolve(name, true) {
            Some(Memo::Value(v)) => v,
            Some(Memo::Expr(a)) => V::Ast(a),
            None => V::Null,
        }
    }

    fn resolve_expr(&mut self, name: &Arc<str>) -> A {
        match self.resolve(name, false) {
            Some(Memo::Expr(a)) => a,
            Some(Memo::Value(_)) | None => Ast::Ph(name.clone()).arc(),
        }
    }

    /// lang-core's resolveRef. None for a name that isn't defined or is being read already (a
    /// cycle), which lang-core reports as unresolved.
    fn resolve(&mut self, name: &Arc<str>, value: bool) -> Option<Memo> {
        if self.visited.contains(name) || !self.syms.contains_key(name) {
            self.unres_count += 1;
            if self.unres.len() < super::MAX_ISSUES {
                self.unres.push(name.clone());
            }
            return None;
        }
        let target = self.syms[name].clone();
        self.reached.insert(name.clone());
        if let Ast::Comp(c) = &*target
            && is_reserved(&c.name)
        {
            let r = Ast::RuntimeRef {
                n: name.clone(),
                mutation: &*c.name == "Mutation",
            }
            .arc();
            return Some(if value {
                Memo::Value(V::Ast(r))
            } else {
                Memo::Expr(r)
            });
        }
        let key = (value, name.clone());
        if let Some(m) = self.memo.get(&key) {
            return Some(m.clone());
        }
        self.visited.insert(name.clone());
        let out = self.with_statement(name.clone(), |ctx| {
            let before = ctx.unres_count;
            let m = if value {
                Memo::Value(ctx.value(&target))
            } else {
                Memo::Expr(ctx.expr(&target))
            };
            if ctx.unres_count == before {
                ctx.memo.insert(key, m.clone());
            }
            m
        });
        self.visited.remove(name);
        Some(out)
    }

    fn runtime_call(name: &str) -> bool {
        is_builtin(name) || is_reserved(name)
    }

    /// lang-core's materializeCall: the arguments read as expressions, and named when the
    /// built-in isn't lazy.
    fn call(&mut self, c: &Comp, scoped: &HashSet<Arc<str>>) -> A {
        let args: Vec<A> = c.args.iter().map(|a| self.expr_in(a, scoped)).collect();
        let mapped = if is_builtin(&c.name) && !lazy(&c.name) {
            Some(
                builtin_params(&c.name)
                    .iter()
                    .zip(args.iter())
                    .map(|(p, a)| (Arc::from(*p), a.clone()))
                    .collect(),
            )
        } else {
            c.mapped.clone()
        };
        Ast::Comp(Comp {
            name: c.name.clone(),
            args,
            mapped,
            func: c.func,
        })
        .arc()
    }

    /// @Each(array, varName, template): the loop variable stays a name inside the template.
    fn each(&mut self, c: &Comp, scoped: &HashSet<Arc<str>>) -> Option<A> {
        if &*c.name != "Each" || c.args.len() < 3 {
            return None;
        }
        let var = match &*c.args[1] {
            Ast::Ref(n) => n.clone(),
            Ast::Str(s) => s.clone(),
            _ => return None,
        };
        let mut inner = scoped.clone();
        inner.insert(var);
        let args = c
            .args
            .iter()
            .enumerate()
            .map(|(i, a)| {
                if i == 1 {
                    a.clone()
                } else {
                    self.expr_in(a, &inner)
                }
            })
            .collect();
        Some(
            Ast::Comp(Comp {
                name: c.name.clone(),
                args,
                mapped: c.mapped.clone(),
                func: c.func,
            })
            .arc(),
        )
    }

    fn expr(&mut self, node: &A) -> A {
        self.expr_in(node, &HashSet::new())
    }

    /// lang-core's materializeExpr: references inlined, catalog calls given their named props,
    /// the rest kept as an expression for the runtime.
    fn expr_in(&mut self, node: &A, scoped: &HashSet<Arc<str>>) -> A {
        if !self.enter() {
            return Ast::Null.arc();
        }
        let out = match &**node {
            Ast::Ref(n) => {
                if scoped.contains(n) {
                    node.clone()
                } else {
                    self.resolve_expr(n)
                }
            }
            Ast::Comp(c) => {
                if c.func && !Self::runtime_call(&c.name) {
                    self.issue("unknown-function", &c.name, "", format!("Unknown function \"@{}\": not a built-in or action step. The call evaluates to null", c.name));
                    Ast::Null.arc()
                } else if let Some(each) = self.each(c, scoped) {
                    each
                } else if Self::runtime_call(&c.name) {
                    self.call(c, scoped)
                } else {
                    let args: Vec<A> = c.args.iter().map(|a| self.expr_in(a, scoped)).collect();
                    let mapped = self.lib.get(&c.name).map(|def| {
                        def.params
                            .iter()
                            .zip(args.iter())
                            .map(|(p, a)| (Arc::from(p.name.as_str()), a.clone()))
                            .collect()
                    });
                    if mapped.is_none() {
                        self.issue(
                            "unknown-component",
                            &c.name,
                            "",
                            format!(
                                "Unknown component \"{}\" — not found in catalog or builtins",
                                c.name
                            ),
                        );
                    }
                    Ast::Comp(Comp {
                        name: c.name.clone(),
                        args,
                        mapped,
                        func: c.func,
                    })
                    .arc()
                }
            }
            Ast::Arr(els) => Ast::Arr(els.iter().map(|e| self.expr_in(e, scoped)).collect()).arc(),
            Ast::Obj(entries) => Ast::Obj(
                entries
                    .iter()
                    .map(|(k, v)| (k.clone(), self.expr_in(v, scoped)))
                    .collect(),
            )
            .arc(),
            Ast::BinOp { op, left, right } => Ast::BinOp {
                op: *op,
                left: self.expr_in(left, scoped),
                right: self.expr_in(right, scoped),
            }
            .arc(),
            Ast::Not(e) => Ast::Not(self.expr_in(e, scoped)).arc(),
            Ast::Neg(e) => Ast::Neg(self.expr_in(e, scoped)).arc(),
            Ast::Ternary { cond, then, other } => Ast::Ternary {
                cond: self.expr_in(cond, scoped),
                then: self.expr_in(then, scoped),
                other: self.expr_in(other, scoped),
            }
            .arc(),
            Ast::Member { obj, field } => Ast::Member {
                obj: self.expr_in(obj, scoped),
                field: field.clone(),
            }
            .arc(),
            Ast::Index { obj, index } => Ast::Index {
                obj: self.expr_in(obj, scoped),
                index: self.expr_in(index, scoped),
            }
            .arc(),
            Ast::Assign { target, value } => Ast::Assign {
                target: target.clone(),
                value: self.expr_in(value, scoped),
            }
            .arc(),
            _ => node.clone(),
        };
        self.leave();
        out
    }

    /// lang-core's materializeValue: components become elements with checked props, literals
    /// plain values, and anything that reads state or calls a built-in stays an expression.
    fn value(&mut self, node: &A) -> V {
        if !self.enter() {
            return V::Null;
        }
        let out = match &**node {
            Ast::Ref(n) => self.resolve_value(n),
            Ast::Str(s) => V::Str(s.clone()),
            Ast::Num(n) => V::Num(*n),
            Ast::Bool(b) => V::Bool(*b),
            Ast::Null | Ast::Ph(_) => V::Null,
            Ast::Arr(els) => {
                let mut items = Vec::with_capacity(els.len());
                for e in els {
                    if matches!(**e, Ast::Ph(_)) {
                        continue;
                    }
                    let v = self.value(e);
                    if matches!(v, V::Null) && matches!(**e, Ast::Comp(_) | Ast::Ref(_)) {
                        continue;
                    }
                    items.push(v);
                }
                V::arr(items)
            }
            Ast::Obj(entries) => {
                let mut o = Obj::new();
                for (k, v) in entries {
                    let v = self.value(v);
                    o.set(k.clone(), v);
                }
                V::obj(o)
            }
            Ast::Comp(c) => self.component(c),
            Ast::StateRef(_)
            | Ast::RuntimeRef { .. }
            | Ast::BinOp { .. }
            | Ast::Not(_)
            | Ast::Neg(_)
            | Ast::Ternary { .. }
            | Ast::Member { .. }
            | Ast::Index { .. }
            | Ast::Assign { .. } => V::Ast(self.expr(node)),
            Ast::Lit(v) => v.clone(),
        };
        self.leave();
        out
    }

    fn component(&mut self, c: &Comp) -> V {
        let name = &c.name;
        if c.func && !Self::runtime_call(name) {
            self.issue("unknown-function", name, "", format!("Unknown function \"@{name}\": not a built-in or action step. The call evaluates to null"));
            return V::Null;
        }
        if is_reserved(name) {
            self.issue(
                "inline-reserved",
                name,
                "",
                format!(
                    "{name}() must be declared as a top-level statement, not used inline as a value"
                ),
            );
            return V::Null;
        }
        if Self::runtime_call(name) {
            if let Some(each) = self.each(c, &HashSet::new()) {
                return V::Ast(each);
            }
            return V::Ast(self.call(c, &HashSet::new()));
        }
        let lib = self.lib;
        let Some(def) = lib.get(name) else {
            self.issue(
                "unknown-component",
                name,
                "",
                format!("Unknown component \"{name}\" — not found in catalog or builtins"),
            );
            return V::Null;
        };
        let mut props = Obj::new();
        let mut drop = false;
        for (p, arg) in def.params.iter().zip(c.args.iter()) {
            let value = self.value(arg);
            props.set(p.name.as_str(), value.clone());
            let path = format!("/{}", p.name);
            let next = match self.check(&value, &p.schema, name, &path) {
                Ok(n) => Some(n),
                Err(Invalid) => resolve_invalid(p.required, &p.schema).ok(),
            };
            match next {
                None => drop = true,
                Some(Some(V::Undef)) => props.remove(&p.name),
                Some(Some(v)) => props.set(p.name.as_str(), v),
                Some(None) => {}
            }
        }
        if c.args.len() > def.params.len() {
            let (declared, got) = (def.params.len(), c.args.len());
            self.issue(
                "excess-args",
                name,
                "",
                format!(
                    "{name} takes {declared} arg(s), got {got} ({} excess dropped)",
                    got - declared
                ),
            );
        }
        let mut still = Vec::new();
        let missing: Vec<_> = def
            .params
            .iter()
            .filter(|p| p.required && props.get(&p.name).is_none_or(|v| matches!(v, V::Null)))
            .collect();
        for p in missing {
            match p.schema.default_value() {
                Some(d) => props.set(p.name.as_str(), d),
                None => still.push(p),
            }
        }
        if !still.is_empty() {
            let signature = self.signature(name);
            for p in still {
                let present = props.has(&p.name);
                let path = format!("/{}", p.name);
                let (code, message) = if present {
                    (
                        "null-required",
                        format!(
                            "required field \"{path}\" cannot be null — signature: {signature}"
                        ),
                    )
                } else {
                    (
                        "missing-required",
                        format!("missing required field \"{path}\" — signature: {signature}"),
                    )
                };
                self.issue(code, name, &path, message);
            }
            return V::Null;
        }
        if drop {
            return V::Null;
        }
        let mut left = 100_000;
        let dynamic = props.iter().any(|(_, v)| dynamic_in(v, &mut left));
        V::El(Arc::new(El {
            name: name.clone(),
            props,
            dynamic,
        }))
    }

    /// lang-core's validateSchemaValue.
    fn check(&mut self, value: &V, s: &Schema, comp: &str, path: &str) -> Checked {
        if !self.enter() {
            return Ok(None);
        }
        let out = self.check_inner(value, s, comp, path);
        self.leave();
        out
    }

    fn check_inner(&mut self, value: &V, s: &Schema, comp: &str, path: &str) -> Checked {
        match value {
            V::Undef | V::Null | V::Ast(_) => return Ok(None),
            V::El(e) => {
                if s.composite() {
                    return Ok(None);
                }
                if s.ty().is_some() || s.one_of.is_some() || s.constant.is_some() {
                    let expected = s.ty().unwrap_or("a literal value").to_string();
                    self.issue(
                        "type-mismatch",
                        comp,
                        path,
                        format!(
                            "field \"{path}\" expects {expected} but got component \"{}\"",
                            e.name
                        ),
                    );
                    return Err(Invalid);
                }
                return Ok(None);
            }
            _ => {}
        }
        if s.composite() {
            let names: Vec<Option<&str>> = match &s.any_of {
                Some(opts) => opts.iter().map(Schema::ref_name).collect(),
                None => vec![s.ref_name()],
            };
            if names.is_empty() || !names.iter().all(|n| n.is_some_and(|n| self.lib.has(n))) {
                return Ok(None);
            }
            let expected = names
                .iter()
                .flatten()
                .copied()
                .collect::<Vec<_>>()
                .join(" | ");
            let actual = match value {
                V::Arr(_) => "array",
                V::Obj(_) => "plain object",
                v => v.js_type(),
            };
            self.issue(
                "type-mismatch",
                comp,
                path,
                format!("field \"{path}\" expects {expected} but got {actual}"),
            );
            return Err(Invalid);
        }
        match s.ty() {
            Some("object") => self.check_object(value, s, comp, path),
            Some("array") => self.check_array(value, s, comp, path),
            _ => self.check_leaf(value, s, comp, path),
        }
    }

    fn check_object(&mut self, value: &V, s: &Schema, comp: &str, path: &str) -> Checked {
        let V::Obj(obj) = value else {
            self.issue(
                "type-mismatch",
                comp,
                path,
                format!(
                    "field \"{path}\" expects object but got {}",
                    value.js_type()
                ),
            );
            return Err(Invalid);
        };
        let props: &[(String, Schema)] = s.properties.as_deref().unwrap_or(&[]);
        let required: &[String] = s.required.as_deref().unwrap_or(&[]);
        let sub = |key: &str| props.iter().find(|(k, _)| k == key).map(|(_, s)| s);
        let mut out: Option<Obj> = None;
        let mut invalid = false;
        if !self.partial {
            for key in required {
                let present = obj.has(key);
                if !present || obj.get(key).is_some_and(V::is_nullish) {
                    if let Some(d) = sub(key).and_then(Schema::default_value) {
                        out.get_or_insert_with(|| (**obj).clone())
                            .set(key.as_str(), d);
                        continue;
                    }
                    let p = format!("{path}/{key}");
                    let (code, message) = if present {
                        (
                            "null-required",
                            format!("required field \"{p}\" cannot be null"),
                        )
                    } else {
                        (
                            "missing-required",
                            format!("missing required field \"{p}\""),
                        )
                    };
                    self.issue(code, comp, &p, message);
                    invalid = true;
                }
            }
        }
        for (key, sub) in props {
            let Some(current) = obj.get(key) else {
                continue;
            };
            let next = match self.check(current, sub, comp, &format!("{path}/{key}")) {
                Ok(n) => n,
                Err(Invalid) => match resolve_invalid(required.contains(key), sub) {
                    Ok(n) => n,
                    Err(Invalid) => {
                        invalid = true;
                        continue;
                    }
                },
            };
            let Some(next) = next else { continue };
            let o = out.get_or_insert_with(|| (**obj).clone());
            if matches!(next, V::Undef) {
                o.remove(key);
            } else {
                o.set(key.as_str(), next);
            }
        }
        if invalid {
            return Err(Invalid);
        }
        Ok(out.map(V::obj))
    }

    fn check_array(&mut self, value: &V, s: &Schema, comp: &str, path: &str) -> Checked {
        let V::Arr(items) = value else {
            self.issue(
                "type-mismatch",
                comp,
                path,
                format!("field \"{path}\" expects array but got {}", value.js_type()),
            );
            return Err(Invalid);
        };
        let Some(item_schema) = s.items.as_deref() else {
            return Ok(None);
        };
        let mut out: Option<Vec<V>> = None;
        for (i, x) in items.iter().enumerate() {
            let next = self.check(x, item_schema, comp, &format!("{path}/{i}"));
            let changed = !matches!(next, Ok(None));
            if changed && out.is_none() {
                out = Some(items[..i].to_vec());
            }
            if let Some(o) = out.as_mut() {
                match next {
                    Ok(None) => o.push(x.clone()),
                    Ok(Some(v)) => o.push(v),
                    Err(Invalid) => {}
                }
            }
        }
        Ok(out.map(V::arr))
    }

    fn check_leaf(&mut self, value: &V, s: &Schema, comp: &str, path: &str) -> Checked {
        let members: Option<Vec<V>> = match (&s.one_of, &s.constant) {
            (Some(e), _) => Some(e.iter().map(json_value).collect()),
            (None, Some(c)) => Some(vec![json_value(c)]),
            _ => None,
        };
        let expected = match s.ty() {
            Some("string") => Some("string"),
            Some("number" | "integer") => Some("number"),
            Some("boolean") => Some("boolean"),
            _ => None,
        };
        let scalar = matches!(value, V::Str(_) | V::Num(_) | V::Bool(_));
        if let Some(members) = members {
            if self.partial || !scalar || members.iter().any(|m| same_value_zero(m, value)) {
                return Ok(None);
            }
            let list = members
                .iter()
                .map(|m| to_json(m, 0))
                .collect::<Vec<_>>()
                .join(", ");
            self.issue(
                "type-mismatch",
                comp,
                path,
                format!(
                    "field \"{path}\" expects one of [{list}] but got {}",
                    to_json(value, 0)
                ),
            );
            return Err(Invalid);
        }
        if let Some(t) = expected {
            let actual = match value {
                V::Str(_) => "string",
                V::Num(_) => "number",
                V::Bool(_) => "boolean",
                _ => "object",
            };
            if t != actual {
                self.issue(
                    "type-mismatch",
                    comp,
                    path,
                    format!("field \"{path}\" expects {t} but got {}", value.js_type()),
                );
                return Err(Invalid);
            }
        }
        Ok(None)
    }
}

/// lang-core's resolveInvalidValue: the schema's default stands in; without one a required
/// value makes its parent invalid, and an optional one is left out.
fn resolve_invalid(required: bool, s: &Schema) -> Checked {
    match s.default_value() {
        Some(d) => Ok(Some(d)),
        None if required => Err(Invalid),
        None => Ok(Some(V::Undef)),
    }
}

/// Whether a prop needs evaluating (lang-core's containsDynamicValue). Lists can share parts
/// through references, so once `left` values have been looked at it answers yes, and
/// evaluation, which counts its work, decides.
fn dynamic_in(v: &V, left: &mut usize) -> bool {
    if *left == 0 {
        return true;
    }
    *left -= 1;
    match v {
        V::Ast(_) | V::Plan(_) | V::Bind(_) => true,
        V::Arr(a) => a.iter().any(|x| dynamic_in(x, left)),
        V::El(e) => e.dynamic,
        V::Obj(o) => {
            o.get("steps").is_some_and(|s| matches!(s, V::Arr(_)))
                || o.iter().any(|(_, x)| dynamic_in(x, left))
        }
        _ => false,
    }
}

fn state_refs(a: &Ast) -> Vec<Arc<str>> {
    let mut out: Vec<Arc<str>> = Vec::new();
    let mut seen = HashSet::new();
    a.walk(&mut |n| {
        if let Ast::StateRef(name) = n
            && seen.insert(name.clone())
        {
            out.push(name.clone());
        }
    });
    out
}

fn only_builtin_functions(a: &Ast) -> bool {
    let mut ok = true;
    let whole = a.walk(&mut |n| {
        if let Ast::Comp(c) = n
            && !syntax::FUNCTIONS.contains(&&*c.name)
        {
            ok = false;
        }
    });
    ok && whole
}

/// A $variable's declared default: evaluated once against the defaults above it, and null when
/// it calls anything but a built-in function (lang-core's stateDefault).
fn state_default(v: V, declared: &Obj, budget: &mut Budget) -> V {
    match v {
        V::Ast(a) if only_builtin_functions(&a) => {
            let state = declared.clone();
            Eval::new(budget, &|name| state.get(name).cloned().unwrap_or(V::Null)).plain(&a)
        }
        V::Ast(_) => V::Null,
        V::Arr(items) => V::arr(
            items
                .iter()
                .map(|x| state_default(x.clone(), declared, budget))
                .collect(),
        ),
        V::Obj(o) => {
            let mut out = Obj::new();
            for (k, x) in o.iter() {
                out.set(k.clone(), state_default(x.clone(), declared, budget));
            }
            V::obj(out)
        }
        other => other,
    }
}

/// lang-core's parse(input, schema, "Stack") for a complete program.
pub(crate) fn parse(input: &str, budget: &mut Budget) -> Result<Parsed, CardError> {
    let lib = &*LIBRARY;
    let (raw, incomplete) = syntax::statements(input);
    let mut order: Vec<Arc<str>> = Vec::new();
    let mut stmts: HashMap<Arc<str>, Kind> = HashMap::new();
    for s in &raw {
        let expr = syntax::expression(s).ok_or(CardError::TooDeep)?;
        let kind = match &*expr {
            Ast::Comp(c) if &*c.name == "Query" => Kind::Query(expr.clone()),
            Ast::Comp(c) if &*c.name == "Mutation" => Kind::Mutation(expr.clone()),
            _ if s.state => Kind::State(expr),
            _ => Kind::Value(expr),
        };
        if !stmts.contains_key(&s.id) {
            order.push(s.id.clone());
        }
        stmts.insert(s.id.clone(), kind);
    }
    if order.is_empty() {
        return Ok(Parsed {
            root: None,
            root_expression: None,
            state: Obj::new(),
            issues: Vec::new(),
            unresolved: Vec::new(),
            orphaned: Vec::new(),
            incomplete,
            statements: 0,
        });
    }
    let entry: Option<(Arc<str>, bool)> = if stmts.contains_key("root") {
        Some((Arc::from("root"), false))
    } else {
        order.iter().find(|id| matches!(&stmts[*id], Kind::Value(e) if is_component_call(e) && matches!(&**e, Ast::Comp(c) if *c.name == *lib.root))).map(|id| (id.clone(), true))
    };
    let syms: HashMap<Arc<str>, A> = stmts
        .iter()
        .map(|(k, v)| (k.clone(), v.expr().clone()))
        .collect();
    let mut ctx = Ctx {
        lib,
        syms,
        issues: Vec::new(),
        unres: Vec::new(),
        unres_count: 0,
        visited: HashSet::new(),
        partial: incomplete,
        current: entry.as_ref().map(|(e, _)| e.clone()),
        reached: HashSet::new(),
        memo: HashMap::new(),
        budget,
    };
    let mut root = None;
    let mut root_expression = None;
    if let Some((id, _)) = &entry {
        let target = ctx.syms[id].clone();
        match ctx.value(&target) {
            v @ V::El(_) => root = Some(v),
            V::Ast(a) if contains_component(&a) => root_expression = Some(a),
            _ => {}
        }
    }
    // The $variables: declared ones get their defaults, and any read without a declaration is
    // null (lang-core's extractStatements).
    let mut state = Obj::new();
    for id in &order {
        match stmts[id].clone() {
            Kind::State(init) => {
                let v = ctx.value(&init);
                let d = state_default(v, &state, ctx.budget);
                if nests_deeper(&d, super::MAX_DEPTH) {
                    ctx.budget.failed.get_or_insert(CardError::TooDeep);
                    break;
                }
                state.set(id.clone(), d);
            }
            Kind::Query(e) | Kind::Mutation(e) => {
                if let Ast::Comp(c) = &*e {
                    ctx.with_statement(id.clone(), |ctx| {
                        c.args.iter().for_each(|a| drop(ctx.expr(a)))
                    });
                }
            }
            Kind::Value(_) => {}
        }
    }
    for id in &order {
        let nodes: Vec<A> = match &stmts[id] {
            Kind::State(a) | Kind::Value(a) => vec![a.clone()],
            Kind::Query(e) | Kind::Mutation(e) => match &**e {
                Ast::Comp(c) => c.args.clone(),
                _ => Vec::new(),
            },
        };
        for n in nodes {
            for dep in state_refs(&n) {
                if !state.has(&dep) {
                    state.set(dep, V::Null);
                }
            }
        }
    }
    ctx.current = entry.as_ref().map(|(e, _)| e.clone());
    let root_name = lib.root.clone();
    match &entry {
        None => ctx.issue(
            "no-root",
            &root_name,
            "",
            format!("No root statement, and no statement calls the root component {root_name}"),
        ),
        Some((id, fallback)) => {
            if root.is_none() && root_expression.is_none() && !resolves_to_component(&ctx.syms, id)
            {
                ctx.issue(
                    "no-root",
                    &root_name,
                    "",
                    "root does not evaluate to a component".into(),
                );
            } else if *fallback {
                let warning = root.is_some();
                let message = if warning {
                    format!(
                        "No root statement. Rendering the first statement that calls the root component {root_name}"
                    )
                } else {
                    format!(
                        "No root statement, and no statement calls the root component {root_name}"
                    )
                };
                ctx.issue("no-root", &root_name, "", message);
                if let Some(last) = ctx.issues.last_mut() {
                    last.warning = warning;
                }
            }
        }
    }
    if let Some(e) = ctx.budget.failed.clone() {
        return Err(e);
    }
    // The value statements nothing reached from the entry: lang-core leaves them out.
    let orphaned = match &entry {
        Some((e, _)) => order
            .iter()
            .filter(|id| {
                *id != e && matches!(stmts[*id], Kind::Value(_)) && !ctx.reached.contains(*id)
            })
            .map(|s| s.to_string())
            .collect(),
        None => Vec::new(),
    };
    Ok(Parsed {
        root,
        root_expression,
        state,
        issues: ctx.issues,
        unresolved: ctx.unres.iter().map(|s| s.to_string()).collect(),
        orphaned,
        incomplete,
        statements: order.len(),
    })
}

fn contains_component(a: &Ast) -> bool {
    let mut found = false;
    let whole = a.walk(&mut |n| found |= is_component_call(n));
    found && whole
}

fn resolves_to_component(syms: &HashMap<Arc<str>, A>, id: &Arc<str>) -> bool {
    let mut node = syms.get(id).cloned();
    let mut seen = HashSet::new();
    while let Some(n) = node.clone() {
        let Ast::Ref(name) = &*n else { break };
        if !seen.insert(name.clone()) {
            break;
        }
        node = syms.get(name).cloned();
    }
    node.is_some_and(|n| is_component_call(&n))
}
