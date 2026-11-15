//! OpenUI Lang's text to statements and expressions: lang-core 1.1.0's lexer, statement splitter
//! and Pratt parser, followed step by step (src/parser/lexer.ts, statements.ts, expressions.ts,
//! and the fence and comment stripping in parser.ts) so the same text reads the same way. The one
//! addition is a nesting limit: past MAX_DEPTH the parse stops and the card isn't shown, where
//! lang-core would recurse until the stack ran out.

use super::MAX_DEPTH;
use super::value::{Obj, V, js_space, js_trim, number};
use regex::Regex;
use std::sync::{Arc, LazyLock};

pub(crate) type A = Arc<Ast>;

/// The most nodes a walk over an expression visits, or writing one out as data writes.
const MAX_AST_NODES: usize = 100_000;

/// lang-core's ASTNode.
#[derive(Clone, Debug)]
pub(crate) enum Ast {
    Comp(Comp),
    Str(Arc<str>),
    Num(f64),
    Bool(bool),
    Null,
    Arr(Vec<A>),
    Obj(Vec<(Arc<str>, A)>),
    Ref(Arc<str>),
    Ph(Arc<str>),
    StateRef(Arc<str>),
    RuntimeRef {
        n: Arc<str>,
        mutation: bool,
    },
    BinOp {
        op: Op,
        left: A,
        right: A,
    },
    Not(A),
    Neg(A),
    Ternary {
        cond: A,
        then: A,
        other: A,
    },
    Member {
        obj: A,
        field: Arc<str>,
    },
    Index {
        obj: A,
        index: A,
    },
    Assign {
        target: Arc<str>,
        value: A,
    },
    /// A value put back into an expression where lang-core would write it as a literal: an
    /// @Each loop variable that holds a component or an action.
    Lit(V),
}

#[derive(Clone, Debug)]
pub(crate) struct Comp {
    pub name: Arc<str>,
    pub args: Vec<A>,
    /// The arguments by parameter name, once the parser knows the call (mappedProps).
    pub mapped: Option<Vec<(Arc<str>, A)>>,
    /// An @Name(...) call that isn't a built-in.
    pub func: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
    And,
    Or,
}

impl Op {
    fn text(self) -> &'static str {
        match self {
            Op::Add => "+",
            Op::Sub => "-",
            Op::Mul => "*",
            Op::Div => "/",
            Op::Rem => "%",
            Op::Eq => "==",
            Op::Ne => "!=",
            Op::Gt => ">",
            Op::Lt => "<",
            Op::Ge => ">=",
            Op::Le => "<=",
            Op::And => "&&",
            Op::Or => "||",
        }
    }
}

impl Ast {
    pub fn arc(self) -> A {
        Arc::new(self)
    }

    /// The node as lang-core's JSON (without mappedProps), for the golden tests and for a
    /// value that has to be read back as data. References share subtrees, so a small program
    /// can be a huge tree; past MAX_AST_NODES nodes the rest reads as null.
    pub fn to_value(&self) -> V {
        let mut room = MAX_AST_NODES;
        self.value_in(&mut room)
    }

    fn value_in(&self, room: &mut usize) -> V {
        if *room == 0 {
            return V::Null;
        }
        *room -= 1;
        let mut o = Obj::new();
        let node = |o: &mut Obj, k: &str| o.set("k", V::str(k));
        let list = |items: &[A], room: &mut usize| {
            V::arr(items.iter().map(|a| a.value_in(room)).collect())
        };
        match self {
            Ast::Comp(c) => {
                node(&mut o, "Comp");
                o.set("name", V::Str(c.name.clone()));
                o.set("args", list(&c.args, room));
                if c.func {
                    o.set("fn", V::Bool(true));
                }
            }
            Ast::Str(s) => {
                node(&mut o, "Str");
                o.set("v", V::Str(s.clone()));
            }
            Ast::Num(n) => {
                node(&mut o, "Num");
                o.set("v", V::Num(*n));
            }
            Ast::Bool(b) => {
                node(&mut o, "Bool");
                o.set("v", V::Bool(*b));
            }
            Ast::Null => node(&mut o, "Null"),
            Ast::Arr(els) => {
                node(&mut o, "Arr");
                o.set("els", list(els, room));
            }
            Ast::Obj(entries) => {
                node(&mut o, "Obj");
                o.set(
                    "entries",
                    V::arr(
                        entries
                            .iter()
                            .map(|(k, v)| V::arr(vec![V::Str(k.clone()), v.value_in(room)]))
                            .collect(),
                    ),
                );
            }
            Ast::Ref(n) | Ast::Ph(n) | Ast::StateRef(n) => {
                node(
                    &mut o,
                    if matches!(self, Ast::Ref(_)) {
                        "Ref"
                    } else if matches!(self, Ast::Ph(_)) {
                        "Ph"
                    } else {
                        "StateRef"
                    },
                );
                o.set("n", V::Str(n.clone()));
            }
            Ast::RuntimeRef { n, mutation } => {
                node(&mut o, "RuntimeRef");
                o.set("n", V::Str(n.clone()));
                o.set(
                    "refType",
                    V::str(if *mutation { "mutation" } else { "query" }),
                );
            }
            Ast::BinOp { op, left, right } => {
                node(&mut o, "BinOp");
                o.set("op", V::str(op.text()));
                o.set("left", left.value_in(room));
                o.set("right", right.value_in(room));
            }
            Ast::Not(e) | Ast::Neg(e) => {
                node(&mut o, "UnaryOp");
                o.set(
                    "op",
                    V::str(if matches!(self, Ast::Not(_)) {
                        "!"
                    } else {
                        "-"
                    }),
                );
                o.set("operand", e.value_in(room));
            }
            Ast::Ternary { cond, then, other } => {
                node(&mut o, "Ternary");
                o.set("cond", cond.value_in(room));
                o.set("then", then.value_in(room));
                o.set("else", other.value_in(room));
            }
            Ast::Member { obj, field } => {
                node(&mut o, "Member");
                o.set("obj", obj.value_in(room));
                o.set("field", V::Str(field.clone()));
            }
            Ast::Index { obj, index } => {
                node(&mut o, "Index");
                o.set("obj", obj.value_in(room));
                o.set("index", index.value_in(room));
            }
            Ast::Assign { target, value } => {
                node(&mut o, "Assign");
                o.set("target", V::Str(target.clone()));
                o.set("value", value.value_in(room));
            }
            Ast::Lit(v) => return v.clone(),
        }
        V::obj(o)
    }

    /// Every node of the tree, mappedProps included, as lang-core's walkAST visits them; false
    /// when it stopped after MAX_AST_NODES nodes (shared subtrees can make a huge tree).
    pub fn walk(&self, visit: &mut dyn FnMut(&Ast)) -> bool {
        let mut room = MAX_AST_NODES;
        self.walk_in(visit, &mut room)
    }

    fn walk_in(&self, visit: &mut dyn FnMut(&Ast), left: &mut usize) -> bool {
        if *left == 0 {
            return false;
        }
        *left -= 1;
        visit(self);
        let kids: Vec<&A> = match self {
            Ast::Comp(c) => c
                .args
                .iter()
                .chain(c.mapped.iter().flatten().map(|(_, a)| a))
                .collect(),
            Ast::Arr(els) => els.iter().collect(),
            Ast::Obj(entries) => entries.iter().map(|(_, a)| a).collect(),
            Ast::BinOp { left, right, .. } => vec![left, right],
            Ast::Not(e) | Ast::Neg(e) => vec![e],
            Ast::Ternary { cond, then, other } => vec![cond, then, other],
            Ast::Member { obj, .. } => vec![obj],
            Ast::Index { obj, index } => vec![obj, index],
            Ast::Assign { value, .. } => vec![value],
            _ => Vec::new(),
        };
        kids.into_iter().all(|k| k.walk_in(visit, left))
    }
}

/// The built-in functions, action steps and Action (lang-core's BUILTIN_CALLS): calls the
/// runtime evaluates rather than components.
pub(crate) const FUNCTIONS: &[&str] = &[
    "Count", "First", "Last", "Sum", "Avg", "Min", "Max", "Sort", "Filter", "Round", "Abs",
    "Floor", "Ceil", "Each",
];
pub(crate) const STEPS: &[&str] = &["Run", "ToAssistant", "OpenUrl", "Set", "Reset", "Action"];

pub(crate) fn is_builtin(name: &str) -> bool {
    FUNCTIONS.contains(&name) || STEPS.contains(&name)
}

pub(crate) fn is_reserved(name: &str) -> bool {
    name == "Query" || name == "Mutation"
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Newline,
    LParen,
    RParen,
    LBrack,
    RBrack,
    LBrace,
    RBrace,
    Comma,
    Colon,
    Eq,
    True,
    False,
    Null,
    Eof,
    Str(Arc<str>),
    Num(f64),
    Ident(Arc<str>),
    Type(Arc<str>),
    State(Arc<str>),
    Dot,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    EqEq,
    NotEq,
    Gt,
    Lt,
    Ge,
    Le,
    And,
    Or,
    Not,
    Question,
    At(Arc<str>),
}

fn word_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn word_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn tokenize(src: &str) -> Vec<Tok> {
    let s = src.as_bytes();
    let n = s.len();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < n {
        while i < n && matches!(s[i], b' ' | b'\t' | b'\r') {
            i += 1;
        }
        if i >= n {
            break;
        }
        let c = s[i];
        let next = s.get(i + 1).copied();
        let single = match c {
            b'\n' => Some(Tok::Newline),
            b'(' => Some(Tok::LParen),
            b')' => Some(Tok::RParen),
            b'[' => Some(Tok::LBrack),
            b']' => Some(Tok::RBrack),
            b'{' => Some(Tok::LBrace),
            b'}' => Some(Tok::RBrace),
            b',' => Some(Tok::Comma),
            b':' => Some(Tok::Colon),
            b'.' => Some(Tok::Dot),
            b'?' => Some(Tok::Question),
            b'+' => Some(Tok::Plus),
            b'*' => Some(Tok::Star),
            b'%' => Some(Tok::Percent),
            _ => None,
        };
        if let Some(t) = single {
            tokens.push(t);
            i += 1;
            continue;
        }
        let pair = |two: Tok, one: Tok, second: u8| {
            if next == Some(second) {
                (two, 2)
            } else {
                (one, 1)
            }
        };
        let op = match c {
            b'=' => Some(pair(Tok::EqEq, Tok::Eq, b'=')),
            b'!' => Some(pair(Tok::NotEq, Tok::Not, b'=')),
            b'>' => Some(pair(Tok::Ge, Tok::Gt, b'=')),
            b'<' => Some(pair(Tok::Le, Tok::Lt, b'=')),
            b'&' => Some(pair(Tok::And, Tok::And, b'&')),
            b'|' => Some(pair(Tok::Or, Tok::Or, b'|')),
            _ => None,
        };
        if let Some((t, len)) = op {
            tokens.push(t);
            i += len;
            continue;
        }
        if c == b'#' || (c == b'/' && next == Some(b'/')) {
            while i < n && s[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if c == b'/' {
            tokens.push(Tok::Slash);
            i += 1;
            continue;
        }
        if c == b'"' {
            let start = i;
            i += 1;
            let mut closed = false;
            while i < n {
                if s[i] == b'\\' {
                    i += 2;
                } else if s[i] == b'"' {
                    i += 1;
                    closed = true;
                    break;
                } else {
                    i += 1;
                }
            }
            let raw = &src[start..i.min(n)];
            let text = if closed {
                json_string(raw)
            } else {
                json_string(&format!("{raw}\""))
            };
            let text = text.unwrap_or_else(|| {
                let r = raw.strip_prefix('"').unwrap_or(raw);
                r.strip_suffix('"').unwrap_or(r).to_string()
            });
            tokens.push(Tok::Str(Arc::from(text)));
            continue;
        }
        if c == b'\'' {
            i += 1;
            let mut out = String::new();
            while i < n {
                if s[i] == b'\\' {
                    i += 1;
                    if let Some(esc) = src[i..].chars().next() {
                        match esc {
                            'n' => out.push('\n'),
                            't' => out.push('\t'),
                            e => out.push(e),
                        }
                        i += esc.len_utf8();
                    }
                } else if s[i] == b'\'' {
                    i += 1;
                    break;
                } else {
                    let ch = src[i..].chars().next().unwrap_or('\u{fffd}');
                    out.push(ch);
                    i += ch.len_utf8();
                }
            }
            tokens.push(Tok::Str(Arc::from(out)));
            continue;
        }
        let digit_next = next.is_some_and(|b| b.is_ascii_digit());
        if c == b'-' {
            let after_value = matches!(
                tokens.last(),
                Some(
                    Tok::Num(_)
                        | Tok::Str(_)
                        | Tok::Ident(_)
                        | Tok::Type(_)
                        | Tok::RParen
                        | Tok::RBrack
                        | Tok::True
                        | Tok::False
                        | Tok::Null
                        | Tok::State(_)
                        | Tok::At(_)
                )
            );
            if after_value || !digit_next {
                tokens.push(Tok::Minus);
                i += 1;
                continue;
            }
        }
        if c.is_ascii_digit() || (c == b'-' && digit_next) {
            let start = i;
            if s[i] == b'-' {
                i += 1;
            }
            while i < n && s[i].is_ascii_digit() {
                i += 1;
            }
            if i + 1 < n && s[i] == b'.' && s[i + 1].is_ascii_digit() {
                i += 1;
                while i < n && s[i].is_ascii_digit() {
                    i += 1;
                }
            }
            if i < n && (s[i] == b'e' || s[i] == b'E') {
                i += 1;
                if i < n && (s[i] == b'+' || s[i] == b'-') {
                    i += 1;
                }
                while i < n && s[i].is_ascii_digit() {
                    i += 1;
                }
            }
            tokens.push(Tok::Num(number(&src[start..i])));
            continue;
        }
        if c == b'$' && next.is_some_and(word_start) {
            let start = i;
            i += 1;
            while i < n && word_byte(s[i]) {
                i += 1;
            }
            tokens.push(Tok::State(Arc::from(&src[start..i])));
            continue;
        }
        if word_start(c) {
            let start = i;
            while i < n && word_byte(s[i]) {
                i += 1;
            }
            let word = &src[start..i];
            tokens.push(match word {
                "true" => Tok::True,
                "false" => Tok::False,
                "null" => Tok::Null,
                _ if c.is_ascii_uppercase() => Tok::Type(Arc::from(word)),
                _ => Tok::Ident(Arc::from(word)),
            });
            continue;
        }
        if c == b'@' && next.is_some_and(word_start) {
            i += 1;
            let start = i;
            while i < n && word_byte(s[i]) {
                i += 1;
            }
            tokens.push(Tok::At(Arc::from(&src[start..i])));
            continue;
        }
        i += 1;
    }
    tokens.push(Tok::Eof);
    tokens
}

/// JSON.parse of a double-quoted string literal, or None where JSON.parse would throw (a raw
/// control character, an unknown escape), and lang-core keeps the text as it is.
fn json_string(raw: &str) -> Option<String> {
    let body = raw.strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::with_capacity(body.len());
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return None,
            '\\' => match chars.next()? {
                '"' => out.push('"'),
                '\\' => out.push('\\'),
                '/' => out.push('/'),
                'b' => out.push('\u{8}'),
                'f' => out.push('\u{c}'),
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                't' => out.push('\t'),
                'u' => {
                    let mut unit = || -> Option<u16> {
                        let hex: String =
                            (0..4).map(|_| chars.next()).collect::<Option<String>>()?;
                        if hex.len() != 4 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                            return None;
                        }
                        u16::from_str_radix(&hex, 16).ok()
                    };
                    let first = unit()?;
                    if (0xd800..0xdc00).contains(&first) {
                        // A pair's second half has to follow straight away for one character;
                        // a lone half can't be held in a Rust string, so it reads as U+FFFD.
                        let rest = chars.as_str();
                        if let Some(hex) = rest.strip_prefix("\\u").and_then(|r| r.get(..4))
                            && hex.bytes().all(|b| b.is_ascii_hexdigit())
                            && let Ok(second) = u16::from_str_radix(hex, 16)
                            && (0xdc00..0xe000).contains(&second)
                        {
                            chars = rest[6..].chars();
                            out.extend(
                                char::decode_utf16([first, second])
                                    .map(|r| r.unwrap_or('\u{fffd}')),
                            );
                            continue;
                        }
                        out.push('\u{fffd}');
                    } else {
                        out.push(char::from_u32(u32::from(first)).unwrap_or('\u{fffd}'));
                    }
                }
                _ => return None,
            },
            c if (c as u32) < 0x20 => return None,
            c => out.push(c),
        }
    }
    Some(out)
}

/// Closes an unterminated string and any open brackets, as lang-core does for text cut short.
fn auto_close(input: &str) -> (String, bool) {
    let mut stack = Vec::new();
    let mut in_str: Option<char> = None;
    let mut esc = false;
    for c in input.chars() {
        if esc {
            esc = false;
            continue;
        }
        if c == '\\' && in_str.is_some() {
            esc = true;
            continue;
        }
        if let Some(q) = in_str {
            if c == q {
                in_str = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => in_str = Some(c),
            '(' | '[' | '{' => stack.push(c),
            ')' if stack.last() == Some(&'(') => {
                stack.pop();
            }
            ']' if stack.last() == Some(&'[') => {
                stack.pop();
            }
            '}' if stack.last() == Some(&'{') => {
                stack.pop();
            }
            _ => {}
        }
    }
    if in_str.is_none() && stack.is_empty() {
        return (input.to_string(), false);
    }
    let mut out = input.to_string();
    if let Some(q) = in_str {
        if esc {
            out.push('\\');
        }
        out.push(q);
    }
    for open in stack.iter().rev() {
        out.push(match open {
            '(' => ')',
            '[' => ']',
            _ => '}',
        });
    }
    (out, true)
}

/// One `name = expression` line (or more, inside brackets or a ternary).
pub(crate) struct RawStatement {
    pub id: Arc<str>,
    pub state: bool,
    tokens: Vec<Tok>,
}

fn split(tokens: Vec<Tok>) -> Vec<RawStatement> {
    let mut stmts = Vec::new();
    let mut pos = 0;
    let len = tokens.len();
    let skip_line = |pos: &mut usize| {
        while *pos < len && !matches!(tokens[*pos], Tok::Newline | Tok::Eof) {
            *pos += 1;
        }
    };
    while pos < len {
        while pos < len && tokens[pos] == Tok::Newline {
            pos += 1;
        }
        if pos >= len || tokens[pos] == Tok::Eof {
            break;
        }
        let (id, state) = match &tokens[pos] {
            Tok::Ident(v) | Tok::Type(v) => (v.clone(), false),
            Tok::State(v) => (v.clone(), true),
            _ => {
                skip_line(&mut pos);
                continue;
            }
        };
        pos += 1;
        if pos >= len || tokens[pos] != Tok::Eq {
            skip_line(&mut pos);
            continue;
        }
        pos += 1;
        let mut expr = Vec::new();
        let (mut depth, mut ternary) = (0i64, 0i64);
        while pos < len && tokens[pos] != Tok::Eof {
            let t = &tokens[pos];
            if *t == Tok::Newline && depth <= 0 && ternary <= 0 {
                let mut peek = pos + 1;
                while peek < len && tokens[peek] == Tok::Newline {
                    peek += 1;
                }
                let next = tokens.get(peek).unwrap_or(&Tok::Eof);
                if *next == Tok::Question || (*next == Tok::Colon && ternary > 0) {
                    pos += 1;
                    continue;
                }
                break;
            }
            if *t == Tok::Newline {
                pos += 1;
                continue;
            }
            match t {
                Tok::LParen | Tok::LBrack | Tok::LBrace => depth += 1,
                Tok::RParen | Tok::RBrack | Tok::RBrace if depth > 0 => depth -= 1,
                Tok::Question if depth == 0 => ternary += 1,
                Tok::Colon if depth == 0 && ternary > 0 => ternary -= 1,
                _ => {}
            }
            expr.push(t.clone());
            pos += 1;
        }
        if !expr.is_empty() {
            stmts.push(RawStatement {
                id,
                state,
                tokens: expr,
            });
        }
    }
    stmts
}

const PREC_TERNARY: u8 = 1;
const PREC_OR: u8 = 2;
const PREC_AND: u8 = 3;
const PREC_EQ: u8 = 4;
const PREC_CMP: u8 = 5;
const PREC_ADD: u8 = 6;
const PREC_MUL: u8 = 7;
const PREC_UNARY: u8 = 8;
const PREC_MEMBER: u8 = 9;

struct Pratt<'a> {
    tokens: &'a [Tok],
    pos: usize,
    depth: usize,
    too_deep: bool,
}

impl Pratt<'_> {
    fn cur(&self) -> &Tok {
        self.tokens.get(self.pos).unwrap_or(&Tok::Eof)
    }

    fn adv(&mut self) -> Tok {
        let t = self.cur().clone();
        self.pos += 1;
        t
    }

    fn eat(&mut self, t: &Tok) {
        if self.cur() == t {
            self.pos += 1;
        }
    }

    fn infix_prec(t: &Tok) -> u8 {
        match t {
            Tok::Question => PREC_TERNARY,
            Tok::Or => PREC_OR,
            Tok::And => PREC_AND,
            Tok::EqEq | Tok::NotEq => PREC_EQ,
            Tok::Gt | Tok::Lt | Tok::Ge | Tok::Le => PREC_CMP,
            Tok::Plus | Tok::Minus => PREC_ADD,
            Tok::Star | Tok::Slash | Tok::Percent => PREC_MUL,
            Tok::Dot | Tok::LBrack => PREC_MEMBER,
            _ => 0,
        }
    }

    fn expr(&mut self, min: u8) -> A {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            // Too deep to follow: stop here, and skip to the end so every loop above ends.
            self.too_deep = true;
            self.pos = self.tokens.len();
            self.depth -= 1;
            return Ast::Null.arc();
        }
        let mut left = self.prefix();
        // A chain like 1 + 1 + 1 or a.b.c is built here in a loop, not by recursion, but each
        // link nests the tree one deeper, and dropping or walking a tree recurses; so each link
        // counts as a level, and a long enough chain stops the parse like deep brackets do.
        let mut links = 0;
        while Self::infix_prec(self.cur()) > min {
            links += 1;
            self.depth += 1;
            if self.depth > MAX_DEPTH {
                self.too_deep = true;
                self.pos = self.tokens.len();
                break;
            }
            left = self.infix(left);
        }
        self.depth -= 1 + links;
        left
    }

    fn prefix(&mut self) -> A {
        let tok = self.cur().clone();
        let next_is_paren = self.tokens.get(self.pos + 1) == Some(&Tok::LParen);
        match tok {
            Tok::Str(v) => {
                self.adv();
                Ast::Str(v).arc()
            }
            Tok::Num(v) => {
                self.adv();
                Ast::Num(v).arc()
            }
            Tok::True | Tok::False => {
                self.adv();
                Ast::Bool(tok == Tok::True).arc()
            }
            Tok::Null => {
                self.adv();
                Ast::Null.arc()
            }
            Tok::LBrack => self.array(),
            Tok::LBrace => self.object(),
            Tok::State(name) => {
                self.adv();
                if *self.cur() == Tok::Eq {
                    self.adv();
                    let value = self.expr(0);
                    return Ast::Assign {
                        target: name,
                        value,
                    }
                    .arc();
                }
                Ast::StateRef(name).arc()
            }
            Tok::Type(name) => {
                if next_is_paren && (!is_builtin(&name) || &*name == "Action") {
                    return self.call(false);
                }
                self.adv();
                Ast::Ref(name).arc()
            }
            Tok::At(name) => {
                if next_is_paren {
                    return self.call(!is_builtin(&name));
                }
                self.adv();
                Ast::Ref(name).arc()
            }
            Tok::Ident(name) => {
                self.adv();
                Ast::Ref(name).arc()
            }
            Tok::Not => {
                self.adv();
                Ast::Not(self.expr(PREC_UNARY)).arc()
            }
            Tok::Minus => {
                self.adv();
                Ast::Neg(self.expr(PREC_UNARY)).arc()
            }
            Tok::LParen => {
                self.adv();
                let inner = self.expr(0);
                self.eat(&Tok::RParen);
                inner
            }
            _ => {
                self.adv();
                Ast::Null.arc()
            }
        }
    }

    fn infix(&mut self, left: A) -> A {
        let tok = self.cur().clone();
        let bin = |op, prec| (op, prec);
        let op = match tok {
            Tok::Plus => Some(bin(Op::Add, PREC_ADD)),
            Tok::Minus => Some(bin(Op::Sub, PREC_ADD)),
            Tok::Star => Some(bin(Op::Mul, PREC_MUL)),
            Tok::Slash => Some(bin(Op::Div, PREC_MUL)),
            Tok::Percent => Some(bin(Op::Rem, PREC_MUL)),
            Tok::EqEq => Some(bin(Op::Eq, PREC_EQ)),
            Tok::NotEq => Some(bin(Op::Ne, PREC_EQ)),
            Tok::Gt => Some(bin(Op::Gt, PREC_CMP)),
            Tok::Lt => Some(bin(Op::Lt, PREC_CMP)),
            Tok::Ge => Some(bin(Op::Ge, PREC_CMP)),
            Tok::Le => Some(bin(Op::Le, PREC_CMP)),
            Tok::And => Some(bin(Op::And, PREC_AND)),
            Tok::Or => Some(bin(Op::Or, PREC_OR)),
            _ => None,
        };
        if let Some((op, prec)) = op {
            self.adv();
            let right = self.expr(prec);
            return Ast::BinOp { op, left, right }.arc();
        }
        match tok {
            Tok::Question => {
                self.adv();
                let then = self.expr(0);
                self.eat(&Tok::Colon);
                let other = self.expr(0);
                Ast::Ternary {
                    cond: left,
                    then,
                    other,
                }
                .arc()
            }
            Tok::Dot => {
                self.adv();
                let field: Arc<str> = match self.adv() {
                    Tok::Ident(v) | Tok::Type(v) | Tok::Str(v) => v,
                    Tok::Num(n) => Arc::from(super::value::num_text(n)),
                    Tok::State(v) => Arc::from(v.trim_start_matches('$')),
                    _ => Arc::from("?"),
                };
                Ast::Member { obj: left, field }.arc()
            }
            Tok::LBrack => {
                self.adv();
                let index = self.expr(0);
                self.eat(&Tok::RBrack);
                Ast::Index { obj: left, index }.arc()
            }
            _ => left,
        }
    }

    fn call(&mut self, func: bool) -> A {
        let name = match self.adv() {
            Tok::Type(n) | Tok::At(n) => n,
            _ => Arc::from(""),
        };
        self.eat(&Tok::LParen);
        let mut args = Vec::new();
        while !matches!(self.cur(), Tok::RParen | Tok::Eof) {
            args.push(self.expr(0));
            if *self.cur() == Tok::Comma {
                self.adv();
            }
        }
        self.eat(&Tok::RParen);
        Ast::Comp(Comp {
            name,
            args,
            mapped: None,
            func,
        })
        .arc()
    }

    fn array(&mut self) -> A {
        self.adv();
        let mut els = Vec::new();
        while !matches!(self.cur(), Tok::RBrack | Tok::Eof) {
            els.push(self.expr(0));
            if *self.cur() == Tok::Comma {
                self.adv();
            }
        }
        self.eat(&Tok::RBrack);
        Ast::Arr(els).arc()
    }

    fn object(&mut self) -> A {
        self.adv();
        let mut entries = Vec::new();
        while !matches!(self.cur(), Tok::RBrace | Tok::Eof) {
            let key: Arc<str> = match self.adv() {
                Tok::Ident(v) | Tok::Str(v) | Tok::Type(v) => v,
                Tok::Num(n) => Arc::from(super::value::num_text(n)),
                Tok::State(v) => Arc::from(v.trim_start_matches('$')),
                _ => Arc::from("?"),
            };
            self.eat(&Tok::Colon);
            entries.push((key, self.expr(0)));
            if *self.cur() == Tok::Comma {
                self.adv();
            }
        }
        self.eat(&Tok::RBrace);
        Ast::Obj(entries).arc()
    }
}

/// A statement's expression, or None when it nests deeper than MAX_DEPTH.
pub(crate) fn expression(stmt: &RawStatement) -> Option<A> {
    let mut p = Pratt {
        tokens: &stmt.tokens,
        pos: 0,
        depth: 0,
        too_deep: false,
    };
    let e = p.expr(0);
    if p.too_deep { None } else { Some(e) }
}

/// The text after lang-core's preprocess (fences and comments stripped) and auto-closing, split
/// into statements; and whether the text was cut short.
pub(crate) fn statements(input: &str) -> (Vec<RawStatement>, bool) {
    let cleaned = preprocess(input);
    if cleaned.is_empty() {
        return (Vec::new(), true);
    }
    let (text, incomplete) = auto_close(&cleaned);
    (split(tokenize(&text)), incomplete)
}

/// /^\s*\$?[A-Za-z_]\w*\s*=(?!=)/: text that starts a statement.
fn statement_start(s: &str) -> bool {
    let t = s.trim_start_matches(js_space);
    let t = t.strip_prefix('$').unwrap_or(t);
    let b = t.as_bytes();
    if !b.first().is_some_and(|&c| word_start(c)) {
        return false;
    }
    let word = b.iter().take_while(|&&c| word_byte(c)).count();
    let rest = t[word..].trim_start_matches(js_space);
    rest.starts_with('=') && !rest.starts_with("==")
}

static FENCE_LINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[ \t]*```[ \t]*([A-Za-z0-9_.+\-]*)[ \t]*([^\n\r\u{2028}\u{2029}]*?)[ \t]*\r?$")
        .expect("fence pattern")
});

fn opener_code(tag: &str, rest: &str) -> Option<String> {
    if statement_start(rest) {
        return Some(rest.to_string());
    }
    if !tag.is_empty() && rest.starts_with('=') && !rest.starts_with("==") {
        return Some(format!("{tag} {rest}"));
    }
    None
}

struct Scan {
    open: bool,
    depth: usize,
}

fn scan_code(line: &str, st: &mut Scan) {
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if st.open {
            if c == b'\\' {
                i += 1;
            } else if c == b'"' {
                st.open = false;
            }
        } else if c == b'"' {
            st.open = true;
        } else if matches!(c, b'(' | b'[' | b'{') {
            st.depth += 1;
        } else if matches!(c, b')' | b']' | b'}') && st.depth > 0 {
            st.depth -= 1;
        }
        i += 1;
    }
}

/// Whether a block's first code line, past blank and comment lines, starts a statement.
fn starts_program(block: &str) -> bool {
    let mut rest = block;
    loop {
        let t = rest.trim_start_matches(js_space);
        if (t.starts_with("//") || t.starts_with('#'))
            && let Some(nl) = t.find('\n')
        {
            rest = &t[nl + 1..];
            continue;
        }
        break;
    }
    statement_start(rest)
}

/// The program out of Markdown code fences, or the input when no fence holds one.
fn strip_fences(input: &str) -> String {
    if !input.contains("```") {
        return input.to_string();
    }
    let mut blocks: Vec<String> = Vec::new();
    let mut outside: Vec<&str> = Vec::new();
    let mut body: Option<Vec<String>> = None;
    let mut script = false;
    let mut scan = Scan {
        open: false,
        depth: 0,
    };
    for line in input.split('\n') {
        let fence = if scan.open {
            None
        } else {
            FENCE_LINE.captures(line)
        };
        let Some(fence) = fence else {
            if script {
                continue;
            }
            match body.as_mut() {
                Some(b) => b.push(line.to_string()),
                None => outside.push(line),
            }
            if scan.open || scan.depth > 0 || body.is_some() || statement_start(line) {
                scan_code(line, &mut scan);
            }
            continue;
        };
        scan.depth = 0;
        let tag = fence.get(1).map_or("", |m| m.as_str());
        let rest = fence.get(2).map_or("", |m| m.as_str());
        if let Some(b) = body.take() {
            if !script {
                blocks.push(b.join("\n"));
            }
            script = false;
            continue;
        }
        if rest.contains("```") {
            continue;
        }
        let code = opener_code(tag, rest);
        script = tag.eq_ignore_ascii_case("openui-js");
        body = Some(code.into_iter().collect());
    }
    if let Some(b) = body
        && !script
    {
        blocks.push(b.join("\n"));
    }
    if blocks.iter().any(|b| starts_program(b)) {
        return blocks.join("\n");
    }
    if outside.iter().any(|l| statement_start(l)) || blocks.iter().all(|b| js_trim(b).is_empty()) {
        outside.join("\n")
    } else {
        blocks.join("\n")
    }
}

/// The text without // and # comments outside strings.
fn strip_comments(input: &str) -> String {
    let mut in_str: Option<u8> = None;
    let lines: Vec<String> = input
        .split('\n')
        .map(|line| {
            let b = line.as_bytes();
            let mut i = 0;
            while i < b.len() {
                let c = b[i];
                if let Some(q) = in_str {
                    if c == b'\\' && i + 1 < b.len() {
                        i += 2;
                        continue;
                    }
                    if c == q {
                        in_str = None;
                    }
                    i += 1;
                    continue;
                }
                if c == b'"' || c == b'\'' {
                    in_str = Some(c);
                    i += 1;
                    continue;
                }
                if (c == b'/' && b.get(i + 1) == Some(&b'/')) || c == b'#' {
                    return line[..i].trim_end_matches(js_space).to_string();
                }
                i += 1;
            }
            line.to_string()
        })
        .collect();
    lines.join("\n")
}

fn preprocess(input: &str) -> String {
    js_trim(&strip_comments(&strip_fences(js_trim(input)))).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(s: &str) -> Vec<Tok> {
        tokenize(s)
    }

    #[test]
    fn strings_read_as_json_parse_reads_them() {
        assert_eq!(tokens(r#""a\nb""#)[0], Tok::Str(Arc::from("a\nb")));
        assert_eq!(tokens(r#""😀""#)[0], Tok::Str(Arc::from("😀")));
        assert_eq!(tokens(r#""\ud83d""#)[0], Tok::Str(Arc::from("\u{fffd}")));
        // An escape JSON doesn't know keeps the text as it was, quotes off.
        assert_eq!(tokens(r#""a\qb""#)[0], Tok::Str(Arc::from(r"a\qb")));
        assert_eq!(tokens("'it\\'s'")[0], Tok::Str(Arc::from("it's")));
        assert_eq!(tokens("\"open")[0], Tok::Str(Arc::from("open")));
    }

    #[test]
    fn a_minus_after_a_value_is_subtraction() {
        assert_eq!(tokens("a -1")[1], Tok::Minus);
        assert_eq!(tokens("(-1")[1], Tok::Num(-1.0));
        assert!(matches!(tokens("1e")[0], Tok::Num(n) if n.is_nan()));
    }

    #[test]
    fn fences_and_comments_come_off() {
        assert_eq!(
            preprocess("Here:\n```openui\nroot = Stack([]) # shell\n```\nDone."),
            "root = Stack([])"
        );
        assert_eq!(
            preprocess("```\nroot = Stack([\"a // b\"])\n```"),
            "root = Stack([\"a // b\"])"
        );
        assert_eq!(
            preprocess("```openui root = Stack([])\n```"),
            "root = Stack([])"
        );
    }

    #[test]
    fn a_chain_past_the_limit_stops_the_parse() {
        for chain in [" + 1", ".a", "[0]", " == 1"] {
            let long = format!("root = x{}", chain.repeat(MAX_DEPTH + 5));
            let (stmts, _) = statements(&long);
            assert!(expression(&stmts[0]).is_none(), "{chain}");
            let short = format!("root = x{}", chain.repeat(20));
            let (stmts, _) = statements(&short);
            assert!(expression(&stmts[0]).is_some(), "{chain}");
        }
    }

    #[test]
    fn nesting_past_the_limit_stops_the_parse() {
        let deep = format!(
            "root = {}{}",
            "[".repeat(MAX_DEPTH + 5),
            "]".repeat(MAX_DEPTH + 5)
        );
        let (stmts, _) = statements(&deep);
        assert!(expression(&stmts[0]).is_none());
        let fine = format!("root = {}{}", "[".repeat(10), "]".repeat(10));
        let (stmts, _) = statements(&fine);
        assert!(expression(&stmts[0]).is_some());
    }
}
