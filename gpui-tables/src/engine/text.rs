//! Rows' text as the filter's text conditions and the sorts compare it: lowercased once per row
//! and property, and kept until the rows change. A filter typed a key at a time, or a sort
//! changed, then reads each row's text again without lowercasing anything.
//!
//! The text is lowercased as the filter and the sorts always have (`sort::fold`): by
//! case alone, accents kept (the search, not these, drops accents).

use super::Field;
use crate::model::{Doc, Property, SelectOption, Value};

/// Lists of more than this many properties' text are forgotten when the next query starts, so
/// filtering by one property after another doesn't keep every one of them.
const KEPT: usize = 16;

/// Rows' lowercased text, a list for each property a filter or sort read it by, kept for the
/// next query of the same rows ([`super::query_with`]). [`clear`](Self::clear) it whenever the
/// rows change; a change to the database (its views, its properties) needs nothing, as a list
/// made through a property's options is made again once they change.
#[derive(Clone, Debug, Default)]
pub struct RowText {
    /// The rows the lists were made of (their address and count): other rows, and the lists
    /// are made again. A check only, as a new slice may be where the last one was.
    rows: (usize, usize),
    lists: Vec<List>,
}

/// Where a list's text is read from.
#[derive(Clone, Debug, PartialEq)]
enum Source {
    Title,
    Stored(String),
}

#[derive(Clone, Debug)]
struct List {
    source: Source,
    /// The options the text was shown through (a property that was a select shows its options'
    /// names for their ids), or None.
    through: Option<Vec<SelectOption>>,
    /// Every row's text, lowercased, one after another.
    text: String,
    /// Where each row's text ends in `text`; it starts where the row before's ended.
    ends: Vec<usize>,
}

impl RowText {
    /// Nothing kept yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// Forgets every row's text: for when the rows change.
    pub fn clear(&mut self) {
        self.lists.clear();
        self.rows = (0, 0);
    }

    /// Readies the lists for a query of `rows`: those of other rows are forgotten, and all of
    /// them once there are too many.
    pub(crate) fn begin(&mut self, rows: &[Doc]) {
        let id = (rows.as_ptr() as usize, rows.len());
        if self.rows != id || self.lists.len() > KEPT {
            self.clear();
            self.rows = id;
        }
    }

    /// The list of `field`'s text (shown through `shown`'s options, when given), made now if
    /// it isn't kept: None for a field without text. Call [`begin`](Self::begin) first.
    pub(crate) fn list(
        &mut self,
        rows: &[Doc],
        field: Field<'_>,
        shown: Option<&Property>,
    ) -> Option<usize> {
        let source = match field {
            Field::Title => Source::Title,
            Field::Stored(p) => Source::Stored(p.id.clone()),
            _ => return None,
        };
        let through = shown.map(Property::options);
        if let Some(i) = self
            .lists
            .iter()
            .position(|l| l.source == source && l.through.as_deref() == through)
        {
            return Some(i);
        }
        let mut text = String::new();
        let mut ends = Vec::with_capacity(rows.len());
        for row in rows {
            let s = match &source {
                Source::Title => Some(row.title.as_str()),
                Source::Stored(id) => match row.value(id) {
                    Some(Value::Text(s)) => Some(s.as_str()),
                    _ => None,
                },
            };
            if let Some(s) = s {
                fold_into(shown.map_or(s, |p| p.shown_text(s)), &mut text);
            }
            ends.push(text.len());
        }
        self.lists.push(List {
            source,
            through: through.map(<[SelectOption]>::to_vec),
            text,
            ends,
        });
        Some(self.lists.len() - 1)
    }

    /// Row `row`'s text in list `list`, lowercased: empty for a row without text.
    pub(crate) fn get(&self, list: usize, row: usize) -> &str {
        let l = &self.lists[list];
        let start = if row == 0 { 0 } else { l.ends[row - 1] };
        &l.text[start..l.ends[row]]
    }
}

/// [`super::sort::fold`], appended to `out` without a string of its own.
pub(crate) fn fold_into(s: &str, out: &mut String) {
    if s.is_ascii() {
        let start = out.len();
        out.push_str(s);
        out[start..].make_ascii_lowercase();
    } else if s.contains('Σ') {
        // A capital sigma lowercases by where it stands in its word, which only the whole
        // string's lowercasing looks at.
        out.push_str(&s.to_lowercase());
    } else {
        // Otherwise `str::to_lowercase` is each character's lowercase in turn.
        out.extend(s.chars().flat_map(char::to_lowercase));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PropertyConfig, PropertyType, Values};

    #[test]
    fn folds_as_the_sorts_fold() {
        let samples = [
            "",
            "Plain ASCII, MiXeD",
            "Café Über ÉCOLE",
            "ΣΊΣΥΦΟΣ",
            "ΌΣΟΣ Σ ΑΣ.",
            "aΣ bΣc Σ",
            "İstanbul ǅ ǈ ẞ",
            "Cafe\u{301} \u{1F600} 東京",
            "ﬃ ŉ Ω K Å",
        ];
        for s in samples {
            let mut out = String::from("before");
            fold_into(s, &mut out);
            assert_eq!(out, format!("before{}", super::super::sort::fold(s)), "{s}");
        }
    }

    #[test]
    fn keeps_each_property_until_its_options_or_the_rows_change() {
        let mut values = Values::new();
        values.insert("t".into(), Value::text("o1"));
        let rows = vec![
            Doc::row("a", "db", "Ana ÚRSULA", "a0", values),
            Doc::row("b", "db", "", "a1", Values::new()),
        ];
        let options = |name: &str| PropertyConfig {
            options: Some(vec![SelectOption {
                id: "o1".into(),
                name: name.into(),
                color: Default::default(),
            }]),
            ..PropertyConfig::default()
        };
        let title = Property::new("name", "Name", PropertyType::Title, "a0");
        let text = Property::new("t", "T", PropertyType::Text, "a1").with_config(options("Warm"));
        let mut kept = RowText::new();
        kept.begin(&rows);
        let t = kept.list(&rows, Field::of(&title), None).unwrap();
        assert_eq!((kept.get(t, 0), kept.get(t, 1)), ("ana úrsula", ""));
        let s = kept.list(&rows, Field::of(&text), Some(&text)).unwrap();
        assert_eq!((kept.get(s, 0), kept.get(s, 1)), ("warm", ""));
        let plain = kept.list(&rows, Field::of(&text), None).unwrap();
        assert_eq!(kept.get(plain, 0), "o1");
        // Kept: the same lists again.
        assert_eq!(kept.list(&rows, Field::of(&title), None), Some(t));
        // An option renamed: made again.
        let renamed = text.clone().with_config(options("Cold"));
        let r = kept
            .list(&rows, Field::of(&renamed), Some(&renamed))
            .unwrap();
        assert_ne!(r, s);
        assert_eq!(kept.get(r, 0), "cold");
        // Other rows: all forgotten.
        let other = rows[..1].to_vec();
        kept.begin(&other);
        assert_eq!(kept.lists.len(), 0);
        let created = Property::new("c", "C", PropertyType::CreatedTime, "a2");
        assert_eq!(kept.list(&other, Field::of(&created), None), None);
    }
}
