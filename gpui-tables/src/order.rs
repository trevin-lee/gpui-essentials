//! Fractional indexes: the `order` strings that place rows, properties, views and columns.
//!
//! A key sorts by plain byte comparison, and a new key fits between any two others, so moving
//! something changes only its own key. The scheme is the one rocicorp's `fractional-indexing`
//! uses (and Figma's before it), so keys made by any client using that scheme with the same cap
//! mix with these: an integer part whose first character says its length (`a0`, `a1`, ... `az`,
//! `b00`, ...), then an optional fraction in base 62 that never ends in `0`. Appending at the end
//! steps the integer part, so 10,000 appends make keys of four characters, not thousands.
//!
//! Keys are at most [`MAX_LEN`] characters. Keys from elsewhere may be malformed (an agent wrote
//! one, say, or an app's older scheme did) or too long; [`between`] then says so rather than
//! guessing, and the host renumbers the list with [`spread`]. `tests/fixtures/order-vectors.json`
//! holds the cases, with expected keys from fractional-indexing itself, for other clients to test
//! against. One difference from fractional-indexing: a key with a character outside the alphabet
//! is invalid here (it accepts some).

use std::fmt;

const DIGITS: &[u8; 62] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const ZERO: u8 = b'0';
const LAST: u8 = b'z';
/// The smallest integer part, `A` and 26 zeros, which no key may be on its own.
const SMALLEST_INT_LEN: usize = 27;
/// The longest key, in characters (all ASCII, so bytes too): longer ones are invalid, and a key
/// that would be longer isn't made ([`OrderError::TooLong`]).
pub const MAX_LEN: usize = 256;

/// Why a key couldn't be made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OrderError {
    /// A key that isn't a valid fractional index.
    Invalid(String),
    /// The lower key isn't below the upper one.
    NotBelow(String, String),
    /// The key between would be longer than [`MAX_LEN`]: renumber the list.
    TooLong,
    /// The key space ran out at one end (after about 10^46 appends).
    Exhausted,
}

impl fmt::Display for OrderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(k) => write!(f, "{k:?} isn't a fractional index"),
            Self::NotBelow(a, b) => write!(f, "{a:?} isn't below {b:?}"),
            Self::TooLong => write!(f, "the key would be longer than {MAX_LEN} characters"),
            Self::Exhausted => write!(f, "no more keys at that end"),
        }
    }
}

impl std::error::Error for OrderError {}

impl OrderError {
    /// The error's name in the test vectors: `invalid`, `notBelow`, `tooLong` or `exhausted`.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Invalid(_) => "invalid",
            Self::NotBelow(..) => "notBelow",
            Self::TooLong => "tooLong",
            Self::Exhausted => "exhausted",
        }
    }
}

fn digit(c: u8) -> Option<usize> {
    match c {
        b'0'..=b'9' => Some((c - b'0') as usize),
        b'A'..=b'Z' => Some((c - b'A') as usize + 10),
        b'a'..=b'z' => Some((c - b'a') as usize + 36),
        _ => None,
    }
}

/// The length of the integer part whose first character is `head`.
fn int_len(head: u8) -> Option<usize> {
    match head {
        b'a'..=b'z' => Some((head - b'a') as usize + 2),
        b'A'..=b'Z' => Some((b'Z' - head) as usize + 2),
        _ => None,
    }
}

/// Splits a key into its integer part and fraction, checking it.
fn split(key: &str) -> Result<(&str, &str), OrderError> {
    let bad = || OrderError::Invalid(key.to_owned());
    let b = key.as_bytes();
    if b.len() > MAX_LEN {
        return Err(bad());
    }
    let n = int_len(*b.first().ok_or_else(bad)?).ok_or_else(bad)?;
    if n > b.len() || !b[1..].iter().all(|&c| digit(c).is_some()) {
        return Err(bad());
    }
    let (int, frac) = key.split_at(n);
    if frac.as_bytes().last() == Some(&ZERO) {
        return Err(bad());
    }
    if frac.is_empty() && n == SMALLEST_INT_LEN && int[1..].bytes().all(|c| c == ZERO) {
        return Err(bad());
    }
    Ok((int, frac))
}

/// Whether `key` is a valid fractional index.
pub fn is_valid(key: &str) -> bool {
    split(key).is_ok()
}

/// A fraction between two (`a` < `b`; `b` None for 1). Neither ends in `0`.
fn midpoint(a: &[u8], b: Option<&[u8]>) -> Vec<u8> {
    if let Some(b) = b {
        // Keep the common prefix (a padded with zeros) and recurse past it.
        let mut n = 0;
        while n < b.len() && a.get(n).copied().unwrap_or(ZERO) == b[n] {
            n += 1;
        }
        if n > 0 {
            let mut out = b[..n].to_vec();
            out.extend(midpoint(a.get(n..).unwrap_or(&[]), Some(&b[n..])));
            return out;
        }
    }
    let da = a.first().map_or(0, |&c| digit(c).unwrap_or(0));
    let db = b.map_or(DIGITS.len(), |b| digit(b[0]).unwrap_or(DIGITS.len()));
    if db.saturating_sub(da) > 1 {
        vec![DIGITS[(da + db).div_ceil(2)]]
    } else if let Some(b) = b.filter(|b| b.len() > 1) {
        // b's first digit alone is between a and b.
        vec![b[0]]
    } else {
        let mut out = vec![DIGITS[da]];
        out.extend(midpoint(a.get(1..).unwrap_or(&[]), None));
        out
    }
}

/// The next integer part, or None past the largest.
fn increment(int: &str) -> Option<String> {
    let b = int.as_bytes();
    let (head, mut digits) = (b[0], b[1..].to_vec());
    for d in digits.iter_mut().rev() {
        if *d == LAST {
            *d = ZERO;
        } else {
            *d = DIGITS[digit(*d)? + 1];
            return Some(as_string(head, digits));
        }
    }
    // Carried past the first digit: the next length.
    match head {
        b'Z' => Some("a0".into()),
        b'z' => None,
        _ => {
            let h = head + 1;
            if h > b'a' {
                digits.push(ZERO);
            } else {
                digits.pop();
            }
            Some(as_string(h, digits))
        }
    }
}

/// The previous integer part, or None before the smallest.
fn decrement(int: &str) -> Option<String> {
    let b = int.as_bytes();
    let (head, mut digits) = (b[0], b[1..].to_vec());
    for d in digits.iter_mut().rev() {
        if *d == ZERO {
            *d = LAST;
        } else {
            *d = DIGITS[digit(*d)? - 1];
            return Some(as_string(head, digits));
        }
    }
    match head {
        b'a' => Some("Zz".into()),
        b'A' => None,
        _ => {
            let h = head - 1;
            if h < b'Z' {
                digits.push(LAST);
            } else {
                digits.pop();
            }
            Some(as_string(h, digits))
        }
    }
}

fn as_string(head: u8, digits: Vec<u8>) -> String {
    let mut v = Vec::with_capacity(digits.len() + 1);
    v.push(head);
    v.extend(digits);
    // Only ASCII digits and letters are ever written.
    String::from_utf8(v).unwrap_or_default()
}

fn joined(int: &str, frac: Vec<u8>) -> String {
    let mut s = int.to_owned();
    s.push_str(std::str::from_utf8(&frac).unwrap_or_default());
    s
}

/// The first key of an empty list: `a0`.
pub fn first() -> String {
    "a0".into()
}

/// A key between `a` and `b`: after `a` (None: at the start) and before `b` (None: at the
/// end). With both None, the first key.
pub fn between(a: Option<&str>, b: Option<&str>) -> Result<String, OrderError> {
    let key = between_uncapped(a, b)?;
    if key.len() > MAX_LEN {
        return Err(OrderError::TooLong);
    }
    Ok(key)
}

fn between_uncapped(a: Option<&str>, b: Option<&str>) -> Result<String, OrderError> {
    let sa = a.map(split).transpose()?;
    let sb = b.map(split).transpose()?;
    match (a, b, sa, sb) {
        (Some(a), Some(b), _, _) if a >= b => Err(OrderError::NotBelow(a.into(), b.into())),
        (None, None, _, _) => Ok(first()),
        (None, Some(b), _, Some((ib, fb))) => {
            if ib.len() == SMALLEST_INT_LEN && ib[1..].bytes().all(|c| c == ZERO) {
                return Ok(joined(ib, midpoint(&[], Some(fb.as_bytes()))));
            }
            if ib.len() < b.len() {
                return Ok(ib.to_owned());
            }
            decrement(ib).ok_or(OrderError::Exhausted)
        }
        (Some(_), None, Some((ia, fa)), _) => Ok(match increment(ia) {
            Some(i) => i,
            None => joined(ia, midpoint(fa.as_bytes(), None)),
        }),
        (Some(_), Some(b), Some((ia, fa)), Some((ib, fb))) => {
            if ia == ib {
                return Ok(joined(ia, midpoint(fa.as_bytes(), Some(fb.as_bytes()))));
            }
            let i = increment(ia).ok_or(OrderError::Exhausted)?;
            if i.as_str() < b {
                Ok(i)
            } else {
                Ok(joined(ia, midpoint(fa.as_bytes(), None)))
            }
        }
        _ => unreachable!("split checked both keys"),
    }
}

/// `n` keys in order between `a` and `b` (as [`between`]), spread so none is much longer than
/// it needs to be: for dropping several rows at once.
pub fn n_between(a: Option<&str>, b: Option<&str>, n: usize) -> Result<Vec<String>, OrderError> {
    match n {
        0 => return Ok(Vec::new()),
        1 => return Ok(vec![between(a, b)?]),
        _ => {}
    }
    match (a, b) {
        (_, None) => {
            let mut out = Vec::with_capacity(n);
            let mut c = between(a, None)?;
            for _ in 1..n {
                let next = between(Some(&c), None)?;
                out.push(std::mem::replace(&mut c, next));
            }
            out.push(c);
            Ok(out)
        }
        (None, Some(_)) => {
            let mut out = Vec::with_capacity(n);
            let mut c = between(None, b)?;
            for _ in 1..n {
                let next = between(None, Some(&c))?;
                out.push(std::mem::replace(&mut c, next));
            }
            out.push(c);
            out.reverse();
            Ok(out)
        }
        (Some(_), Some(_)) => {
            let mid = n / 2;
            let c = between(a, b)?;
            let mut out = n_between(a, Some(&c), mid)?;
            out.push(c.clone());
            out.extend(n_between(Some(&c), b, n - mid - 1)?);
            Ok(out)
        }
    }
}

/// `n` fresh keys in order, as short as they can be: for renumbering a list whose keys are
/// malformed or have grown long.
pub fn spread(n: usize) -> Vec<String> {
    // From nothing to nothing, every step is an increment, which can't fail this side of 10^46.
    n_between(None, None, n).unwrap_or_default()
}

/// The keys for moving `count` items so they sit between `before` and `after` in a list sorted
/// by key, where either may be None (the start or end). When those neighbours' keys are
/// malformed or out of order, the error says so and the host renumbers with [`spread`].
pub fn for_drop(
    before: Option<&str>,
    after: Option<&str>,
    count: usize,
) -> Result<Vec<String>, OrderError> {
    n_between(before, after, count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(a: Option<&str>, c: Option<&str>) -> String {
        between(a, c).unwrap()
    }

    #[test]
    fn keys_match_the_reference_implementation() {
        // Cases from rocicorp/fractional-indexing's own tests.
        assert_eq!(b(None, None), "a0");
        assert_eq!(b(None, Some("a0")), "Zz");
        assert_eq!(b(None, Some("Zz")), "Zy");
        assert_eq!(b(Some("a0"), None), "a1");
        assert_eq!(b(Some("a1"), None), "a2");
        assert_eq!(b(Some("a0"), Some("a1")), "a0V");
        assert_eq!(b(Some("a1"), Some("a2")), "a1V");
        assert_eq!(b(Some("a0V"), Some("a1")), "a0l");
        assert_eq!(b(Some("Zz"), Some("a0")), "ZzV");
        assert_eq!(b(Some("Zz"), Some("a1")), "a0");
        assert_eq!(b(None, Some("Y00")), "Xzzz");
        assert_eq!(b(Some("bzz"), None), "c000");
        assert_eq!(b(Some("a0"), Some("a0V")), "a0G");
        assert_eq!(b(Some("a0"), Some("a0G")), "a08");
        assert_eq!(b(Some("b125"), Some("b129")), "b127");
        assert_eq!(b(Some("a0"), Some("a1V")), "a1");
        assert_eq!(b(Some("Zz"), Some("a01")), "a0");
        assert_eq!(b(None, Some("a0V")), "a0");
        assert_eq!(b(None, Some("b999")), "b99");
        assert_eq!(
            b(Some("zzzzzzzzzzzzzzzzzzzzzzzzzzy"), None),
            "zzzzzzzzzzzzzzzzzzzzzzzzzzz"
        );
        assert_eq!(
            b(Some("zzzzzzzzzzzzzzzzzzzzzzzzzzz"), None),
            "zzzzzzzzzzzzzzzzzzzzzzzzzzzV"
        );
    }

    #[test]
    fn malformed_keys_are_refused() {
        for bad in [
            "",
            "0",
            "a",
            "a00",
            "a0 ",
            "b0",
            "A00000000000000000000000000",
            "a0/",
            "é0",
            "a0é",
        ] {
            if bad == "a00" {
                // A trailing zero in the fraction.
                assert!(!is_valid(bad));
                continue;
            }
            assert!(!is_valid(bad), "{bad:?}");
            assert!(matches!(
                between(Some(bad), None),
                Err(OrderError::Invalid(_))
            ));
            assert!(matches!(
                between(None, Some(bad)),
                Err(OrderError::Invalid(_))
            ));
        }
        assert_eq!(
            between(Some("a1"), Some("a0")),
            Err(OrderError::NotBelow("a1".into(), "a0".into()))
        );
        assert!(between(Some("a1"), Some("a1")).is_err());
    }

    #[test]
    fn n_keys_are_ordered_and_short() {
        for (a, c) in [
            (None, None),
            (Some("a0"), None),
            (None, Some("a0")),
            (Some("a0"), Some("a1")),
        ] {
            let keys = n_between(a, c, 1_000).unwrap();
            assert_eq!(keys.len(), 1_000);
            assert!(keys.windows(2).all(|w| w[0] < w[1]));
            assert!(a.is_none_or(|a| a < keys[0].as_str()));
            assert!(c.is_none_or(|c| keys[999].as_str() < c));
            assert!(
                keys.iter().all(|k| is_valid(k) && k.len() <= 6),
                "{a:?} {c:?}"
            );
        }
        assert_eq!(n_between(None, None, 0).unwrap(), Vec::<String>::new());
        assert_eq!(spread(3), ["a0", "a1", "a2"]);
    }

    #[test]
    fn inserting_at_one_place_again_and_again_grows_slowly() {
        // The worst case: always just after the same key.
        let mut hi = "a1".to_owned();
        for _ in 0..300 {
            let k = b(Some("a0"), Some(&hi));
            assert!("a0" < k.as_str() && k < hi);
            hi = k;
        }
        assert!(hi.len() < 60, "{}", hi.len());
    }
}
