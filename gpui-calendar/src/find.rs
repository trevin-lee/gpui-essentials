//! Finding events by text: a query's words, each of which an event's title, place, notes or
//! link labels must contain, ignoring case and accents ("cafe" finds "Café", "zurich" finds "Zürich").
//! Pure string work, tested on its own.

use std::ops::Range;

use crate::model::Event;

/// A query, folded once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Query {
    /// What was typed, as given.
    pub text: String,
    /// Its words, folded.
    words: Vec<String>,
}

impl Query {
    /// None for a query with no words in it.
    pub fn new(text: &str) -> Option<Self> {
        let (folded, _) = fold(text);
        let words: Vec<String> = folded.split_whitespace().map(str::to_string).collect();
        (!words.is_empty()).then(|| Self {
            text: text.to_string(),
            words,
        })
    }

    /// Whether every word is in one of the event's texts (not necessarily the same one).
    pub fn matches(&self, event: &Event) -> bool {
        let texts: Vec<String> = std::iter::once(event.title.as_ref())
            .chain(event.location.as_deref())
            .chain(event.notes.as_deref())
            .chain(event.links.iter().map(|l| l.label.as_ref()))
            .map(|t| fold(t).0)
            .collect();
        self.words
            .iter()
            .all(|w| texts.iter().any(|t| t.contains(w.as_str())))
    }

    /// Where the words are in `text`, as byte ranges of `text` itself, in order and merged.
    pub fn ranges(&self, text: &str) -> Vec<Range<usize>> {
        let (folded, map) = fold(text);
        let mut out: Vec<Range<usize>> = Vec::new();
        for w in &self.words {
            let mut from = 0;
            while let Some(at) = folded[from..].find(w.as_str()) {
                let (a, b) = (from + at, from + at + w.len());
                out.push(map[a]..map[b]);
                from = b;
            }
        }
        out.sort_by_key(|r| r.start);
        let mut merged: Vec<Range<usize>> = Vec::new();
        for r in out {
            match merged.last_mut() {
                Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
                _ => merged.push(r),
            }
        }
        merged
    }
}

/// `text` in lower case without accents, and for each of its bytes (and its end) the byte of
/// `text` it came from.
pub(crate) fn fold(text: &str) -> (String, Vec<usize>) {
    let mut out = String::with_capacity(text.len());
    let mut map = Vec::with_capacity(text.len() + 1);
    for (at, c) in text.char_indices() {
        for lower in c.to_lowercase() {
            // Combining marks (an accent typed apart from its letter) go.
            if ('\u{300}'..='\u{36f}').contains(&lower) {
                continue;
            }
            let plain = plain(lower);
            for _ in 0..plain.len() {
                map.push(at);
            }
            out.push_str(plain.as_ref());
        }
    }
    map.push(text.len());
    (out, map)
}

/// A lower-case letter without its accent: "é" is "e", "ß" is "ss", "æ" is "ae".
fn plain(c: char) -> std::borrow::Cow<'static, str> {
    let s: &'static str = match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => "a",
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => "c",
        'ď' | 'đ' | 'ð' => "d",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => "e",
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => "g",
        'ĥ' | 'ħ' => "h",
        'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' => "i",
        'ĵ' => "j",
        'ķ' => "k",
        'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => "l",
        'ñ' | 'ń' | 'ņ' | 'ň' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ŏ' | 'ő' => "o",
        'ŕ' | 'ŗ' | 'ř' => "r",
        'ś' | 'ŝ' | 'ş' | 'š' | 'ș' => "s",
        'ţ' | 'ť' | 'ŧ' | 'ț' => "t",
        'ù' | 'ú' | 'û' | 'ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => "u",
        'ŵ' => "w",
        'ý' | 'ÿ' | 'ŷ' => "y",
        'ź' | 'ż' | 'ž' => "z",
        'ß' => "ss",
        'æ' => "ae",
        'œ' => "oe",
        'þ' => "th",
        _ => return std::borrow::Cow::Owned(c.to_string()),
    };
    std::borrow::Cow::Borrowed(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EventLink, EventTime};
    use chrono::NaiveDate;

    fn event(title: &str) -> Event {
        let d = NaiveDate::from_ymd_opt(2026, 11, 18).unwrap();
        Event::new("e", title.to_string(), EventTime::all_day(d, d))
    }

    #[test]
    fn case_and_accents_dont_matter() {
        let q = Query::new("CAFE zurich").unwrap();
        assert!(q.matches(&event("Lunch at Café Zürich")));
        assert!(!q.matches(&event("Lunch at Café Bern")));
        // An accent typed apart from its letter (e + combining acute).
        assert!(Query::new("cafe").unwrap().matches(&event("Cafe\u{301}")));
        assert!(
            Query::new("strasse")
                .unwrap()
                .matches(&event("Hauptstraße 4"))
        );
        assert!(Query::new("É").unwrap().matches(&event("événement")));
    }

    #[test]
    fn words_can_be_in_any_of_the_texts() {
        let e = event("Design review")
            .location("Room 4")
            .link(EventLink::doc("d", "Q4 notes"));
        assert!(Query::new("review room").unwrap().matches(&e));
        assert!(Query::new("q4").unwrap().matches(&e));
        assert!(!Query::new("review kitchen").unwrap().matches(&e));
        // Notes count too, accents and all.
        let e = e.notes("Bring the résumé");
        assert!(Query::new("resume review").unwrap().matches(&e));
        assert!(Query::new("   ").is_none());
    }

    #[test]
    fn ranges_point_into_the_original_text() {
        let q = Query::new("zur cafe").unwrap();
        let text = "Café Zürich, café";
        let r = q.ranges(text);
        let found: Vec<&str> = r.iter().map(|r| &text[r.clone()]).collect();
        assert_eq!(found, vec!["Café", "Zür", "café"]);
        // Overlapping words merge.
        let q = Query::new("revi view").unwrap();
        assert_eq!(q.ranges("Review"), vec![0..6]);
    }
}
