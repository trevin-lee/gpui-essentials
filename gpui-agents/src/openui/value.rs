//! The values an OpenUI program computes with, as JavaScript has them, since lang-core is
//! JavaScript and a card has to read the same here as on the web: numbers are doubles printed
//! the way JavaScript prints them, objects keep JavaScript's key order, `==` is JavaScript's
//! loose equality, and undefined is not null (an undefined prop is left out, a null one isn't).

use super::syntax::Ast;
use std::sync::Arc;

#[derive(Clone, Debug, Default)]
pub(crate) enum V {
    #[default]
    Undef,
    Null,
    Bool(bool),
    Num(f64),
    Str(Arc<str>),
    Arr(Arc<Vec<V>>),
    Obj(Arc<Obj>),
    /// A component with its props, as the parser makes it (lang-core's ElementNode).
    El(Arc<El>),
    /// An expression the parser kept for evaluation (a prop that reads state, a step's value).
    Ast(Arc<Ast>),
    /// An evaluated action: its steps, each an object with a type.
    Plan(Arc<Vec<V>>),
    /// A two-way binding of a prop to a $variable (lang-core's ReactiveAssign).
    Bind(Arc<str>),
}

#[derive(Clone, Debug)]
pub(crate) struct El {
    pub name: Arc<str>,
    pub props: Obj,
    /// False when every prop is a literal, so evaluation can hand it back as it is.
    pub dynamic: bool,
}

/// An object's own properties in JavaScript's order: keys that are array indices first, in
/// numeric order, then the rest in the order they were added. A key added again keeps its place.
#[derive(Clone, Debug, Default)]
pub(crate) struct Obj(Vec<(Arc<str>, V)>);

fn index_key(k: &str) -> Option<u32> {
    if k.is_empty() || (k.len() > 1 && k.starts_with('0')) || !k.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    k.parse::<u32>().ok().filter(|&n| n != u32::MAX)
}

impl Obj {
    pub fn new() -> Obj {
        Obj(Vec::new())
    }

    pub fn get(&self, k: &str) -> Option<&V> {
        self.0.iter().find(|(key, _)| &**key == k).map(|(_, v)| v)
    }

    pub fn has(&self, k: &str) -> bool {
        self.get(k).is_some()
    }

    pub fn set(&mut self, k: impl Into<Arc<str>>, v: V) {
        let k = k.into();
        // __proto__ is never an own property of a plain object in lang-core's code paths that
        // matter here; dropping it keeps a program from smuggling one in.
        if &*k == "__proto__" {
            return;
        }
        if let Some(slot) = self.0.iter_mut().find(|(key, _)| *key == k) {
            slot.1 = v;
            return;
        }
        // Lookups are a scan, so an object stops taking new keys at MAX_KEYS.
        if self.0.len() >= super::MAX_KEYS {
            return;
        }
        match index_key(&k) {
            Some(n) => {
                let at = self
                    .0
                    .iter()
                    .position(|(key, _)| index_key(key).is_none_or(|m| m > n))
                    .unwrap_or(self.0.len());
                self.0.insert(at, (k, v));
            }
            None => self.0.push((k, v)),
        }
    }

    pub fn remove(&mut self, k: &str) {
        self.0.retain(|(key, _)| &**key != k);
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Arc<str>, &V)> {
        self.0.iter().map(|(k, v)| (k, v))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl V {
    pub fn str(s: &str) -> V {
        V::Str(Arc::from(s))
    }

    pub fn arr(v: Vec<V>) -> V {
        V::Arr(Arc::new(v))
    }

    pub fn obj(o: Obj) -> V {
        V::Obj(Arc::new(o))
    }

    pub fn is_nullish(&self) -> bool {
        matches!(self, V::Undef | V::Null)
    }

    /// typeof v === "object" && v !== null: everything that isn't a primitive.
    pub fn is_object(&self) -> bool {
        matches!(
            self,
            V::Arr(_) | V::Obj(_) | V::El(_) | V::Ast(_) | V::Plan(_) | V::Bind(_)
        )
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            V::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_arr(&self) -> Option<&[V]> {
        match self {
            V::Arr(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_el(&self) -> Option<&El> {
        match self {
            V::El(e) => Some(e),
            _ => None,
        }
    }

    pub fn truthy(&self) -> bool {
        match self {
            V::Undef | V::Null => false,
            V::Bool(b) => *b,
            V::Num(n) => !(*n == 0.0 || n.is_nan()),
            V::Str(s) => !s.is_empty(),
            _ => true,
        }
    }

    /// JavaScript's typeof, with "array" for arrays, as lang-core's messages say it.
    pub fn js_type(&self) -> &'static str {
        match self {
            V::Undef => "undefined",
            V::Null => "object",
            V::Bool(_) => "boolean",
            V::Num(_) => "number",
            V::Str(_) => "string",
            V::Arr(_) => "array",
            _ => "object",
        }
    }

    /// The property `field` of the value, as `value[field]` reads it (undefined when there's none).
    pub fn field(&self, field: &str) -> V {
        match self {
            V::Obj(o) => o.get(field).cloned().unwrap_or(V::Undef),
            V::Arr(a) => match field {
                "length" => V::Num(a.len() as f64),
                _ => index_key(field)
                    .and_then(|i| a.get(i as usize).cloned())
                    .unwrap_or(V::Undef),
            },
            V::Str(s) => match field {
                "length" => V::Num(s.encode_utf16().count() as f64),
                _ => index_key(field)
                    .map(|i| utf16_at(s, i as usize))
                    .unwrap_or(V::Undef),
            },
            V::El(e) => match field {
                "type" => V::str("element"),
                "typeName" => V::Str(e.name.clone()),
                "props" => V::obj(e.props.clone()),
                "partial" => V::Bool(false),
                _ => V::Undef,
            },
            V::Plan(steps) if field == "steps" => V::Arr(steps.clone()),
            _ => V::Undef,
        }
    }
}

/// The UTF-16 unit at i of s as a one-character string, as "abc"[i] is; a lone half of a pair
/// becomes the replacement character, which Rust strings can't hold otherwise.
fn utf16_at(s: &str, i: usize) -> V {
    match s.encode_utf16().nth(i) {
        Some(u) => V::Str(Arc::from(
            char::decode_utf16([u])
                .next()
                .and_then(Result::ok)
                .unwrap_or('\u{fffd}')
                .to_string(),
        )),
        None => V::Undef,
    }
}

/// Whether a value nests deeper than `limit` (lists in lists, objects in objects, components
/// in props). $variables' defaults are evaluated one after another, so `$b = [$a]`, `$c = [$b]`
/// and so on build a value as deep as the program is long without recursing, and dropping or
/// writing one out does recurse. Shared parts are looked at once each, by address.
pub(crate) fn nests_deeper(v: &V, limit: usize) -> bool {
    fn depth(
        v: &V,
        level: usize,
        limit: usize,
        seen: &mut std::collections::HashMap<usize, usize>,
    ) -> Result<usize, ()> {
        let (key, kids): (usize, Vec<&V>) = match v {
            V::Arr(a) | V::Plan(a) => (Arc::as_ptr(a) as *const () as usize, a.iter().collect()),
            V::Obj(o) => (
                Arc::as_ptr(o) as *const () as usize,
                o.iter().map(|(_, x)| x).collect(),
            ),
            V::El(e) => (
                Arc::as_ptr(e) as *const () as usize,
                e.props.iter().map(|(_, x)| x).collect(),
            ),
            _ => return Ok(0),
        };
        if level >= limit {
            return Err(());
        }
        let d = match seen.get(&key) {
            Some(&d) => d,
            None => {
                let mut d = 0;
                for k in kids {
                    d = d.max(depth(k, level + 1, limit, seen)?);
                }
                seen.insert(key, d + 1);
                d + 1
            }
        };
        if level + d > limit { Err(()) } else { Ok(d) }
    }
    depth(v, 0, limit, &mut std::collections::HashMap::new()).is_err()
}

/// The same value, as JavaScript's SameValueZero compares (Array.prototype.includes).
pub(crate) fn same_value_zero(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Num(x), V::Num(y)) => x == y || (x.is_nan() && y.is_nan()),
        _ => strict_eq(a, b),
    }
}

fn strict_eq(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Undef, V::Undef) | (V::Null, V::Null) => true,
        (V::Bool(x), V::Bool(y)) => x == y,
        (V::Num(x), V::Num(y)) => x == y,
        (V::Str(x), V::Str(y)) => x == y,
        (V::Arr(x), V::Arr(y)) => Arc::ptr_eq(x, y),
        (V::Obj(x), V::Obj(y)) => Arc::ptr_eq(x, y),
        (V::El(x), V::El(y)) => Arc::ptr_eq(x, y),
        (V::Plan(x), V::Plan(y)) => Arc::ptr_eq(x, y),
        _ => false,
    }
}

/// JavaScript's ==.
pub(crate) fn loose_eq(a: &V, b: &V) -> bool {
    match (a, b) {
        (x, y) if x.is_nullish() || y.is_nullish() => x.is_nullish() && y.is_nullish(),
        (V::Num(x), V::Str(s)) | (V::Str(s), V::Num(x)) => *x == number(s),
        (V::Bool(x), y) | (y, V::Bool(x)) => loose_eq(&V::Num(if *x { 1.0 } else { 0.0 }), y),
        (x, y) if x.is_object() && !y.is_object() => loose_eq(&V::str(&to_string(x)), y),
        (x, y) if y.is_object() && !x.is_object() => loose_eq(x, &V::str(&to_string(y))),
        (x, y) => strict_eq(x, y),
    }
}

/// lang-core's toNumber: a number, a string as Number() reads it (0 for one that isn't a
/// number), 1 or 0 for a boolean, 0 for anything else.
pub(crate) fn to_num(v: &V) -> f64 {
    match v {
        V::Num(n) => *n,
        V::Str(s) => {
            let n = number(s);
            if n.is_nan() { 0.0 } else { n }
        }
        V::Bool(b) => f64::from(u8::from(*b)),
        _ => 0.0,
    }
}

/// JavaScript's whitespace (\s, and what trim removes).
pub(crate) fn js_space(c: char) -> bool {
    c.is_whitespace() || c == '\u{feff}'
}

/// String.prototype.trim.
pub(crate) fn js_trim(s: &str) -> &str {
    s.trim_matches(js_space)
}

/// JavaScript's Number(string): NaN when the whole (trimmed) string isn't a number literal.
pub(crate) fn number(s: &str) -> f64 {
    let t = js_trim(s);
    if t.is_empty() {
        return 0.0;
    }
    for (prefix, radix) in [
        ("0x", 16),
        ("0X", 16),
        ("0o", 8),
        ("0O", 8),
        ("0b", 2),
        ("0B", 2),
    ] {
        if let Some(digits) = t.strip_prefix(prefix) {
            if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
                return f64::NAN;
            }
            return digits.chars().fold(0.0, |acc, c| {
                acc * f64::from(radix) + f64::from(c.to_digit(radix).unwrap_or(0))
            });
        }
    }
    match t {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    if decimal_literal(t) {
        t.parse::<f64>().unwrap_or(f64::NAN)
    } else {
        f64::NAN
    }
}

/// [+-]? (digits [. digits?] | . digits) ([eE] [+-]? digits)?, the decimal literal Number()
/// accepts; Rust's own parser also takes "inf" and "nan", which JavaScript doesn't.
fn decimal_literal(t: &str) -> bool {
    let b = t.as_bytes();
    let mut i = usize::from(matches!(b.first(), Some(b'+' | b'-')));
    let int = b[i..].iter().take_while(|c| c.is_ascii_digit()).count();
    i += int;
    let mut frac = 0;
    if b.get(i) == Some(&b'.') {
        i += 1;
        frac = b[i..].iter().take_while(|c| c.is_ascii_digit()).count();
        i += frac;
    }
    if int + frac == 0 {
        return false;
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(b.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        let exp = b[i..].iter().take_while(|c| c.is_ascii_digit()).count();
        if exp == 0 {
            return false;
        }
        i += exp;
    }
    i == b.len()
}

/// parseFloat: the longest number literal at the start of the (left-trimmed) string.
pub(crate) fn parse_float(s: &str) -> f64 {
    let t = s.trim_start_matches(js_space);
    for (lit, n) in [
        ("Infinity", f64::INFINITY),
        ("+Infinity", f64::INFINITY),
        ("-Infinity", f64::NEG_INFINITY),
    ] {
        if t.starts_with(lit) {
            return n;
        }
    }
    (1..=t.len())
        .rev()
        .filter(|&end| t.is_char_boundary(end))
        .map(|end| &t[..end])
        .find(|p| decimal_literal(p))
        .and_then(|p| p.parse().ok())
        .unwrap_or(f64::NAN)
}

/// Number.prototype.toString(): the shortest digits that read back as n, laid out the way
/// JavaScript does (1e+21, 0.000001, 1e-7, 1.5).
pub(crate) fn num_text(n: f64) -> String {
    if n.is_nan() {
        return "NaN".into();
    }
    if n.is_infinite() {
        return if n > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if n == 0.0 {
        return "0".into();
    }
    let sci = format!("{:e}", n.abs());
    let (mantissa, exp) = sci.split_once('e').unwrap_or((&sci, "0"));
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let k = digits.len() as i32;
    let point = exp.parse::<i32>().unwrap_or(0) + 1;
    let sign = if n < 0.0 { "-" } else { "" };
    let body = if k <= point && point <= 21 {
        format!("{digits}{}", "0".repeat((point - k) as usize))
    } else if 0 < point && point <= 21 {
        format!(
            "{}.{}",
            &digits[..point as usize],
            &digits[point as usize..]
        )
    } else if -6 < point && point <= 0 {
        format!("0.{}{digits}", "0".repeat((-point) as usize))
    } else {
        let e = point - 1;
        let e = if e < 0 {
            format!("-{}", -e)
        } else {
            format!("+{e}")
        };
        if k == 1 {
            format!("{digits}e{e}")
        } else {
            format!("{}.{}e{e}", &digits[..1], &digits[1..])
        }
    };
    format!("{sign}{body}")
}

/// String(v), with texts capped by `limit` characters so a program can't grow one without end.
pub(crate) fn to_string_capped(v: &V, limit: usize) -> String {
    let mut out = String::new();
    write_string(v, limit, &mut out, 0);
    out
}

pub(crate) fn to_string(v: &V) -> String {
    to_string_capped(v, super::MAX_EVAL_TEXT)
}

fn write_string(v: &V, limit: usize, out: &mut String, depth: usize) {
    if out.len() >= limit {
        return;
    }
    match v {
        V::Undef => out.push_str("undefined"),
        V::Null => out.push_str("null"),
        V::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        V::Num(n) => out.push_str(&num_text(*n)),
        V::Str(s) => out.push_str(s),
        // Array.prototype.join: null and undefined are empty, a nested array joins in place.
        V::Arr(a) if depth < super::MAX_DEPTH => {
            for (i, x) in a.iter().enumerate() {
                if out.len() >= limit {
                    break;
                }
                if i > 0 {
                    out.push(',');
                }
                if !x.is_nullish() {
                    write_string(x, limit, out, depth + 1);
                }
            }
        }
        V::Arr(_) => {}
        _ => out.push_str("[object Object]"),
    }
    if out.len() > limit {
        let mut end = limit;
        while !out.is_char_boundary(end) {
            end -= 1;
        }
        out.truncate(end);
    }
}

/// JSON.stringify(v, null, indent), as the web writes the context and form values it sends:
/// undefined and functions left out of objects, null in arrays; NaN and Infinity as null.
pub(crate) fn to_json(v: &V, indent: usize) -> String {
    let mut out = String::new();
    write_json(v, indent, 0, &mut out);
    out
}

fn write_json(v: &V, indent: usize, level: usize, out: &mut String) {
    // Values can share parts (a $variable used many times), so what's written stops at
    // MAX_JSON bytes rather than growing with the sharing.
    if out.len() > MAX_JSON {
        return;
    }
    let nl = |out: &mut String, level: usize| {
        if indent > 0 {
            out.push('\n');
            out.push_str(&" ".repeat(indent * level));
        }
    };
    match v {
        V::Undef | V::Null => out.push_str("null"),
        V::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        V::Num(n) if n.is_finite() => out.push_str(&num_text(*n)),
        V::Num(_) => out.push_str("null"),
        V::Str(s) => json_str(s, out),
        V::Plan(steps) => {
            let mut o = Obj::new();
            o.set("steps", V::Arr(steps.clone()));
            write_json(&V::obj(o), indent, level, out)
        }
        V::Arr(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (i, x) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                nl(out, level + 1);
                write_json(x, indent, level + 1, out);
            }
            nl(out, level);
            out.push(']');
        }
        _ => {
            let o = json_object(v);
            let entries: Vec<_> = o.iter().filter(|(_, x)| !matches!(x, V::Undef)).collect();
            if entries.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push('{');
            for (i, (k, x)) in entries.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                nl(out, level + 1);
                json_str(k, out);
                out.push(':');
                if indent > 0 {
                    out.push(' ');
                }
                write_json(x, indent, level + 1, out);
            }
            nl(out, level);
            out.push('}');
        }
    }
}

/// The value as the plain object JavaScript would serialize.
fn json_object(v: &V) -> Obj {
    match v {
        V::Obj(o) => (**o).clone(),
        V::El(e) => {
            let mut o = Obj::new();
            o.set("type", V::str("element"));
            o.set("typeName", V::Str(e.name.clone()));
            o.set("props", V::obj(e.props.clone()));
            o.set("partial", V::Bool(false));
            o
        }
        V::Ast(a) => match a.to_value() {
            V::Obj(o) => (*o).clone(),
            _ => Obj::new(),
        },
        V::Bind(target) => {
            let mut o = Obj::new();
            o.set("__reactive", V::str("assign"));
            o.set("target", V::Str(target.clone()));
            let mut expr = Obj::new();
            expr.set("k", V::str("StateRef"));
            expr.set("n", V::str("$value"));
            o.set("expr", V::obj(expr));
            o
        }
        _ => Obj::new(),
    }
}

/// A JSON string as JSON.stringify writes it: \uXXXX only for control characters.
fn json_str(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// The most bytes of JSON written for one value.
const MAX_JSON: usize = 256 * 1024;

/// The value as serde_json has it, for the golden tests and the public API's field values;
/// past 100,000 nodes (shared parts counted each time) the rest is null.
pub(crate) fn to_serde(v: &V) -> serde_json::Value {
    serde_in(v, &mut 100_000)
}

fn serde_in(v: &V, left: &mut usize) -> serde_json::Value {
    use serde_json::Value as J;
    if *left == 0 {
        return J::Null;
    }
    *left -= 1;
    match v {
        V::Undef | V::Null => J::Null,
        V::Bool(b) => J::Bool(*b),
        // A whole number reads back as one (2, not 2.0), as JSON from the web would.
        V::Num(n) if n.fract() == 0.0 && n.abs() < 9_007_199_254_740_992.0 => {
            J::Number((*n as i64).into())
        }
        V::Num(n) => serde_json::Number::from_f64(*n)
            .map(J::Number)
            .unwrap_or(J::Null),
        V::Str(s) => J::String(s.to_string()),
        V::Arr(a) => J::Array(a.iter().map(|x| serde_in(x, left)).collect()),
        V::Plan(steps) => {
            serde_json::json!({ "steps": steps.iter().map(|x| serde_in(x, left)).collect::<Vec<_>>() })
        }
        V::Obj(_) | V::El(_) | V::Ast(_) | V::Bind(_) => {
            let o = json_object(v);
            J::Object(
                o.iter()
                    .filter(|(_, x)| !matches!(x, V::Undef))
                    .map(|(k, x)| (k.to_string(), serde_in(x, left)))
                    .collect(),
            )
        }
    }
}

/// A serde_json value as a program's value (a field value the host hands in).
pub(crate) fn from_serde(j: &serde_json::Value) -> V {
    use serde_json::Value as J;
    match j {
        J::Null => V::Null,
        J::Bool(b) => V::Bool(*b),
        J::Number(n) => V::Num(n.as_f64().unwrap_or(0.0)),
        J::String(s) => V::str(s),
        J::Array(a) => V::arr(a.iter().map(from_serde).collect()),
        J::Object(m) => {
            let mut o = Obj::new();
            for (k, x) in m {
                o.set(k.as_str(), from_serde(x));
            }
            V::obj(o)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn depth_is_measured_once_per_shared_part() {
        let mut v = V::Num(1.0);
        for _ in 0..10 {
            v = V::arr(vec![v.clone(), v.clone(), v.clone(), v]);
        }
        assert!(!nests_deeper(&v, 10));
        assert!(nests_deeper(&v, 9));
        let mut deep = V::Null;
        for _ in 0..5_000 {
            deep = V::arr(vec![deep]);
        }
        assert!(nests_deeper(&deep, 160));
        // A value this deep can't be dropped by recursion on a small stack; unwind it by hand.
        while let V::Arr(a) = deep {
            deep = Arc::try_unwrap(a)
                .map(|mut items| items.pop().unwrap_or_default())
                .unwrap_or_default();
        }
    }

    #[test]
    fn numbers_print_as_javascript_prints_them() {
        for (n, s) in [
            (1.0, "1"),
            (1.5, "1.5"),
            (-0.25, "-0.25"),
            (100.0, "100"),
            (1e21, "1e+21"),
            (1.5e21, "1.5e+21"),
            (123456789012345680000.0, "123456789012345680000"),
            (0.000001, "0.000001"),
            (1e-7, "1e-7"),
            (2.5e-8, "2.5e-8"),
            (0.1 + 0.2, "0.30000000000000004"),
            (f64::NAN, "NaN"),
            (f64::NEG_INFINITY, "-Infinity"),
            (-0.0, "0"),
        ] {
            assert_eq!(num_text(n), s, "{n}");
        }
    }

    #[test]
    fn strings_read_as_number_reads_them() {
        for (s, n) in [
            ("12", 12.0),
            (" 12 ", 12.0),
            ("", 0.0),
            ("1e3", 1000.0),
            ("0x1A", 26.0),
            ("-Infinity", f64::NEG_INFINITY),
            (".5", 0.5),
            ("5.", 5.0),
        ] {
            assert_eq!(number(s), n, "{s:?}");
        }
        for s in [
            "12px",
            "inf",
            "nan",
            "1e",
            "--1",
            "0x",
            "1_000",
            "Infinityx",
        ] {
            assert!(number(s).is_nan(), "{s:?}");
        }
        assert_eq!(parse_float("12px"), 12.0);
        assert!(parse_float("px").is_nan());
    }

    #[test]
    fn loose_equality_is_javascripts() {
        let s = V::str;
        assert!(loose_eq(&V::Num(1.0), &s("1")));
        assert!(loose_eq(&V::Bool(true), &s("1")));
        assert!(loose_eq(&V::Null, &V::Undef));
        assert!(!loose_eq(&V::Null, &V::Num(0.0)));
        assert!(!loose_eq(&s(""), &V::Null));
        assert!(loose_eq(&s(""), &V::Num(0.0)));
        assert!(loose_eq(&V::arr(vec![V::Num(1.0), V::Num(2.0)]), &s("1,2")));
        assert!(!loose_eq(&V::Num(f64::NAN), &V::Num(f64::NAN)));
    }

    #[test]
    fn objects_keep_javascripts_key_order() {
        let mut o = Obj::new();
        for k in ["b", "2", "a", "1", "b", "01"] {
            o.set(k, V::Null);
        }
        let keys: Vec<_> = o.iter().map(|(k, _)| k.to_string()).collect();
        assert_eq!(keys, ["1", "2", "b", "a", "01"]);
        o.set("__proto__", V::Null);
        assert!(!o.has("__proto__"));
    }

    #[test]
    fn json_is_written_as_json_stringify_writes_it() {
        let mut o = Obj::new();
        o.set("a", V::Num(1.0));
        o.set("skip", V::Undef);
        o.set(
            "list",
            V::arr(vec![V::Undef, V::str("x\n\"y\""), V::Num(f64::NAN)]),
        );
        o.set("empty", V::obj(Obj::new()));
        assert_eq!(
            to_json(&V::obj(o.clone()), 0),
            r#"{"a":1,"list":[null,"x\n\"y\"",null],"empty":{}}"#
        );
        assert_eq!(
            to_json(&V::obj(o), 2),
            "{\n  \"a\": 1,\n  \"list\": [\n    null,\n    \"x\\n\\\"y\\\"\",\n    null\n  ],\n  \"empty\": {}\n}"
        );
    }
}
