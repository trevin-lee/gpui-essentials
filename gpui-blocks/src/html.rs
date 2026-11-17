//! HTML from the clipboard as Markdown, so a paste from a web page, Google Docs or another app
//! keeps its headings, lists, quotes, code, tables, links and bold, italic, strikethrough and
//! code marks.
//!
//! GPUI's clipboard holds plain text, so the app reads the HTML the system clipboard also has and
//! gives it to the editor with [`Options::html_clipboard`](crate::Options::html_clipboard); the
//! editor turns it into Markdown here. [`to_markdown`] is public for apps that paste elsewhere.
//!
//! It's lenient, as browsers are: unclosed paragraphs and list items close themselves, unknown
//! tags give their content, and scripts, styles and the document's head are left out. Text keeps
//! its characters: Markdown's own (`*`, `_`, `#`…) are escaped. Pictures keep only `http` and
//! `https` addresses, and links only those and `mailto:` (a `data:` picture would be megabytes of
//! text).

/// HTML as Markdown, or `None` when it holds no text.
pub fn to_markdown(html: &str) -> Option<String> {
    let nodes = parse(html);
    let mut out = Vec::new();
    blocks(&nodes, &Marks::default(), &mut out);
    let markdown = out.join("\n\n");
    let markdown = markdown.trim_matches('\n');
    (!markdown.trim().is_empty()).then(|| markdown.to_string())
}

#[derive(Debug, PartialEq)]
enum Node {
    Element {
        name: String,
        attrs: Vec<(String, String)>,
        children: Vec<Node>,
    },
    Text(String),
}

impl Node {
    fn attr(&self, key: &str) -> Option<&str> {
        match self {
            Node::Element { attrs, .. } => attrs
                .iter()
                .find(|(k, _)| k == key)
                .map(|(_, v)| v.as_str()),
            Node::Text(_) => None,
        }
    }
}

const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr",
];
/// Elements whose content is left out.
const SKIPPED: &[&str] = &[
    "head", "script", "style", "title", "template", "noscript", "svg", "math", "iframe", "object",
    "select", "button", "canvas",
];
const BLOCK: &[&str] = &[
    "address",
    "article",
    "aside",
    "blockquote",
    "body",
    "center",
    "dd",
    "details",
    "dialog",
    "div",
    "dl",
    "dt",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hr",
    "html",
    "li",
    "main",
    "nav",
    "ol",
    "p",
    "pre",
    "section",
    "summary",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "tr",
    "ul",
];

fn is_block(node: &Node) -> bool {
    matches!(node, Node::Element { name, .. } if BLOCK.contains(&name.as_str()))
}

// --- Parsing --------------------------------------------------------------------------------

/// An open element while parsing: its name, attributes and children so far.
type Open = (String, Vec<(String, String)>, Vec<Node>);

/// The nodes of `html`, as a browser would build them for the common cases.
fn parse(html: &str) -> Vec<Node> {
    // Open elements: name, attributes, children so far.
    let mut stack: Vec<Open> = vec![(String::new(), Vec::new(), Vec::new())];
    fn close(stack: &mut Vec<Open>) {
        if stack.len() < 2 {
            return;
        }
        let (name, attrs, children) = stack.pop().unwrap();
        stack.last_mut().unwrap().2.push(Node::Element {
            name,
            attrs,
            children,
        });
    }
    let open_has = |stack: &[Open], name: &str| stack.iter().skip(1).any(|(n, ..)| n == name);
    let mut rest = html;
    while !rest.is_empty() {
        let Some(lt) = rest.find('<') else {
            push_text(&mut stack.last_mut().unwrap().2, rest);
            break;
        };
        push_text(&mut stack.last_mut().unwrap().2, &rest[..lt]);
        rest = &rest[lt..];
        if let Some(after) = rest.strip_prefix("<!--") {
            rest = after.find("-->").map_or("", |end| &after[end + 3..]);
            continue;
        }
        if rest.starts_with("<!") || rest.starts_with("<?") {
            rest = rest.find('>').map_or("", |end| &rest[end + 1..]);
            continue;
        }
        let closing = rest.starts_with("</");
        let name_at = if closing { 2 } else { 1 };
        let name_len = rest[name_at..]
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == ':'))
            .unwrap_or(rest.len() - name_at);
        if name_len == 0 || !rest[name_at..].starts_with(|c: char| c.is_ascii_alphabetic()) {
            // A `<` that starts no tag is text.
            push_text(&mut stack.last_mut().unwrap().2, "<");
            rest = &rest[1..];
            continue;
        }
        let name = rest[name_at..name_at + name_len].to_ascii_lowercase();
        let (attrs, self_closing, end) = tag_rest(&rest[name_at + name_len..]);
        rest = &rest[name_at + name_len + end..];
        if closing {
            if open_has(&stack, &name) {
                while stack.last().is_some_and(|(n, ..)| *n != name) {
                    close(&mut stack);
                }
                close(&mut stack);
            }
            continue;
        }
        // Elements a new one closes, as browsers close them.
        let closes: &[&str] = match name.as_str() {
            "li" => &["li"],
            "dt" | "dd" => &["dt", "dd"],
            "tr" => &["tr", "td", "th"],
            "td" | "th" => &["td", "th"],
            "tbody" | "thead" | "tfoot" => &["tbody", "thead", "tfoot", "tr", "td", "th"],
            _ => &[],
        };
        if !closes.is_empty() {
            // Only within the nearest list or table: a nested list's item doesn't close its
            // parent's.
            let scope: &[&str] = if name == "li" {
                &["ul", "ol"]
            } else if name == "dt" || name == "dd" {
                &["dl"]
            } else {
                &["table"]
            };
            if let Some(at) = stack
                .iter()
                .rposition(|(n, ..)| closes.contains(&n.as_str()) || scope.contains(&n.as_str()))
                && closes.contains(&stack[at].0.as_str())
            {
                while stack.len() > at {
                    close(&mut stack);
                }
            }
        }
        if BLOCK.contains(&name.as_str())
            && stack.last().is_some_and(|(n, ..)| n == "p")
            && name != "li"
        {
            close(&mut stack);
        }
        if SKIPPED.contains(&name.as_str()) {
            // Skip to its end tag, whatever is inside.
            let end_tag = format!("</{name}");
            let lower = rest.to_ascii_lowercase();
            rest = match lower.find(&end_tag) {
                Some(at) => {
                    let after = &rest[at..];
                    after.find('>').map_or("", |end| &after[end + 1..])
                }
                None => "",
            };
            continue;
        }
        if VOID.contains(&name.as_str()) || self_closing {
            stack.last_mut().unwrap().2.push(Node::Element {
                name,
                attrs,
                children: Vec::new(),
            });
            continue;
        }
        stack.push((name, attrs, Vec::new()));
    }
    while stack.len() > 1 {
        close(&mut stack);
    }
    stack
        .pop()
        .map(|(.., children)| children)
        .unwrap_or_default()
}

/// A tag's attributes, whether it closes itself, and the length up to and including its `>`.
fn tag_rest(s: &str) -> (Vec<(String, String)>, bool, usize) {
    let bytes = s.as_bytes();
    let mut attrs = Vec::new();
    let mut i = 0;
    let mut self_closing = false;
    while i < bytes.len() {
        match bytes[i] {
            b'>' => return (attrs, self_closing, i + 1),
            b'/' => {
                self_closing = true;
                i += 1;
            }
            c if c.is_ascii_whitespace() => i += 1,
            _ => {
                self_closing = false;
                let start = i;
                while i < bytes.len()
                    && !bytes[i].is_ascii_whitespace()
                    && !matches!(bytes[i], b'=' | b'>' | b'/')
                {
                    i += 1;
                }
                let key = s[start..i].to_ascii_lowercase();
                while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                let mut value = String::new();
                if i < bytes.len() && bytes[i] == b'=' {
                    i += 1;
                    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                        i += 1;
                    }
                    if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
                        let quote = bytes[i];
                        let from = i + 1;
                        let to = s[from..]
                            .find(quote as char)
                            .map_or(s.len(), |end| from + end);
                        value = decode(&s[from..to]);
                        i = (to + 1).min(s.len());
                    } else {
                        let from = i;
                        while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>'
                        {
                            i += 1;
                        }
                        value = decode(&s[from..i]);
                    }
                }
                if !key.is_empty() {
                    attrs.push((key, value));
                }
            }
        }
    }
    (attrs, self_closing, s.len())
}

fn push_text(children: &mut Vec<Node>, raw: &str) {
    if raw.is_empty() {
        return;
    }
    let text = decode(raw);
    if let Some(Node::Text(last)) = children.last_mut() {
        last.push_str(&text);
    } else {
        children.push(Node::Text(text));
    }
}

/// Text with its character references decoded.
fn decode(raw: &str) -> String {
    if !raw.contains('&') {
        return raw.to_string();
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let end = rest[1..]
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '#'))
            .map(|e| e + 1)
            .unwrap_or(rest.len());
        let name = &rest[1..end];
        let ch = if let Some(num) = name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
            u32::from_str_radix(num, 16).ok().and_then(char::from_u32)
        } else if let Some(num) = name.strip_prefix('#') {
            num.parse().ok().and_then(char::from_u32)
        } else {
            named(name)
        };
        match ch {
            Some(ch) => {
                out.push(ch);
                rest = &rest[end..];
                rest = rest.strip_prefix(';').unwrap_or(rest);
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn named(name: &str) -> Option<char> {
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{a0}',
        "mdash" => '—',
        "ndash" => '–',
        "hellip" => '…',
        "lsquo" => '‘',
        "rsquo" => '’',
        "ldquo" => '“',
        "rdquo" => '”',
        "bull" => '•',
        "middot" => '·',
        "times" => '×',
        "copy" => '©',
        "reg" => '®',
        "trade" => '™',
        "euro" => '€',
        "pound" => '£',
        "deg" => '°',
        "rarr" => '→',
        "larr" => '←',
        _ => return None,
    })
}

// --- Writing --------------------------------------------------------------------------------

/// The marks the text being written is already in, so a nested `<b>` doesn't double them.
#[derive(Clone, Copy, Default)]
struct Marks {
    bold: bool,
    italic: bool,
    strike: bool,
}

/// `nodes` as Markdown blocks, pushed onto `out`.
fn blocks(nodes: &[Node], marks: &Marks, out: &mut Vec<String>) {
    let mut run: Vec<&Node> = Vec::new();
    let flush = |run: &mut Vec<&Node>, out: &mut Vec<String>| {
        if run.is_empty() {
            return;
        }
        let text = paragraph(run, marks);
        run.clear();
        if !text.is_empty() {
            out.push(text);
        }
    };
    for node in nodes {
        // An inline element holding blocks (Google Docs wraps everything in a `<b>`) is a
        // container: its blocks are written as blocks.
        let holds_blocks = matches!(node, Node::Element { children, .. }
            if !is_block(node) && children.iter().any(is_block));
        if holds_blocks {
            flush(&mut run, out);
            if let Node::Element { children, .. } = node {
                blocks(children, marks, out);
            }
            continue;
        }
        if !is_block(node) {
            run.push(node);
            continue;
        }
        flush(&mut run, out);
        block(node, marks, out);
    }
    flush(&mut run, out);
}

/// Inline nodes as one paragraph: whitespace collapsed, a leading marker escaped.
fn paragraph(nodes: &[&Node], marks: &Marks) -> String {
    let mut text = String::new();
    for node in nodes {
        inline(node, marks, &mut text);
    }
    // Spaces either side of a tag are one space, as HTML shows them.
    let mut single = String::with_capacity(text.len());
    for c in text.chars() {
        if !(c == ' ' && single.ends_with(' ')) {
            single.push(c);
        }
    }
    let text = single
        .split('\n')
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("\n");
    let text = text.trim().trim_end_matches('\\').trim_end().to_string();
    escape_line_start(&text)
}

/// Escapes what would start a block at the start of a line: `-`, `+`, `>`, `1.`.
fn escape_line_start(text: &str) -> String {
    text.split('\n')
        .map(|line| {
            let digits = line.bytes().take_while(u8::is_ascii_digit).count();
            if line.starts_with("- ") || line.starts_with("+ ") || line.starts_with('>') {
                format!("\\{line}")
            } else if digits > 0 && line[digits..].starts_with(['.', ')']) {
                format!("{}\\{}", &line[..digits], &line[digits..])
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn block(node: &Node, marks: &Marks, out: &mut Vec<String>) {
    let Node::Element { name, children, .. } = node else {
        return;
    };
    match name.as_str() {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            let level = usize::from(name.as_bytes()[1] - b'0');
            let refs: Vec<&Node> = children.iter().collect();
            let text = paragraph(&refs, marks).replace('\n', " ");
            if !text.is_empty() {
                out.push(format!("{} {}", "#".repeat(level), text));
            }
        }
        "hr" => out.push("---".to_string()),
        "pre" => out.push(code_block(node)),
        "ul" | "ol" => {
            let list = list(node, marks);
            if !list.is_empty() {
                out.push(list);
            }
        }
        "blockquote" => {
            let mut inner = Vec::new();
            blocks(children, marks, &mut inner);
            if !inner.is_empty() {
                let quoted = inner
                    .join("\n\n")
                    .split('\n')
                    .map(|line| {
                        if line.is_empty() {
                            ">".to_string()
                        } else {
                            format!("> {line}")
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                out.push(quoted);
            }
        }
        "table" => {
            if let Some(table) = table(node, marks) {
                out.push(table);
            }
        }
        // Everything else block-level: its content, as blocks.
        _ => blocks(children, marks, out),
    }
}

/// A list's items, each `- ` or `N. ` with its later lines indented under it.
fn list(node: &Node, marks: &Marks) -> String {
    let Node::Element { name, children, .. } = node else {
        return String::new();
    };
    let ordered = name == "ol";
    let mut n: u64 = node
        .attr("start")
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(1);
    let mut items = Vec::new();
    for child in children {
        let item_children: &[Node] = match child {
            Node::Element { name, children, .. } if name == "li" => children,
            // A list straight inside a list (no item around it): part of the previous item.
            Node::Element { name, .. } if name == "ul" || name == "ol" => {
                let nested = list(child, marks);
                if let Some(last) = items.last_mut() {
                    let last: &mut String = last;
                    last.push('\n');
                    last.push_str(&indent(&nested, "  "));
                } else if !nested.is_empty() {
                    items.push(nested);
                }
                continue;
            }
            Node::Text(t) if t.trim().is_empty() => continue,
            _ => std::slice::from_ref(child),
        };
        // A checkbox first makes a to-do item.
        let task = item_children.iter().find_map(|c| match c {
            Node::Element { name, .. }
                if name == "input"
                    && c.attr("type")
                        .is_some_and(|t| t.eq_ignore_ascii_case("checkbox")) =>
            {
                Some(c.attr("checked").is_some())
            }
            _ => None,
        });
        let marker = if ordered {
            let m = format!("{n}. ");
            n += 1;
            m
        } else {
            "- ".to_string()
        };
        let mut inner = Vec::new();
        blocks(item_children, marks, &mut inner);
        let mut body = String::new();
        for (ix, part) in inner.iter().enumerate() {
            if ix > 0 {
                // A nested list follows its line directly; other blocks after a blank line.
                let nested = part.starts_with("- ")
                    || part.split_once(". ").is_some_and(|(d, _)| {
                        !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit())
                    });
                body.push_str(if nested { "\n" } else { "\n\n" });
            }
            body.push_str(part);
        }
        let body = match task {
            Some(done) => format!("[{}] {body}", if done { "x" } else { " " }),
            None => body,
        };
        let pad = " ".repeat(marker.len());
        let mut lines = body.split('\n');
        let first = lines.next().unwrap_or_default();
        let mut item = format!("{marker}{first}");
        for line in lines {
            item.push('\n');
            if !line.is_empty() {
                item.push_str(&pad);
                item.push_str(line);
            }
        }
        items.push(item.trim_end().to_string());
    }
    items.join("\n")
}

fn indent(text: &str, pad: &str) -> String {
    text.split('\n')
        .map(|l| {
            if l.is_empty() {
                String::new()
            } else {
                format!("{pad}{l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A `<pre>` as a fenced code block, with the language its class names.
fn code_block(node: &Node) -> String {
    let mut code = String::new();
    raw_text(node, &mut code);
    let code = code
        .strip_prefix('\n')
        .unwrap_or(&code)
        .trim_end_matches('\n');
    let lang = language(node).unwrap_or_default();
    let longest = longest_run(code, '`');
    let fence = "`".repeat(longest.max(2) + 1);
    format!("{fence}{lang}\n{code}\n{fence}")
}

fn language(node: &Node) -> Option<String> {
    let from_class = |n: &Node| {
        n.attr("class")?.split_whitespace().find_map(|class| {
            ["language-", "lang-", "highlight-source-"]
                .iter()
                .find_map(|prefix| class.strip_prefix(prefix))
                .filter(|l| {
                    !l.is_empty()
                        && l.chars()
                            .all(|c| c.is_ascii_alphanumeric() || "+#-_".contains(c))
                })
                .map(str::to_string)
        })
    };
    from_class(node).or_else(|| match node {
        Node::Element { children, .. } => children.iter().find_map(|c| match c {
            Node::Element { name, .. } if name == "code" => from_class(c),
            _ => None,
        }),
        Node::Text(_) => None,
    })
}

/// The text inside `node` as it is, `<br>` as a line break.
fn raw_text(node: &Node, out: &mut String) {
    match node {
        Node::Text(t) => out.push_str(&t.replace('\u{a0}', " ")),
        Node::Element { name, children, .. } => {
            if name == "br" {
                out.push('\n');
            }
            for child in children {
                raw_text(child, out);
            }
            if matches!(name.as_str(), "div" | "p" | "li" | "tr") && !out.ends_with('\n') {
                out.push('\n');
            }
        }
    }
}

fn longest_run(text: &str, ch: char) -> usize {
    let (mut best, mut run) = (0, 0);
    for c in text.chars() {
        run = if c == ch { run + 1 } else { 0 };
        best = best.max(run);
    }
    best
}

/// A table as a GitHub table: its first row the header, every row as wide as the widest.
fn table(node: &Node, marks: &Marks) -> Option<String> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    fn collect(node: &Node, marks: &Marks, rows: &mut Vec<Vec<String>>) {
        let Node::Element { name, children, .. } = node else {
            return;
        };
        if name == "tr" {
            let cells = children
                .iter()
                .filter(|c| matches!(c, Node::Element { name, .. } if name == "td" || name == "th"))
                .map(|cell| {
                    let Node::Element { children, .. } = cell else {
                        return String::new();
                    };
                    let mut parts = Vec::new();
                    blocks(children, marks, &mut parts);
                    parts.join(" ").replace('\n', " ")
                })
                .collect::<Vec<_>>();
            rows.push(cells);
            return;
        }
        for child in children {
            collect(child, marks, rows);
        }
    }
    collect(node, marks, &mut rows);
    rows.retain(|r| !r.is_empty());
    let width = rows.iter().map(Vec::len).max()?;
    let line = |cells: &[String]| {
        let mut cells = cells.to_vec();
        cells.resize(width, String::new());
        format!("| {} |", cells.join(" | "))
    };
    let mut out = vec![line(&rows[0]), line(&vec!["---".to_string(); width])];
    out.extend(rows[1..].iter().map(|r| line(r)));
    Some(out.join("\n"))
}

/// Inline content as Markdown, pushed onto `out`.
fn inline(node: &Node, marks: &Marks, out: &mut String) {
    match node {
        Node::Text(t) => out.push_str(&escape(&collapse(t))),
        Node::Element { name, children, .. } => {
            let inner = |marks: &Marks| {
                let mut s = String::new();
                for child in children {
                    inline(child, marks, &mut s);
                }
                s
            };
            let style = node.attr("style").unwrap_or_default().to_ascii_lowercase();
            let style_bold = style_has(&style, "font-weight", |v| {
                v == "bold" || v == "bolder" || v.parse::<u32>().is_ok_and(|w| w >= 600)
            });
            let style_not_bold = style_has(&style, "font-weight", |v| {
                v == "normal" || v.parse::<u32>().is_ok_and(|w| w < 600)
            });
            let style_italic = style_has(&style, "font-style", |v| v == "italic" || v == "oblique");
            let style_strike = style_has(&style, "text-decoration", |v| v.contains("line-through"))
                || style_has(&style, "text-decoration-line", |v| {
                    v.contains("line-through")
                });
            match name.as_str() {
                "br" => out.push_str("\\\n"),
                "img" => {
                    let src = node.attr("src").unwrap_or_default();
                    if src.starts_with("http://") || src.starts_with("https://") {
                        let alt = escape(&collapse(node.attr("alt").unwrap_or_default()));
                        out.push_str(&format!("![{}]({})", alt.trim(), destination(src)));
                    }
                }
                "a" => {
                    let text = inner(marks);
                    let href = node.attr("href").unwrap_or_default().trim();
                    let linkable = href.starts_with("http://")
                        || href.starts_with("https://")
                        || href.starts_with("mailto:");
                    if linkable && !text.trim().is_empty() {
                        let (lead, body, trail) = split_space(&text);
                        out.push_str(&format!("{lead}[{body}]({}){trail}", destination(href)));
                    } else {
                        out.push_str(&text);
                    }
                }
                "code" | "kbd" | "samp" | "tt" => {
                    let mut raw = String::new();
                    for child in children {
                        raw_text(child, &mut raw);
                    }
                    let raw = collapse(&raw);
                    let (lead, body, trail) = split_space(&raw);
                    if body.is_empty() {
                        out.push_str(&raw);
                    } else {
                        let ticks = "`".repeat(longest_run(body, '`') + 1);
                        let pad = if body.starts_with('`') || body.ends_with('`') {
                            " "
                        } else {
                            ""
                        };
                        out.push_str(&format!("{lead}{ticks}{pad}{body}{pad}{ticks}{trail}"));
                    }
                }
                _ => {
                    let bold =
                        (matches!(name.as_str(), "b" | "strong") && !style_not_bold) || style_bold;
                    let italic =
                        matches!(name.as_str(), "i" | "em" | "cite" | "dfn") || style_italic;
                    let strike = matches!(name.as_str(), "s" | "del" | "strike") || style_strike;
                    let new = Marks {
                        bold: marks.bold || bold,
                        italic: marks.italic || italic,
                        strike: marks.strike || strike,
                    };
                    let mut text = inner(&new);
                    // A block inside inline content (a `<div>` in a `<span>`) is a line of its own.
                    if is_block(node) && !text.is_empty() {
                        text = format!("{}\n", text.trim_end());
                    }
                    let mut wrap = |on: bool, already: bool, fence: &str| {
                        if on && !already {
                            let (lead, body, trail) = split_space(&text);
                            if !body.is_empty() {
                                text = format!("{lead}{fence}{body}{fence}{trail}");
                            }
                        }
                    };
                    wrap(strike, marks.strike, "~~");
                    wrap(italic, marks.italic, "*");
                    wrap(bold, marks.bold, "**");
                    out.push_str(&text);
                }
            }
        }
    }
}

/// Whether the CSS `style` sets `property` to a value `test` accepts.
fn style_has(style: &str, property: &str, test: impl Fn(&str) -> bool) -> bool {
    style.split(';').any(|decl| {
        decl.split_once(':').is_some_and(|(p, v)| {
            p.trim() == property && test(v.trim().trim_end_matches("!important").trim())
        })
    })
}

/// Whitespace runs as one space, as HTML shows them; a no-break space stays one space.
fn collapse(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for c in text.chars() {
        if c.is_whitespace() && c != '\u{a0}' {
            if !space {
                out.push(' ');
            }
            space = true;
        } else {
            out.push(if c == '\u{a0}' { ' ' } else { c });
            space = false;
        }
    }
    out
}

/// Markdown's own characters escaped, so the text reads back as it was.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(
            c,
            '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '~' | '|' | '#'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// A link's address as a Markdown destination: in `<…>` when it has spaces or parentheses.
fn destination(url: &str) -> String {
    if url.contains([' ', '(', ')', '<', '>']) {
        format!("<{}>", url.replace('<', "%3C").replace('>', "%3E"))
    } else {
        url.to_string()
    }
}

/// `text` as its leading space, its body and its trailing space, so marks wrap only the body.
fn split_space(text: &str) -> (&str, &str, &str) {
    let body = text.trim();
    if body.is_empty() {
        return (text, "", "");
    }
    let start = text.find(body).unwrap_or(0);
    (&text[..start], body, &text[start + body.len()..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn md(html: &str) -> String {
        to_markdown(html).unwrap_or_default()
    }

    #[test]
    fn headings_paragraphs_and_marks() {
        assert_eq!(
            md("<h1>Plan</h1><p>Ship <b>this</b> and <em>that</em>, not <s>those</s>.</p>"),
            "# Plan\n\nShip **this** and *that*, not ~~those~~."
        );
        assert_eq!(
            md("<p>a <strong> b </strong> c</p>"),
            "a **b** c",
            "spaces go outside"
        );
        assert_eq!(
            md("<b>bold <b>inside</b></b>"),
            "**bold inside**",
            "not doubled"
        );
        assert_eq!(md("<p>Use <code>cargo test</code></p>"), "Use `cargo test`");
        assert_eq!(md("<code>a`b</code>"), "``a`b``");
    }

    #[test]
    fn text_keeps_its_characters() {
        assert_eq!(
            md("<p>2 * 3 = 6 and a_b [x]</p>"),
            r"2 \* 3 = 6 and a\_b \[x\]"
        );
        assert_eq!(md("<p>- not a list</p>"), r"\- not a list");
        assert_eq!(md("<p>1. not a list either</p>"), r"1\. not a list either");
        assert_eq!(md("<p>#hashtag</p>"), r"\#hashtag");
        assert_eq!(
            md("<p>Tom &amp; Jerry &lt;3 &#8212; &#x2192; &nbsp;x</p>"),
            r"Tom & Jerry \<3 — → x"
        );
        assert_eq!(md("<p>a < b</p>"), r"a \< b", "a < that starts no tag");
    }

    #[test]
    fn links_and_pictures() {
        assert_eq!(
            md(r#"<p>See <a href="https://example.com/a(b)">the docs</a></p>"#),
            "See [the docs](<https://example.com/a(b)>)"
        );
        assert_eq!(
            md(r#"<a href="javascript:alert(1)">x</a>"#),
            "x",
            "no script links"
        );
        assert_eq!(
            md(
                r#"<img src="https://e.com/p.png" alt="A chart"><img src="data:image/png;base64,AAAA">"#
            ),
            "![A chart](https://e.com/p.png)"
        );
    }

    #[test]
    fn lists_nest_and_number() {
        assert_eq!(
            md("<ul><li>One<ul><li>One a</li><li>One b</li></ul></li><li>Two</li></ul>"),
            "- One\n  - One a\n  - One b\n- Two"
        );
        assert_eq!(
            md(r#"<ol start="3"><li>c<li>d</ol>"#),
            "3. c\n4. d",
            "unclosed items"
        );
        assert_eq!(
            md(
                r#"<ul><li><input type="checkbox" checked> Done</li><li><input type="checkbox"> Open</li></ul>"#
            ),
            "- [x] Done\n- [ ] Open"
        );
        assert_eq!(
            md("<ul><li><p>Para one</p><p>Para two</p></li></ul>"),
            "- Para one\n\n  Para two"
        );
    }

    #[test]
    fn quotes_rules_code_and_tables() {
        assert_eq!(
            md("<blockquote><p>Hi</p><p>There</p></blockquote>"),
            "> Hi\n>\n> There"
        );
        assert_eq!(md("<p>a</p><hr><p>b</p>"), "a\n\n---\n\nb");
        assert_eq!(
            md(r#"<pre><code class="language-rust">fn main() {
    println!("&lt;hi&gt;");
}
</code></pre>"#),
            "```rust\nfn main() {\n    println!(\"<hi>\");\n}\n```"
        );
        assert_eq!(
            md(
                "<table><thead><tr><th>Name</th><th>Role</th></tr></thead><tbody><tr><td>Avery</td><td>Design | UX</td></tr><tr><td>Sam</td></tr></tbody></table>"
            ),
            "| Name | Role |\n| --- | --- |\n| Avery | Design \\| UX |\n| Sam |  |"
        );
    }

    #[test]
    fn a_browsers_clipboard_html() {
        // What Chrome puts on the clipboard: a meta tag, fragment comments, styles.
        let html = r#"<meta charset='utf-8'><!--StartFragment--><h2 style="color: red;">Setup</h2><p>Run it<br>then wait.</p><style>p{}</style><script>alert(1)</script><!--EndFragment-->"#;
        assert_eq!(md(html), "## Setup\n\nRun it\\\nthen wait.");
    }

    #[test]
    fn google_docs_bold_and_italic_are_styles() {
        // Google Docs wraps everything in a `<b>` that isn't bold, and marks text with styles.
        let html = r#"<b style="font-weight:normal;" id="docs-internal-guid-1"><p dir="ltr"><span style="font-weight:700;">Bold</span><span style="font-weight:400;"> and </span><span style="font-style:italic;">italic</span></p><ul><li dir="ltr"><p dir="ltr"><span>Item</span></p></li></ul></b>"#;
        assert_eq!(md(html), "**Bold** and *italic*\n\n- Item");
    }

    #[test]
    fn nothing_but_markup_is_none() {
        assert_eq!(to_markdown("<div><br></div>"), None);
        assert_eq!(to_markdown("<script>x</script>"), None);
        assert_eq!(to_markdown(""), None);
    }
}
