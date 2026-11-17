//! Markdown (GitHub's flavour) to the blocks TextView draws. Nothing in it is ever run or
//! fetched: raw HTML shows as its text, and an image shows as its description.

use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use super::paragraph::{Marks, Paragraph};

/// The most text, and the deepest nesting, a TextView takes; past them the rest is dropped, so
/// hostile input can't stall a frame.
const MAX_LEN: usize = 512 * 1024;
const MAX_DEPTH: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Paragraph(Paragraph),
    Heading(u8, Paragraph),
    Code {
        lang: Option<String>,
        text: String,
    },
    Quote(Vec<Block>),
    List {
        start: Option<u32>,
        items: Vec<Item>,
    },
    Rule,
    /// The first row is the head.
    Table {
        align: Vec<Align>,
        rows: Vec<Vec<Paragraph>>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// A task list item's box: ticked or not.
    pub checked: Option<bool>,
    pub blocks: Vec<Block>,
}

pub fn parse(source: &str) -> Vec<Block> {
    parse_with(source, "").blocks
}

/// What [`parse_with`] read: the blocks, and the link reference definitions in the source
/// itself (not the context's), as `[label]: <url>` lines for another source's context.
#[derive(Debug, Default)]
pub struct Parsed {
    pub blocks: Vec<Block>,
    pub definitions: String,
}

/// Reads `source` with `context` after it: link reference definitions from elsewhere (other
/// parts of one document), so a `[text][label]` here resolves to one defined there. The
/// context's definitions draw nothing.
pub fn parse_with(source: &str, context: &str) -> Parsed {
    let mut end = source.len().min(MAX_LEN);
    while !source.is_char_boundary(end) {
        end -= 1;
    }
    let source = &source[..end];
    let mut text = String::new();
    let doc = if context.is_empty() {
        source
    } else {
        // A blank line first, so the definitions can't join the source's last paragraph.
        let mut room = MAX_LEN.saturating_sub(source.len());
        while room < context.len() && !context.is_char_boundary(room) {
            room -= 1;
        }
        text.reserve(source.len() + room.min(context.len()) + 2);
        text.push_str(source);
        text.push_str("\n\n");
        text.push_str(&context[..room.min(context.len())]);
        &text
    };
    // pulldown-cmark doesn't panic on what it's given (it's fuzzed for that, and so is this, in
    // the tests below); the guard is there anyway, where a panic can be caught.
    std::panic::catch_unwind(|| {
        let parser = Parser::new_ext(doc, options());
        let definitions = own_definitions(&parser, source.len());
        Parsed {
            blocks: convert_events(parser, 0),
            definitions,
        }
    })
    .unwrap_or_else(|_| Parsed {
        blocks: vec![Block::Paragraph(Paragraph::plain(source))],
        definitions: String::new(),
    })
}

/// The link reference definitions that start before `own_end` (the source's own, not the
/// context's) as `[label]: <url>` lines. A URL that can't be written back so is left out.
fn own_definitions(parser: &Parser, own_end: usize) -> String {
    let mut defs: Vec<(usize, String, String)> = parser
        .reference_definitions()
        .iter()
        .filter(|(_, d)| d.span.start < own_end)
        .filter(|(label, d)| {
            !d.dest.contains(['<', '>', '\n', '\r', '\\'])
                && !label.contains(['[', ']', '\\', '\n', '\r'])
        })
        .map(|(label, d)| (d.span.start, label.to_string(), d.dest.to_string()))
        .collect();
    // In the order they're written, so the first of a label stays first for the others.
    defs.sort();
    defs.into_iter()
        .map(|(_, label, url)| format!("[{label}]: <{url}>\n"))
        .collect()
}

/// GitHub's flavour: tables, task lists, strikethrough and footnotes.
fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_GFM
}

/// What blocks are put into as they're read.
enum Frame {
    Blocks(Vec<Block>),
    Quote(Vec<Block>),
    List {
        start: Option<u32>,
        items: Vec<Item>,
    },
    Item(Item),
    Footnote(Vec<Block>),
    Table {
        align: Vec<Align>,
        rows: Vec<Vec<Paragraph>>,
        row: Vec<Paragraph>,
    },
}

/// What inline text is going into: a paragraph (or a list item's text, which in a tight list
/// comes with no paragraph around it), a heading, or a table cell.
#[derive(Clone, Copy, PartialEq)]
enum Into_ {
    Paragraph,
    Heading(u8),
    Cell,
}

struct Builder {
    frames: Vec<Frame>,
    text: Option<(Into_, Paragraph)>,
    marks: Vec<Marks>,
    code: Option<(Option<String>, String)>,
    html: Option<String>,
    /// An image's description, while its tags are open.
    image: Option<String>,
    /// How deep inside containers past the limit: their content is dropped.
    skipping: usize,
    /// How deep this reading is: an HTML block's text is read again, one deeper.
    depth: usize,
}

impl Builder {
    fn marks(&self) -> Marks {
        self.marks.last().cloned().unwrap_or_default()
    }

    fn push_block(&mut self, b: Block) {
        match self.frames.last_mut() {
            Some(Frame::Blocks(v) | Frame::Quote(v) | Frame::Footnote(v)) => v.push(b),
            Some(Frame::Item(i)) => i.blocks.push(b),
            _ => {}
        }
    }

    /// Inline text, into the open paragraph, heading or cell, or a new paragraph (a tight list
    /// item's text).
    fn inline(&mut self, text: &str, marks: Marks) {
        if self.text.is_none() {
            self.text = Some((Into_::Paragraph, Paragraph::default()));
        }
        if let Some((_, p)) = self.text.as_mut() {
            p.push(text, marks);
        }
    }

    /// Ends the open paragraph or heading (a cell ends with its tag).
    fn flush(&mut self) {
        match self.text.take() {
            Some((Into_::Paragraph, p)) => {
                if !p.text.is_empty() {
                    self.push_block(Block::Paragraph(linkify(p)));
                }
            }
            Some((Into_::Heading(level), p)) => self.push_block(Block::Heading(level, linkify(p))),
            Some((Into_::Cell, p)) => self.text = Some((Into_::Cell, p)),
            None => {}
        }
    }

    fn start(&mut self, tag: Tag) {
        if self.skipping > 0 || self.frames.len() > MAX_DEPTH {
            self.skipping += 1;
            return;
        }
        match tag {
            Tag::Paragraph => {
                self.flush();
                self.text = Some((Into_::Paragraph, Paragraph::default()));
            }
            Tag::Heading { level, .. } => {
                self.flush();
                let level = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                };
                self.text = Some((Into_::Heading(level), Paragraph::default()));
            }
            Tag::BlockQuote(_) => {
                self.flush();
                self.frames.push(Frame::Quote(Vec::new()));
            }
            Tag::CodeBlock(kind) => {
                self.flush();
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => info
                        .split_whitespace()
                        .next()
                        .filter(|l| !l.is_empty())
                        .map(str::to_string),
                    CodeBlockKind::Indented => None,
                };
                self.code = Some((lang, String::new()));
            }
            Tag::HtmlBlock => {
                self.flush();
                self.html = Some(String::new());
            }
            Tag::List(start) => {
                self.flush();
                self.frames.push(Frame::List {
                    start: start.map(|n| n.min(u32::MAX as u64) as u32),
                    items: Vec::new(),
                });
            }
            Tag::Item => {
                self.flush();
                self.frames.push(Frame::Item(Item {
                    checked: None,
                    blocks: Vec::new(),
                }));
            }
            Tag::FootnoteDefinition(_) => {
                self.flush();
                self.frames.push(Frame::Footnote(Vec::new()));
            }
            Tag::Table(align) => {
                self.flush();
                self.frames.push(Frame::Table {
                    align: align
                        .iter()
                        .map(|a| match a {
                            Alignment::Center => Align::Center,
                            Alignment::Right => Align::Right,
                            _ => Align::Left,
                        })
                        .collect(),
                    rows: Vec::new(),
                    row: Vec::new(),
                });
            }
            Tag::TableHead | Tag::TableRow => {
                if let Some(Frame::Table { row, .. }) = self.frames.last_mut() {
                    row.clear();
                }
            }
            Tag::TableCell => self.text = Some((Into_::Cell, Paragraph::default())),
            Tag::Emphasis => self.marks.push(Marks {
                italic: true,
                ..self.marks()
            }),
            Tag::Strong => self.marks.push(Marks {
                bold: true,
                ..self.marks()
            }),
            Tag::Strikethrough => self.marks.push(Marks {
                strike: true,
                ..self.marks()
            }),
            Tag::Link { dest_url, .. } => self.marks.push(Marks {
                link: Some(dest_url.to_string().into()),
                ..self.marks()
            }),
            Tag::Image { .. } => self.image = Some(String::new()),
            // Not turned on: superscript, subscript, definition lists, metadata.
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        if self.skipping > 0 {
            self.skipping -= 1;
            return;
        }
        match tag {
            TagEnd::Paragraph | TagEnd::Heading(_) => self.flush(),
            TagEnd::CodeBlock => {
                if let Some((lang, mut text)) = self.code.take() {
                    if text.ends_with('\n') {
                        text.pop();
                    }
                    self.push_block(Block::Code { lang, text });
                }
            }
            TagEnd::HtmlBlock => {
                // Raw HTML is never rendered: it reads as the text it is, and the Markdown after
                // it in the same block (which runs to a blank line) is still read. So the block
                // is read again with its first `<` escaped.
                if let Some(html) = self.html.take() {
                    let escaped = match html.find('<') {
                        Some(i) => format!("{}\\{}", &html[..i], &html[i..]),
                        None => html,
                    };
                    let blocks = if self.depth < MAX_DEPTH {
                        convert(&escaped, self.depth + 1)
                    } else {
                        vec![Block::Paragraph(Paragraph::plain(escaped))]
                    };
                    for b in blocks {
                        self.push_block(b);
                    }
                }
            }
            TagEnd::BlockQuote(_) => {
                self.flush();
                if let Some(Frame::Quote(v)) = self.frames.pop() {
                    self.push_block(Block::Quote(v));
                }
            }
            TagEnd::Item => {
                self.flush();
                if let Some(Frame::Item(item)) = self.frames.pop()
                    && let Some(Frame::List { items, .. }) = self.frames.last_mut()
                {
                    items.push(item);
                }
            }
            TagEnd::List(_) => {
                self.flush();
                if let Some(Frame::List { start, items }) = self.frames.pop() {
                    self.push_block(Block::List { start, items });
                }
            }
            TagEnd::FootnoteDefinition => {
                self.flush();
                if let Some(Frame::Footnote(v)) = self.frames.pop() {
                    for b in v {
                        self.push_block(b);
                    }
                }
            }
            TagEnd::TableCell => {
                let cell = match self.text.take() {
                    Some((_, p)) => linkify(p),
                    None => Paragraph::default(),
                };
                if let Some(Frame::Table { row, .. }) = self.frames.last_mut() {
                    row.push(cell);
                }
            }
            TagEnd::TableHead | TagEnd::TableRow => {
                if let Some(Frame::Table { rows, row, .. }) = self.frames.last_mut() {
                    rows.push(std::mem::take(row));
                }
            }
            TagEnd::Table => {
                if let Some(Frame::Table { align, rows, .. }) = self.frames.pop() {
                    self.push_block(Block::Table { align, rows });
                }
            }
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link => {
                self.marks.pop();
            }
            TagEnd::Image => {
                if let Some(alt) = self.image.take() {
                    let alt = if alt.is_empty() {
                        "image".to_string()
                    } else {
                        alt
                    };
                    let marks = Marks {
                        muted: true,
                        ..self.marks()
                    };
                    self.inline(&alt, marks);
                }
            }
            _ => {}
        }
    }

    fn event(&mut self, e: Event) {
        match e {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            _ if self.skipping > 0 => {}
            Event::Text(t) => {
                if let Some((_, code)) = self.code.as_mut() {
                    code.push_str(&t);
                } else if let Some(html) = self.html.as_mut() {
                    html.push_str(&t);
                } else if let Some(alt) = self.image.as_mut() {
                    alt.push_str(&t);
                } else {
                    let marks = self.marks();
                    self.inline(&t, marks);
                }
            }
            Event::Code(t) => {
                if let Some(alt) = self.image.as_mut() {
                    alt.push_str(&t);
                } else {
                    let marks = Marks {
                        code: true,
                        ..self.marks()
                    };
                    self.inline(&t, marks);
                }
            }
            Event::Html(t) => {
                if let Some(html) = self.html.as_mut() {
                    html.push_str(&t);
                } else {
                    let marks = self.marks();
                    self.inline(&t, marks);
                }
            }
            Event::InlineHtml(t) => {
                let marks = self.marks();
                self.inline(&t, marks);
            }
            Event::FootnoteReference(label) => {
                let marks = Marks {
                    muted: true,
                    ..self.marks()
                };
                self.inline(&format!("[{label}]"), marks);
            }
            // A soft line break reads as a space, as CommonMark and the web read it.
            Event::SoftBreak => {
                if let Some(alt) = self.image.as_mut() {
                    alt.push(' ');
                } else {
                    let marks = self.marks();
                    self.inline(" ", marks);
                }
            }
            Event::HardBreak => {
                let marks = self.marks();
                self.inline("\n", marks);
            }
            Event::Rule => {
                self.flush();
                self.push_block(Block::Rule);
            }
            Event::TaskListMarker(checked) => {
                if let Some(Frame::Item(item)) = self.frames.last_mut() {
                    item.checked = Some(checked);
                }
            }
            Event::InlineMath(t) | Event::DisplayMath(t) => {
                let marks = Marks {
                    code: true,
                    ..self.marks()
                };
                self.inline(&t, marks);
            }
        }
    }
}

/// Reads `source`'s blocks (`depth` deep in re-readings of HTML blocks' text).
fn convert(source: &str, depth: usize) -> Vec<Block> {
    convert_events(Parser::new_ext(source, options()), depth)
}

/// The blocks the parser's events make.
fn convert_events(parser: Parser, depth: usize) -> Vec<Block> {
    let mut b = Builder {
        frames: vec![Frame::Blocks(Vec::new())],
        text: None,
        marks: Vec::new(),
        code: None,
        html: None,
        image: None,
        skipping: 0,
        depth,
    };
    for e in parser {
        b.event(e);
    }
    b.flush();
    // Anything left open (it can't be, but never mind) is closed into the blocks.
    while b.frames.len() > 1 {
        match b.frames.pop() {
            Some(Frame::Quote(v)) => b.push_block(Block::Quote(v)),
            Some(Frame::List { start, items }) => b.push_block(Block::List { start, items }),
            Some(Frame::Item(item)) => {
                if let Some(Frame::List { items, .. }) = b.frames.last_mut() {
                    items.push(item);
                }
            }
            Some(Frame::Footnote(v)) => {
                for x in v {
                    b.push_block(x);
                }
            }
            Some(Frame::Table { align, rows, .. }) => b.push_block(Block::Table { align, rows }),
            _ => {}
        }
    }
    match b.frames.pop() {
        Some(Frame::Blocks(v)) => v,
        _ => Vec::new(),
    }
}

/// `p` with GitHub's bare links (`https://…`, `www.…`) made links, where its text isn't a link
/// or code already. Found in the whole paragraph, since the parser hands text over in pieces.
fn linkify(p: Paragraph) -> Paragraph {
    let mut found = bare_links(&p.text);
    // Addresses (foo@example.com), where they aren't inside a link found already.
    for e in bare_emails(&p.text) {
        if found.iter().all(|(s, t, _)| e.1 <= *s || e.0 >= *t) {
            found.push(e);
        }
    }
    found.sort_by_key(|f| f.0);
    let found: Vec<(usize, usize, String)> = found
        .into_iter()
        .filter(|(s, e, _)| {
            p.spans
                .iter()
                .all(|(r, m)| (m.link.is_none() && !m.code) || r.end <= *s || r.start >= *e)
        })
        .collect();
    if found.is_empty() {
        return p;
    }
    let mut out = Paragraph {
        text: String::with_capacity(p.text.len()),
        spans: Vec::with_capacity(p.spans.len() + found.len() * 2),
        boxes: p.boxes.clone(),
    };
    for (r, m) in &p.spans {
        let mut at = r.start;
        while at < r.end {
            match found.iter().find(|(s, e, _)| *e > at && *s < r.end) {
                Some((s, e, url)) if *s <= at => {
                    let to = (*e).min(r.end);
                    let marks = Marks {
                        link: Some(url.clone().into()),
                        ..m.clone()
                    };
                    out.push(&p.text[at..to], marks);
                    at = to;
                }
                Some((s, _, _)) => {
                    let to = (*s).min(r.end);
                    out.push(&p.text[at..to], m.clone());
                    at = to;
                }
                None => {
                    out.push(&p.text[at..r.end], m.clone());
                    at = r.end;
                }
            }
        }
    }
    out
}

/// GitHub's bare email addresses in `text` (`foo@example.com`), as `mailto:` links: a local
/// part of letters, digits and `.+-_`, then a domain of letters, digits and `-_` with at least
/// one dot, not ending in `-` or `_`; a full stop after it is the sentence's.
fn bare_emails(text: &str) -> Vec<(usize, usize, String)> {
    let b = text.as_bytes();
    let local = |c: u8| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'+' | b'-' | b'_');
    let domain = |c: u8| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'-' | b'_');
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = text[from..].find('@').map(|i| from + i) {
        from = i + 1;
        let mut start = i;
        while start > 0 && local(b[start - 1]) {
            start -= 1;
        }
        let mut end = i + 1;
        while end < b.len() && domain(b[end]) {
            end += 1;
        }
        while end > i + 1 && b[end - 1] == b'.' {
            end -= 1;
        }
        let host = &text[i + 1..end];
        let ok = start < i
            && host.contains('.')
            && !host.starts_with('.')
            && !host.ends_with(['-', '_'])
            && !host.contains("..");
        if ok {
            out.push((start, end, format!("mailto:{}", &text[start..end])));
            from = end;
        }
    }
    out
}

/// GitHub's bare links in `text`: `http://`, `https://` and `www.` addresses, to the first
/// space or `<`, without the punctuation that ends a sentence after them. Each with where it is
/// and the address it goes to.
fn bare_links(text: &str) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    let mut from = 0;
    while from < text.len() {
        let rest = &text[from..];
        let found = ["https://", "http://", "www."]
            .iter()
            .filter_map(|p| rest.find(p).map(|i| (i, *p)))
            .min_by_key(|(i, _)| *i);
        let Some((i, prefix)) = found else { break };
        let start = from + i;
        // At a word's start only.
        let before = text[..start].chars().next_back();
        if before.is_some_and(|c| c.is_alphanumeric() || c == '/' || c == '.' || c == '@') {
            from = start + prefix.len();
            continue;
        }
        let mut end = text[start..]
            .find(|c: char| c.is_whitespace() || c == '<')
            .map_or(text.len(), |n| start + n);
        // Trailing punctuation, and a closing bracket without its opener, aren't the address's.
        while let Some(c) = text[start..end].chars().next_back() {
            let unmatched_paren = c == ')'
                && text[start..end].matches('(').count() < text[start..end].matches(')').count();
            if matches!(
                c,
                '.' | ',' | ':' | ';' | '!' | '?' | '"' | '\'' | '*' | '_' | '~'
            ) || unmatched_paren
            {
                end -= c.len_utf8();
            } else {
                break;
            }
        }
        let body = &text[start + prefix.len()..end];
        // An address has a dot in its host.
        let host = body.split(['/', '?', '#']).next().unwrap_or("");
        if !body.is_empty()
            && (prefix != "www." || host.contains('.'))
            && (prefix == "www." || host.contains('.'))
        {
            let url = if prefix == "www." {
                format!("http://{}", &text[start..end])
            } else {
                text[start..end].to_string()
            };
            out.push((start, end, url));
        }
        from = end.max(start + prefix.len());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_and_marks() {
        let b = parse(
            "# Title\n\nSome **bold** and [a link](https://x.test) and `code`.\n\n- [x] done\n- [ ] open\n\n```rust\nfn main() {}\n```\n\n> quoted\n\n---\n\n| a | b |\n|:-:|--:|\n| 1 | 2 |\n",
        );
        assert!(matches!(&b[0], Block::Heading(1, p) if p.text == "Title"));
        let Block::Paragraph(p) = &b[1] else { panic!() };
        assert_eq!(p.text, "Some bold and a link and code.");
        assert!(
            p.spans
                .iter()
                .any(|(r, m)| m.bold && &p.text[r.clone()] == "bold")
        );
        assert_eq!(
            p.link_at(p.text.find("link").unwrap()).map(|s| s.as_ref()),
            Some("https://x.test")
        );
        assert!(
            matches!(&b[2], Block::List { start: None, items } if items[0].checked == Some(true) && items[1].checked == Some(false))
        );
        assert!(
            matches!(&b[3], Block::Code { lang: Some(l), text } if l == "rust" && text == "fn main() {}")
        );
        assert!(matches!(&b[4], Block::Quote(_)));
        assert!(matches!(&b[5], Block::Rule));
        assert!(
            matches!(&b[6], Block::Table { align, rows } if align == &[Align::Center, Align::Right] && rows.len() == 2)
        );
    }

    #[test]
    fn html_is_text_and_images_are_their_words() {
        // HTML right before Markdown, with no blank line between: the Markdown is still read.
        let h = parse("<div>raw</div>\n# A heading\n- an item");
        assert!(
            matches!(&h[0], Block::Paragraph(p) if p.text.contains("<div>raw</div>")),
            "{h:?}"
        );
        assert!(
            h.iter()
                .any(|b| matches!(b, Block::Heading(1, p) if p.text == "A heading")),
            "{h:?}"
        );
        assert!(h.iter().any(|b| matches!(b, Block::List { .. })), "{h:?}");
        let b = parse("<script>alert(1)</script>\n\n![a cat](https://x.test/cat.png)");
        assert!(matches!(&b[0], Block::Paragraph(p) if p.text.contains("<script>")));
        assert!(matches!(&b[1], Block::Paragraph(p) if p.text == "a cat" && p.spans[0].1.muted));
    }

    #[test]
    fn a_single_line_ending_is_a_space_and_a_hard_break_a_line() {
        let b = parse(
            "one line\nand the next,  \nthen a hard break\\\nand another\r\n  indented **bold\nacross**",
        );
        let Block::Paragraph(p) = &b[0] else {
            panic!("{b:?}")
        };
        assert_eq!(
            p.text,
            "one line and the next,\nthen a hard break\nand another indented bold across"
        );
        assert!(
            p.spans
                .iter()
                .any(|(r, m)| m.bold && &p.text[r.clone()] == "bold across")
        );
        // In a list item and a quote too; a code block keeps its lines.
        let b = parse("- an item\n  that wraps\n\n> a quote\n> on two lines\n\n```\na\nb\n```");
        let Block::List { items, .. } = &b[0] else {
            panic!("{b:?}")
        };
        assert!(
            matches!(&items[0].blocks[0], Block::Paragraph(p) if p.text == "an item that wraps"),
            "{b:?}"
        );
        let Block::Quote(q) = &b[1] else {
            panic!("{b:?}")
        };
        assert!(
            matches!(&q[0], Block::Paragraph(p) if p.text == "a quote on two lines"),
            "{q:?}"
        );
        assert!(matches!(&b[2], Block::Code { text, .. } if text == "a\nb"));
    }

    #[test]
    fn input_the_old_parser_panicked_on_is_read() {
        // The `markdown` crate panicked on each: a fence opened in a list item, then a list of
        // the other kind; and a table, then a quote with a tab and a backtick.
        for doc in [
            "- a\n  ```\n1. a\n",
            "x\n- ```\n1. a\n",
            "1. a\n   ```\n- b\n",
            "| a | b |\n|---|---|\n> \t`",
        ] {
            assert!(!convert(doc, 0).is_empty(), "{doc:?}");
        }
        // Read as CommonMark reads it: the unclosed fence is code to its item's end, and the
        // other list a list.
        let b = parse("- a\n  ```\n1. a\n");
        assert!(
            matches!(&b[0], Block::List { start: None, items } if items[0].blocks.iter().any(|x| matches!(x, Block::Code { .. }))),
            "{b:?}"
        );
        assert!(matches!(&b[1], Block::List { start: Some(1), .. }), "{b:?}");
    }

    #[test]
    fn bare_addresses_are_links_as_on_github() {
        let b = parse(
            "See https://x.test/a_(b), or www.example.com. Not foo.https://x or `https://code`.",
        );
        let Block::Paragraph(p) = &b[0] else {
            panic!("{b:?}")
        };
        let link = |word: &str| p.link_at(p.text.find(word).unwrap()).map(|s| s.to_string());
        assert_eq!(
            link("https://x.test").as_deref(),
            Some("https://x.test/a_(b)")
        );
        assert_eq!(link("www.").as_deref(), Some("http://www.example.com"));
        assert_eq!(link("foo.https"), None);
        assert_eq!(link("https://code"), None);
        let b = parse("Write to foo.bar+x@example.co.uk. Not @nobody or a@b.");
        let Block::Paragraph(p) = &b[0] else {
            panic!("{b:?}")
        };
        let link = |word: &str| p.link_at(p.text.find(word).unwrap()).map(|s| s.to_string());
        assert_eq!(
            link("foo.bar").as_deref(),
            Some("mailto:foo.bar+x@example.co.uk")
        );
        assert_eq!(link("@nobody"), None);
        assert_eq!(link("a@b"), None);
    }

    /// Random Markdown made of the pieces parsers trip on (list markers, fences, indents,
    /// quotes, tables, line ends, tabs, HTML): none of it may panic the parser, unguarded.
    #[test]
    fn random_markdown_never_panics() {
        const PIECES: [&str; 22] = [
            "- ",
            "* ",
            "+ ",
            "1. ",
            "2) ",
            "10. ",
            "```",
            "~~~",
            "    ",
            "  ",
            "\t",
            "\n",
            "\n\n",
            "> ",
            "# ",
            "| a | b |\n|---|---|\n",
            "a",
            "word ",
            "[x]: http://x\n",
            "[x] ",
            "`",
            "<div>",
        ];
        // A fixed seed: the same documents every run, so a failure can be found again.
        let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let quiet = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let mut failed = None;
        for _ in 0..200_000 {
            let n = 1 + (next() % 24) as usize;
            let doc: String = (0..n)
                .map(|_| PIECES[(next() % PIECES.len() as u64) as usize])
                .collect();
            // The parser itself, unguarded: where panics abort, nothing could catch one.
            if std::panic::catch_unwind(|| convert(&doc, 0)).is_err() {
                failed = Some(doc);
                break;
            }
        }
        std::panic::set_hook(quiet);
        assert_eq!(failed, None, "parse panicked on this");
    }

    #[test]
    fn hostile_nesting_and_size_are_cut_short() {
        let deep = ">".repeat(5000) + " x";
        let _ = parse(&deep);
        let big = "a".repeat(MAX_LEN * 2);
        let b = parse(&big);
        assert!(matches!(&b[0], Block::Paragraph(p) if p.text.len() <= MAX_LEN));
    }
}
