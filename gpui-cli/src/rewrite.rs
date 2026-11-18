//! Turns one of gpui-core's component files into the host's copy.
//!
//! In code (found with Rust's lexer, so strings and comments that say `crate::` aren't code):
//! - `crate::base::…` becomes `gpui_core::base::…`, but `crate::base::init` and
//!   `crate::base::init_soon` become `super::init` and `super::init_soon`, the copies' own,
//!   which also bind the copies' keys;
//! - `crate::<component>::…` becomes `super::<component>::…`;
//! - each copied component's action namespace (`gpui_core_select`) becomes the host's
//!   (`ui_select`), and its key contexts (`"GpuiCoreSelect"`, in any string) the host's
//!   (`"UiSelect"`), so a copy and gpui-core's original can live in one app;
//! - `#[cfg(test)]` items go: they test gpui-core's version and need GPUI's test support.
//!
//! In comments the same paths and names change as text, and doc examples (which use gpui-core's
//! version, not the copy) are marked `ignore`.
//!
//! The text is edited in place, so the copy keeps its formatting and comments.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use proc_macro2::{Delimiter, Spacing, TokenStream, TokenTree};
use syn::spanned::Spanned;

/// What the rewrite renames, for every component being copied (a copy may name another's
/// contexts or actions).
pub struct Names {
    /// The components the host has (or is getting): their `crate::` paths become `super::`.
    pub components: BTreeSet<String>,
    /// `gpui_core_select` → `ui_select`.
    pub actions: BTreeMap<String, String>,
    /// `GpuiCoreSelect` → `UiSelect`.
    pub contexts: BTreeMap<String, String>,
}

impl Names {
    /// The names for copying `components` into the namespace `namespace` (`ui`).
    pub fn new<'a>(
        namespace: &str,
        components: impl IntoIterator<Item = (&'a String, &'a crate::registry::Component)>,
    ) -> Names {
        let mut names = Names {
            components: BTreeSet::new(),
            actions: BTreeMap::new(),
            contexts: BTreeMap::new(),
        };
        let camel = camel(namespace);
        for (name, c) in components {
            names.components.insert(name.clone());
            if let Some(a) = &c.actions {
                let rest = a.strip_prefix("gpui_core_").unwrap_or(a);
                names
                    .actions
                    .insert(a.clone(), format!("{namespace}_{rest}"));
            }
            for k in &c.contexts {
                let rest = k.strip_prefix("GpuiCore").unwrap_or(k);
                names.contexts.insert(k.clone(), format!("{camel}{rest}"));
            }
        }
        names
    }
}

/// `my_ui` → `MyUi`.
pub fn camel(namespace: &str) -> String {
    namespace
        .split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect::<String>())
                .unwrap_or_default()
        })
        .collect()
}

struct Edit {
    range: Range<usize>,
    text: String,
}

/// The copy of `source` (a component's file), or why it can't be made.
pub fn rewrite(file: &str, source: &str, names: &Names) -> Result<String, String> {
    let source = &without_tests(file, source)?;
    let mut edits = Vec::new();
    let stream: TokenStream = source.parse().map_err(|e| format!("{file}: {e:?}"))?;
    let mut leaves = Vec::new();
    flatten(stream, source, &mut leaves);
    code_edits(file, source, &leaves, names, &mut edits)?;

    // Comments: whatever text lies between the code's tokens.
    let mut at = 0;
    for leaf in &leaves {
        let r = leaf.range.clone();
        if r.start > at {
            comment_edits(source, at..r.start, names, &mut edits);
        }
        at = at.max(r.end);
    }
    if at < source.len() {
        comment_edits(source, at..source.len(), names, &mut edits);
    }

    let out = apply(file, source, edits)?;
    syn::parse_file(&out).map_err(|e| format!("{file}: the copy doesn't parse ({e})"))?;
    Ok(out)
}

/// `source` without its `#[cfg(test)]` items (and the blank lines before each).
fn without_tests(file: &str, source: &str) -> Result<String, String> {
    let parsed = syn::parse_file(source).map_err(|e| format!("{file}: {e}"))?;
    let mut edits = Vec::new();
    for item in &parsed.items {
        let attrs = match item {
            syn::Item::Mod(m) => &m.attrs,
            syn::Item::Fn(f) => &f.attrs,
            syn::Item::Use(u) => &u.attrs,
            syn::Item::Impl(i) => &i.attrs,
            syn::Item::Struct(s) => &s.attrs,
            syn::Item::Enum(e) => &e.attrs,
            syn::Item::Const(c) => &c.attrs,
            syn::Item::Static(s) => &s.attrs,
            _ => continue,
        };
        let is_test = attrs.iter().any(|a| {
            a.path().is_ident("cfg")
                && a.meta
                    .require_list()
                    .is_ok_and(|l| l.tokens.to_string() == "test")
        });
        if is_test {
            let r = item.span().byte_range();
            let mut start = r.start;
            while start > 0 && source.as_bytes()[start - 1].is_ascii_whitespace() {
                start -= 1;
            }
            edits.push(Edit {
                range: start..r.end,
                text: String::new(),
            });
        }
    }
    apply(file, source, edits)
}

/// `source` with `edits` made, which mustn't overlap.
fn apply(file: &str, source: &str, mut edits: Vec<Edit>) -> Result<String, String> {
    edits.sort_by_key(|e| std::cmp::Reverse(e.range.start));
    let mut out = source.to_string();
    let mut last = usize::MAX;
    for e in edits {
        if e.range.end > last {
            return Err(format!(
                "{file}: overlapping rewrites at byte {}",
                e.range.start
            ));
        }
        out.replace_range(e.range.clone(), &e.text);
        last = e.range.start;
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    Ok(out)
}

/// A token of code: an identifier, a punctuation mark or a literal, with its place.
struct Leaf {
    tree: TokenTree,
    range: Range<usize>,
}

/// Every leaf token in order. Doc comments (which the lexer turns into `#[doc = "…"]`, all of
/// it spanning the comment) are left out, so they read as comments.
fn flatten(stream: TokenStream, source: &str, out: &mut Vec<Leaf>) {
    let mut skip_doc_group = false;
    for tree in stream {
        let range = tree.span().byte_range();
        let is_comment =
            source[range.clone()].starts_with("//") || source[range.clone()].starts_with("/*");
        match &tree {
            TokenTree::Group(g) => {
                if std::mem::take(&mut skip_doc_group) || is_comment {
                    continue;
                }
                let open = g.span_open().byte_range();
                let close = g.span_close().byte_range();
                if g.delimiter() != Delimiter::None {
                    out.push(Leaf {
                        tree: tree.clone(),
                        range: open,
                    });
                }
                flatten(g.stream(), source, out);
                if g.delimiter() != Delimiter::None {
                    out.push(Leaf { tree, range: close });
                }
            }
            _ if is_comment => {
                // The `#` (and an inner one's `!`) of a doc comment: its bracket group follows.
                if matches!(&tree, TokenTree::Punct(p) if p.as_char() == '#') {
                    skip_doc_group = true;
                }
            }
            _ => out.push(Leaf { tree, range }),
        }
    }
}

fn ident(leaf: Option<&Leaf>) -> Option<String> {
    match leaf.map(|l| &l.tree) {
        Some(TokenTree::Ident(i)) => Some(i.to_string()),
        _ => None,
    }
}

/// Whether `leaves[i..]` starts with `::`.
fn path_sep(leaves: &[Leaf], i: usize) -> bool {
    matches!(
        (leaves.get(i).map(|l| &l.tree), leaves.get(i + 1).map(|l| &l.tree)),
        (Some(TokenTree::Punct(a)), Some(TokenTree::Punct(b)))
            if a.as_char() == ':' && a.spacing() == Spacing::Joint && b.as_char() == ':'
    )
}

fn code_edits(
    file: &str,
    source: &str,
    leaves: &[Leaf],
    names: &Names,
    edits: &mut Vec<Edit>,
) -> Result<(), String> {
    for (i, leaf) in leaves.iter().enumerate() {
        match &leaf.tree {
            TokenTree::Ident(id) if id == "crate" => {
                let after_dollar = i > 0
                    && matches!(&leaves[i - 1].tree, TokenTree::Punct(p) if p.as_char() == '$');
                if after_dollar || !path_sep(leaves, i + 1) {
                    continue;
                }
                let Some(first) = ident(leaves.get(i + 3)) else {
                    let line = source[..leaf.range.start].matches('\n').count() + 1;
                    return Err(format!(
                        "{file}:{line}: a `crate::{{…}}` group: gpui-core's components name \
                         `crate::base::…` and `crate::<component>::…` paths one by one"
                    ));
                };
                if first == "base" {
                    let own = path_sep(leaves, i + 4)
                        .then(|| ident(leaves.get(i + 6)))
                        .flatten()
                        .filter(|n| n == "init" || n == "init_soon");
                    if own.is_some() {
                        // `crate::base::init_soon` → `super::init_soon`.
                        edits.push(Edit {
                            range: leaf.range.start..leaves[i + 5].range.end,
                            text: "super::".into(),
                        });
                    } else {
                        edits.push(Edit {
                            range: leaf.range.clone(),
                            text: "gpui_core".into(),
                        });
                    }
                } else if names.components.contains(&first) {
                    edits.push(Edit {
                        range: leaf.range.clone(),
                        text: "super".into(),
                    });
                } else {
                    let line = source[..leaf.range.start].matches('\n').count() + 1;
                    return Err(format!(
                        "{file}:{line}: `crate::{first}` is neither gpui-core's base nor a \
                         component being copied (is components.toml out of date?)"
                    ));
                }
            }
            TokenTree::Ident(id) => {
                if let Some(to) = names.actions.get(&id.to_string()) {
                    edits.push(Edit {
                        range: leaf.range.clone(),
                        text: to.clone(),
                    });
                }
            }
            TokenTree::Literal(_) => {
                let text = &source[leaf.range.clone()];
                if text.starts_with('"') || text.starts_with('r') {
                    let renamed = rename_words(text, names);
                    if renamed != text {
                        edits.push(Edit {
                            range: leaf.range.clone(),
                            text: renamed,
                        });
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Renames the contexts and action namespaces in `text`, whole words only.
fn rename_words(text: &str, names: &Names) -> String {
    let mut out = String::with_capacity(text.len());
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        if let Some(to) = names.contexts.get(word.as_str()) {
            out.push_str(to);
        } else if let Some(to) = names.actions.get(word.as_str()) {
            out.push_str(to);
        } else {
            out.push_str(word);
        }
        word.clear();
    };
    for c in text.chars() {
        if c.is_alphanumeric() || c == '_' {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}

/// Rewrites the comments in `range` (text outside code): paths, names, and doc examples'
/// fences.
fn comment_edits(source: &str, range: Range<usize>, names: &Names, edits: &mut Vec<Edit>) {
    let text = &source[range.clone()];
    if !text.contains("//") && !text.contains("/*") {
        return;
    }
    let mut out = String::with_capacity(text.len());
    let mut in_example = false;
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let doc = ["//!", "///"].iter().find(|p| trimmed.starts_with(**p));
        if let Some(prefix) = doc {
            let body = trimmed[prefix.len()..].trim();
            if let Some(info) = body.strip_prefix("```") {
                if in_example {
                    in_example = false;
                } else {
                    in_example = true;
                    let rust = info.is_empty()
                        || info
                            .split(',')
                            .all(|w| matches!(w.trim(), "rust" | "no_run" | "should_panic"));
                    if rust {
                        // The example uses gpui-core's component, not this copy.
                        let at = line.find("```").unwrap() + 3;
                        out.push_str(&line[..at]);
                        out.push_str("ignore");
                        out.push_str(&line[at + info.trim_end().len()..]);
                        continue;
                    }
                }
            }
        }
        out.push_str(&rewrite_comment_paths(line, names));
    }
    let out = rename_words(&out, names);
    if out != text {
        edits.push(Edit { range, text: out });
    }
}

/// `crate::base::X` → `gpui_core::base::X` and `crate::<component>` → `super::<component>` in
/// comment text (intra-doc links resolve from the copy's module).
fn rewrite_comment_paths(line: &str, names: &Names) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(at) = rest.find("crate::") {
        let before = &rest[..at];
        out.push_str(before);
        let after = &rest[at + "crate::".len()..];
        let glued = before
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$');
        let first: String = after
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if glued {
            out.push_str("crate::");
        } else if first == "base" {
            let tail = &after["base".len()..];
            if let Some(own) = ["::init_soon", "::init"]
                .iter()
                .find(|p| tail.starts_with(**p) && !tail[p.len()..].starts_with('_'))
            {
                out.push_str("super");
                out.push_str(own);
                rest = &tail[own.len()..];
                continue;
            }
            out.push_str("gpui_core::");
        } else if names.components.contains(&first) {
            out.push_str("super::");
        } else {
            out.push_str("crate::");
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Component;

    fn names() -> Names {
        let mut comps = BTreeMap::new();
        comps.insert(
            "select".to_string(),
            Component {
                actions: Some("gpui_core_select".into()),
                contexts: vec!["GpuiCoreSelect".into(), "GpuiCoreSelectMenu".into()],
                ..Default::default()
            },
        );
        comps.insert("button".to_string(), Component::default());
        Names::new("ui", comps.iter())
    }

    const SOURCE: &str = r#"//! A select. It isn't searchable: that's [`Picker`](crate::base::Picker), and
//! [`Button`](crate::button::Button) is its trigger.
//!
//! ```no_run
//! use gpui_core::Select;
//! ```

use gpui::{App, actions};

use crate::base::{ActiveTheme as _, Icon};
use crate::button::Button;

const CONTEXT: &str = "GpuiCoreSelect";
const MENU: &str = "GpuiCoreSelectMenu > Item";
const OTHER: &str = "GpuiCoreSelectish";

actions!(gpui_core_select, [Open, Close]);

/// Binds the keys. See `crate::base::init_soon`.
pub(crate) fn init(cx: &mut App) {
    // A comment's paths change (crate::button), a string's don't.
    let _ = "crate::button";
    cx.bind_keys([gpui::KeyBinding::new("down", Open, Some(CONTEXT))]);
}

fn draw(cx: &mut App) {
    crate::base::init_soon(cx);
    crate::base::init(cx);
    let _ = crate::base::Theme::default();
}

#[cfg(test)]
mod tests {
    use crate::base::Root;
}
"#;

    #[test]
    fn rewrites_paths_names_and_examples() {
        let out = rewrite("src/select.rs", SOURCE, &names()).unwrap();
        let want = r#"//! A select. It isn't searchable: that's [`Picker`](gpui_core::base::Picker), and
//! [`Button`](super::button::Button) is its trigger.
//!
//! ```ignore
//! use gpui_core::Select;
//! ```

use gpui::{App, actions};

use gpui_core::base::{ActiveTheme as _, Icon};
use super::button::Button;

const CONTEXT: &str = "UiSelect";
const MENU: &str = "UiSelectMenu > Item";
const OTHER: &str = "GpuiCoreSelectish";

actions!(ui_select, [Open, Close]);

/// Binds the keys. See `super::init_soon`.
pub(crate) fn init(cx: &mut App) {
    // A comment's paths change (super::button), a string's don't.
    let _ = "crate::button";
    cx.bind_keys([gpui::KeyBinding::new("down", Open, Some(CONTEXT))]);
}

fn draw(cx: &mut App) {
    super::init_soon(cx);
    super::init(cx);
    let _ = gpui_core::base::Theme::default();
}
"#;
        assert_eq!(out, want, "\n{out}");
    }

    #[test]
    fn unknown_crate_paths_are_errors() {
        let err = rewrite(
            "src/x.rs",
            "fn f() { crate::dialog::Dialog::new(); }",
            &names(),
        )
        .unwrap_err();
        assert!(err.contains("crate::dialog"), "{err}");
    }

    #[test]
    fn rewriting_twice_changes_nothing_more() {
        let once = rewrite("src/select.rs", SOURCE, &names()).unwrap();
        assert_eq!(rewrite("src/select.rs", &once, &names()).unwrap(), once);
    }

    #[test]
    fn camel_case() {
        assert_eq!(camel("ui"), "Ui");
        assert_eq!(camel("my_ui"), "MyUi");
    }
}
