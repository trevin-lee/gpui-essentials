//! A draft's text as blocks: what's settled (laid out once and kept) and the open tail (read
//! again on each delta), and how the tail shows while it's still being written.

use std::ops::Range;

use gpui::SharedString;
use gpui_core::text::{Block, BlockKind, parse_markdown};

/// The caret at the end of a draft's text while it's being written: a steady bar, drawn as text
/// in the text's own colour, so it sits where the next word goes and wraps with it.
pub const CARET: &str = "\u{258D}";

/// A settled block: Markdown that nothing written after it can change.
pub(crate) struct Settled {
    /// Where it is in the text, from its first character to where the next block starts.
    pub range: Range<usize>,
    /// The end of the line after it, which its check read: a change before there unsettles it.
    pub seen: usize,
    /// Its text, without the blank lines around it.
    pub source: SharedString,
    /// The kinds of its first and last blocks (none for text that draws nothing, as a link's
    /// definition), for the space above and below it.
    pub kinds: Option<(BlockKind, BlockKind)>,
    /// Its own number, for its element's id.
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
    pub fn reopen_from(&mut self, at: usize) {
        let keep = self.blocks.iter().take_while(|b| b.seen <= at).count();
        self.spare = self.blocks.split_off(keep);
        self.settled = self.blocks.last().map_or(0, |b| b.range.end);
        self.checked = self.settled;
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
    /// opens a block again once something has closed it; the one exception, a link defined
    /// further down, is taken care of when the draft ends ([`Doc::defines_links`]).
    pub fn settle(&mut self) {
        self.split(false, true);
    }

    /// After the draft ended with other text ([`Doc::replace`]): settles again, where they
    /// were, the blocks the change opened that are still as they were, so their views keep
    /// their layouts; the text is whole, so its last line counts as finished. No block is split
    /// off that wasn't one before, so what was drawn as one view stays one.
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
                                kinds: kinds(&alone),
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

    /// Whether the text defines a link or a footnote for its references (`[a]: https://…`):
    /// a block that settled before the definition came reads its reference as text, so a draft
    /// that ends with one is read again whole.
    pub fn defines_links(&self) -> bool {
        self.text.lines().any(|l| {
            let t = l.trim_start_matches(' ');
            l.len() - t.len() < 4
                && t.starts_with('[')
                && t.find("]:")
                    .is_some_and(|i| i > 1 && !t[1..i].contains(['[', ']']))
        })
    }
}

/// The first and last kinds of some blocks.
pub(crate) fn kinds(blocks: &[Block]) -> Option<(BlockKind, BlockKind)> {
    Some((kind(blocks.first()?), kind(blocks.last()?)))
}

/// A block's kind, as gpui-core's TextView names it for its gaps.
pub(crate) fn kind(b: &Block) -> BlockKind {
    match b {
        Block::Paragraph(_) => BlockKind::Paragraph,
        Block::Heading(level, _) => BlockKind::Heading(*level),
        Block::List { start, .. } => BlockKind::List {
            ordered: start.is_some(),
        },
        Block::Code { .. } => BlockKind::Code,
        Block::Quote(_) => BlockKind::Quote,
        Block::Rule => BlockKind::Rule,
        Block::Table { .. } => BlockKind::Table,
    }
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
/// back, what's open is closed for now, and the caret goes at the end.
///
/// - A last line of only marks (`1.`, `#`, `---`, a fence's backticks) waits for its words or
///   its newline, so a paragraph doesn't flash as a heading and back.
/// - A line that starts a table waits for its delimiter row, so it doesn't show as text first.
/// - An open code fence draws as code.
/// - Emphasis, strike and inline code open on the last line are closed after the caret, so
///   the words being written are already bold (and never the rest of the text and back); a
///   marker with nothing after it yet isn't shown.
/// - A link being written shows as its words.
pub(crate) fn display(tail: &str) -> String {
    let mut s = tail.to_string();
    // A last line still being written that's only marks.
    let start = s.rfind('\n').map_or(0, |i| i + 1);
    if only_marks(&s[start..]) {
        s.truncate(start);
    }
    match open_fence(&s) {
        Some(opener) => {
            let t = s.trim_end_matches(['\n', '\r']).len();
            s.truncate(t);
            // On the opener's line the caret would be the code's language.
            if !s[opener..].contains('\n') {
                s.push('\n');
            }
            s.push_str(CARET);
            return s;
        }
        None => {
            hold_table_header(&mut s);
        }
    }
    let t = s.trim_end().len();
    s.truncate(t);
    let start = s.rfind('\n').map_or(0, |i| i + 1);
    let last = &s[start..];
    if last.is_empty() {
        s.push_str(CARET);
        return s;
    }
    // After a closing fence or a rule, a caret would change what the line is.
    if only_marks(last) {
        return s;
    }
    let (shown, closers) = close_line(last);
    s.truncate(start);
    s.push_str(&shown);
    s.push_str(CARET);
    s.push_str(&closers);
    s
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

/// The last line's words with what's open closed for now: the words to show, and what goes
/// after the caret.
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
/// after the caret. An opener with nothing after it yet comes off `s`.
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

    fn shown(tail: &str) -> String {
        display(tail).replace(CARET, "|")
    }

    #[test]
    fn open_things_close_for_now() {
        assert_eq!(shown("Some **bold"), "Some **bold|**");
        assert_eq!(shown("Some **bold** and *it"), "Some **bold** and *it|*");
        assert_eq!(shown("a **b *c"), "a **b *c|***");
        assert_eq!(shown("~~gone"), "~~gone|~~");
        assert_eq!(shown("run `cargo te"), "run `cargo te|`");
        assert_eq!(shown("x `` a ` b"), "x `` a ` b|``");
        // A marker with nothing after it yet isn't shown.
        assert_eq!(shown("Some **"), "Some|");
        assert_eq!(shown("Some `"), "Some|");
        // Not emphasis: a times sign, a word's underscores.
        assert_eq!(shown("2 * 3 is 6"), "2 * 3 is 6|");
        // A line of only numbers and marks waits for its words or its newline.
        assert_eq!(shown("Sum:\n2 * 3 = 6"), "Sum:|");
        assert_eq!(shown("snake_case_name"), "snake_case_name|");
        // Closed already.
        assert_eq!(shown("**done**"), "**done**|");
        assert_eq!(shown("`a*b`"), "`a*b`|");
    }

    #[test]
    fn a_link_being_written_shows_as_its_words() {
        assert_eq!(shown("see [the do"), "see the do|");
        assert_eq!(shown("see [the docs]"), "see the docs|");
        assert_eq!(shown("see [the docs](https://exa"), "see the docs|");
        assert_eq!(shown("see ![a cat](https://x"), "see a cat|");
        assert_eq!(
            shown("see [docs](https://x.test) now"),
            "see [docs](https://x.test) now|"
        );
        assert_eq!(shown("- [ ] a task"), "- [ ] a task|");
    }

    #[test]
    fn marks_wait_for_their_words() {
        assert_eq!(shown("Para\n\n#"), "Para|");
        assert_eq!(shown("Para\n1."), "Para|");
        assert_eq!(shown("Title\n=="), "Title|");
        assert_eq!(shown("Para\n\n# Head"), "Para\n\n# Head|");
        assert_eq!(shown("- one\n- **tw"), "- one\n- **tw|**");
        // A finished rule or fence takes no caret.
        assert_eq!(shown("Para\n\n---\n"), "Para\n\n---");
        assert_eq!(shown("```\ncode\n```\n"), "```\ncode\n```");
    }

    #[test]
    fn an_open_fence_draws_as_code() {
        assert_eq!(shown("```rust\nfn main() {\n"), "```rust\nfn main() {|");
        assert_eq!(shown("```rust\n"), "```rust\n|");
        // Its closing backticks wait for the newline: the block stays open meanwhile.
        assert_eq!(shown("```\ncode\n``"), "```\ncode|");
        // Nothing inside is closed.
        assert_eq!(shown("```\na **b"), "```\na **b|");
    }

    #[test]
    fn a_table_waits_for_its_delimiter_row() {
        assert_eq!(shown("Intro\n\n| a | b |\n"), "Intro|");
        assert_eq!(shown("Intro\n\n| a | b |\n|--"), "Intro|");
        assert_eq!(
            shown("| a | b |\n|---|---|\n| 1 | x"),
            "| a | b |\n|---|---|\n| 1 | x|"
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

    #[test]
    fn link_definitions_are_found() {
        let mut d = Doc::default();
        d.append("See [the docs].\n\n[the docs]: https://x.test\n");
        assert!(d.defines_links());
        d.replace("A [b] c\n\n  [x] y");
        assert!(!d.defines_links());
    }
}
