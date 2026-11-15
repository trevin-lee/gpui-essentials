//! Expressions to values against the card's state: lang-core 1.1.0's evaluator.ts, its
//! built-in functions and steps (builtins.ts), and the prop evaluation the renderer runs before
//! drawing (evaluate-prop.ts, evaluate-tree.ts). Every step of work counts against the card's
//! budget, so a program that loops or doubles a value on itself stops instead of hanging the host.

use super::Budget;
use super::schema::LIBRARY;
use super::syntax::{A, Ast, Comp, FUNCTIONS, Op, STEPS};
use super::value::{El, Obj, V, js_trim, loose_eq, number, to_num, to_string};
use std::cmp::Ordering;
use std::sync::Arc;

pub(crate) struct Eval<'a> {
    budget: &'a mut Budget,
    /// A $variable's value (lang-core's getState, with a field's { value } unwrapped).
    state: &'a dyn Fn(&str) -> V,
    /// @Each's loop variables, innermost last.
    scope: Vec<(Arc<str>, V)>,
}

impl<'a> Eval<'a> {
    pub fn new(budget: &'a mut Budget, state: &'a dyn Fn(&str) -> V) -> Eval<'a> {
        Eval {
            budget,
            state,
            scope: Vec::new(),
        }
    }

    /// An expression evaluated outside any component (a $variable's default, a step's value).
    pub fn plain(&mut self, a: &Ast) -> V {
        self.eval(a, false)
    }

    /// lang-core's evaluateRoot: the root with every prop evaluated, as the web first draws it.
    pub fn root(&mut self, root: Option<&V>, expression: Option<&A>) -> Option<V> {
        if let Some(V::El(el)) = root {
            return Some(self.element(el));
        }
        let v = self.eval(expression?, true);
        match &v {
            V::El(el) => Some(self.element(el)),
            _ => None,
        }
    }

    fn state(&self, name: &str) -> V {
        (self.state)(name)
    }

    fn resolve(&self, name: &str) -> V {
        self.scope
            .iter()
            .rev()
            .find(|(n, _)| &**n == name)
            .map(|(_, v)| v.clone())
            .unwrap_or(V::Null)
    }

    /// lang-core's evaluate. `schema` is whether it has the library to hand (schemaCtx):
    /// without it a component made in an expression keeps its $variable props unevaluated.
    fn eval(&mut self, node: &Ast, schema: bool) -> V {
        if !self.budget.enter() {
            return V::Null;
        }
        let out = match node {
            Ast::Str(s) => V::Str(s.clone()),
            Ast::Num(n) => V::Num(*n),
            Ast::Bool(b) => V::Bool(*b),
            Ast::Null | Ast::Ph(_) => V::Null,
            Ast::StateRef(n) => self.state(n),
            Ast::Ref(n) | Ast::RuntimeRef { n, .. } => self.resolve(n),
            Ast::Arr(els) => V::arr(els.iter().map(|e| self.eval(e, false)).collect()),
            Ast::Obj(entries) => {
                let mut o = Obj::new();
                for (k, v) in entries {
                    let v = self.eval(v, false);
                    o.set(k.clone(), v);
                }
                V::obj(o)
            }
            Ast::Comp(c) => self.call(c, schema),
            Ast::BinOp { op, left, right } => self.binop(*op, left, right),
            Ast::Not(e) => V::Bool(!self.eval(e, false).truthy()),
            Ast::Neg(e) => V::Num(-to_num(&self.eval(e, false))),
            Ast::Ternary { cond, then, other } => {
                if self.eval(cond, false).truthy() {
                    self.eval(then, false)
                } else {
                    self.eval(other, false)
                }
            }
            Ast::Member { obj, field } => {
                let o = self.eval(obj, false);
                match &o {
                    V::Undef | V::Null => V::Null,
                    V::Arr(items) if &**field != "length" => {
                        self.budget.charge(items.len());
                        V::arr(
                            items
                                .iter()
                                .map(|it| {
                                    if it.is_nullish() {
                                        V::Null
                                    } else {
                                        nullish_to_null(it.field(field))
                                    }
                                })
                                .collect(),
                        )
                    }
                    _ => o.field(field),
                }
            }
            Ast::Index { obj, index } => {
                let o = self.eval(obj, false);
                let i = self.eval(index, false);
                if o.is_nullish() || i.is_nullish() {
                    V::Null
                } else if let V::Arr(items) = &o {
                    let n = to_num(&i);
                    if n >= 0.0 && n.fract() == 0.0 && n < items.len() as f64 {
                        items[n as usize].clone()
                    } else {
                        V::Undef
                    }
                } else {
                    o.field(&to_string(&i))
                }
            }
            Ast::Assign { target, .. } => V::Bind(target.clone()),
            Ast::Lit(v) => v.clone(),
        };
        self.budget.leave();
        out
    }

    fn binop(&mut self, op: Op, left: &Ast, right: &Ast) -> V {
        let l = self.eval(left, false);
        match op {
            Op::And => {
                return if l.truthy() {
                    self.eval(right, false)
                } else {
                    l
                };
            }
            Op::Or => {
                return if l.truthy() {
                    l
                } else {
                    self.eval(right, false)
                };
            }
            _ => {}
        }
        let r = self.eval(right, false);
        let (a, b) = (to_num(&l), to_num(&r));
        match op {
            Op::Add if matches!(l, V::Str(_)) || matches!(r, V::Str(_)) => {
                let text = |v: &V| {
                    if v.is_nullish() {
                        String::new()
                    } else {
                        to_string(v)
                    }
                };
                let mut s = text(&l);
                s.push_str(&text(&r));
                V::str(cap(&s))
            }
            Op::Add => V::Num(a + b),
            Op::Sub => V::Num(a - b),
            Op::Mul => V::Num(a * b),
            Op::Div => V::Num(if b == 0.0 { 0.0 } else { a / b }),
            Op::Rem => V::Num(if b == 0.0 { 0.0 } else { a % b }),
            Op::Eq => V::Bool(loose_eq(&l, &r)),
            Op::Ne => V::Bool(!loose_eq(&l, &r)),
            Op::Gt => V::Bool(a > b),
            Op::Lt => V::Bool(a < b),
            Op::Ge => V::Bool(a >= b),
            Op::Le => V::Bool(a <= b),
            Op::And | Op::Or => V::Null,
        }
    }

    fn call(&mut self, c: &Comp, schema: bool) -> V {
        let name = &*c.name;
        if FUNCTIONS.contains(&name) {
            if name == "Each" {
                return self.each(c, schema);
            }
            let args = self.args(c);
            return self.function(name, &args);
        }
        if STEPS.contains(&name) {
            return self.step(c);
        }
        let Some(mapped) = &c.mapped else {
            return V::Null;
        };
        let mut props = Obj::new();
        for (key, val) in mapped {
            let v = match &**val {
                Ast::StateRef(n) if schema && LIBRARY.reactive(name, key) => V::Bind(n.clone()),
                Ast::StateRef(n) if schema => self.state(n),
                Ast::StateRef(_) => V::Ast(val.clone()),
                _ => self.eval(val, schema),
            };
            props.set(key.clone(), v);
        }
        if schema {
            let keys: Vec<Arc<str>> = props.iter().map(|(k, _)| k.clone()).collect();
            for k in keys {
                let next = match props.get(&k) {
                    Some(V::El(e)) => Some(self.element(&e.clone())),
                    Some(V::Arr(items)) => Some(V::arr(
                        items
                            .clone()
                            .iter()
                            .map(|it| {
                                if let V::El(e) = it {
                                    self.element(e)
                                } else {
                                    it.clone()
                                }
                            })
                            .collect(),
                    )),
                    _ => None,
                };
                if let Some(n) = next {
                    props.set(k, n);
                }
            }
        }
        V::El(Arc::new(El {
            name: c.name.clone(),
            props,
            dynamic: true,
        }))
    }

    /// A built-in's arguments by name, evaluated.
    fn args(&mut self, c: &Comp) -> Obj {
        let mut out = Obj::new();
        if let Some(mapped) = &c.mapped {
            for (k, a) in mapped {
                let v = self.eval(a, false);
                out.set(k.clone(), v);
            }
        }
        out
    }

    fn function(&mut self, name: &str, args: &Obj) -> V {
        let arg = |k: &str| args.get(k).cloned().unwrap_or(V::Undef);
        let list = |k: &str| match args.get(k) {
            Some(V::Arr(a)) => Some(a.clone()),
            _ => None,
        };
        match name {
            "Count" => V::Num(list("array").map_or(0.0, |a| a.len() as f64)),
            "First" => list("array")
                .and_then(|a| a.first().cloned())
                .map_or(V::Null, nullish_to_null),
            "Last" => list("array")
                .and_then(|a| a.last().cloned())
                .map_or(V::Null, nullish_to_null),
            "Sum" => V::Num(list("numbers").map_or(0.0, |a| a.iter().map(to_num).sum())),
            "Avg" => V::Num(
                list("numbers")
                    .filter(|a| !a.is_empty())
                    .map_or(0.0, |a| a.iter().map(to_num).sum::<f64>() / a.len() as f64),
            ),
            "Min" | "Max" => V::Num(list("numbers").filter(|a| !a.is_empty()).map_or(0.0, |a| {
                a.iter().skip(1).fold(to_num(&a[0]), |acc, b| {
                    let b = to_num(b);
                    if acc.is_nan() || b.is_nan() {
                        f64::NAN
                    } else if name == "Min" {
                        acc.min(b)
                    } else {
                        acc.max(b)
                    }
                })
            })),
            "Sort" => match list("array") {
                Some(items) => {
                    let field = text_or_empty(&arg("field"));
                    let desc = text_or(&arg("direction"), "asc") == "desc";
                    self.budget
                        .charge(items.len() * (usize::BITS - items.len().leading_zeros()) as usize);
                    let mut keyed: Vec<(V, V)> = items
                        .iter()
                        .map(|it| {
                            (
                                if field.is_empty() {
                                    it.clone()
                                } else {
                                    resolve_field(it, &field)
                                },
                                it.clone(),
                            )
                        })
                        .collect();
                    merge_sort(&mut keyed, &|a: &(V, V), b: &(V, V)| {
                        let o = sort_order(&a.0, &b.0);
                        if desc { o.reverse() } else { o }
                    });
                    V::arr(keyed.into_iter().map(|(_, it)| it).collect())
                }
                None => arg("array"),
            },
            "Filter" => match list("array") {
                Some(items) => {
                    let field = text_or_empty(&arg("field"));
                    let op = text_or(&arg("operator"), "==");
                    let value = arg("value");
                    self.budget.charge(items.len());
                    V::arr(
                        items
                            .iter()
                            .filter(|it| {
                                keep(
                                    &if field.is_empty() {
                                        (*it).clone()
                                    } else {
                                        resolve_field(it, &field)
                                    },
                                    &op,
                                    &value,
                                )
                            })
                            .cloned()
                            .collect(),
                    )
                }
                None => V::arr(Vec::new()),
            },
            "Round" => {
                let d = match args.get("decimals") {
                    Some(v) if !v.is_nullish() => to_num(v),
                    _ => 0.0,
                };
                let factor = if d.fract() == 0.0 && d.abs() < 400.0 {
                    10f64.powi(d as i32)
                } else {
                    10f64.powf(d)
                };
                self.elementwise(&arg("number"), &|n| js_round(n * factor) / factor)
            }
            "Abs" => self.elementwise(&arg("number"), &f64::abs),
            "Floor" => self.elementwise(&arg("number"), &f64::floor),
            "Ceil" => self.elementwise(&arg("number"), &f64::ceil),
            _ => V::Null,
        }
    }

    fn elementwise(&mut self, v: &V, f: &dyn Fn(f64) -> f64) -> V {
        if !self.budget.enter() {
            return V::Null;
        }
        let out = match v {
            V::Arr(items) => V::arr(items.iter().map(|it| self.elementwise(it, f)).collect()),
            other => V::Num(f(to_num(other))),
        };
        self.budget.leave();
        out
    }

    /// @Each(array, var, template): the template once per item, the item written into it as a
    /// literal so a step made there keeps its value after the loop.
    fn each(&mut self, c: &Comp, schema: bool) -> V {
        if c.args.len() < 3 {
            return V::arr(Vec::new());
        }
        let V::Arr(items) = self.eval(&c.args[0], false) else {
            return V::arr(Vec::new());
        };
        let var = match &*c.args[1] {
            Ast::Ref(n) => n.clone(),
            Ast::Str(s) => s.clone(),
            _ => return V::arr(Vec::new()),
        };
        let mut out = Vec::with_capacity(items.len());
        for item in items.iter() {
            if self.budget.failed.is_some() {
                break;
            }
            let t = substitute(&c.args[2], &var, item, self.budget);
            self.scope.push((var.clone(), item.clone()));
            let mut r = self.eval(&t, schema);
            if schema && let V::El(e) = &r {
                r = self.element(&e.clone());
            }
            self.scope.pop();
            out.push(r);
        }
        V::arr(out)
    }

    /// An action step call is a one-step plan, an invalid one an empty plan, and Action([...])
    /// flattens the plans in it (lang-core's evaluateActionCall).
    fn step(&mut self, c: &Comp) -> V {
        let mut step = Obj::new();
        match &*c.name {
            "Action" => {
                let list = match c.args.first() {
                    Some(a) => self.eval(a, false),
                    None => V::arr(Vec::new()),
                };
                return flatten(list.as_arr().unwrap_or(&[]));
            }
            "ToAssistant" => {
                let args = self.args(c);
                let message = args
                    .get("message")
                    .filter(|v| !v.is_nullish())
                    .map(to_string)
                    .unwrap_or_default();
                step.set("type", V::str("continue_conversation"));
                step.set("message", V::str(&message));
                step.set(
                    "context",
                    args.get("context")
                        .filter(|v| !v.is_nullish())
                        .cloned()
                        .unwrap_or(V::Undef),
                );
            }
            "OpenUrl" => {
                let args = self.args(c);
                let url = args
                    .get("url")
                    .filter(|v| !v.is_nullish())
                    .map(to_string)
                    .unwrap_or_default();
                step.set("type", V::str("open_url"));
                step.set("url", V::str(&url));
            }
            "Run" => match c.args.first().map(|a| &**a) {
                Some(Ast::RuntimeRef { n, mutation }) => {
                    step.set("type", V::str("run"));
                    step.set("statementId", V::Str(n.clone()));
                    step.set(
                        "refType",
                        V::str(if *mutation { "mutation" } else { "query" }),
                    );
                }
                _ => return V::Plan(Arc::new(Vec::new())),
            },
            "Set" => match (c.args.first().map(|a| &**a), c.args.get(1)) {
                (Some(Ast::StateRef(target)), Some(value)) => {
                    step.set("type", V::str("set"));
                    step.set("target", V::Str(target.clone()));
                    step.set("valueAST", V::Ast(value.clone()));
                }
                _ => return V::Plan(Arc::new(Vec::new())),
            },
            "Reset" => {
                let targets: Vec<V> = c
                    .args
                    .iter()
                    .filter_map(|a| {
                        if let Ast::StateRef(n) = &**a {
                            Some(V::Str(n.clone()))
                        } else {
                            None
                        }
                    })
                    .collect();
                if targets.is_empty() {
                    return V::Plan(Arc::new(Vec::new()));
                }
                step.set("type", V::str("reset"));
                step.set("targets", V::arr(targets));
            }
            _ => return V::Null,
        }
        V::Plan(Arc::new(vec![V::obj(step)]))
    }

    /// lang-core's evaluateElementProps: each prop of a component evaluated, a $binding prop kept
    /// as its binding.
    pub fn element(&mut self, el: &Arc<El>) -> V {
        if !el.dynamic {
            return V::El(el.clone());
        }
        if !self.budget.enter() {
            return V::Null;
        }
        let mut props = Obj::new();
        for (k, v) in el.props.iter() {
            let reactive = LIBRARY.reactive(&el.name, k);
            let next = self.prop(v, reactive);
            props.set(k.clone(), next);
        }
        self.budget.leave();
        V::El(Arc::new(El {
            name: el.name.clone(),
            props,
            dynamic: el.dynamic,
        }))
    }

    /// lang-core's evaluatePropCore.
    fn prop(&mut self, v: &V, reactive: bool) -> V {
        if !self.budget.enter() {
            return V::Null;
        }
        let out = match v {
            V::Undef | V::Null | V::Bool(_) | V::Num(_) | V::Str(_) => v.clone(),
            V::Bind(t) if !reactive => nullish_to_null(self.state(t)),
            V::Bind(_) | V::Plan(_) => v.clone(),
            V::Ast(a) => match &**a {
                Ast::StateRef(n) if reactive => V::Bind(n.clone()),
                _ => match self.eval(a, true) {
                    V::El(e) => self.element(&e),
                    V::Arr(items) => V::arr(
                        items
                            .iter()
                            .map(|it| {
                                if let V::El(e) = it {
                                    self.element(e)
                                } else {
                                    it.clone()
                                }
                            })
                            .collect(),
                    ),
                    V::Bind(t) if !reactive => nullish_to_null(self.state(&t)),
                    r => r,
                },
            },
            V::Arr(items) => V::arr(items.iter().map(|it| self.prop(it, reactive)).collect()),
            V::El(e) => self.element(e),
            V::Obj(o) => {
                if o.iter().any(|(_, x)| x.is_object()) {
                    let mut out = Obj::new();
                    for (k, x) in o.iter() {
                        let next = self.prop(x, reactive);
                        out.set(k.clone(), next);
                    }
                    V::obj(out)
                } else {
                    v.clone()
                }
            }
        };
        self.budget.leave();
        match out {
            V::Plan(steps) => own_steps(steps),
            other => other,
        }
    }
}

fn nullish_to_null(v: V) -> V {
    if v.is_nullish() { V::Null } else { v }
}

fn cap(s: &str) -> &str {
    if s.len() <= super::MAX_EVAL_TEXT {
        return s;
    }
    let mut end = super::MAX_EVAL_TEXT;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

fn text_or_empty(v: &V) -> String {
    if v.is_nullish() {
        String::new()
    } else {
        to_string(v)
    }
}

fn text_or(v: &V, fallback: &str) -> String {
    if v.is_nullish() {
        fallback.to_string()
    } else {
        to_string(v)
    }
}

/// Math.round: halves go up, toward +Infinity.
fn js_round(n: f64) -> f64 {
    let f = n.floor();
    if n - f >= 0.5 { f + 1.0 } else { f }
}

/// A field path ("state.name") into an object, as @Sort and @Filter read one.
fn resolve_field(obj: &V, path: &str) -> V {
    if path.is_empty() || obj.is_nullish() {
        return V::Undef;
    }
    let mut cur = obj.clone();
    for part in path.split('.') {
        if cur.is_nullish() {
            return V::Undef;
        }
        cur = cur.field(part);
    }
    cur
}

fn keep(v: &V, op: &str, value: &V) -> bool {
    match op {
        "==" => loose_eq(v, value),
        "!=" => !loose_eq(v, value),
        ">" => to_num(v) > to_num(value),
        "<" => to_num(v) < to_num(value),
        ">=" => to_num(v) >= to_num(value),
        "<=" => to_num(v) <= to_num(value),
        "contains" => text_or_empty(v).contains(&text_or_empty(value)),
        _ => false,
    }
}

fn numeric(v: &V) -> bool {
    match v {
        V::Num(_) => true,
        V::Str(s) => !s.is_empty() && !number(s).is_nan(),
        _ => false,
    }
}

/// @Sort's comparison: numbers (and numeric strings) by value, the rest as text by
/// localeCompare, which here is case-insensitive first with lower case ahead on a tie.
fn sort_order(a: &V, b: &V) -> Ordering {
    if numeric(a) && numeric(b) {
        return (to_num(a) - to_num(b))
            .partial_cmp(&0.0)
            .unwrap_or(Ordering::Equal);
    }
    let (x, y) = (text_or_empty(a), text_or_empty(b));
    let fold = |s: &str| js_trim(s).to_lowercase();
    fold(&x).cmp(&fold(&y)).then_with(|| {
        let lower_first = |s: &str| {
            s.chars()
                .map(|c| if c.is_lowercase() { 0 } else { 1 })
                .collect::<Vec<_>>()
        };
        lower_first(&x).cmp(&lower_first(&y))
    })
}

/// A stable merge sort. It never panics on a comparison that isn't a total order (a mix of
/// numbers and text can make one), where the standard library's sort may.
fn merge_sort<T: Clone>(items: &mut [T], cmp: &dyn Fn(&T, &T) -> Ordering) {
    let n = items.len();
    if n < 2 {
        return;
    }
    let mid = n / 2;
    merge_sort(&mut items[..mid], cmp);
    merge_sort(&mut items[mid..], cmp);
    let mut merged = Vec::with_capacity(n);
    let (mut i, mut j) = (0, mid);
    while i < mid && j < n {
        if cmp(&items[j], &items[i]) == Ordering::Less {
            merged.push(items[j].clone());
            j += 1;
        } else {
            merged.push(items[i].clone());
            i += 1;
        }
    }
    merged.extend_from_slice(&items[i..mid]);
    merged.extend_from_slice(&items[j..n]);
    items.clone_from_slice(&merged);
}

/// One plan out of a list of plans and stray step objects (lang-core's flattenPlans), without
/// custom steps, which no call made here can produce.
fn flatten(values: &[V]) -> V {
    let mut steps = Vec::new();
    for v in values {
        match v {
            V::Plan(s) => steps.extend(s.iter().cloned()),
            V::Obj(o) if o.has("type") => steps.push(v.clone()),
            V::El(_) => steps.push(v.clone()),
            _ => {}
        }
    }
    own_steps(Arc::new(steps))
}

/// A plan without the custom steps data posed as (lang-core's ownSteps).
fn own_steps(steps: Arc<Vec<V>>) -> V {
    let custom = |s: &V| matches!(s, V::Obj(o) if o.get("type").and_then(V::as_str) == Some("custom_action"));
    if steps.iter().any(custom) {
        V::Plan(Arc::new(
            steps.iter().filter(|s| !custom(s)).cloned().collect(),
        ))
    } else {
        V::Plan(steps)
    }
}

/// A value as an expression, the way lang-core writes a loop item into a template.
fn literal(v: &V) -> A {
    match v {
        V::Undef | V::Null => Ast::Null.arc(),
        V::Str(s) => Ast::Str(s.clone()).arc(),
        V::Num(n) => Ast::Num(*n).arc(),
        V::Bool(b) => Ast::Bool(*b).arc(),
        V::Arr(items) => Ast::Arr(items.iter().map(literal).collect()).arc(),
        V::Obj(o) => Ast::Obj(o.iter().map(|(k, x)| (k.clone(), literal(x))).collect()).arc(),
        other => Ast::Lit(other.clone()).arc(),
    }
}

/// lang-core's substituteRef: the loop variable replaced by the item in the template. Each node
/// is a step of work: a template that shares subtrees can be far bigger than its text.
fn substitute(node: &A, var: &Arc<str>, value: &V, budget: &mut Budget) -> A {
    budget.charge(1);
    if budget.failed.is_some() {
        return node.clone();
    }
    let mut sub = |a: &A| substitute(a, var, value, budget);
    match &**node {
        Ast::Ref(n) if n == var => literal(value),
        Ast::Member { obj, field } => {
            let o = sub(obj);
            if let Ast::Obj(entries) = &*o
                && let Some((_, v)) = entries.iter().find(|(k, _)| k == field)
            {
                return v.clone();
            }
            Ast::Member {
                obj: o,
                field: field.clone(),
            }
            .arc()
        }
        Ast::Index { obj, index } => Ast::Index {
            obj: sub(obj),
            index: sub(index),
        }
        .arc(),
        Ast::BinOp { op, left, right } => Ast::BinOp {
            op: *op,
            left: sub(left),
            right: sub(right),
        }
        .arc(),
        Ast::Not(e) => Ast::Not(sub(e)).arc(),
        Ast::Neg(e) => Ast::Neg(sub(e)).arc(),
        Ast::Ternary { cond, then, other } => Ast::Ternary {
            cond: sub(cond),
            then: sub(then),
            other: sub(other),
        }
        .arc(),
        Ast::Arr(els) => Ast::Arr(els.iter().map(&mut sub).collect()).arc(),
        Ast::Obj(entries) => {
            Ast::Obj(entries.iter().map(|(k, v)| (k.clone(), sub(v))).collect()).arc()
        }
        Ast::Comp(c) => Ast::Comp(Comp {
            name: c.name.clone(),
            args: c.args.iter().map(&mut sub).collect(),
            mapped: c
                .mapped
                .as_ref()
                .map(|m| m.iter().map(|(k, v)| (k.clone(), sub(v))).collect()),
            func: c.func,
        })
        .arc(),
        Ast::Assign { target, value: v } => Ast::Assign {
            target: target.clone(),
            value: sub(v),
        }
        .arc(),
        _ => node.clone(),
    }
}
