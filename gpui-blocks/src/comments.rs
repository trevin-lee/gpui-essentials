//! Tinting the words comments are about. A comment is anchored to a block's lasting id and the
//! words quoted, as [`Editor::selection_anchor`] gives them; while its thread is shown, the app
//! tints those words with [`Editor::highlight_quotes`]. The quote is found again after every
//! edit, so the tint follows the words as the text around them changes.

use std::ops::Range;

use gpui::*;
use markdown_editor::{BlockEditorModel, BlockId};

use crate::Editor;

/// What stands for a mention or a picture in a leaf's text; quotes leave it out.
const ATOM: char = '\u{FFFC}';

impl Editor {
    /// Tints the quoted words of each anchor: a block's lasting id (as in
    /// [`Editor::document`]) and the words, lines joined with `\n`, as
    /// [`Editor::selection_anchor`] gives them. The words are found from the start of that
    /// block, and again after every edit; an anchor whose words are gone, or whose block is, is
    /// left untinted, and an empty quote tints nothing. Replaces the anchors set before; an
    /// empty list clears the tints.
    pub fn highlight_quotes(&mut self, anchors: Vec<(String, String)>, cx: &mut Context<Self>) {
        self.quotes = anchors;
        self.refresh_tints(cx);
    }

    /// Finds the quotes again and tints them.
    pub(crate) fn refresh_tints(&mut self, cx: &mut Context<Self>) {
        let tints = {
            let state = self.state.read(cx);
            let model = state.model();
            self.quotes
                .iter()
                .filter_map(|(id, quote)| {
                    let block = self.lock_ids().block_of(model, id)?;
                    Some(quote_ranges(model, block, quote))
                })
                .flatten()
                .collect::<Vec<_>>()
        };
        self.state
            .update(cx, |state, cx| state.set_tints(tints, cx));
    }
}

/// Where `quote` is in `block`: its first line found in the first of the block's leaves that
/// holds it, and each later line at the start of the leaves after that one.
fn quote_ranges(
    model: &BlockEditorModel,
    block: BlockId,
    quote: &str,
) -> Vec<(BlockId, Range<usize>)> {
    if quote.trim().is_empty() {
        return Vec::new();
    }
    let doc = model.document();
    let inside = |leaf: markdown_editor::BlockRef<'_>| {
        let mut at = Some(leaf);
        while let Some(b) = at {
            if b.id() == block {
                return true;
            }
            at = b.parent();
        }
        false
    };
    let leaves: Vec<_> = doc.leaves().collect();
    let mut lines = quote.split('\n');
    let first = lines.next().unwrap_or_default();
    let Some((start, range)) = leaves.iter().enumerate().find_map(|(ix, leaf)| {
        if !inside(*leaf) {
            return None;
        }
        Some((ix, find(leaf.text()?, first, false)?))
    }) else {
        return Vec::new();
    };
    let mut out = vec![(leaves[start].id(), range)];
    for (n, line) in lines.enumerate() {
        let Some(leaf) = leaves.get(start + 1 + n) else {
            break;
        };
        match leaf.text().and_then(|text| find(text, line, true)) {
            Some(range) => out.push((leaf.id(), range)),
            None => break,
        }
    }
    out
}

/// The byte range of `needle` in `text`, mentions and pictures skipped as quotes skip them;
/// `at_start` only at the start of the text.
fn find(text: &str, needle: &str, at_start: bool) -> Option<Range<usize>> {
    // The text without its atoms, and where each of its characters is in the text.
    let mut stripped = String::with_capacity(text.len());
    let mut at = Vec::with_capacity(text.len());
    for (ix, c) in text.char_indices() {
        if c != ATOM {
            for _ in 0..c.len_utf8() {
                at.push(ix);
            }
            stripped.push(c);
        }
    }
    at.push(text.len());
    let found = if at_start {
        stripped.starts_with(needle).then_some(0)
    } else {
        stripped.find(needle)
    }?;
    let end = found + needle.len();
    // The end is after the needle's last character, not at the next one's start (an atom may
    // follow it).
    let end = match stripped[..end].chars().next_back() {
        Some(last) => at[end - last.len_utf8()] + last.len_utf8(),
        None => at[found],
    };
    Some(at[found]..end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::prelude::rust_2024::test;

    #[test]
    fn a_quote_is_found_past_mentions_and_pictures() {
        assert_eq!(find("plan the week", "the", false), Some(5..8));
        let text = format!("see {ATOM} then go");
        // "see  then" with the atom left out, as a quote is written.
        let range = find(&text, "see  then", false).unwrap();
        assert_eq!(&text[range], format!("see {ATOM} then"));
        assert_eq!(find("héllo", "é", false), Some(1..3));
        assert_eq!(find("abc", "b", true), None, "only at the start");
        assert_eq!(find("abc", "zz", false), None);
    }
}
