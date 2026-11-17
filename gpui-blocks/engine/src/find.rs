// Added in Trevin Lee's fork (2026): finding text on the page.
//! Finding text on the page: every place a query occurs in the document's text, in document order.

use std::ops::Range;

use super::document::DocumentRef;
use super::id::BlockId;
use super::inline::ATOM_CHAR;

/// A place the query occurs: a text leaf, and the byte range of the text found in it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindMatch {
    pub block: BlockId,
    pub range: Range<usize>,
}

/// Every place `query` occurs in `doc`'s text (paragraphs, headings, list items, table cells and
/// code), in document order, without overlaps. A match never spans a mention or a picture.
/// Letters match whatever their case unless `case_sensitive`. An empty query matches nothing.
pub(crate) fn find(doc: DocumentRef<'_>, query: &str, case_sensitive: bool) -> Vec<FindMatch> {
    if query.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for leaf in doc.leaves() {
        let Some(text) = leaf.text() else {
            continue;
        };
        out.extend(
            matches_in(text, query, case_sensitive)
                .into_iter()
                .map(|range| FindMatch {
                    block: leaf.id(),
                    range,
                }),
        );
    }
    out
}

/// The byte ranges of `query` in `text`, left to right and apart.
fn matches_in(text: &str, query: &str, case_sensitive: bool) -> Vec<Range<usize>> {
    let same =
        |a: char, b: char| a == b || (!case_sensitive && a.to_lowercase().eq(b.to_lowercase()));
    let query: Vec<char> = query.chars().collect();
    let mut out = Vec::new();
    if query.is_empty() {
        return out;
    }
    let mut from = 0;
    while from < text.len() {
        let mut chars = text[from..].char_indices();
        let mut end = None;
        let mut q = query.iter();
        loop {
            match (q.next(), chars.next()) {
                (None, next) => {
                    end = Some(from + next.map_or(text.len() - from, |(i, _)| i));
                    break;
                }
                (Some(want), Some((_, got))) if got != ATOM_CHAR && same(*want, got) => {}
                _ => break,
            }
        }
        match end {
            // Every match is at least one character, so the loop always moves on and never
            // finds more matches than the text has bytes.
            Some(end) if end > from => {
                out.push(from..end);
                from = end;
            }
            Some(_) => break,
            None => {
                // On to the next character.
                from += text[from..].chars().next().map_or(1, char::len_utf8);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_are_found_apart_and_by_bytes() {
        assert_eq!(matches_in("abcabc", "bc", true), [1..3, 4..6]);
        assert_eq!(
            matches_in("aaaa", "aa", true),
            [0..2, 2..4],
            "apart, not overlapping"
        );
        assert_eq!(matches_in("héllo hé", "hé", true), [0..3, 7..10]);
        // An empty query matches nothing (once it matched everywhere, forever, without moving on).
        assert_eq!(matches_in("abc", "", true), Vec::<Range<usize>>::new());
    }

    #[test]
    fn case_matters_only_when_asked() {
        assert_eq!(matches_in("Plan the PLAN", "plan", false), [0..4, 9..13]);
        assert_eq!(
            matches_in("Plan the PLAN", "plan", true),
            Vec::<Range<usize>>::new()
        );
        let m = matches_in("ÉTÉ", "été", false);
        assert_eq!((m.len(), m[0].clone()), (1, 0..5));
    }

    #[test]
    fn a_match_never_spans_a_mention_or_picture() {
        let text = format!("a{ATOM_CHAR}b ab");
        assert_eq!(
            matches_in(&text, &format!("a{ATOM_CHAR}b"), true),
            Vec::<Range<usize>>::new()
        );
        let m = matches_in(&text, "ab", true);
        assert_eq!((m.len(), m[0].clone()), (1, 6..8));
    }
}
