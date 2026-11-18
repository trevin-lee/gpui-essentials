//! Columns: blocks side by side, as Notion's. A `Columns` block sits at the top level and holds
//! 2 to [`MAX_COLUMNS`] `Column`s, each of any blocks but columns; normalization keeps it that
//! way after every edit (see `transaction.rs`), so the editor never writes another shape.
//!
//! Markdown has no columns, so they're written between HTML comments, which other tools don't
//! show, leaving the blocks in order:
//!
//! ```markdown
//! <!-- columns -->
//!
//! <!-- column 0.5 -->
//!
//! On the left.
//!
//! <!-- column 0.5 -->
//!
//! On the right.
//!
//! <!-- /columns -->
//! ```
//!
//! A column's width is its share of the row; a bare `<!-- column -->` takes an even share.
//! Markers that don't make a whole columns block at the top level (a column outside columns, a
//! missing `<!-- /columns -->`, one columns inside another, a bad width, one or more than
//! [`MAX_COLUMNS`] columns) are read as the HTML they are, and nothing between them is lost.

use crate::kind::MAX_COLUMNS;

/// What a top-level HTML comment says about columns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Marker {
    /// `<!-- columns -->`
    Open,
    /// `<!-- column -->`, or `<!-- column 0.5 -->`.
    Column(Option<f32>),
    /// `<!-- /columns -->`
    Close,
    /// `<!-- column … -->` with something after it that isn't a width.
    Bad,
}

/// The marker an HTML block's source is, if it's one.
pub(crate) fn marker(source: &str) -> Option<Marker> {
    let inner = source
        .trim()
        .strip_prefix("<!--")?
        .strip_suffix("-->")?
        .trim();
    let mut words = inner.split_ascii_whitespace();
    let first = words.next()?;
    let rest: Vec<&str> = words.collect();
    Some(match (first, rest.as_slice()) {
        ("columns", []) => Marker::Open,
        ("/columns", []) => Marker::Close,
        ("column", []) => Marker::Column(None),
        ("column", [width]) => match width.parse::<f32>() {
            Ok(w) if w.is_finite() && w > 0. && w <= 1. => Marker::Column(Some(w)),
            _ => Marker::Bad,
        },
        ("column", _) => Marker::Bad,
        _ => return None,
    })
}

/// The widths a row is read with: a missing one is an even share, and the row is rescaled to
/// add up to 1 only when it's more than 0.01 off, so rounding to 3 decimals doesn't drift on
/// every round trip.
pub(crate) fn read_widths(written: &[Option<f32>]) -> Vec<f32> {
    let even = 1. / written.len().max(1) as f32;
    let widths: Vec<f32> = written.iter().map(|w| w.unwrap_or(even)).collect();
    let sum: f32 = widths.iter().sum();
    if (sum - 1.).abs() > 0.01 && sum > 0. {
        widths.iter().map(|w| w / sum).collect()
    } else {
        widths
    }
}

/// The widths a row is written with: shares of 1, to 3 decimals.
pub fn written_widths(widths: &[f32]) -> Vec<f32> {
    let sum: f32 = widths.iter().filter(|w| w.is_finite() && **w > 0.).sum();
    widths
        .iter()
        .map(|w| {
            let share = if sum > 0. && w.is_finite() && *w > 0. {
                w / sum
            } else {
                1. / widths.len().max(1) as f32
            };
            (share * 1000.).round().max(1.) / 1000.
        })
        .collect()
}

/// A width as it's written: up to 3 decimals, without trailing zeros (`0.5`, `0.333`, `1`).
pub fn width_text(width: f32) -> String {
    let text = format!("{width:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    text.to_string()
}

/// A columns block's Markdown, from each column's width and its blocks' Markdown. The widths
/// are written as shares of 1.
pub fn columns_markdown(columns: &[(f32, String)]) -> String {
    let widths: Vec<f32> = columns.iter().map(|(w, _)| *w).collect();
    let mut parts = vec!["<!-- columns -->".to_string()];
    for ((_, body), width) in columns.iter().zip(written_widths(&widths)) {
        parts.push(format!("<!-- column {} -->", width_text(width)));
        if !body.is_empty() {
            parts.push(body.clone());
        }
    }
    parts.push("<!-- /columns -->".to_string());
    parts.join("\n\n")
}

/// Groups a top-level run of blocks into columns: where `marker_of` finds `<!-- columns -->`,
/// then 2 to [`MAX_COLUMNS`] `<!-- column -->`s each before its blocks, then
/// `<!-- /columns -->`, `make` builds the columns block from each column's width and blocks
/// (empty for a column with none). Anything else stays as it is, markers included.
pub(crate) fn group<B>(
    blocks: Vec<B>,
    marker_of: impl Fn(&B) -> Option<Marker>,
    make: impl Fn(Vec<(f32, Vec<B>)>) -> B,
) -> Vec<B> {
    if !blocks.iter().any(|b| marker_of(b) == Some(Marker::Open)) {
        return blocks;
    }
    let markers: Vec<Option<Marker>> = blocks.iter().map(&marker_of).collect();
    // Where each whole columns block ends, by where it starts.
    let mut ends = std::collections::HashMap::new();
    let mut ix = 0;
    while ix < markers.len() {
        if markers[ix] == Some(Marker::Open) {
            let mut columns = 0;
            let mut at = ix + 1;
            let end = loop {
                match markers.get(at) {
                    Some(Some(Marker::Column(_))) => columns += 1,
                    Some(Some(Marker::Close)) => break Some(at),
                    // Nesting, a bad width, the end: not columns.
                    Some(Some(Marker::Open | Marker::Bad)) | None => break None,
                    // Blocks before the first column: not columns.
                    Some(None) if columns == 0 => break None,
                    Some(None) => {}
                }
                at += 1;
            };
            if let Some(end) = end
                && (2..=MAX_COLUMNS).contains(&columns)
            {
                ends.insert(ix, end);
                ix = end + 1;
                continue;
            }
        }
        ix += 1;
    }
    let mut out = Vec::new();
    let mut blocks = blocks.into_iter().enumerate().peekable();
    while let Some((ix, block)) = blocks.next() {
        let Some(&end) = ends.get(&ix) else {
            out.push(block);
            continue;
        };
        let mut written = Vec::new();
        let mut columns: Vec<Vec<B>> = Vec::new();
        for (at, block) in blocks.by_ref() {
            if at == end {
                break;
            }
            match markers[at] {
                Some(Marker::Column(width)) => {
                    written.push(width);
                    columns.push(Vec::new());
                }
                _ => {
                    if let Some(column) = columns.last_mut() {
                        column.push(block);
                    }
                }
            }
        }
        out.push(make(
            read_widths(&written).into_iter().zip(columns).collect(),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_read_strictly() {
        assert_eq!(marker("<!-- columns -->"), Some(Marker::Open));
        assert_eq!(marker("<!--columns-->"), Some(Marker::Open));
        assert_eq!(marker("<!-- /columns -->"), Some(Marker::Close));
        assert_eq!(marker("<!-- column -->"), Some(Marker::Column(None)));
        assert_eq!(
            marker("<!-- column 0.25 -->"),
            Some(Marker::Column(Some(0.25)))
        );
        assert_eq!(marker("<!-- column 2 -->"), Some(Marker::Bad));
        assert_eq!(marker("<!-- column wide -->"), Some(Marker::Bad));
        assert_eq!(marker("<!-- column 0 -->"), Some(Marker::Bad));
        assert_eq!(marker("<!-- a note -->"), None);
        assert_eq!(marker("<div>"), None);
    }

    #[test]
    fn widths_fill_in_and_rescale_only_when_off() {
        assert_eq!(read_widths(&[None, None]), [0.5, 0.5]);
        assert_eq!(
            read_widths(&[Some(0.333), Some(0.333), Some(0.333)]),
            [0.333; 3]
        );
        let w = read_widths(&[Some(0.5), Some(0.25)]);
        assert!((w[0] - 2. / 3.).abs() < 1e-6 && (w[1] - 1. / 3.).abs() < 1e-6);
        assert_eq!(written_widths(&[1., 1., 1.]), [0.333; 3]);
        assert_eq!(width_text(0.5), "0.5");
        assert_eq!(width_text(0.333), "0.333");
        assert_eq!(width_text(1.), "1");
    }

    /// Blocks are numbers; a marker is a string's.
    fn grouped(items: &[&str]) -> Vec<String> {
        let blocks: Vec<String> = items.iter().map(|s| s.to_string()).collect();
        group(
            blocks,
            |b| marker(b),
            |columns| {
                columns
                    .into_iter()
                    .map(|(w, blocks)| format!("{}:{}", width_text(w), blocks.join(",")))
                    .collect::<Vec<_>>()
                    .join("|")
            },
        )
    }

    #[test]
    fn whole_columns_group_and_anything_else_stays() {
        let open = "<!-- columns -->";
        let col = "<!-- column -->";
        let close = "<!-- /columns -->";
        assert_eq!(
            grouped(&["x", open, col, "a", "b", col, "c", close, "y"]),
            ["x", "0.5:a,b|0.5:c", "y"]
        );
        // An empty column is kept, empty.
        assert_eq!(grouped(&[open, col, col, "c", close]), ["0.5:|0.5:c"]);
        // One column, a missing close, a block before the first column, nesting, a bad width:
        // as they were.
        for items in [
            vec![open, col, "a", close],
            vec![open, col, "a", col, "b"],
            vec![open, "a", col, "b", col, "c", close],
            vec![open, col, open, col, "a", col, "b", close, col, "c", close],
            vec![open, col, "a", "<!-- column 7 -->", "b", close],
        ] {
            let expected: Vec<String> = items.iter().map(|s| s.to_string()).collect();
            let got = grouped(&items);
            // The nested case keeps its outer markers and may group the inner whole one.
            if items.iter().filter(|s| **s == open).count() > 1 {
                assert!(got.len() < expected.len() && got.contains(&open.to_string()));
            } else {
                assert_eq!(got, expected);
            }
        }
        // Six columns: too many.
        let six = [
            open, col, "a", col, "b", col, "c", col, "d", col, "e", col, "f", close,
        ];
        assert_eq!(grouped(&six).len(), six.len());
    }
}
