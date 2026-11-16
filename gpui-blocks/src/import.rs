//! Notes from other apps, as documents: Markdown exported from Notion, and Obsidian vaults.
//!
//! An import is many files that link to each other by path or by name. Only the app knows which
//! document each file becomes, so it goes in two passes: first it gives every note a document
//! id, then it reads each file with [`note`], which returns the note's title and its
//! [`Document`] with every link to another note pointing at that note's document. The app
//! answers [`Targets`] from the ids it gave out.
//!
//! What's taken care of:
//! - **Titles:** a note's title is the heading it opens with (taken out of the text, as Notion
//!   starts every page with its name), else front matter's `title`, else the file's name
//!   without Notion's page id.
//! - **Links between notes:** `[text](Other%20Page%200123….md)` (Notion's, URL-encoded),
//!   relative paths, reference definitions, and Obsidian's `[[Note]]`, `[[Note|text]]` and
//!   `[[folder/Note#Heading]]` become mentions of the other document. A note embedded on a line
//!   of its own (`![[Note]]`) becomes a sub-page. A link to a note that wasn't imported is
//!   left as its text.
//! - **Pictures and other files** in the import load from where the app says
//!   ([`Targets::file`]), or are left as their description.
//! - **Callouts:** Obsidian's (`> [!tip] Title`, with any of its names) become GitHub's five,
//!   with the title in bold; a foldable one (`[!note]-`) becomes a toggle. Notion's `<aside>`
//!   becomes a note callout.
//! - **Highlights:** Obsidian's `==text==` becomes `<mark>text</mark>`.
//! - Code is left as it is, and a byte order mark and Windows line endings are dropped.
//!
//! Databases (Notion's CSV files) aren't documents; read them with a table crate.

use std::ops::Range;

use markdown::mdast::Node;
use markdown::{ParseOptions, to_mdast};

use crate::docs;
use crate::json::Document;

/// What the app made of the import's files.
pub trait Targets {
    /// The document the note at `path` became. Paths are relative to the import, with `/`
    /// between folders, as in the zip or folder it came from.
    fn note(&self, path: &str) -> Option<String>;

    /// The document the note Obsidian calls `name` became: a file name without `.md`, maybe with
    /// folders in front (`Projects/Plan`), found wherever it is in the vault.
    fn named(&self, name: &str) -> Option<String>;

    /// Where the file at `path` (a picture, a PDF) can be loaded from now, as a document link
    /// or an address, or `None` to leave its description. `path` is a path in the import, or
    /// for Obsidian a bare file name.
    fn file(&self, path: &str) -> Option<String>;
}

/// An imported note.
#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub title: String,
    pub document: Document,
}

/// Reads the note at `path` in the import (see the module's docs).
pub fn note(path: &str, text: &str, targets: &impl Targets) -> Note {
    let text = text
        .strip_prefix('\u{feff}')
        .unwrap_or(text)
        .replace("\r\n", "\n");
    let (front_matter, body) = crate::editor::split_front_matter(&text);
    let body = asides(body);
    let body = callouts(&body);
    let body = outside_code(&body, |prose| wiki_links(&highlights(prose), targets));
    let body = links(&body, path, targets);
    let (heading, body) = opening_heading(&body);
    let title = heading
        .or_else(|| front_matter_title(&front_matter))
        .unwrap_or_else(|| title_from_path(path));
    let markdown = if front_matter.is_empty() {
        body.to_string()
    } else {
        format!("{front_matter}\n{body}")
    };
    Note {
        title,
        document: Document::from_markdown(&markdown, &[]),
    }
}

/// A file's name as a title: no folders, no extension, and no Notion page id (a space and 32
/// hex digits at the end).
pub fn title_from_path(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    let name = match name.rfind('.') {
        Some(dot) if dot > 0 => &name[..dot],
        _ => name,
    };
    clean_name(name).to_string()
}

/// A Notion name without its page id.
pub fn clean_name(name: &str) -> &str {
    let bytes = name.as_bytes();
    if bytes.len() > 33 {
        let (head, id) = name.split_at(name.len() - 32);
        if head.ends_with(' ') && id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return head.trim_end();
        }
    }
    name
}

fn front_matter_title(front: &str) -> Option<String> {
    front.lines().find_map(|line| {
        let value = line.strip_prefix("title:")?.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
            .unwrap_or(value);
        (!value.is_empty() && !matches!(value, "|" | ">")).then(|| value.to_string())
    })
}

/// The `# Title` (or `Title` underlined with `===`) the body opens with, and the body without
/// it.
fn opening_heading(body: &str) -> (Option<String>, &str) {
    let Ok(tree) = to_mdast(body, &ParseOptions::gfm()) else {
        return (None, body);
    };
    let Some(Node::Heading(h)) = tree.children().and_then(|c| c.first()) else {
        return (None, body);
    };
    if h.depth != 1 {
        return (None, body);
    }
    let Some(end) = h.position.as_ref().map(|p| p.end.offset) else {
        return (None, body);
    };
    let title = plain_text(&h.children);
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    (
        (!title.is_empty()).then_some(title),
        body[end..].trim_start_matches('\n'),
    )
}

fn plain_text(nodes: &[Node]) -> String {
    let mut out = String::new();
    for node in nodes {
        match node {
            Node::Text(t) => out.push_str(&t.value),
            Node::InlineCode(c) => out.push_str(&c.value),
            other => {
                if let Some(children) = other.children() {
                    out.push_str(&plain_text(children));
                }
            }
        }
    }
    out
}

// Line passes, outside fenced code.

/// Whether each line is inside fenced code (fences included).
fn fenced(lines: &[&str]) -> Vec<bool> {
    let mut open: Option<(char, usize)> = None;
    lines
        .iter()
        .map(|line| {
            let t = line.trim_start();
            let indent = line.len() - t.len();
            let fence = t.chars().next().filter(|c| matches!(c, '`' | '~'));
            let run = fence.map_or(0, |c| t.chars().take_while(|x| *x == c).count());
            match (open, fence) {
                (None, Some(c)) if indent < 4 && run >= 3 => {
                    open = Some((c, run));
                    true
                }
                (Some((c, n)), Some(f))
                    if f == c && run >= n && t[run * c.len_utf8()..].trim().is_empty() =>
                {
                    open = None;
                    true
                }
                (Some(_), _) => true,
                _ => false,
            }
        })
        .collect()
}

/// Notion's `<aside>` callouts as note callouts.
fn asides(body: &str) -> String {
    let lines: Vec<&str> = body.split('\n').collect();
    let code = fenced(&lines);
    let mut out: Vec<String> = Vec::new();
    let mut ix = 0;
    while ix < lines.len() {
        if !code[ix] && lines[ix].trim() == "<aside>" {
            let end = (ix + 1..lines.len()).find(|&j| !code[j] && lines[j].trim() == "</aside>");
            if let Some(end) = end {
                let inner: Vec<&str> = lines[ix + 1..end].to_vec();
                let inner = trim_blank(&inner);
                out.push("> [!NOTE]".into());
                out.extend(inner.iter().map(|l| quote_line(l)));
                ix = end + 1;
                continue;
            }
        }
        out.push(lines[ix].to_string());
        ix += 1;
    }
    out.join("\n")
}

fn trim_blank<'a>(lines: &'a [&'a str]) -> &'a [&'a str] {
    let start = lines
        .iter()
        .position(|l| !l.trim().is_empty())
        .unwrap_or(lines.len());
    let end = lines
        .iter()
        .rposition(|l| !l.trim().is_empty())
        .map_or(start, |e| e + 1);
    &lines[start..end]
}

fn quote_line(line: &str) -> String {
    if line.is_empty() {
        ">".into()
    } else {
        format!("> {line}")
    }
}

/// GitHub's alert for one of Obsidian's callout names, or `None` for a plain quote.
fn alert(kind: &str) -> Option<&'static str> {
    Some(match kind.to_ascii_lowercase().as_str() {
        "tip" | "hint" | "success" | "check" | "done" => "TIP",
        "important" => "IMPORTANT",
        "warning" | "attention" => "WARNING",
        "caution" | "danger" | "error" | "failure" | "fail" | "missing" | "bug" => "CAUTION",
        "quote" | "cite" => return None,
        _ => "NOTE",
    })
}

/// Obsidian's callouts (`> [!kind]± Title` at the start of a quote) as GitHub's alerts, or as
/// toggles when they fold.
fn callouts(body: &str) -> String {
    let lines: Vec<&str> = body.split('\n').collect();
    let code = fenced(&lines);
    let mut out: Vec<String> = Vec::new();
    let mut ix = 0;
    while ix < lines.len() {
        let head = (!code[ix]).then(|| callout_head(lines[ix])).flatten();
        let Some((kind, fold, title)) = head else {
            out.push(lines[ix].to_string());
            ix += 1;
            continue;
        };
        let mut end = ix + 1;
        while end < lines.len() && lines[end].starts_with('>') {
            end += 1;
        }
        let body: Vec<String> = lines[ix + 1..end]
            .iter()
            .map(|l| {
                let l = &l[1..];
                l.strip_prefix(' ').unwrap_or(l).to_string()
            })
            .collect();
        let shown = if title.is_empty() {
            let mut k = kind.to_string();
            if let Some(first) = k.get_mut(..1) {
                first.make_ascii_uppercase();
            }
            k
        } else {
            title.to_string()
        };
        match fold {
            Some(open) => {
                out.push(if open { "<details open>" } else { "<details>" }.into());
                out.push(format!("<summary>{shown}</summary>"));
                out.push(String::new());
                out.extend(body);
                out.push(String::new());
                out.push("</details>".into());
            }
            None => {
                // A quote callout is a plain quote.
                if let Some(name) = alert(kind) {
                    out.push(format!("> [!{name}]"));
                }
                if !title.is_empty() {
                    out.push(format!("> **{title}**"));
                    // Its own paragraph, not the start of the first line's.
                    if body.iter().any(|l| !l.trim().is_empty()) {
                        out.push(">".into());
                    }
                }
                out.extend(body.iter().map(|l| quote_line(l)));
            }
        }
        ix = end;
    }
    out.join("\n")
}

/// `> [!kind]± title` read: the kind, whether it folds (and starts open), and the title.
fn callout_head(line: &str) -> Option<(&str, Option<bool>, &str)> {
    let rest = line.strip_prefix('>')?.trim_start().strip_prefix("[!")?;
    let close = rest.find(']')?;
    let kind = &rest[..close];
    if kind.is_empty()
        || !kind
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return None;
    }
    let rest = &rest[close + 1..];
    let (fold, rest) = match rest.chars().next() {
        Some('+') => (Some(true), &rest[1..]),
        Some('-') => (Some(false), &rest[1..]),
        _ => (None, rest),
    };
    Some((kind, fold, rest.trim()))
}

// Inline passes, outside code.

/// Runs `change` over the text outside fenced code and inline code spans.
fn outside_code(body: &str, mut change: impl FnMut(&str) -> String) -> String {
    let lines: Vec<&str> = body.split('\n').collect();
    let code = fenced(&lines);
    let mut out = String::with_capacity(body.len());
    let mut prose = String::new();
    let flush = |prose: &mut String, out: &mut String, change: &mut dyn FnMut(&str) -> String| {
        if !prose.is_empty() {
            out.push_str(&outside_inline_code(prose, change));
            prose.clear();
        }
    };
    for (ix, line) in lines.iter().enumerate() {
        let piece = if ix + 1 < lines.len() {
            format!("{line}\n")
        } else {
            line.to_string()
        };
        if code[ix] {
            flush(&mut prose, &mut out, &mut change);
            out.push_str(&piece);
        } else {
            prose.push_str(&piece);
        }
    }
    flush(&mut prose, &mut out, &mut change);
    out
}

fn outside_inline_code(text: &str, change: &mut dyn FnMut(&str) -> String) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('`') {
        let run = rest[open..].bytes().take_while(|b| *b == b'`').count();
        let fence = &rest[open..open + run];
        let after = &rest[open + run..];
        match after.find(fence) {
            Some(close) => {
                out.push_str(&change(&rest[..open]));
                out.push_str(&rest[open..open + run + close + run]);
                rest = &after[close + run..];
            }
            None => {
                out.push_str(&change(&rest[..open + run]));
                rest = after;
            }
        }
    }
    out.push_str(&change(rest));
    out
}

/// `==text==` as `<mark>text</mark>`, on one line, not starting or ending with a space.
fn highlights(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find("==") {
        let after = &rest[open + 2..];
        let close = after.find("==").filter(|&c| {
            let inner = &after[..c];
            c > 0
                && !inner.contains('\n')
                && !inner.starts_with(char::is_whitespace)
                && !inner.ends_with(char::is_whitespace)
        });
        match close {
            Some(c) => {
                out.push_str(&rest[..open]);
                out.push_str("<mark>");
                out.push_str(&after[..c]);
                out.push_str("</mark>");
                rest = &after[c + 2..];
            }
            None => {
                out.push_str(&rest[..open + 2]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Obsidian's `[[Note]]`, `[[Note|text]]`, `[[Note#Heading]]` and `![[embed]]`.
fn wiki_links(text: &str, targets: &impl Targets) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find("[[") {
        let Some(close) = rest[open + 2..]
            .find("]]")
            .filter(|&c| !rest[open + 2..open + 2 + c].contains('\n'))
        else {
            out.push_str(&rest[..open + 2]);
            rest = &rest[open + 2..];
            continue;
        };
        let embed = rest[..open].ends_with('!');
        let before = if embed {
            &rest[..open - 1]
        } else {
            &rest[..open]
        };
        let inner = &rest[open + 2..open + 2 + close];
        let alone = embed
            && before
                .rsplit('\n')
                .next()
                .is_some_and(|l| l.trim().is_empty())
            && rest[open + 4 + close..]
                .split('\n')
                .next()
                .is_some_and(|l| l.trim().is_empty());
        out.push_str(before);
        out.push_str(&wiki_link(inner, embed, alone, targets));
        rest = &rest[open + 4 + close..];
    }
    out.push_str(rest);
    out
}

fn wiki_link(inner: &str, embed: bool, alone: bool, targets: &impl Targets) -> String {
    let (target, label) = match inner.split_once('|') {
        Some((t, l)) => (t.trim(), Some(l.trim())),
        None => (inner.trim(), None),
    };
    let (name, heading) = match target.split_once('#') {
        Some((n, h)) => (n.trim(), Some(h.trim())),
        None => (target, None),
    };
    let last = name.rsplit('/').next().unwrap_or(name);
    if is_file(last) && !last.ends_with(".md") {
        // A picture or another file. After an embedded image's bar comes its width.
        let alt = if embed { last } else { label.unwrap_or(last) };
        return match targets.file(name) {
            Some(url) if embed => format!("![{}]({})", escape(alt), destination(&url)),
            Some(url) => format!("[{}]({})", escape(alt), destination(&url)),
            None => escape(alt),
        };
    }
    let name = name.strip_suffix(".md").unwrap_or(name);
    let shown = match (label, heading) {
        (Some(l), _) if !l.is_empty() => l.to_string(),
        (_, Some(h)) if !h.is_empty() => format!("{} › {h}", last.trim_end_matches(".md")),
        _ => last.trim_end_matches(".md").to_string(),
    };
    match targets.named(name).filter(|id| valid_doc(id)) {
        Some(id) if alone => docs::page(&id, &shown),
        Some(id) => docs::mention(&id, &shown),
        None => escape(&shown),
    }
}

fn is_file(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(stem, ext)| {
        !stem.is_empty()
            && (1..=5).contains(&ext.len())
            && ext.chars().all(|c| c.is_ascii_alphanumeric())
    })
}

fn valid_doc(id: &str) -> bool {
    docs::doc_id(&format!("{}{id}", docs::SCHEME)).is_some()
}

/// Text that reads as itself in Markdown.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(
            c,
            '\\' | '[' | ']' | '*' | '_' | '`' | '<' | '>' | '#' | '|'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// A link destination, in angle brackets when it holds spaces or parentheses.
fn destination(url: &str) -> String {
    if url.contains([' ', '(', ')', '<', '>']) {
        format!("<{}>", url.replace(['<', '>'], ""))
    } else {
        url.to_string()
    }
}

// Markdown links and pictures, found by the parser.

/// Every link, picture and reference definition pointing into the import, pointed where it
/// went.
fn links(body: &str, path: &str, targets: &impl Targets) -> String {
    let Ok(tree) = to_mdast(body, &ParseOptions::gfm()) else {
        return body.to_string();
    };
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    visit(&tree, &mut |node| {
        let Some(span) = node.position().map(|p| p.start.offset..p.end.offset) else {
            return;
        };
        match node {
            Node::Link(link) => {
                let text = children_source(body, &link.children).unwrap_or_default();
                let target = match local(&link.url, path) {
                    Local::Elsewhere => return,
                    // A link out of the import goes nowhere now: its text stays.
                    Local::Outside => return edits.push((span, text.to_string())),
                    Local::Path(target) => target,
                };
                let note = target.ends_with(".md") || target.ends_with(".markdown");
                let found = if note {
                    targets
                        .note(&target)
                        .filter(|id| valid_doc(id))
                        .map(|id| format!("{}{id}", docs::SCHEME))
                } else {
                    targets.file(&target)
                };
                let markdown = match found {
                    Some(url) => format!("[{text}]({})", destination(&url)),
                    None => text.to_string(),
                };
                edits.push((span, markdown));
            }
            Node::Image(image) => {
                let target = match local(&image.url, path) {
                    Local::Elsewhere => return,
                    Local::Outside => return edits.push((span, escape(&image.alt))),
                    Local::Path(target) => target,
                };
                let markdown = match targets.file(&target) {
                    Some(url) => format!("![{}]({})", escape(&image.alt), destination(&url)),
                    None => escape(&image.alt),
                };
                edits.push((span, markdown));
            }
            Node::Definition(def) => {
                let target = match local(&def.url, path) {
                    Local::Elsewhere => return,
                    Local::Outside => String::new(),
                    Local::Path(target) => target,
                };
                let note = target.ends_with(".md") || target.ends_with(".markdown");
                let found = if note {
                    targets
                        .note(&target)
                        .filter(|id| valid_doc(id))
                        .map(|id| format!("{}{id}", docs::SCHEME))
                } else {
                    targets.file(&target)
                };
                // A definition that goes nowhere points at nothing, so its uses read as text.
                let url = found.unwrap_or_else(|| "#".into());
                edits.push((
                    span,
                    format!(
                        "[{}]: {}",
                        def.label.as_deref().unwrap_or(&def.identifier),
                        destination(&url)
                    ),
                ));
            }
            _ => {}
        }
    });
    edits.sort_by_key(|(r, _)| r.start);
    let mut out = String::with_capacity(body.len());
    let mut at = 0;
    for (range, text) in edits {
        if range.start < at {
            continue;
        }
        out.push_str(&body[at..range.start]);
        out.push_str(&text);
        at = range.end;
    }
    out.push_str(&body[at..]);
    out
}

fn visit(node: &Node, f: &mut impl FnMut(&Node)) {
    f(node);
    // A link's children are its text, never another link.
    if matches!(node, Node::Link(_)) {
        return;
    }
    if let Some(children) = node.children() {
        for child in children {
            visit(child, f);
        }
    }
}

fn children_source<'a>(body: &'a str, children: &[Node]) -> Option<&'a str> {
    let start = children.first()?.position()?.start.offset;
    let end = children.last()?.position()?.end.offset;
    body.get(start..end)
}

/// Where a link points.
enum Local {
    /// Not into the import: an address with a scheme, or a place in the same page.
    Elsewhere,
    /// Out of the import, by an absolute path or by climbing out with `..`.
    Outside,
    /// A path in the import.
    Path(String),
}

/// Where `href`, written in the note at `from`, points.
fn local(href: &str, from: &str) -> Local {
    let href = href.trim();
    if href.is_empty() || href.starts_with('#') || has_scheme(href) {
        return Local::Elsewhere;
    }
    if href.starts_with('/') {
        return Local::Outside;
    }
    let href = href.split(['#', '?']).next().unwrap_or(href);
    let href = percent_decode(href);
    let mut parts: Vec<&str> = from.split('/').collect();
    parts.pop();
    for piece in href.split('/') {
        match piece {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Local::Outside;
                }
            }
            p => parts.push(p),
        }
    }
    Local::Path(parts.join("/"))
}

fn has_scheme(href: &str) -> bool {
    match href.find(':') {
        Some(colon) => {
            let scheme = &href[..colon];
            !scheme.is_empty()
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
                && !href[..colon].contains('/')
        }
        None => false,
    }
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut ix = 0;
    while ix < bytes.len() {
        if bytes[ix] == b'%'
            && let Some(hex) = text.get(ix + 1..ix + 3)
            && let Ok(byte) = u8::from_str_radix(hex, 16)
        {
            out.push(byte);
            ix += 3;
        } else {
            out.push(bytes[ix]);
            ix += 1;
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::Kind;
    use std::collections::HashMap;
    use std::prelude::rust_2024::test;

    struct Vault {
        notes: HashMap<&'static str, &'static str>,
        names: HashMap<&'static str, &'static str>,
        files: HashMap<&'static str, &'static str>,
    }

    impl Targets for Vault {
        fn note(&self, path: &str) -> Option<String> {
            self.notes.get(path).map(|s| s.to_string())
        }
        fn named(&self, name: &str) -> Option<String> {
            self.names.get(name).map(|s| s.to_string())
        }
        fn file(&self, path: &str) -> Option<String> {
            self.files.get(path).map(|s| s.to_string())
        }
    }

    fn vault() -> Vault {
        Vault {
            notes: HashMap::from([
                ("Home/Roadmap 0123456789abcdef0123456789abcdef.md", "road"),
                ("Home/Sub/Plan 0123456789abcdef0123456789abcdef.md", "plan"),
            ]),
            names: HashMap::from([("Roadmap", "road"), ("Projects/Plan", "plan")]),
            files: HashMap::from([
                ("Home/pic.png", "https://files.example/pic.png"),
                ("chart.png", "https://files.example/chart.png"),
            ]),
        }
    }

    #[test]
    fn notion_names_lose_their_ids() {
        assert_eq!(
            title_from_path("Home/Roadmap 0123456789abcdef0123456789abcdef.md"),
            "Roadmap"
        );
        assert_eq!(title_from_path("Notes.md"), "Notes");
        assert_eq!(clean_name("Short abc"), "Short abc");
    }

    #[test]
    fn a_notion_page_is_titled_and_linked() {
        let text = "\u{feff}# Weekly review\r\n\r\nSee [the roadmap](Roadmap%200123456789abcdef0123456789abcdef.md) and [plan](Sub/Plan%200123456789abcdef0123456789abcdef.md).\r\n\r\n![Chart](pic.png)\r\n\r\n![Gone](missing.png)\r\n\r\n[Outside](../../etc/passwd) [Web](https://example.com)\r\n\r\n<aside>\r\n💡 Remember this\r\n\r\n</aside>";
        let note = note(
            "Home/Weekly review 0123456789abcdef0123456789abcdef.md",
            text,
            &vault(),
        );
        assert_eq!(note.title, "Weekly review");
        assert_eq!(
            note.document.to_markdown(),
            "See [the roadmap](doc:road) and [plan](doc:plan).\n\n![Chart](https://files.example/pic.png)\n\nGone\n\nOutside [Web](https://example.com)\n\n> [!NOTE]\n> 💡 Remember this"
        );
        assert_eq!(note.document.links(), ["road", "plan"]);
    }

    #[test]
    fn an_obsidian_note_keeps_its_links_callouts_and_highlights() {
        let text = "---\ntitle: \"From front matter\"\ntags: [a]\n---\n\nSee [[Roadmap]], [[Projects/Plan|the plan]], [[Roadmap#Dates]] and [[Nowhere]]. A ==bright== word.\n\n![[Projects/Plan]]\n\n![[chart.png|300]]\n\n> [!tip] Do this\n> First\n\n> [!faq]- Folded\n> Hidden body\n\n```\n[[Roadmap]] ==not== here\n```\n\n`[[Roadmap]]` too";
        let note = note("Vault/Daily.md", text, &vault());
        assert_eq!(note.title, "From front matter");
        let md = note.document.to_markdown();
        assert!(
            md.starts_with("---\ntitle: \"From front matter\"\ntags: [a]\n---\n"),
            "{md}"
        );
        assert!(md.contains("See [Roadmap](doc:road), [the plan](doc:plan), [Roadmap › Dates](doc:road) and Nowhere."), "{md}");
        assert!(md.contains("A <mark>bright</mark> word."), "{md}");
        assert!(md.contains("[Plan](doc:plan \"page\")"), "{md}");
        assert!(
            md.contains("![chart.png](https://files.example/chart.png)"),
            "{md}"
        );
        assert!(md.contains("> [!TIP]\n> **Do this**\n>\n> First"), "{md}");
        assert!(
            md.contains("<details>\n<summary>Folded</summary>\n\nHidden body\n\n</details>"),
            "{md}"
        );
        assert!(
            md.contains("```\n[[Roadmap]] ==not== here\n```"),
            "code is left alone: {md}"
        );
        assert!(md.contains("`[[Roadmap]]` too"), "{md}");
        assert_eq!(note.document.pages(), ["plan"]);
        let kinds: Vec<_> = note
            .document
            .blocks
            .iter()
            .map(|b| std::mem::discriminant(&b.kind))
            .collect();
        assert!(kinds.contains(&std::mem::discriminant(&Kind::Toggle { open: false })));
    }

    #[test]
    fn reference_links_point_at_documents_too() {
        let note = note(
            "Home/Index.md",
            "Read [the roadmap][r] and [this][gone].\n\n[r]: Roadmap%200123456789abcdef0123456789abcdef.md\n[gone]: ../../outside.md",
            &vault(),
        );
        assert_eq!(note.document.links(), ["road"]);
        assert!(
            note.document
                .to_markdown()
                .starts_with("Read [the roadmap](doc:road) and "),
            "{}",
            note.document.to_markdown()
        );
    }

    #[test]
    fn hostile_names_and_ids_stay_text() {
        struct Bad;
        impl Targets for Bad {
            fn note(&self, _: &str) -> Option<String> {
                Some("../not an id".into())
            }
            fn named(&self, _: &str) -> Option<String> {
                Some("bad id".into())
            }
            fn file(&self, _: &str) -> Option<String> {
                None
            }
        }
        let note = note("a.md", "[x](b.md) [[c]] [[<script>|*y*]] ![[d.png]]", &Bad);
        assert_eq!(note.document.links(), Vec::<String>::new());
        assert_eq!(note.document.to_markdown(), "x c \\*y\\* d.png");
    }
}
