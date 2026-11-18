//! A draft's text as blocks: what's settled (laid out once and kept) and the open tail (read
//! again on each delta), and how the tail shows while it's still being written.

use std::ops::Range;

use gpui::SharedString;
use gpui_core::text::{Block, MATH_SOURCE_LANGUAGE, parse_markdown};

/// The most of the new text settling reads in one go (a frame), a line at a time: past it, the
/// rest waits for the next frame.
pub(crate) const SCAN: usize = 1024 * 1024;

/// The most the checks read again in one go (a frame), past the first check: about a few
/// milliseconds of parsing. Past it, the rest waits for the next frame.
pub(crate) const REREAD: usize = 16 * 1024;

/// How much of a text gpui-core's Markdown parser reads: what's after is dropped.
const CAP: usize = 512 * 1024;

/// A settled block: Markdown that nothing written after it can change.
pub(crate) struct Settled {
    /// Where it is in the text, from its first character to where the next block starts.
    pub range: Range<usize>,
    /// The end of the line after it, which its check read: a change before there unsettles it.
    pub seen: usize,
    /// Its text, without the blank lines around it.
    pub source: SharedString,
    /// Its own number: its key in the TextView.
    pub key: usize,
}

/// A draft's Markdown, split where it can't change any more.
#[derive(Default)]
pub(crate) struct Doc {
    pub text: String,
    pub blocks: Vec<Settled>,
    /// Where the open tail starts: the end of the last settled block.
    pub settled: usize,
    /// Lines that start before this were tried as a block's start already.
    checked: usize,
    /// What the open block's lines before `checked` make it.
    open: Open,
    /// The last check's read of the open block with its line, which it didn't settle: the next
    /// check's read of the block, if the line after is checked too. The tail's start, the end
    /// of what it read, and the blocks.
    read: Option<(usize, usize, Vec<Block>)>,
    /// Bytes the checks read, for the tests: each check reads the open block (unless the last
    /// check read it already) and the line.
    pub reread: usize,
    next_key: usize,
    /// Blocks opened again by a change: one that settles again where it was, with the same
    /// text, keeps its key, so its view keeps its layout.
    spare: Vec<Settled>,
    /// How many blocks were made, for the tests: each is made once.
    pub made: usize,
}

impl Doc {
    pub fn append(&mut self, delta: &str) {
        self.text.push_str(delta);
    }

    /// The whole text in place of what was there. Settled blocks before the first change (and
    /// the line after each, which settling it read) stay as they are; the rest open again.
    pub fn replace(&mut self, text: &str) {
        let mut same = self
            .text
            .bytes()
            .zip(text.bytes())
            .take_while(|(a, b)| a == b)
            .count();
        while !text.is_char_boundary(same) {
            same -= 1;
        }
        if same < self.text.len() {
            self.reopen_from(same);
        }
        self.text.clear();
        self.text.push_str(text);
    }

    /// Opens every block whose text or checked line reaches past `at` again.
    fn reopen_from(&mut self, at: usize) {
        let keep = self.blocks.iter().take_while(|b| b.seen <= at).count();
        self.spare = self.blocks.split_off(keep);
        self.settled = self.blocks.last().map_or(0, |b| b.range.end);
        self.checked = self.settled;
        self.open = Open::default();
        self.read = None;
    }

    /// The open tail: everything after the settled blocks.
    pub fn tail(&self) -> &str {
        &self.text[self.settled..]
    }

    /// The key the tail's view has: the key its first block will have when it settles, so its
    /// view's state carries over; or, when the tail is a block the draft had settled (the last
    /// of a message shorter than its draft), that block's.
    pub fn tail_key(&self) -> usize {
        let source = trim_blank_lines(self.tail());
        self.spare
            .iter()
            .find(|b| b.range.start == self.settled && b.source.as_ref() == source)
            .map_or(self.next_key, |b| b.key)
    }

    /// Settles every block the text has finished. A block is finished at the start of a line
    /// when that line is whole (its newline came) and reading the block and the line together
    /// gives the block and then the line, each as read alone: the line didn't carry on the
    /// block (a list's next item, a paragraph's next line, a table's next row, the inside of a
    /// code fence) and didn't make it something else (a heading's underline, a table's
    /// delimiter row). Nothing after that line can change the block then, as Markdown never
    /// opens a block again once something has closed it; the one exception, a link or a
    /// footnote defined further down, is resolved across blocks by the TextView.
    ///
    /// The check reads the open block, so it's only made at a line that could end it: each new
    /// line is read once, into what the open block is so far ([`Open`]), and a line that surely
    /// carries the block on (the inside of a code fence, a list's next item, a quote's next
    /// line, a paragraph's next line of words) isn't checked. What's read in one go is bounded
    /// ([`SCAN`], [`REREAD`]): true when there's more, for the next frame.
    pub fn settle(&mut self) -> bool {
        self.split(false, true)
    }

    /// After the draft ended with other text ([`Doc::replace`]): settles again, where they
    /// were, the blocks the change opened that are still as they were, so their views keep
    /// their layouts; the text is whole, so its last line counts as finished. No block is split
    /// off that wasn't one before, so what was drawn as one view stays one.
    ///
    /// Bounded as [`Doc::settle`] is: true when there's more, for the next frame.
    pub fn restore(&mut self) -> bool {
        !self.spare.is_empty() && self.split(true, false)
    }

    fn split(&mut self, complete: bool, new: bool) -> bool {
        let mut at = self.checked.max(self.settled);
        let (mut scanned, mut reread) = (0, 0);
        loop {
            let end = match self.text[at..].find('\n') {
                Some(nl) => at + nl + 1,
                None if complete && at < self.text.len() => self.text.len(),
                None => break,
            };
            if scanned >= SCAN {
                return true;
            }
            let line = &self.text[at..end];
            let checks = at > self.settled && !line.trim().is_empty() && self.open.words;
            let step = match self.open.next(line, at - self.settled) {
                Step::Ends if !checks => Step::Check,
                step => step,
            };
            let kind = match step {
                Step::On(kind) => kind,
                // Not checked: the block's first line, or a line of white space.
                Step::Check if !checks => match self.open.kind {
                    Kind::None if !self.open.words => first(line),
                    _ => Kind::Unknown,
                },
                Step::Check => {
                    let cached =
                        matches!(self.read, Some((s, e, _)) if s == self.settled && e == at);
                    let block = if cached { 0 } else { at - self.settled };
                    let cost = block + (end - self.settled) + line.len();
                    if reread > 0 && reread + cost > REREAD {
                        return true;
                    }
                    reread += cost;
                    self.reread += cost;
                    let open = &self.text[self.settled..at];
                    let alone = match self.read.take() {
                        Some((_, _, read)) if cached => read,
                        _ => parse_markdown(open),
                    };
                    let mut both = alone.clone();
                    both.extend(parse_markdown(line));
                    let read = parse_markdown(&self.text[self.settled..end]);
                    if read == both {
                        match self.settle_at(at, end, new) {
                            Some(kind) => kind,
                            None => break,
                        }
                    } else {
                        let kind = after_check(line, &read);
                        self.read = Some((self.settled, end, read));
                        kind
                    }
                }
                // A closed code fence, and a line with words after it: the check would settle
                // it, and the fence is code from end to end.
                Step::Ends => match self.settle_at(at, end, new) {
                    Some(kind) => kind,
                    None => break,
                },
            };
            self.open = self.open.with(kind, &self.text[at..end]);
            scanned += end - at;
            at = end;
            self.checked = end;
        }
        false
    }

    /// Settles the open block before the line at `at` (which ends at `end`), the check having
    /// found it finished: the block that was there before a change, if it's the same, else a
    /// new one if `new`. What the open block is then, from the line; none when it's not
    /// settled (not `new` and no block from before).
    fn settle_at(&mut self, at: usize, end: usize, new: bool) -> Option<Kind> {
        let source = trim_blank_lines(&self.text[self.settled..at]);
        let start = self.settled;
        let old = self
            .spare
            .iter()
            .position(|b| b.range.start == start && b.source.as_ref() == source)
            .map(|i| self.spare.remove(i));
        let block = match old {
            Some(b) => Settled {
                range: start..at,
                seen: end,
                ..b
            },
            None if !new => return None,
            None => {
                self.next_key += 1;
                self.made += 1;
                Settled {
                    range: start..at,
                    seen: end,
                    source: SharedString::from(source.to_string()),
                    key: self.next_key - 1,
                }
            }
        };
        self.blocks.push(block);
        self.settled = at;
        self.open = Open::default();
        self.read = None;
        Some(first(&self.text[at..end]))
    }
}

/// What the open block is, as far as its lines so far show: enough to tell, for most lines, that
/// a line carries the block on, so the check that reads the whole block isn't needed.
///
/// It describes the open text read on its own, from its first line (where a document starts), as
/// the check reads it. Each kind says only what a line surely does; a line it can't be sure of is
/// checked, and if the check doesn't settle the block, the kind becomes `Unknown` (every line
/// checked) unless the check's read shows what it is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Open {
    kind: Kind,
    /// A line so far has words (not only white space).
    words: bool,
    /// The last line was blank (only spaces and tabs).
    blank: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Kind {
    /// No line yet that isn't blank.
    #[default]
    None,
    /// A paragraph, every line since its first carrying it on.
    Paragraph,
    /// A code fence its first line opened, not closed yet: its character and length.
    Fence { mark: u8, len: usize },
    /// Code indented four columns.
    Indented,
    /// A list: a bullet's character or an ordered list's delimiter, and the column its current
    /// item's text starts at.
    List {
        ordered: bool,
        mark: u8,
        content: usize,
    },
    /// A quote.
    Quote,
    /// A table, rows starting with `|`.
    Table,
    /// A code fence its first line opened, and closed: a line with words after it settles it.
    Closed,
    /// One that's ended (a heading, a rule): the next line with words is checked.
    Done,
    /// One the lines don't tell: every line with words is checked.
    Unknown,
}

/// What a line does to the open block.
#[derive(Debug, PartialEq, Eq)]
enum Step {
    /// It surely carries the block on (or it's blank): the check would find the block changed
    /// by it. The block is this then.
    On(Kind),
    /// It may end the block: the check reads the block and the line.
    Check,
    /// It surely ends the block: the check would settle it.
    Ends,
}

impl Open {
    /// What `line` does to the open block, `len` bytes long.
    fn next(self, line: &str, len: usize) -> Step {
        let s = content(line);
        if blank(s) {
            return Step::On(self.kind);
        }
        // A carriage return inside a line ends a line for Markdown: lines aren't what they seem.
        if s.contains('\r') {
            return Step::Check;
        }
        let (col, r) = indent(s);
        // The parser reads the first CAP bytes: past them, the check reads the block alone and
        // with the line the same, and settles it only before a line that reads as nothing on
        // its own (a link's definition).
        if len >= CAP {
            return if col <= 3 && r.starts_with('[') {
                Step::Check
            } else {
                Step::On(self.kind)
            };
        }
        // Where the cap cuts the block or the line, it's checked.
        if len + line.len() + 4 > CAP {
            return Step::Check;
        }
        let on = |carries: bool| {
            if carries {
                Step::On(self.kind)
            } else {
                Step::Check
            }
        };
        let tabbed = s.len() - r.len() != col;
        match self.kind {
            Kind::None | Kind::Done | Kind::Unknown => Step::Check,
            Kind::Closed => Step::Ends,
            // A paragraph's next line, unless it can interrupt the paragraph (an HTML block's
            // start among them) or change it (an underline, a table's delimiter row).
            Kind::Paragraph => on(!self.blank && (col >= 4 || !interrupts(r))),
            Kind::Fence { mark, len: n } => Step::On(if col <= 3 && closes(r, mark, n) {
                Kind::Closed
            } else {
                self.kind
            }),
            Kind::Indented => on(col >= 4),
            Kind::List {
                ordered,
                mark,
                content,
            } => {
                if tabbed {
                    Step::Check
                } else if col >= content {
                    // In the item. A definition there reads as nothing, so the check would
                    // find the list as it was.
                    on(!r.starts_with('['))
                } else if col <= 3 && !rule(r) {
                    // The list's next item.
                    match item(r) {
                        Some((o, m, width)) if o == ordered && m == mark => {
                            Step::On(match item_content(col, width, r) {
                                Some(content) => Kind::List {
                                    ordered,
                                    mark,
                                    content,
                                },
                                // An empty item, or a tab after the marker: in the list, but
                                // where its text starts is harder to say.
                                None => Kind::Unknown,
                            })
                        }
                        _ => Step::Check,
                    }
                } else {
                    Step::Check
                }
            }
            Kind::Quote => on(!self.blank && !tabbed && col <= 3 && r.starts_with('>')),
            Kind::Table => on(!self.blank && !tabbed && col <= 3 && r.starts_with('|')),
        }
    }

    /// With `line` read into the block as `kind`.
    fn with(self, kind: Kind, line: &str) -> Open {
        Open {
            kind,
            words: self.words || !line.trim().is_empty(),
            blank: blank(content(line)),
        }
    }
}

/// What a block is from its first line, read where a document starts.
fn first(line: &str) -> Kind {
    let s = content(line);
    if blank(s) {
        return Kind::None;
    }
    if s.contains('\r') {
        return Kind::Unknown;
    }
    let (col, r) = indent(s);
    if col >= 4 {
        return Kind::Indented;
    }
    if let Some((c, n, info)) = fence(r) {
        // A backtick fence's info has no backticks; one that does is inline code.
        if c == '~' || !info.contains('`') {
            return Kind::Fence {
                mark: c as u8,
                len: n,
            };
        }
    }
    if atx(r) || rule(r) {
        return Kind::Done;
    }
    if r.starts_with('>') {
        return Kind::Quote;
    }
    if let Some((ordered, mark, width)) = item(r) {
        return match item_content(col, width, r) {
            Some(content) if s.len() - r.len() == col => Kind::List {
                ordered,
                mark,
                content,
            },
            _ => Kind::Unknown,
        };
    }
    // A link's definition (which reads as nothing), or an HTML block, which a parser that reads
    // HTML ends by its own rules (some at a closing mark, mid-line); else a paragraph.
    if r.starts_with('[') || html_start(r) {
        return Kind::Unknown;
    }
    Kind::Paragraph
}

/// What a block is after a check that didn't settle it at `line`: a table, when the check read
/// the block and the line as one table and the line is its row; else unknown.
fn after_check(line: &str, read: &[Block]) -> Kind {
    let s = content(line);
    let (col, r) = indent(s);
    let table = matches!(read, [Block::Table { .. }]);
    if table && !s.contains(['\r', '\t']) && col <= 3 && r.starts_with('|') {
        Kind::Table
    } else {
        Kind::Unknown
    }
}

/// A line without its line ending.
fn content(line: &str) -> &str {
    let s = line.strip_suffix('\n').unwrap_or(line);
    s.strip_suffix('\r').unwrap_or(s)
}

/// Whether a line is blank for Markdown: nothing but spaces and tabs.
fn blank(s: &str) -> bool {
    s.bytes().all(|b| b == b' ' || b == b'\t')
}

/// A line's indent in columns (a tab to the next multiple of four), and what comes after it.
fn indent(s: &str) -> (usize, &str) {
    let mut col = 0;
    for (i, b) in s.bytes().enumerate() {
        match b {
            b' ' => col += 1,
            b'\t' => col += 4 - col % 4,
            _ => return (col, &s[i..]),
        }
    }
    (col, "")
}

/// An ATX heading's opening: one to six #s, then a space, a tab or the end.
fn atx(r: &str) -> bool {
    let n = r.bytes().take_while(|b| *b == b'#').count();
    (1..=6).contains(&n) && matches!(r.as_bytes().get(n), None | Some(b' ' | b'\t'))
}

/// A thematic break: three or more of one of `-`, `*` and `_`, with spaces and tabs between.
fn rule(r: &str) -> bool {
    let Some(c) = r.bytes().next().filter(|c| matches!(c, b'-' | b'*' | b'_')) else {
        return false;
    };
    let mut n = 0;
    for b in r.bytes() {
        if b == c {
            n += 1;
        } else if b != b' ' && b != b'\t' {
            return false;
        }
    }
    n >= 3
}

/// A list item's marker: whether it's ordered, its bullet or delimiter, and its width.
fn item(r: &str) -> Option<(bool, u8, usize)> {
    let b = r.as_bytes();
    let after = |w: usize| matches!(b.get(w), None | Some(b' ' | b'\t'));
    if matches!(b.first(), Some(b'-' | b'+' | b'*')) && after(1) {
        return Some((false, b[0], 1));
    }
    let digits = b.iter().take_while(|c| c.is_ascii_digit()).count();
    if (1..=9).contains(&digits) && matches!(b.get(digits), Some(b'.' | b')')) && after(digits + 1)
    {
        return Some((true, b[digits], digits + 1));
    }
    None
}

/// The column a list item's text starts at, from its marker's column and width; none for an
/// empty item or a tab after the marker.
fn item_content(col: usize, width: usize, r: &str) -> Option<usize> {
    let after = &r[width..];
    let spaces = after.bytes().take_while(|b| *b == b' ').count();
    let rest = &after[spaces..];
    if rest.is_empty() || rest.starts_with('\t') {
        return None;
    }
    // Five spaces or more: the text is indented code, a space after the marker.
    Some(col + width + if spaces >= 5 { 1 } else { spaces })
}

/// Whether a line can end a paragraph or change it: a heading, a fence, a quote, a rule, a
/// heading's underline, a list item, a table's delimiter row, a footnote's definition or the
/// start of an HTML block.
fn interrupts(r: &str) -> bool {
    let underline = {
        let t = r.trim_end_matches([' ', '\t']);
        !t.is_empty() && (t.bytes().all(|b| b == b'=') || t.bytes().all(|b| b == b'-'))
    };
    let delimiter_row = r.contains('-') && r.bytes().all(|b| b"|-: \t".contains(&b));
    atx(r)
        || fence(r).is_some()
        || r.starts_with('>')
        || rule(r)
        || underline
        || item(r).is_some()
        || delimiter_row
        || r.starts_with("[^")
        || html_start(r)
}

/// Whether a line, from its first character past the indent, starts an HTML block that can end
/// a paragraph: CommonMark's kinds 1 to 6, which a parser that reads HTML blocks (as GitHub's
/// does) ends a paragraph at. A `<script`, `<pre`, `<style` or `<textarea`; a comment, a
/// processing instruction, a declaration or CDATA (`<!`, `<?`); or a block tag, opening or
/// closing (`<div`, `</p`). Read generously, as a line taken for one is only checked: any `<!`
/// or `<?`, and a name from the list, in any case, followed by anything but a letter or a digit.
/// A line that doesn't start with `<` is never one, so text is told apart at its first byte.
fn html_start(r: &str) -> bool {
    let Some(rest) = r.strip_prefix('<') else {
        return false;
    };
    if rest.starts_with(['!', '?']) {
        return true;
    }
    let name = rest.strip_prefix('/').unwrap_or(rest);
    let n = name.bytes().take_while(u8::is_ascii_alphanumeric).count();
    let name = &name[..n];
    n > 0 && HTML_BLOCK_TAGS.iter().any(|t| name.eq_ignore_ascii_case(t))
}

/// The tags that start an HTML block able to end a paragraph: kind 1's, then kind 6's
/// (CommonMark 0.31).
#[rustfmt::skip]
const HTML_BLOCK_TAGS: &[&str] = &[
    "script", "pre", "style", "textarea", "address", "article", "aside", "base", "basefont",
    "blockquote", "body", "caption", "center", "col", "colgroup", "dd", "details", "dialog", "dir",
    "div", "dl", "dt", "fieldset", "figcaption", "figure", "footer", "form", "frame", "frameset",
    "h1", "h2", "h3", "h4", "h5", "h6", "head", "header", "hr", "html", "iframe", "legend", "li",
    "link", "main", "menu", "menuitem", "nav", "noframes", "ol", "optgroup", "option", "p", "param",
    "search", "section", "summary", "table", "tbody", "td", "tfoot", "th", "thead", "title", "tr",
    "track", "ul",
];

/// Whether a line closes a fence of `mark` at least `len` long.
fn closes(r: &str, mark: u8, len: usize) -> bool {
    let n = r.bytes().take_while(|b| *b == mark).count();
    n >= len && blank(&r[n..])
}

/// Text without the blank lines before it and the white space after it.
pub(crate) fn trim_blank_lines(s: &str) -> &str {
    let mut start = 0;
    for line in s.split_inclusive('\n') {
        if !line.trim().is_empty() {
            break;
        }
        start += line.len();
    }
    s[start..].trim_end()
}

/// Characters that only mark Markdown up: a line made of nothing else can't be read yet (a
/// list's number, a heading's #, a rule or a heading's underline, a fence, a table's delimiter
/// row), so while it's still being written it isn't shown.
fn only_marks(line: &str) -> bool {
    !line.trim().is_empty()
        && line.chars().all(|c| {
            c.is_ascii_digit()
                || matches!(
                    c,
                    ' ' | '\t'
                        | '\r'
                        | '-'
                        | '='
                        | '*'
                        | '_'
                        | '#'
                        | '>'
                        | '|'
                        | '`'
                        | '~'
                        | '+'
                        | ':'
                        | '.'
                        | ')'
                        | '['
                        | ']'
                )
        })
}

/// A fence's opening or closing line: its character and length.
fn fence(line: &str) -> Option<(char, usize, &str)> {
    let t = line.trim_start_matches(' ');
    if line.len() - t.len() > 3 {
        return None;
    }
    let c = t.chars().next().filter(|c| *c == '`' || *c == '~')?;
    let n = t.chars().take_while(|x| *x == c).count();
    (n >= 3).then(|| (c, n, &t[n..]))
}

/// Where the display equation the text ends inside starts (its `$$` or `\\[`), and its
/// opener's length, if it does: `$$`s outside code fences, code spans and backslash escapes,
/// taken in pairs, or a `\\[` with no `\\]` after it.
fn open_display_math(s: &str) -> Option<(usize, usize)> {
    // Where it opened, and with what (`$` or `[`).
    let mut open: Option<(usize, u8)> = None;
    let mut fenced: Option<(char, usize)> = None;
    let mut at = 0;
    for line in s.split('\n') {
        match (fenced, fence(line)) {
            (None, Some((c, n, _))) => fenced = Some((c, n)),
            (Some((c, n)), Some((c2, n2, rest)))
                if c == c2 && n2 >= n && rest.trim().is_empty() =>
            {
                fenced = None;
            }
            (Some(_), _) => {}
            (None, None) => {
                let (code, _) = code_spans(line);
                let b = line.as_bytes();
                let mut i = 0;
                while i < b.len() {
                    if b[i] == b'\\' {
                        match (b.get(i + 1), open) {
                            (Some(b'['), None) if !code[i] => open = Some((at + i, b'[')),
                            (Some(b']'), Some((_, b'['))) if !code[i] => open = None,
                            _ => {}
                        }
                        i += 2;
                    } else if !code[i] && b[i..].starts_with(b"$$") {
                        open = match open {
                            Some((_, b'$')) => None,
                            None => Some((at + i, b'$')),
                            other => other,
                        };
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
            }
        }
        at += line.len() + 1;
    }
    open.map(|(o, _)| (o, 2))
}

/// Where the code fence the text ends inside starts, if it does.
fn open_fence(s: &str) -> Option<usize> {
    let mut open: Option<(char, usize, usize)> = None;
    let mut at = 0;
    for line in s.split('\n') {
        match (open, fence(line)) {
            (None, Some((c, n, info))) => {
                // A backtick fence's info has no backticks.
                if c == '~' || !info.contains('`') {
                    open = Some((c, n, at));
                }
            }
            (Some((c, n, _)), Some((c2, n2, rest)))
                if c == c2 && n2 >= n && rest.trim().is_empty() =>
            {
                open = None;
            }
            _ => {}
        }
        at += line.len() + 1;
    }
    open.map(|(_, _, at)| at)
}

/// How the open tail shows while it's still being written: what can't be read yet is held
/// back and what's open is closed for now. (The caret isn't text: the TextView draws it after
/// the last character.)
///
/// - A last line of only marks (`1.`, `#`, `---`, a fence's backticks) waits for its words or
///   its newline, so a paragraph doesn't flash as a heading and back.
/// - A line that starts a table waits for its delimiter row, so it doesn't show as text first.
/// - An open code fence draws as code.
/// - Emphasis, strike and inline code open on the last line are closed, so the words being
///   written are already bold (and never the rest of the text and back); a marker with nothing
///   after it yet isn't shown.
/// - A link being written shows as its words.
/// - A display equation being written (`$$` not closed yet) draws as one, its TeX as written:
///   it's closed for now, and an opening `$$` with nothing after it yet isn't shown.
pub(crate) fn display(tail: &str) -> String {
    let mut s = tail.to_string();
    // A last line still being written that's only marks.
    let start = s.rfind('\n').map_or(0, |i| i + 1);
    if only_marks(&s[start..]) {
        s.truncate(start);
    }
    if open_fence(&s).is_some() {
        let t = s.trim_end_matches(['\n', '\r']).len();
        s.truncate(t);
        return s;
    }
    hold_table_header(&mut s);
    let t = s.trim_end().len();
    s.truncate(t);
    // A display equation still being written: its TeX, in its box, not typeset until it's
    // closed (typeset as it grows, its TeX would flip between typeset and not, as a command's
    // name or a group is half written). An opening with nothing after it yet isn't shown.
    if let Some((at, len)) = open_display_math(&s) {
        let tex = s[at + len..].trim().to_string();
        s.truncate(at);
        let t = s.trim_end().len();
        s.truncate(t);
        if tex.is_empty() {
            return display_words(s);
        }
        if !s.is_empty() {
            s.push_str("\n\n");
        }
        let fence = "`".repeat(longest_run(&tex, '`').max(2) + 1);
        s.push_str(&format!("{fence}{MATH_SOURCE_LANGUAGE}\n{tex}\n{fence}"));
        return s;
    }
    let start = s.rfind('\n').map_or(0, |i| i + 1);
    // Inline math open on the last line: its TeX as inline code, what's open before it closed
    // after it, until it's closed and typeset. A `$` with nothing after it yet isn't shown.
    if let Some((i, len)) = open_inline_math(&s[start..]) {
        let at = start + i;
        let tex = s[at + len..].trim().to_string();
        let before = s[start..at].to_string();
        s.truncate(start);
        if tex.is_empty() {
            s.push_str(before.trim_end());
            return display_words(s);
        }
        let words = before.trim_end();
        let (shown, closers) = close_line(words);
        s.push_str(&shown);
        s.push_str(&before[words.len()..]);
        let ticks = "`".repeat(longest_run(&tex, '`') + 1);
        let pad = if tex.starts_with('`') || tex.ends_with('`') {
            " "
        } else {
            ""
        };
        s.push_str(&format!("{ticks}{pad}{tex}{pad}{ticks}"));
        s.push_str(&closers);
        return s;
    }
    display_words(s)
}

/// The rest of [`display`]: the last line's open emphasis, strike, code and link closed.
fn display_words(mut s: String) -> String {
    let start = s.rfind('\n').map_or(0, |i| i + 1);
    let last = &s[start..];
    // Nothing, or a closing fence or a rule: nothing's open.
    if last.is_empty() || only_marks(last) {
        return s;
    }
    let (shown, closers) = close_line(last);
    s.truncate(start);
    s.push_str(&shown);
    s.push_str(&closers);
    s
}

/// The longest run of `c` in `s`.
fn longest_run(s: &str, c: char) -> usize {
    let (mut best, mut run) = (0, 0);
    for ch in s.chars() {
        run = if ch == c { run + 1 } else { 0 };
        best = best.max(run);
    }
    best
}

/// Where inline math the line ends inside starts (its `$` or `\\(`), and its opener's length,
/// if it does: a `$` outside code spans and escapes, not part of a `$$`, and not a price's (one
/// before a digit or a space, which pulldown-cmark doesn't open math with either), not closed by
/// a `$` after it; or a `\\(` with no `\\)` after it.
fn open_inline_math(line: &str) -> Option<(usize, usize)> {
    let (code, _) = code_spans(line);
    let b = line.as_bytes();
    let mut open = None;
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' {
            if !code[i] && open.is_none() && b.get(i + 1) == Some(&b'(') {
                open = Some((i, 2));
            } else if !code[i] && b.get(i + 1) == Some(&b')') && open.is_some_and(|(_, n)| n == 2) {
                open = None;
            }
            i += 2;
            continue;
        }
        if code[i] || b[i] != b'$' {
            i += 1;
            continue;
        }
        if b.get(i + 1) == Some(&b'$') {
            i += 2;
            continue;
        }
        open = match open {
            Some((_, 1)) => None,
            Some(o) => Some(o),
            None if b
                .get(i + 1)
                .is_some_and(|c| c.is_ascii_digit() || c.is_ascii_whitespace()) =>
            {
                None
            }
            None => Some((i, 1)),
        };
        i += 1;
    }
    open
}

/// Holds back a last line that would start a table until its delimiter row comes.
fn hold_table_header(s: &mut String) {
    let trimmed = s.trim_end_matches(['\n', '\r', ' ', '\t']);
    let start = trimmed.rfind('\n').map_or(0, |i| i + 1);
    let last = &trimmed[start..];
    let piped = |l: &str| l.trim_start().starts_with('|');
    if !piped(last) {
        return;
    }
    let before = trimmed[..start].trim_end_matches(['\n', '\r']);
    let prev = before.rsplit('\n').next().unwrap_or("");
    if start == 0 || !piped(prev) {
        s.truncate(start);
    }
}

/// Splits a line into its block marks (indent, quote marks, a list's marker and task box, a
/// heading's #s) and its words.
fn block_prefix(line: &str) -> (&str, &str) {
    let b = line.as_bytes();
    let mut i = 0;
    loop {
        let from = i;
        while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
            i += 1;
        }
        if i < b.len() && b[i] == b'>' {
            i += 1;
            continue;
        }
        // A bullet, or a number, then a space.
        if i + 1 < b.len() && matches!(b[i], b'-' | b'*' | b'+') && b[i + 1] == b' ' {
            i += 2;
            continue;
        }
        let digits = b[i..].iter().take_while(|c| c.is_ascii_digit()).count();
        if digits > 0
            && digits < 10
            && i + digits + 1 < b.len()
            && matches!(b[i + digits], b'.' | b')')
            && b[i + digits + 1] == b' '
        {
            i += digits + 2;
            continue;
        }
        if b[i..].starts_with(b"[ ] ") || b[i..].starts_with(b"[x] ") || b[i..].starts_with(b"[X] ")
        {
            i += 4;
            continue;
        }
        let hashes = b[i..].iter().take_while(|c| **c == b'#').count();
        if (1..=6).contains(&hashes) && b.get(i + hashes) == Some(&b' ') {
            i += hashes + 1;
            continue;
        }
        if i == from {
            break;
        }
    }
    line.split_at(i)
}

/// Which bytes are inside closed inline code (backticks included), and where an unclosed run
/// of backticks starts, with its length.
fn code_spans(s: &str) -> (Vec<bool>, Option<(usize, usize)>) {
    let b = s.as_bytes();
    let mut mask = vec![false; b.len()];
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' {
            i += 2;
            continue;
        }
        if b[i] != b'`' {
            i += 1;
            continue;
        }
        let n = b[i..].iter().take_while(|c| **c == b'`').count();
        // The next run of exactly n closes it.
        let mut j = i + n;
        let mut close = None;
        while j < b.len() {
            if b[j] == b'`' {
                let m = b[j..].iter().take_while(|c| **c == b'`').count();
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
                mask[i..j + n].iter_mut().for_each(|m| *m = true);
                i = j + n;
            }
            None => return (mask, Some((i, n))),
        }
    }
    (mask, None)
}

/// The last line's words with what's open closed for now: the words to show, and the marks
/// that close what's open, to go after them.
fn close_line(line: &str) -> (String, String) {
    let (prefix, body) = block_prefix(line);
    // Four spaces in is code, shown as it is.
    if prefix.len() - prefix.trim_start().len() >= 4 && !prefix.contains(['>', '-', '*', '+']) {
        return (line.to_string(), String::new());
    }
    let mut words = body.to_string();
    let mut closers = String::new();
    // Inline code still open: its words are code; a run of backticks with nothing after it yet
    // isn't shown.
    let (_, open_code) = code_spans(&words);
    let mut code_tail = String::new();
    if let Some((at, n)) = open_code {
        if words[at + n..].trim().is_empty() {
            words.truncate(at);
        } else {
            code_tail = words[at..].to_string();
            words.truncate(at);
            closers.push_str(&"`".repeat(n));
        }
    }
    close_link(&mut words);
    let marks = close_emphasis(&mut words, !code_tail.is_empty());
    let mut shown = format!("{prefix}{words}");
    shown.push_str(&code_tail);
    if code_tail.is_empty() {
        let t = shown.trim_end().len();
        shown.truncate(t);
    }
    closers.push_str(&marks);
    (shown, closers)
}

/// A link (or an image) still being written shows as its words: `[words`, `[words]` and
/// `[words](https://exam` all show as `words`.
fn close_link(s: &mut String) {
    let (mask, _) = code_spans(s);
    let b = s.as_bytes();
    let Some(open) = (0..b.len())
        .rev()
        .find(|&i| b[i] == b'[' && !mask[i] && (i == 0 || b[i - 1] != b'\\'))
    else {
        return;
    };
    let from = if open > 0 && b[open - 1] == b'!' {
        open - 1
    } else {
        open
    };
    match s[open..].find(']') {
        None => {
            let words = s[open + 1..].to_string();
            s.truncate(from);
            s.push_str(&words);
        }
        Some(rel) => {
            let close = open + rel;
            let after = &s[close + 1..];
            let pending = after.is_empty() || (after.starts_with('(') && !after.contains(')'));
            if pending {
                let words = s[open + 1..close].to_string();
                s.truncate(from);
                s.push_str(&words);
            }
        }
    }
}

/// Closes the emphasis and strikes open at the end of `s`, as their markers: the text to put
/// after the words. An opener with nothing after it yet comes off `s`.
fn close_emphasis(s: &mut String, more: bool) -> String {
    let (mask, _) = code_spans(s);
    let b = s.as_bytes();
    let mut stack: Vec<(u8, usize, usize)> = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if mask[i] {
            i += 1;
            continue;
        }
        let c = b[i];
        if c == b'\\' {
            i += 2;
            continue;
        }
        if !matches!(c, b'*' | b'_' | b'~') {
            i += 1;
            continue;
        }
        let n = b[i..].iter().take_while(|x| **x == c).count();
        let before = s[..i].chars().next_back();
        let after = s[i + n..].chars().next();
        let left = after.is_some_and(|a| !a.is_whitespace());
        let right = before.is_some_and(|p| !p.is_whitespace());
        let inside_word =
            before.is_some_and(char::is_alphanumeric) && after.is_some_and(char::is_alphanumeric);
        if (c == b'_' && inside_word) || (c == b'~' && n > 2) {
            i += n;
            continue;
        }
        if right && stack.last().is_some_and(|t| t.0 == c) {
            stack.pop();
        } else if left || (after.is_none() && !right) {
            stack.push((c, n.min(3), i));
        }
        i += n;
    }
    let mut closers = String::new();
    while let Some((c, n, at)) = stack.pop() {
        if !more && s[at + n..].trim().is_empty() {
            s.truncate(at);
            continue;
        }
        closers.push_str(&(c as char).to_string().repeat(n));
    }
    closers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_formula_being_written_shows_as_its_tex_until_its_closed() {
        // A display equation open: its TeX in a math-source fence (drawn in its box, never
        // typeset), the words before it kept.
        assert_eq!(
            display("Text:\n$$x_{k+1} = F"),
            "Text:\n\n```math-source\nx_{k+1} = F\n```"
        );
        assert_eq!(
            display("Text:\n$$\\frac{a}{\\"),
            "Text:\n\n```math-source\n\\frac{a}{\\\n```"
        );
        assert_eq!(
            display("Text:\n\\[ J_{ij} = \\frac{a"),
            "Text:\n\n```math-source\nJ_{ij} = \\frac{a\n```"
        );
        // Just opened, nothing after it: not shown yet.
        assert_eq!(display("Text:\n$$"), "Text:");
        assert_eq!(display("Some **bold** then $$"), "Some **bold** then");
        // Closed, or in code: nothing open.
        assert_eq!(display("a $$x$$ b"), "a $$x$$ b");
        assert_eq!(display("a \\[x\\] b"), "a \\[x\\] b");
        assert_eq!(display("run `echo $$` now"), "run `echo $$` now");
        assert_eq!(
            display("```sh\necho $$\n```\nok"),
            "```sh\necho $$\n```\nok"
        );
        // An escaped one is a dollar and a dollar; prices aren't touched.
        assert_eq!(display(r"costs \$$5"), r"costs \$$5");
        assert_eq!(display("It costs $5 and $10"), "It costs $5 and $10");
        // Inline math open on the last line: its TeX as inline code, emphasis open before it
        // closed after it.
        assert_eq!(display("so $x_{k+1"), "so `x_{k+1`");
        assert_eq!(display("**so $x_{k"), "**so `x_{k`**");
        assert_eq!(display("so \\(J_{i"), "so `J_{i`");
        assert_eq!(display("a `b` then $c`d"), "a `b` then ``c`d``");
        assert_eq!(
            display("an $m \\times n$ matrix"),
            "an $m \\times n$ matrix"
        );
        assert_eq!(display("so $"), "so");
    }

    /// Career's two replies streamed a few characters at a time: a formula is never typeset
    /// and then shown as its TeX again, or typeset differently. Each frame's typeset formulas
    /// are the last frame's and maybe more, ending with every formula of the reply typeset.
    #[test]
    fn a_streamed_formula_never_flickers() {
        use gpui_core::text::{MATH_LANGUAGE, parse_markdown};
        fn typeset(b: &Block, out: &mut Vec<String>) {
            match b {
                Block::Paragraph(p) | Block::Heading(_, p) => out.extend(
                    p.spans
                        .iter()
                        .filter(|(_, m)| m.math)
                        .map(|(r, _)| p.text[r.clone()].to_string()),
                ),
                Block::Code {
                    lang: Some(l),
                    text,
                } if l == MATH_LANGUAGE => out.push(text.clone()),
                Block::List { items, .. } => items
                    .iter()
                    .flat_map(|i| &i.blocks)
                    .for_each(|b| typeset(b, out)),
                Block::Quote(bs) => bs.iter().for_each(|b| typeset(b, out)),
                _ => {}
            }
        }
        let reply = include_str!("../../examples/replies/career-math.md");
        for step in [1, 3, 7] {
            let mut doc = Doc::default();
            let mut before: Vec<String> = Vec::new();
            let chars: Vec<usize> = reply.char_indices().map(|(i, _)| i).collect();
            let mut at = 0;
            while at < reply.len() {
                let end = chars
                    .iter()
                    .copied()
                    .find(|&i| i >= at + step)
                    .unwrap_or(reply.len());
                doc.append(&reply[at..end]);
                doc.settle();
                at = end;
                let mut now = Vec::new();
                for b in &doc.blocks {
                    parse_markdown(&b.source)
                        .iter()
                        .for_each(|b| typeset(b, &mut now));
                }
                parse_markdown(&display(doc.tail()))
                    .iter()
                    .for_each(|b| typeset(b, &mut now));
                assert!(
                    now.starts_with(&before),
                    "step {step}, at {at}: {before:?} then {now:?}"
                );
                before = now;
            }
            // Every formula typeset by the end: Career's replies have 4 display equations and
            // 29 inline ones.
            assert_eq!(before.len(), 33, "step {step}: {before:#?}");
            for tex in &before {
                assert!(
                    gpui_core::text::math::typeset(tex, true).is_some(),
                    "KaTeX reads {tex}"
                );
            }
        }
    }

    /// Career's two replies, streamed a few characters at a time: what's drawn never shows an
    /// equation's TeX as words (with its backslashes eaten), never loses the words before it,
    /// and ends as the whole reply reads.
    #[test]
    fn a_streamed_reply_with_math_never_shows_its_tex_as_words() {
        use gpui_core::text::{MATH_LANGUAGE, parse_markdown};
        let reply = include_str!("../../examples/replies/career-math.md");
        // A paragraph's words, with no TeX command in them.
        fn words_only(b: &Block, at: usize) {
            match b {
                Block::Paragraph(p) | Block::Heading(_, p) => {
                    let words: String = p
                        .spans
                        .iter()
                        .filter(|(_, m)| !m.code)
                        .map(|(r, _)| &p.text[r.clone()])
                        .collect();
                    for tex in ["\\frac", "\\partial", "\\begin", "\\cos", "\\Delta", "_{"] {
                        assert!(!words.contains(tex), "at {at}: {tex} in {words:?}");
                    }
                }
                Block::List { items, .. } => items
                    .iter()
                    .flat_map(|i| &i.blocks)
                    .for_each(|b| words_only(b, at)),
                Block::Quote(bs) => bs.iter().for_each(|b| words_only(b, at)),
                _ => {}
            }
        }
        let mut doc = Doc::default();
        let mut at = 0;
        let bytes: Vec<usize> = reply.char_indices().map(|(i, _)| i).collect();
        while at < reply.len() {
            let end = bytes
                .iter()
                .copied()
                .find(|&i| i >= at + 5)
                .unwrap_or(reply.len());
            doc.append(&reply[at..end]);
            doc.settle();
            at = end;
            let mut blocks = Vec::new();
            for b in &doc.blocks {
                blocks.extend(parse_markdown(&b.source));
            }
            blocks.extend(parse_markdown(&display(doc.tail())));
            for b in &blocks {
                words_only(b, at);
            }
        }
        // Done: every display equation is a block of its own, its TeX intact.
        let mut all = Vec::new();
        for b in &doc.blocks {
            all.extend(parse_markdown(&b.source));
        }
        all.extend(parse_markdown(doc.tail()));
        let equations: Vec<&str> = all
            .iter()
            .filter_map(|b| match b {
                Block::Code {
                    lang: Some(l),
                    text,
                } if l == MATH_LANGUAGE => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(equations.len(), 4, "{equations:#?}");
        assert_eq!(equations[1], r"J_{ij} = \frac{\partial f_i}{\partial x_j}");
        assert!(
            equations[3].contains(r"\cos\theta & -v\sin\theta \\ \sin\theta"),
            "{}",
            equations[3]
        );
    }

    #[test]
    fn open_things_close_for_now() {
        assert_eq!(display("Some **bold"), "Some **bold**");
        assert_eq!(display("Some **bold** and *it"), "Some **bold** and *it*");
        assert_eq!(display("a **b *c"), "a **b *c***");
        assert_eq!(display("~~gone"), "~~gone~~");
        assert_eq!(display("run `cargo te"), "run `cargo te`");
        assert_eq!(display("x `` a ` b"), "x `` a ` b``");
        // A marker with nothing after it yet isn't shown.
        assert_eq!(display("Some **"), "Some");
        assert_eq!(display("Some `"), "Some");
        // Not emphasis: a times sign, a word's underscores.
        assert_eq!(display("2 * 3 is 6"), "2 * 3 is 6");
        // A line of only numbers and marks waits for its words or its newline.
        assert_eq!(display("Sum:\n2 * 3 = 6"), "Sum:");
        assert_eq!(display("snake_case_name"), "snake_case_name");
        // Closed already.
        assert_eq!(display("**done**"), "**done**");
        assert_eq!(display("`a*b`"), "`a*b`");
    }

    #[test]
    fn a_link_being_written_shows_as_its_words() {
        assert_eq!(display("see [the do"), "see the do");
        assert_eq!(display("see [the docs]"), "see the docs");
        assert_eq!(display("see [the docs](https://exa"), "see the docs");
        assert_eq!(display("see ![a cat](https://x"), "see a cat");
        assert_eq!(
            display("see [docs](https://x.test) now"),
            "see [docs](https://x.test) now"
        );
        assert_eq!(display("- [ ] a task"), "- [ ] a task");
    }

    #[test]
    fn marks_wait_for_their_words() {
        assert_eq!(display("Para\n\n#"), "Para");
        assert_eq!(display("Para\n1."), "Para");
        assert_eq!(display("Title\n=="), "Title");
        assert_eq!(display("Para\n\n# Head"), "Para\n\n# Head");
        assert_eq!(display("- one\n- **tw"), "- one\n- **tw**");
        // A finished rule or fence.
        assert_eq!(display("Para\n\n---\n"), "Para\n\n---");
        assert_eq!(display("```\ncode\n```\n"), "```\ncode\n```");
    }

    #[test]
    fn an_open_fence_draws_as_code() {
        assert_eq!(display("```rust\nfn main() {\n"), "```rust\nfn main() {");
        assert_eq!(display("```rust\n"), "```rust");
        // Its closing backticks wait for the newline: the block stays open meanwhile.
        assert_eq!(display("```\ncode\n``"), "```\ncode");
        // Nothing inside is closed.
        assert_eq!(display("```\na **b"), "```\na **b");
    }

    #[test]
    fn a_table_waits_for_its_delimiter_row() {
        assert_eq!(display("Intro\n\n| a | b |\n"), "Intro");
        assert_eq!(display("Intro\n\n| a | b |\n|--"), "Intro");
        assert_eq!(
            display("| a | b |\n|---|---|\n| 1 | x"),
            "| a | b |\n|---|---|\n| 1 | x"
        );
    }

    fn settled(text: &str) -> Vec<String> {
        let mut d = Doc::default();
        d.append(text);
        d.settle();
        d.blocks.iter().map(|b| b.source.to_string()).collect()
    }

    #[test]
    fn blocks_settle_when_the_next_line_can_t_change_them() {
        assert_eq!(settled("Para one\n\nPara two\n"), ["Para one"]);
        assert_eq!(settled("Para one\n\nPara two"), Vec::<String>::new());
        assert_eq!(settled("# Head\nText\nmore\n"), ["# Head"]);
        // A paragraph's next line, a list's next item, a table's next row carry it on.
        assert_eq!(settled("one\ntwo\n"), Vec::<String>::new());
        assert_eq!(settled("- a\n- b\n- c\n"), Vec::<String>::new());
        assert_eq!(settled("- a\n\n  more of a\n"), Vec::<String>::new());
        assert_eq!(
            settled("| a |\n|---|\n| 1 |\n| 2 |\n"),
            Vec::<String>::new()
        );
        // An underline makes the paragraph a heading: then the heading settles.
        assert_eq!(settled("Title\n---\nText\n"), ["Title\n---"]);
        // A fence holds everything until it closes.
        assert_eq!(settled("```\na\n\nb\n```\n\nAfter\n"), ["```\na\n\nb\n```"]);
        assert_eq!(
            settled("- a\n- b\n\nAfter\n\nMore\n"),
            ["- a\n- b", "After"]
        );
    }

    #[test]
    fn a_change_reopens_only_what_it_reaches() {
        let mut d = Doc::default();
        d.append("One\n\nTwo\n\nThree\n\nFour\n");
        d.settle();
        assert_eq!(d.blocks.len(), 3);
        let keys: Vec<usize> = d.blocks.iter().map(|b| b.key).collect();
        let made = d.made;
        // The same text again, longer: nothing reopens.
        d.replace("One\n\nTwo\n\nThree\n\nFour\nand more");
        assert_eq!(d.blocks.len(), 3);
        // "Three" changed: it opens again, and so does "Two", whose check read its line.
        d.replace("One\n\nTwo\n\nThree!\n\nFour\n");
        assert_eq!(d.blocks.len(), 1);
        d.settle();
        assert_eq!(d.blocks.len(), 3);
        assert_eq!(d.blocks[2].source.as_ref(), "Three!");
        // "Two" settled again as it was, under its key: its view keeps its layout.
        assert_eq!(
            d.blocks[..2].iter().map(|b| b.key).collect::<Vec<_>>(),
            keys[..2]
        );
        assert_eq!(d.made, made + 1, "only Three! was made again");
        // A shorter text, as the first of a reply's messages: what's past it goes, and with
        // the text whole its last line settles what's before it.
        d.replace("One\n\nTw");
        d.restore();
        assert_eq!(d.blocks.len(), 1);
        assert_eq!(d.blocks[0].key, keys[0]);
        assert_eq!(d.tail(), "Tw");
        // Cut at a block's end, the last block is the tail, under its key.
        d.replace("One\n\nTwo\n\nThree!\n\nFour\n");
        d.settle();
        d.replace("One\n\nTwo");
        d.restore();
        assert_eq!(d.blocks.len(), 1);
        assert_eq!(d.tail_key(), keys[1]);
    }

    /// Settling as it was before it kept what the open block is: every line checked, reading the
    /// open block again. Settling now must settle the same blocks, at the same places.
    mod before {
        use super::super::{Settled, trim_blank_lines};
        use gpui::SharedString;
        use gpui_core::text::parse_markdown;

        #[derive(Default)]
        pub struct Doc {
            pub text: String,
            pub blocks: Vec<Settled>,
            pub settled: usize,
            checked: usize,
            next_key: usize,
            spare: Vec<Settled>,
            pub made: usize,
        }

        impl Doc {
            pub fn append(&mut self, delta: &str) {
                self.text.push_str(delta);
            }

            pub fn replace(&mut self, text: &str) {
                let mut same = self
                    .text
                    .bytes()
                    .zip(text.bytes())
                    .take_while(|(a, b)| a == b)
                    .count();
                while !text.is_char_boundary(same) {
                    same -= 1;
                }
                if same < self.text.len() {
                    let keep = self.blocks.iter().take_while(|b| b.seen <= same).count();
                    self.spare = self.blocks.split_off(keep);
                    self.settled = self.blocks.last().map_or(0, |b| b.range.end);
                    self.checked = self.settled;
                }
                self.text.clear();
                self.text.push_str(text);
            }

            pub fn settle(&mut self) {
                self.split(false, true);
            }

            pub fn restore(&mut self) {
                if !self.spare.is_empty() {
                    self.split(true, false);
                }
            }

            fn split(&mut self, complete: bool, new: bool) {
                let mut at = self.checked.max(self.settled);
                loop {
                    let end = match self.text[at..].find('\n') {
                        Some(nl) => at + nl + 1,
                        None if complete && at < self.text.len() => self.text.len(),
                        None => break,
                    };
                    let line = &self.text[at..end];
                    let open = &self.text[self.settled..at];
                    if at > self.settled && !line.trim().is_empty() && !open.trim().is_empty() {
                        let alone = parse_markdown(open);
                        let mut both = alone.clone();
                        both.extend(parse_markdown(line));
                        if parse_markdown(&self.text[self.settled..end]) == both {
                            let source = trim_blank_lines(open);
                            let start = self.settled;
                            let old = self
                                .spare
                                .iter()
                                .position(|b| b.range.start == start && b.source.as_ref() == source)
                                .map(|i| self.spare.remove(i));
                            let block = match old {
                                Some(b) => Settled {
                                    range: start..at,
                                    seen: end,
                                    ..b
                                },
                                None if !new => break,
                                None => {
                                    self.next_key += 1;
                                    self.made += 1;
                                    Settled {
                                        range: start..at,
                                        seen: end,
                                        source: SharedString::from(source.to_string()),
                                        key: self.next_key - 1,
                                    }
                                }
                            };
                            self.blocks.push(block);
                            self.settled = at;
                        }
                    }
                    at = end;
                    self.checked = end;
                }
            }
        }
    }

    /// Lines that start, carry on, interrupt and close blocks, and ones that only look as if
    /// they did.
    const LINES: &[&str] = &[
        "a",
        "Words and more",
        "**Bold** start",
        "2024 was",
        "1.5 x",
        "",
        "",
        "   ",
        "\t",
        "# h",
        "#nope",
        "###### six",
        "####### seven",
        "## h ##",
        "---",
        "***",
        "* * *",
        "___",
        "- - -",
        "--",
        "-",
        "===",
        "- a",
        "- ",
        "+ a",
        "* a",
        "-x",
        "- [ ] task",
        "-   a",
        "-     a",
        "1. a",
        "1.",
        "2) b",
        "10. c",
        "1) x",
        "  - n",
        "   - n",
        "    - n",
        "  more",
        "   more",
        "  1. n",
        "    code",
        "      deep",
        "\tcode",
        "  \tx",
        "-\tx",
        "> q",
        ">",
        "> > q",
        "> - a",
        ">```",
        " > q",
        "> # h",
        "```",
        "```rust",
        "``` x`y",
        "~~~",
        "````",
        "   ```",
        "  ~~~",
        "  ```",
        "- ```",
        "1. ```",
        "| a | b |",
        "|---|---|",
        "a | b",
        ":--|--:",
        "--|--",
        "| 1 | x |",
        "| 2",
        "    | x |",
        "[x]: http://u",
        "  [x]: http://u",
        "[x]",
        "see [x]",
        "[^1]: note",
        "[^1]",
        "[foo",
        "bar]: /u",
        "$$",
        "<div>",
        "</div>",
        "<DIV class=\"x\">",
        "  <p>",
        "<p/>",
        "<!-- c -->",
        "<!--",
        "-->",
        "<pre>",
        "</pre>",
        "<script>",
        "<?x ?>",
        "<!DOCTYPE html>",
        "<![CDATA[",
        "]]>",
        "<span>",
        "<b>bold</b> words",
        "<divx>",
        "<",
        "- <div>",
        "> <div>",
        "  <!-- c -->",
        "\u{a0}",
        "a\rb",
        "x\r",
        "\\",
        "![](x)",
        "é words",
        "職務",
        "  [y]: /v",
        "   [x]",
        "    [x]: /u",
    ];

    struct Rng(u64);

    impl Rng {
        fn below(&mut self, n: usize) -> usize {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 % n as u64) as usize
        }

        fn text(&mut self, lines: usize) -> String {
            let mut s = String::new();
            for _ in 0..lines {
                s.push_str(LINES[self.below(LINES.len())]);
                s.push('\n');
            }
            if self.below(4) == 0 {
                s.pop();
            }
            s
        }
    }

    fn same(new: &Doc, old: &before::Doc, text: &str) {
        let blocks = |b: &[Settled]| {
            b.iter()
                .map(|b| (b.range.clone(), b.seen, b.source.to_string(), b.key))
                .collect::<Vec<_>>()
        };
        assert_eq!(blocks(&new.blocks), blocks(&old.blocks), "in {text:?}");
        assert_eq!(new.settled, old.settled, "in {text:?}");
        assert_eq!(new.made, old.made, "in {text:?}");
    }

    /// Settles `text` in both settlers, in one go and a byte at a time, and checks they agree.
    fn agrees(text: &str) {
        let (mut new, mut old) = (Doc::default(), before::Doc::default());
        new.append(text);
        old.append(text);
        while new.settle() {}
        old.settle();
        same(&new, &old, text);
        let (mut new, mut old) = (Doc::default(), before::Doc::default());
        for (i, c) in text.char_indices() {
            new.append(&text[i..i + c.len_utf8()]);
            old.append(&text[i..i + c.len_utf8()]);
            while new.settle() {}
            old.settle();
            same(&new, &old, &text[..i + c.len_utf8()]);
        }
        new.replace(text);
        old.replace(text);
        while new.restore() {}
        old.restore();
        same(&new, &old, text);
    }

    #[test]
    fn a_line_that_starts_an_html_block_is_checked() {
        let para = Open {
            kind: Kind::Paragraph,
            words: true,
            blank: false,
        };
        // Kinds 1 to 6 can end a paragraph, for a parser that reads HTML blocks: checked.
        for line in [
            "<div>",
            "</div>",
            "<DIV class=x>",
            "   <p>",
            "<p/>",
            "<hr/>",
            "<table",
            "<!-- c -->",
            "<!DOCTYPE html>",
            "<![CDATA[x]]>",
            "<?x ?>",
            "<pre>",
            "<Script>",
            "<textarea",
        ] {
            assert_eq!(para.next(&format!("{line}\n"), 9), Step::Check, "{line:?}");
            assert_eq!(first(&format!("{line}\n")), Kind::Unknown, "{line:?}");
        }
        // Kind 7 (any other tag alone on a line) and inline HTML can't: the paragraph's next
        // line. A tag indented four columns is the paragraph's text.
        for line in ["<span>", "<b>bold</b> words", "<divx>", "<", "    <div>"] {
            assert_eq!(
                para.next(&format!("{line}\n"), 9),
                Step::On(Kind::Paragraph),
                "{line:?}"
            );
        }
        assert_eq!(first("<span>\n"), Kind::Paragraph);
    }

    #[test]
    fn html_blocks_settle_as_reading_every_line_again_does() {
        // Whether a parser reads HTML blocks or not, the settler settles where reading every
        // line again would.
        for text in [
            "\n|---|---|\n<div>\n",
            "\n|---|---|\n<div>\nafter\n\nmore\n",
            "A paragraph\n<div>\n",
            "A paragraph\n<div>\ninside\n\nAfter\n\nThe end\n",
            "A paragraph\n<!-- c -->\n",
            "A paragraph\n<!-- c -->\nafter\n\nThe end\n",
            "<!-- c -->\nafter\nmore\n",
            "A paragraph\n<span>\nmore\n\nThe end\n",
            "<span>\nmore\n<div>\nend\n",
            "<pre>\ncode\n\nstill\n</pre>\ntext\n\nThe end\n",
            "A paragraph\n<pre>\ncode\n</pre> text\nafter\n\nThe end\n",
            "- an item\n  <div>\n  inside\n- next\n\nAfter\n\nThe end\n",
            "- an item\n<div>\nafter\n\nThe end\n",
            "- an item\n  <!-- c -->\n  more\n\nAfter\n\nThe end\n",
            "> a quote\n> <div>\n> inside\n\nAfter\n\nThe end\n",
            "> a quote\n<div>\nafter\n\nThe end\n",
            "> <!-- c -->\n> more\n\nAfter\n\nThe end\n",
            "```\ncode\n```\n<div>\nafter\n\nThe end\n",
            "    code\n<div>\nafter\n\nThe end\n",
            "| a | b |\n|---|---|\n| 1 | 2 |\n<div>\nafter\n\nThe end\n",
        ] {
            agrees(text);
        }
        // A tag of kind 7 alone on a line can't end a paragraph, for either kind of parser.
        assert_eq!(settled("A paragraph\n<span>\nmore\n"), Vec::<String>::new());
    }

    #[test]
    fn settling_reads_lines_once_and_settles_as_reading_every_line_again_did() {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        let rounds = if cfg!(debug_assertions) { 400 } else { 2500 };
        for _ in 0..rounds {
            let n = 1 + rng.below(14);
            let text = rng.text(n);
            // A reconnect's text, or the message's: a line changed, some cut off, or more.
            let mut other = text.clone();
            match rng.below(3) {
                0 => {
                    let at = rng.below(other.len() + 1);
                    let at = (0..=at).rev().find(|i| other.is_char_boundary(*i)).unwrap();
                    other.truncate(at);
                }
                1 => {
                    let at = other.find('\n').map_or(0, |i| i + 1);
                    let n = 1 + rng.below(3);
                    other.insert_str(at, &rng.text(n));
                }
                _ => {
                    let n = 1 + rng.below(4);
                    other.push_str(&rng.text(n));
                }
            }
            let (mut new, mut old) = (Doc::default(), before::Doc::default());
            // Streamed in cuts of a few bytes.
            let mut i = 0;
            while i < text.len() {
                let mut j = (i + 1 + rng.below(9)).min(text.len());
                while !text.is_char_boundary(j) {
                    j += 1;
                }
                new.append(&text[i..j]);
                old.append(&text[i..j]);
                while new.settle() {}
                old.settle();
                same(&new, &old, &text[..j]);
                i = j;
            }
            new.replace(&other);
            old.replace(&other);
            if rng.below(2) == 0 {
                while new.settle() {}
                old.settle();
            } else {
                while new.restore() {}
                old.restore();
            }
            same(&new, &old, &other);
        }
    }

    #[test]
    fn open_fences_in_items_and_tabs_in_quotes_settle_as_reading_every_line_again_does() {
        // A code fence left open in an item, then a list of the other kind; a quote with a tab
        // after its marker below a table. Some as the stream ends, some with more after.
        for text in [
            "- a\n  ```\n1. a\n",
            "- a\n  ```\n1. a\n\nAfter\n\nThe end\n",
            "1. a\n   ```\n- a\n",
            "1. a\n   ```\n- a\n\nAfter\n\nThe end\n",
            "- ```\n1. a\n",
            "1. ```\n- a\n2. b\n",
            "- a\n  ~~~\n1) a\n",
            "2) b\n   ```rust\n- n\n  more\n",
            "> - a\n>   ```\n> 1. a\n\nAfter\n",
            "- a\n  ```\n  code\n1. a\n- b\n",
            "| a | b |\n|---|---|\n> \t`",
            "| a | b |\n|---|---|\n> \t`\n",
            "| a | b |\n|---|---|\n> \t`\nmore\n\nThe end\n",
            "| a | b |\n|---|---|\n>\t\tcode\n> q\n",
            "> \t`\n> \t```\n",
            "-\tx\n  ```\n1. a\n",
        ] {
            agrees(text);
        }
    }

    /// Settles `text` given in `pieces`, settling after each as a frame would: the frames it
    /// took, and the most a frame read (new text and checks).
    fn stream<'a>(d: &mut Doc, pieces: impl IntoIterator<Item = &'a str>) -> (usize, usize) {
        let (mut frames, mut most) = (0, 0);
        for piece in pieces {
            d.append(piece);
            loop {
                let (checked, reread) = (d.checked, d.reread);
                let more = d.settle();
                frames += 1;
                most = most.max(d.checked - checked + d.reread - reread);
                if !more {
                    break;
                }
            }
        }
        (frames, most)
    }

    fn fence(lines: usize) -> String {
        let mut s = String::from("```rust\n");
        for i in 0..lines {
            s.push_str(&format!("x{}\n", i % 1000));
        }
        s.push_str("```\n\nAfter.\n\nThe end\n");
        s
    }

    #[test]
    fn a_long_fence_is_read_once() {
        let text = fence(50_000);
        let fence_len = text.find("\nAfter").unwrap() + 1;
        let check = |d: &Doc| {
            assert_eq!(d.blocks.len(), 2);
            assert_eq!(d.blocks[0].range, 0..fence_len);
            assert_eq!(d.blocks[1].source.as_ref(), "After.");
            assert!(d.blocks[0].source.starts_with("```rust\n"));
        };
        // In one go: a frame or two, each bounded.
        let mut d = Doc::default();
        let (frames, most) = stream(&mut d, [text.as_str()]);
        check(&d);
        assert!(frames <= 2, "{frames} frames");
        assert!(most <= SCAN + REREAD, "a frame read {most}");
        // Nothing in the fence was checked: only "After." against the line after it.
        assert!(d.reread < 64, "the checks read {}", d.reread);
        // A line at a time.
        let mut d = Doc::default();
        let (frames, most) = stream(&mut d, text.split_inclusive('\n'));
        check(&d);
        assert_eq!(frames, text.lines().count());
        assert!(most < 64, "a frame read {most}");
        assert!(d.reread < 64, "the checks read {}", d.reread);
        // Half, then the whole text in its place (a reconnect), and again with a change
        // inside the fence.
        let mut d = Doc::default();
        stream(&mut d, [&text[..text.len() / 2]]);
        d.replace(&text);
        while d.settle() {}
        check(&d);
        let changed = text.replacen("x500\n", "y500\n", 1);
        d.replace(&changed);
        assert!(d.blocks.is_empty());
        while d.settle() {}
        assert_eq!(d.blocks.len(), 2);
        assert!(d.reread < 128, "the checks read {}", d.reread);
    }

    #[test]
    fn a_long_list_and_deep_nesting_are_read_once() {
        let mut list = String::new();
        for i in 0..20_000 {
            list.push_str(&format!("- item {i}\n"));
            if i % 100 == 0 {
                list.push_str("  - nested\n    more of it\n\n  and a paragraph\n");
            }
        }
        let mut nested = String::new();
        for i in 0..2_000 {
            let depth = 1 + i % 30;
            nested.push_str(&"> ".repeat(depth));
            nested.push_str(&"- ".repeat(i % 7));
            nested.push_str(&format!("line {i}\n"));
        }
        for body in [list, nested] {
            let text = format!("{body}\nAfter.\n\nThe end\n");
            for pieces in [vec![text.as_str()], text.split_inclusive('\n').collect()] {
                let mut d = Doc::default();
                let (_, most) = stream(&mut d, pieces);
                assert_eq!(
                    d.blocks.iter().map(|b| b.range.clone()).collect::<Vec<_>>(),
                    [0..body.len() + 1, body.len() + 1..text.len() - 8]
                );
                // The block's settling check read it twice (alone, and with the line after),
                // and nothing else read it again.
                assert!(
                    d.reread < 2 * body.len() + 64,
                    "the checks read {}",
                    d.reread
                );
                assert!(most <= SCAN + 2 * body.len() + 64, "a frame read {most}");
            }
        }
    }

    #[test]
    fn what_can_t_be_told_is_checked_a_bounded_amount_a_frame() {
        // A list whose items carry on lazily, unindented: each line is checked.
        let mut text = String::new();
        for i in 0..300 {
            text.push_str(&format!("- item {i}\nlazy {i}\n"));
        }
        let mut d = Doc::default();
        d.append(&text);
        let mut frames = 0;
        loop {
            let reread = d.reread;
            let more = d.settle();
            frames += 1;
            // At most the budget, or a single check past it.
            let check = 2 * (d.checked - d.settled) + 64;
            assert!(
                d.reread - reread <= REREAD.max(check),
                "{}",
                d.reread - reread
            );
            if !more {
                break;
            }
        }
        assert!(frames > 1, "spread over frames");
        assert!(d.blocks.is_empty());
        assert_eq!(d.checked, text.len());
    }

    #[test]
    fn the_parser_reads_as_much_as_settling_expects() {
        let heading = |pad: usize| {
            let text = format!("{}\n\n# h", "a".repeat(pad));
            parse_markdown(&text)
                .iter()
                .any(|b| matches!(b, Block::Heading(..)))
        };
        assert!(heading(CAP - 10));
        assert!(!heading(CAP));
    }
}
