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
    parse_with(source, "", "").blocks
}

/// What [`parse_with`] read: the blocks, and the link reference definitions in the source
/// itself (not the context's), as `[label]: <url>` lines for another source's context.
#[derive(Debug, Default)]
pub struct Parsed {
    pub blocks: Vec<Block>,
    pub definitions: String,
}

/// Reads `source` between `before` and `after`: link reference and footnote definitions from
/// other parts of one document (the sources before it and after it), so a `[text][label]` or a
/// `[^note]` here resolves to one defined there, and of two with one label the first in the
/// document wins, as in one document. Their definitions draw nothing here.
pub fn parse_with(source: &str, before: &str, after: &str) -> Parsed {
    let mut end = source.len().min(MAX_LEN);
    while !source.is_char_boundary(end) {
        end -= 1;
    }
    let source = tex_delimiters(&source[..end]);
    let source = source.as_ref();
    // What fits of the others' definitions, within the limit.
    let cut = |text: &str, room: usize| -> usize {
        let mut n = room.min(text.len());
        while !text.is_char_boundary(n) {
            n -= 1;
        }
        n
    };
    let room = MAX_LEN.saturating_sub(source.len());
    let before = &before[..cut(before, room / 2)];
    let after = &after[..cut(after, room - before.len())];
    let mut doc = String::with_capacity(before.len() + source.len() + after.len() + 4);
    if !before.is_empty() {
        // A blank line after, so the source's first line starts a block of its own.
        doc.push_str(before);
        doc.push_str("\n\n");
    }
    let own = doc.len()..doc.len() + source.len();
    doc.push_str(source);
    if !after.is_empty() {
        // And before, so the definitions can't join the source's last paragraph.
        doc.push_str("\n\n");
        doc.push_str(after);
    }
    // pulldown-cmark doesn't panic on what it's given (it's fuzzed for that, and so is this, in
    // the tests below); the guard is there anyway, where a panic can be caught.
    std::panic::catch_unwind(|| {
        let parser = Parser::new_ext(&doc, options());
        // The source's own definitions, whatever the others define: read alone where they're
        // there, since with them a label they define first hides the source's.
        let mut definitions = if before.is_empty() && after.is_empty() {
            own_definitions(&parser, &own)
        } else {
            own_definitions(&Parser::new_ext(source, options()), &(0..source.len()))
        };
        let (blocks, notes) = convert_events(parser, 0, Some(own.clone()));
        // The source's own footnotes, as stubs for the others to be read with.
        for label in notes {
            if !label.contains(['[', ']', '\\', '\n', '\r']) {
                definitions.push_str(&format!("[^{label}]: .\n"));
            }
        }
        Parsed {
            blocks,
            definitions,
        }
    })
    .unwrap_or_else(|_| Parsed {
        blocks: vec![Block::Paragraph(Paragraph::plain(source))],
        definitions: String::new(),
    })
}

/// The link reference definitions that start in `own` (the source's own, not the others') as
/// `[label]: <url>` lines. A URL that can't be written back so is left out.
fn own_definitions(parser: &Parser, own: &std::ops::Range<usize>) -> String {
    let mut defs: Vec<(usize, String, String)> = parser
        .reference_definitions()
        .iter()
        .filter(|(_, d)| own.contains(&d.span.start))
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

/// The language a display equation's block has: `$$…$$`, or a fence of it (GitHub's
/// ```` ```math ````). The TeX is kept as written and drawn as its source, on one line each line,
/// in a box that scrolls sideways when it's wider than the view.
pub const MATH_LANGUAGE: &str = "math";

/// The language of a display equation shown as its TeX, never typeset: a fence of it
/// (```` ```math-source ````), as a reply being written shows an equation it's still writing, so
/// it doesn't flip between typeset and not as its TeX grows.
pub const MATH_SOURCE_LANGUAGE: &str = "math-source";

/// GitHub's flavour: tables, task lists, strikethrough, footnotes and math (`$…$` inline,
/// `$$…$$` on a block of its own, the TeX in both kept as written).
fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_MATH
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
    /// A display equation just ended a paragraph: the line end after it doesn't start the next
    /// one with a space.
    after_math: bool,
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
        self.after_math = false;
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
            // A soft line break reads as a space, as CommonMark and the web read it (but not
            // right after a display equation: the next words start a paragraph of their own).
            Event::SoftBreak if self.after_math && self.text.is_none() => {}
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
            // A display equation is a block of its own, even written inside a paragraph (`$$…$$`
            // on its own line after a line of words), but not in a heading or a table's cell,
            // which hold only words: there, and inline, it's its TeX in the code font.
            Event::DisplayMath(t)
                if self.image.is_none()
                    && matches!(self.text, None | Some((Into_::Paragraph, _))) =>
            {
                if let Some((_, p)) = self.text.as_mut() {
                    trim_end(p);
                }
                self.flush();
                self.push_block(Block::Code {
                    lang: Some(MATH_LANGUAGE.into()),
                    text: t.trim().to_string(),
                });
                self.after_math = true;
            }
            Event::InlineMath(t) | Event::DisplayMath(t) => {
                if let Some(alt) = self.image.as_mut() {
                    alt.push_str(&t);
                } else {
                    let marks = Marks {
                        code: true,
                        math: true,
                        ..self.marks()
                    };
                    self.inline(&t, marks);
                }
            }
        }
    }
}

/// `\(…\)` as `$…$` and `\[…\]` as `$$…$$`, LaTeX's own delimiters, which models and Pandoc
/// write, outside code: a pair in one paragraph (a `\[` to its `\]` across its lines). One
/// whose backslash is escaped (`\\(`) is words, as is one with no partner.
pub(crate) fn tex_delimiters(source: &str) -> std::borrow::Cow<'_, str> {
    if !source.contains("\\(") && !source.contains("\\[") {
        return std::borrow::Cow::Borrowed(source);
    }
    let b = source.as_bytes();
    // Which bytes are code: fences and indented-free inline code spans.
    let mut code = vec![false; b.len()];
    let mut fence: Option<(u8, usize)> = None;
    let mut at = 0;
    for line in source.split_inclusive('\n') {
        let t = line.trim_start_matches(' ');
        let mark = t.bytes().next().filter(|c| *c == b'`' || *c == b'~');
        let run = mark.map_or(0, |m| t.bytes().take_while(|c| *c == m).count());
        let opens = run >= 3 && line.len() - t.len() <= 3;
        match fence {
            Some((m, n)) => {
                code[at..at + line.len()].iter_mut().for_each(|c| *c = true);
                if opens && mark == Some(m) && run >= n && t[run..].trim().is_empty() {
                    fence = None;
                }
            }
            None if opens => {
                fence = mark.map(|m| (m, run));
                code[at..at + line.len()].iter_mut().for_each(|c| *c = true);
            }
            None => {
                // Inline code: a run of backticks to the next run of the same length.
                let mut i = 0;
                let l = line.as_bytes();
                while i < l.len() {
                    if l[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if l[i] != b'`' {
                        i += 1;
                        continue;
                    }
                    let n = l[i..].iter().take_while(|c| **c == b'`').count();
                    let mut j = i + n;
                    let mut close = None;
                    while j < l.len() {
                        if l[j] == b'`' {
                            let m = l[j..].iter().take_while(|c| **c == b'`').count();
                            if m == n {
                                close = Some(j);
                                break;
                            }
                            j += m;
                        } else {
                            j += 1;
                        }
                    }
                    match close {
                        Some(j) => {
                            code[at + i..at + j + n].iter_mut().for_each(|c| *c = true);
                            i = j + n;
                        }
                        None => i += n,
                    }
                }
            }
        }
        at += line.len();
    }
    // The delimiters outside code, each with whether its backslash is its own.
    let mut marks: Vec<(usize, u8)> = Vec::new();
    let mut i = 0;
    while i + 1 < b.len() {
        if b[i] == b'\\' && !code[i] {
            if matches!(b[i + 1], b'(' | b')' | b'[' | b']') {
                marks.push((i, b[i + 1]));
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    // Pairs: an opener and the next closer of its kind, with no blank line between.
    let mut out = String::with_capacity(source.len());
    let mut copied = 0;
    let mut k = 0;
    while k < marks.len() {
        let (open_at, open) = marks[k];
        let close = match open {
            b'(' => b')',
            b'[' => b']',
            _ => {
                k += 1;
                continue;
            }
        };
        let Some(off) = marks[k + 1..].iter().position(|&(_, c)| c == close) else {
            k += 1;
            continue;
        };
        let (close_at, _) = marks[k + 1 + off];
        let inner = &source[open_at + 2..close_at];
        if inner.contains("\n\n") || inner.contains("\n\r\n") || inner.trim().is_empty() {
            k += 1;
            continue;
        }
        let dollars = if open == b'(' { "$" } else { "$$" };
        out.push_str(&source[copied..open_at]);
        out.push_str(dollars);
        // Inline math's TeX can't start or end with a space (pulldown-cmark's rule).
        out.push_str(if open == b'(' { inner.trim() } else { inner });
        out.push_str(dollars);
        copied = close_at + 2;
        k += 1 + off + 1;
    }
    if copied == 0 {
        return std::borrow::Cow::Borrowed(source);
    }
    out.push_str(&source[copied..]);
    std::borrow::Cow::Owned(out)
}

/// Takes the spaces and line ends off a paragraph's end (the soft break before a display
/// equation that ends it).
fn trim_end(p: &mut Paragraph) {
    let end = p.text.trim_end_matches([' ', '\n']).len();
    if end == p.text.len() {
        return;
    }
    p.text.truncate(end);
    p.spans.retain_mut(|(r, _)| {
        r.end = r.end.min(end);
        r.start < r.end
    });
    p.boxes.retain(|(at, _)| *at < end);
}

/// Reads `source`'s blocks (`depth` deep in re-readings of HTML blocks' text).
fn convert(source: &str, depth: usize) -> Vec<Block> {
    convert_events(Parser::new_ext(source, options()), depth, None).0
}

/// The blocks the parser's events make, and the labels of the footnotes defined. With `own`, a
/// footnote defined outside it (another source's, read here for its label) draws nothing, and
/// only those inside it are told.
fn convert_events(
    parser: Parser,
    depth: usize,
    own: Option<std::ops::Range<usize>>,
) -> (Vec<Block>, Vec<String>) {
    let mut b = Builder {
        frames: vec![Frame::Blocks(Vec::new())],
        text: None,
        marks: Vec::new(),
        code: None,
        html: None,
        image: None,
        skipping: 0,
        depth,
        after_math: false,
    };
    let mut notes = Vec::new();
    for (e, at) in parser.into_offset_iter() {
        if b.skipping == 0
            && let Event::Start(Tag::FootnoteDefinition(label)) = &e
        {
            if own.as_ref().is_some_and(|o| !o.contains(&at.start)) {
                // Another source's footnote: only its label was wanted.
                b.skipping = 1;
                continue;
            }
            notes.push(label.to_string());
        }
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
    let blocks = match b.frames.pop() {
        Some(Frame::Blocks(v)) => v,
        _ => Vec::new(),
    };
    (blocks, notes)
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

    /// The inline code spans of a paragraph: what's marked as code, in order.
    fn code_spans(b: &Block) -> Vec<String> {
        let Block::Paragraph(p) = b else {
            panic!("not a paragraph: {b:?}")
        };
        p.spans
            .iter()
            .filter(|(_, m)| m.code)
            .map(|(r, _)| p.text[r.clone()].to_string())
            .collect()
    }

    fn math(text: &str) -> Block {
        Block::Code {
            lang: Some(MATH_LANGUAGE.into()),
            text: text.into(),
        }
    }

    #[test]
    fn inline_math_is_its_tex_as_written_in_the_code_font() {
        let b = parse(
            r"Here $x_{k+1} = F x_k + w$, the Jacobian $J_{ij}$ of $2n+1$ points, an $m \times n$ matrix ($J_{ij}$), and $a \\ b \, c \{ d \} e \_ f & g * h$.",
        );
        assert_eq!(b.len(), 1, "{b:?}");
        assert_eq!(
            code_spans(&b[0]),
            [
                r"x_{k+1} = F x_k + w",
                r"J_{ij}",
                r"2n+1",
                r"m \times n",
                r"J_{ij}",
                r"a \\ b \, c \{ d \} e \_ f & g * h",
            ]
        );
        let Block::Paragraph(p) = &b[0] else {
            unreachable!()
        };
        assert!(p.text.contains(r"matrix (J_{ij}), and"), "{}", p.text);
        assert!(
            p.spans.iter().all(|(_, m)| !m.italic && !m.bold),
            "no emphasis read in it: {:?}",
            p.spans
        );
    }

    #[test]
    fn display_math_is_a_block_of_its_own_with_its_tex_as_written() {
        let cases = [
            (
                r"$$\frac{\partial f_i}{\partial x_j}$$",
                r"\frac{\partial f_i}{\partial x_j}",
            ),
            (r"$$a \qquad b$$", r"a \qquad b"),
            (
                r"$$f(x + \Delta x) \approx f(x) + J\,\Delta x$$",
                r"f(x + \Delta x) \approx f(x) + J\,\Delta x",
            ),
            (
                "$$J = \\begin{pmatrix} \\cos\\theta & -v\\sin\\theta \\\\ \\sin\\theta & v\\cos\\theta \\end{pmatrix}$$",
                r"J = \begin{pmatrix} \cos\theta & -v\sin\theta \\ \sin\theta & v\cos\theta \end{pmatrix}",
            ),
            (
                r"$$a \\ b \, c \{ d \} e \_ f & g * h$$",
                r"a \\ b \, c \{ d \} e \_ f & g * h",
            ),
        ];
        for (source, tex) in cases {
            assert_eq!(parse(source), [math(tex)], "{source}");
        }
        // Over several lines (a matrix), its line ends kept.
        let b = parse("$$f = \\begin{pmatrix} v\\cos\\theta \\\\\nv\\sin\\theta \\end{pmatrix}$$");
        assert_eq!(
            b,
            [math(
                "f = \\begin{pmatrix} v\\cos\\theta \\\\\nv\\sin\\theta \\end{pmatrix}"
            )]
        );
        // A ```math fence is one too.
        assert_eq!(parse("```math\na^2 + b^2\n```"), [math("a^2 + b^2")]);
    }

    #[test]
    fn display_math_on_its_own_line_parts_the_paragraph_around_it() {
        let b = parse(
            "If $f$ takes $n$ inputs, the Jacobian is:\n$$J_{ij} = \\frac{\\partial f_i}{\\partial x_j}$$\nRow $i$ shows how output $i$ changes.",
        );
        assert_eq!(b.len(), 3, "{b:#?}");
        let Block::Paragraph(p) = &b[0] else {
            panic!("{b:?}")
        };
        assert_eq!(p.text, "If f takes n inputs, the Jacobian is:");
        assert_eq!(b[1], math(r"J_{ij} = \frac{\partial f_i}{\partial x_j}"));
        let Block::Paragraph(p) = &b[2] else {
            panic!("{b:?}")
        };
        assert_eq!(p.text, "Row i shows how output i changes.");
        // In a list item, the same.
        let b = parse("- the model:\n  $$x_{k+1} = F x_k + w$$\n  and so on");
        let Block::List { items, .. } = &b[0] else {
            panic!("{b:?}")
        };
        assert_eq!(items[0].blocks.len(), 3, "{:#?}", items[0].blocks);
        assert_eq!(items[0].blocks[1], math(r"x_{k+1} = F x_k + w"));
    }

    #[test]
    fn latex_delimiters_are_math_too() {
        // \(…\) inline, \[…\] display (across lines too), as models and Pandoc write them.
        let b = parse(r"The model \( x_{k+1} = F x_k \) holds, and \(J_{ij}\) too.");
        assert_eq!(code_spans(&b[0]), [r"x_{k+1} = F x_k", r"J_{ij}"]);
        let Block::Paragraph(p) = &b[0] else {
            unreachable!()
        };
        assert!(p.spans.iter().filter(|(_, m)| m.code).all(|(_, m)| m.math));
        assert_eq!(
            parse(
                "Its Jacobian:\n\\[\nJ_{ij} = \\frac{\\partial f_i}{\\partial x_j}\n\\]\nThen more."
            ),
            parse(
                "Its Jacobian:\n$$\nJ_{ij} = \\frac{\\partial f_i}{\\partial x_j}\n$$\nThen more."
            ),
        );
        // In code, escaped, unpaired or across a blank line: as they were.
        for source in [
            r"Run `echo \(x\)` here.",
            "```tex\n\\[ a \\]\n```",
            r"An escaped \\(x\\) stays.",
            r"Only \( one.",
            "Across \\( a\n\nb \\) paragraphs.",
        ] {
            let b = parse(source);
            assert!(
                b.iter()
                    .all(|b| !matches!(b, Block::Code { lang: Some(l), .. } if l == MATH_LANGUAGE)),
                "{source}: {b:?}"
            );
            if let Some(Block::Paragraph(p)) = b.first() {
                assert!(p.spans.iter().all(|(_, m)| !m.math), "{source}: {p:?}");
            }
        }
    }

    #[test]
    fn a_math_source_fence_is_an_equation_shown_as_its_tex() {
        assert_eq!(
            parse("```math-source\n\\frac{a}{b\n```"),
            [Block::Code {
                lang: Some(MATH_SOURCE_LANGUAGE.into()),
                text: "\\frac{a}{b".into()
            }]
        );
    }

    #[test]
    fn prices_stay_words() {
        for source in [
            "$5",
            "$5 and $10",
            "It costs $5 and $10 a month.",
            "**2 bags, about $230 a month.** Then roughly $53.",
            "From $1,200 to $1,500.",
        ] {
            let b = parse(source);
            assert_eq!(b.len(), 1, "{source}: {b:?}");
            let Block::Paragraph(p) = &b[0] else {
                panic!("{source}: {b:?}")
            };
            assert!(
                p.spans.iter().all(|(_, m)| !m.code),
                "{source}: {:?}",
                p.spans
            );
            assert!(p.text.contains('$'), "{source}: {}", p.text);
        }
        // An escaped dollar, as a host app writes a price, is a dollar.
        let b = parse(r"About \$230, then \$53.");
        let Block::Paragraph(p) = &b[0] else {
            panic!("{b:?}")
        };
        assert_eq!(p.text, "About $230, then $53.");
    }

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

    #[test]
    fn of_two_definitions_the_first_in_the_document_wins() {
        let link = |b: &[Block]| match &b[0] {
            Block::Paragraph(p) => p.link_at(0).map(|s| s.to_string()),
            other => panic!("{other:?}"),
        };
        let first = "[x]: <https://first.test>\n";
        let second = "[x]: <https://second.test>\n";
        assert_eq!(
            link(&parse_with("[t][x]", first, second).blocks).as_deref(),
            Some("https://first.test")
        );
        // An earlier source's wins over the source's own, as in one document.
        let own = "[t][x]\n\n[x]: https://own.test";
        assert_eq!(
            link(&parse_with(own, first, "").blocks).as_deref(),
            Some("https://first.test")
        );
        assert_eq!(
            link(&parse_with(own, "", second).blocks).as_deref(),
            Some("https://own.test")
        );
        // Only the source's own are told to the others.
        let p = parse_with(own, first, second);
        assert_eq!(p.definitions, "[x]: <https://own.test>\n");
    }

    #[test]
    fn a_footnote_defined_in_another_source_resolves_and_draws_there_alone() {
        let defined = parse_with("[^1]: the note", "", "");
        assert_eq!(defined.definitions, "[^1]: .\n");
        assert!(format!("{:?}", defined.blocks).contains("the note"));
        // The reference, read with that source's footnote after it.
        let b = parse_with("Text[^1].", "", &defined.definitions).blocks;
        assert_eq!(b.len(), 1, "the stub draws nothing: {b:?}");
        let Block::Paragraph(p) = &b[0] else {
            panic!("{b:?}")
        };
        assert_eq!(p.text, "Text[1].");
        assert!(
            p.spans
                .iter()
                .any(|(r, m)| m.muted && &p.text[r.clone()] == "[1]")
        );
        // Read alone, it's the text as written.
        let alone = parse_with("Text[^1].", "", "").blocks;
        assert!(format!("{alone:?}").contains("[^1]"), "{alone:?}");
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
