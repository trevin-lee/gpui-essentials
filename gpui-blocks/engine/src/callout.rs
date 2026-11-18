//! A callout's look, as Notion's: an icon (one emoji) before its blocks and a background colour,
//! both optional, over its kind's (`note`, `tip`…), which it keeps. In Markdown they're an HTML
//! comment after the marker, which GitHub doesn't show: `> [!TIP] <!-- icon:💡 color:yellow -->`.
//! Only valid values are ever read or written; the server refuses others.

use gpui::SharedString;
use unicode_segmentation::UnicodeSegmentation as _;

/// The colours a callout may have: Notion's background colours, by name.
pub const CALLOUT_COLORS: [&str; 9] = [
    "gray", "brown", "orange", "yellow", "green", "blue", "purple", "pink", "red",
];

/// Whether `icon` may be a callout's icon: one grapheme (an emoji, flags and joined ones
/// included) of at most 16 bytes, with no space or control character, and not an `asset:`.
pub fn callout_icon_ok(icon: &str) -> bool {
    !icon.is_empty()
        && icon.len() <= 16
        && icon.graphemes(true).count() == 1
        && !icon.chars().any(|c| c.is_whitespace() || c.is_control())
        && !icon.starts_with("asset:")
}

/// Whether `color` may be a callout's colour: one of [`CALLOUT_COLORS`].
pub fn callout_color_ok(color: &str) -> bool {
    CALLOUT_COLORS.contains(&color)
}

/// The icon and colour an HTML comment after a callout's marker gives it
/// (`<!-- icon:💡 color:yellow -->`): `None` when it's anything but `icon:` and `color:` keys
/// with valid values, at least one of them.
#[allow(clippy::type_complexity)]
pub fn read_callout_look(comment: &str) -> Option<(Option<SharedString>, Option<SharedString>)> {
    let inner = comment
        .trim()
        .strip_prefix("<!--")?
        .strip_suffix("-->")?
        .trim();
    let (mut icon, mut color) = (None, None);
    for word in inner.split_ascii_whitespace() {
        let (key, value) = word.split_once(':')?;
        match key {
            "icon" if icon.is_none() && callout_icon_ok(value) => icon = Some(value.into()),
            "color" if color.is_none() && callout_color_ok(value) => color = Some(value.into()),
            _ => return None,
        }
    }
    (icon.is_some() || color.is_some()).then_some((icon, color))
}

/// The comment that writes a callout's look after its marker, with a space before it
/// (` <!-- icon:💡 color:yellow -->`); empty for none. Invalid values are left out.
pub fn callout_look_comment(icon: Option<&str>, color: Option<&str>) -> String {
    let mut words = Vec::new();
    if let Some(icon) = icon.filter(|i| callout_icon_ok(i)) {
        words.push(format!("icon:{icon}"));
    }
    if let Some(color) = color.filter(|c| callout_color_ok(c)) {
        words.push(format!("color:{color}"));
    }
    if words.is_empty() {
        String::new()
    } else {
        format!(" <!-- {} -->", words.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_and_colours_are_checked() {
        for icon in ["💡", "👩‍💻", "🇯🇵", "❤️", "A"] {
            assert!(callout_icon_ok(icon), "{icon}");
        }
        for icon in ["", "💡💡", "ab", " ", "asset:x", "\u{7}"] {
            assert!(!callout_icon_ok(icon), "{icon:?}");
        }
        assert!(callout_color_ok("yellow"));
        assert!(!callout_color_ok("teal"));
        assert!(!callout_color_ok("Yellow"));
    }

    #[test]
    fn looks_read_strictly_and_write_back() {
        assert_eq!(
            read_callout_look("<!-- icon:💡 color:yellow -->"),
            Some((Some("💡".into()), Some("yellow".into())))
        );
        assert_eq!(
            read_callout_look("<!-- color:blue -->"),
            Some((None, Some("blue".into())))
        );
        for bad in [
            "<!-- -->",
            "<!-- icon:💡 color:teal -->",
            "<!-- shape:round -->",
            "<!-- icon:ab -->",
            "<!-- a note -->",
            "<!-- icon:💡 icon:💡 -->",
        ] {
            assert_eq!(read_callout_look(bad), None, "{bad}");
        }
        assert_eq!(
            callout_look_comment(Some("💡"), Some("yellow")),
            " <!-- icon:💡 color:yellow -->"
        );
        assert_eq!(callout_look_comment(None, Some("teal")), "");
        assert_eq!(callout_look_comment(None, None), "");
    }
}
