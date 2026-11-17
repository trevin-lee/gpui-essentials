//! Searching a view's rows, as Notion's search in a database's bar: the rows whose title or any
//! shown property's text contains what's typed, matched without regard to case or accents, on
//! top of the view's own filter, sorts and groups.
//!
//! The search is the caller's, never the view's: it isn't saved and isn't an edit. What a row
//! is searched by is the text its cells show (an option's name, a person's name, a number in
//! its format), which only the caller knows how to write, so it hands each row's words to a
//! [`SearchText`] once, folded there, and each query after that is one pass over the folded
//! text: about a tenth of a millisecond for 10,000 rows.
//!
//! ```
//! use gpui_tables::engine::search::{search, Query, SearchText};
//! use gpui_tables::engine::{Group, GroupKey, ViewRows};
//!
//! let mut text = SearchText::new(3);
//! text.set(0, ["Ana Sofía Durán", "Warm"]);
//! text.set(1, ["Rowan Pike", "Lead"]);
//! text.set(2, ["Theo Brandt", "Warm"]);
//! let shown = ViewRows {
//!     rows: vec![0, 1, 2],
//!     groups: Some(vec![
//!         Group { key: GroupKey::Option("lead".into()), rows: vec![1] },
//!         Group { key: GroupKey::Option("warm".into()), rows: vec![0, 2] },
//!     ]),
//!     issues: Vec::new(),
//! };
//! let found = search(&shown, &text, &Query::new("DURAN").unwrap());
//! assert_eq!(found.rows, [0]);
//! // Groups with nothing found are left out.
//! assert_eq!(found.groups.unwrap().len(), 1);
//! ```

use super::{Group, ViewRows};

/// What a search looks for: the text typed, folded as rows' text is ([`fold`]), its ends
/// trimmed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query {
    folded: String,
}

impl Query {
    /// The query for `text`, or None when there's nothing in it but spaces (nothing searched).
    pub fn new(text: &str) -> Option<Self> {
        let folded = fold(text.trim());
        let folded = folded.trim();
        (!folded.is_empty()).then(|| Self {
            folded: folded.to_owned(),
        })
    }

    /// The text looked for, folded.
    pub fn folded(&self) -> &str {
        &self.folded
    }

    /// Whether `text` (as a cell shows it, not folded) contains it.
    pub fn matches(&self, text: &str) -> bool {
        fold(text).contains(self.folded.as_str())
    }
}

/// Each row's text to search, folded once: its title and its shown properties' words, kept
/// apart so a query never matches across two of them. Indexed as the rows given to
/// [`query`](super::query) are; a row never [`set`](Self::set) matches nothing.
#[derive(Clone, Debug, Default)]
pub struct SearchText {
    texts: Vec<Box<str>>,
}

/// Between a row's words in its folded text: a character [`fold`] never leaves in a word or a
/// query, so a query can't match across two words.
const APART: char = '\n';

impl SearchText {
    /// Room for `rows` rows, none of them with text yet.
    pub fn new(rows: usize) -> Self {
        Self {
            texts: vec![Box::default(); rows],
        }
    }

    /// The words row `row` is searched by: its title and what its shown cells show. Rows past
    /// those it was made with are made room for.
    pub fn set<I, S>(&mut self, row: usize, words: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut text = String::new();
        for w in words {
            let w = w.as_ref();
            if w.is_empty() {
                continue;
            }
            if !text.is_empty() {
                text.push(APART);
            }
            fold_into(w, &mut text);
        }
        if row >= self.texts.len() {
            self.texts.resize(row + 1, Box::default());
        }
        self.texts[row] = text.into_boxed_str();
    }

    /// Row `row`'s folded text, its words a line each (empty for a row with none).
    pub fn text(&self, row: usize) -> &str {
        self.texts.get(row).map_or("", |t| t)
    }

    /// Whether row `row`'s title or any of its words contains `query`.
    pub fn matches(&self, row: usize, query: &Query) -> bool {
        self.text(row).contains(query.folded.as_str())
    }
}

/// What `shown` (a view's rows, from [`query`](super::query)) shows of the rows `query` is
/// found in: the same rows in the same order, and the same groups in theirs, each with only its
/// rows found. Groups with none found are left out, so a board's columns and a table's or
/// list's sections show only where there's something. Its issues are the view's.
pub fn search(shown: &ViewRows, text: &SearchText, query: &Query) -> ViewRows {
    let needle = query.folded.as_str();
    let len = shown.rows.iter().max().map_or(0, |&m| m + 1);
    let mut found = vec![false; len];
    let mut rows = Vec::new();
    for &r in &shown.rows {
        if text.text(r).contains(needle) {
            found[r] = true;
            rows.push(r);
        }
    }
    let groups = shown.groups.as_ref().map(|groups| {
        groups
            .iter()
            .filter_map(|g| {
                let rows: Vec<usize> = g
                    .rows
                    .iter()
                    .copied()
                    .filter(|&r| found.get(r).copied().unwrap_or(false))
                    .collect();
                (!rows.is_empty()).then(|| Group {
                    key: g.key.clone(),
                    rows,
                })
            })
            .collect()
    });
    ViewRows {
        rows,
        groups,
        issues: shown.issues.clone(),
    }
}

/// `text` as it's searched: lowercase, Latin letters without their accents ("Durán" is
/// "duran", "Straße" "strasse", "Œuvre" "oeuvre"), combining marks dropped, and line breaks,
/// tabs and other control characters as spaces.
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    fold_into(text, &mut out);
    out
}

/// [`fold`], appended to `out`.
pub fn fold_into(text: &str, out: &mut String) {
    for c in text.chars() {
        if c.is_ascii() {
            out.push(if c.is_ascii_control() {
                ' '
            } else {
                c.to_ascii_lowercase()
            });
            continue;
        }
        for l in c.to_lowercase() {
            fold_char(l, out);
        }
    }
}

/// A lowercase character that isn't ASCII, folded.
fn fold_char(c: char, out: &mut String) {
    let base: &str = match c {
        // Combining marks (an accent typed apart from its letter): dropped.
        '\u{0300}'..='\u{036F}'
        | '\u{1AB0}'..='\u{1AFF}'
        | '\u{1DC0}'..='\u{1DFF}'
        | '\u{20D0}'..='\u{20FF}'
        | '\u{FE20}'..='\u{FE2F}' => "",
        c if c.is_control() => " ",
        // Latin-1.
        'à'..='å' => "a",
        'æ' => "ae",
        'ç' => "c",
        'è'..='ë' => "e",
        'ì'..='ï' => "i",
        'ð' => "d",
        'ñ' => "n",
        'ò'..='ö' | 'ø' => "o",
        'ù'..='ü' => "u",
        'ý' | 'ÿ' => "y",
        'þ' => "th",
        'ß' => "ss",
        // Latin Extended-A (lowercase, as `to_lowercase` leaves it).
        'ā' | 'ă' | 'ą' => "a",
        'ć' | 'ĉ' | 'ċ' | 'č' => "c",
        'ď' | 'đ' => "d",
        'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => "e",
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => "g",
        'ĥ' | 'ħ' => "h",
        'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' => "i",
        'ĳ' => "ij",
        'ĵ' => "j",
        'ķ' | 'ĸ' => "k",
        'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => "l",
        'ń' | 'ņ' | 'ň' | 'ŉ' | 'ŋ' => "n",
        'ō' | 'ŏ' | 'ő' => "o",
        'œ' => "oe",
        'ŕ' | 'ŗ' | 'ř' => "r",
        'ś' | 'ŝ' | 'ş' | 'š' | 'ſ' => "s",
        'ţ' | 'ť' | 'ŧ' => "t",
        'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => "u",
        'ŵ' => "w",
        'ŷ' => "y",
        'ź' | 'ż' | 'ž' => "z",
        // Latin Extended-B and Additional letters used in Romanian and Vietnamese.
        'ș' => "s",
        'ț' => "t",
        'ơ' => "o",
        'ư' => "u",
        '\u{1EA1}'..='\u{1EB7}' => "a",
        '\u{1EB9}'..='\u{1EC7}' => "e",
        '\u{1EC9}' | '\u{1ECB}' => "i",
        '\u{1ECD}'..='\u{1EE3}' => "o",
        '\u{1EE5}'..='\u{1EF1}' => "u",
        '\u{1EF3}'..='\u{1EF9}' => "y",
        _ => {
            out.push(c);
            return;
        }
    };
    out.push_str(base);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::GroupKey;

    #[test]
    fn folds_case_and_accents() {
        assert_eq!(fold("Ana Sofía Durán"), "ana sofia duran");
        assert_eq!(fold("STRASSE Straße"), "strasse strasse");
        assert_eq!(fold("Œuvre Ærø Łódź"), "oeuvre aero lodz");
        assert_eq!(fold("Ștefan Nguyễn"), "stefan nguyen");
        // An accent typed apart from its letter (NFD) folds the same.
        assert_eq!(fold("Dura\u{301}n"), "duran");
        assert_eq!(fold("a\tb\nc"), "a b c");
        // Scripts without case or accents stay as they are.
        assert_eq!(fold("東京 Ελλάδα"), "東京 ελλάδα");
    }

    #[test]
    fn a_query_is_trimmed_and_blank_is_none() {
        assert_eq!(Query::new("  "), None);
        assert_eq!(Query::new(""), None);
        assert_eq!(Query::new("  Durán ").unwrap().folded(), "duran");
        assert!(Query::new("A SOF").unwrap().matches("Ana Sofía"));
    }

    fn grouped() -> (ViewRows, SearchText) {
        let mut text = SearchText::new(5);
        text.set(0, ["Rowan Pike", "Kestrel Ceramics"]);
        text.set(1, ["Imani Castell", "Bluefin Robotics"]);
        text.set(2, ["Theo Brandt", ""]);
        text.set(
            3,
            ["Ana Sofía Durán", "Kestrel Ceramics", "Mentor, Investor"],
        );
        // Row 4 is filtered out by the view: it's never found.
        text.set(4, ["Kestrel"]);
        let g = |k: &str, rows: &[usize]| Group {
            key: GroupKey::Option(k.into()),
            rows: rows.to_vec(),
        };
        let shown = ViewRows {
            rows: vec![3, 0, 1, 2],
            groups: Some(vec![
                Group {
                    key: GroupKey::Empty,
                    rows: vec![2],
                },
                g("warm", &[3, 0]),
                g("lead", &[1]),
                // A row in two groups (a multi-select's) is in each.
                g("mentor", &[3]),
            ]),
            issues: Vec::new(),
        };
        (shown, text)
    }

    #[test]
    fn keeps_the_views_order_and_drops_empty_groups() {
        let (shown, text) = grouped();
        let found = search(&shown, &text, &Query::new("kestrel").unwrap());
        assert_eq!(found.rows, [3, 0], "the view's order, row 4 still left out");
        let groups = found.groups.unwrap();
        let keys: Vec<String> = groups.iter().map(|g| g.key.to_string()).collect();
        assert_eq!(keys, ["option:warm", "option:mentor"]);
        assert_eq!(groups[0].rows, [3, 0]);
    }

    #[test]
    fn matches_any_word_but_never_across_two() {
        let (shown, text) = grouped();
        let rows = |q: &str| search(&shown, &text, &Query::new(q).unwrap()).rows;
        assert_eq!(rows("DURAN"), [3]);
        assert_eq!(rows("investor"), [3]);
        assert_eq!(rows("o"), [3, 0, 1, 2]);
        // "Pike" ends one word and "Kestrel" starts the next.
        assert!(rows("pike kestrel").is_empty());
        assert!(rows("pikekestrel").is_empty());
        assert!(rows("nobody").is_empty());
    }

    #[test]
    fn an_ungrouped_view_stays_ungrouped() {
        let (mut shown, text) = grouped();
        shown.groups = None;
        let found = search(&shown, &text, &Query::new("kestrel").unwrap());
        assert_eq!(found.rows, [3, 0]);
        assert_eq!(found.groups, None);
    }

    #[test]
    fn rows_without_text_are_never_found() {
        let shown = ViewRows {
            rows: vec![0, 7],
            groups: None,
            issues: Vec::new(),
        };
        let mut text = SearchText::new(1);
        text.set(0, ["Zed"]);
        assert_eq!(search(&shown, &text, &Query::new("z").unwrap()).rows, [0]);
    }
}
