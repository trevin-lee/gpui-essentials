//! Icons: Lucide's whole set, bundled, and an element that draws one in the text's colour and
//! size around it.

#[rustfmt::skip]
mod lucide;

use std::borrow::Cow;

use gpui::{
    App, AssetSource, Hsla, IntoElement, Refineable as _, RenderOnce, SharedString,
    StyleRefinement, Styled, Window, svg,
};

pub use lucide::LUCIDE_VERSION;
use lucide::{ALIASES, ICONS};

/// A bundled Lucide icon. Each has a constant named after it (`IconName::ChevronLeft` is
/// Lucide's `chevron-left`); [`IconName::named`] finds one by its Lucide name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct IconName(&'static str);

impl IconName {
    /// The icon Lucide calls `name` (`"chevron-left"`), if it's bundled. Lucide's old names
    /// for its icons (`circle-alert`, now `circle-exclamation-point`), which lucide-react still
    /// takes and so agents still write, find the icon they're now called.
    pub fn named(name: &str) -> Option<Self> {
        let name = ALIASES
            .binary_search_by(|(a, _)| (*a).cmp(name))
            .map_or(name, |i| ALIASES[i].1);
        ICONS
            .binary_search_by(|(n, _)| (*n).cmp(name))
            .ok()
            .map(|i| Self(ICONS[i].0))
    }

    /// Every bundled icon, by name.
    pub fn all() -> impl ExactSizeIterator<Item = Self> {
        ICONS.iter().map(|(n, _)| Self(n))
    }

    /// Lucide's name for it.
    pub fn name(self) -> &'static str {
        self.0
    }

    /// Its path among the assets [`Icons`] serves: `icons/<name>.svg`.
    pub fn path(self) -> SharedString {
        format!("icons/{}.svg", self.0).into()
    }

    /// Its SVG.
    pub fn svg(self) -> &'static [u8] {
        ICONS[ICONS
            .binary_search_by(|(n, _)| (*n).cmp(self.0))
            .expect("a bundled icon")]
        .1
    }
}

/// Serves the bundled icons at `icons/<name>.svg`. Give it to the app
/// (`Application::with_assets`), or ask it from the host's own asset source for paths the host
/// doesn't have.
#[derive(Clone, Copy, Debug, Default)]
pub struct Icons;

impl AssetSource for Icons {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        Ok(path
            .strip_prefix("icons/")
            .and_then(|p| p.strip_suffix(".svg"))
            .and_then(IconName::named)
            .map(|i| Cow::Borrowed(i.svg())))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        Ok(if "icons/".starts_with(path) || path == "icons" {
            IconName::all().map(IconName::path).collect()
        } else {
            Vec::new()
        })
    }
}

/// An icon, drawn as large as the text around it and in its colour unless told otherwise. It
/// takes no part in the accessibility tree: label what it's in.
#[derive(IntoElement)]
pub struct Icon {
    path: SharedString,
    color: Option<Hsla>,
    style: StyleRefinement,
}

impl Icon {
    pub fn new(name: IconName) -> Self {
        Self::from_path(name.path())
    }

    /// An SVG of the host's own, from its assets: drawn the same way, as a mask in one colour.
    pub fn from_path(path: impl Into<SharedString>) -> Self {
        Self {
            path: path.into(),
            color: None,
            style: StyleRefinement::default(),
        }
    }

    /// Its colour (otherwise the text's).
    pub fn color(mut self, color: impl Into<Hsla>) -> Self {
        self.color = Some(color.into());
        self
    }
}

impl From<IconName> for Icon {
    fn from(name: IconName) -> Self {
        Icon::new(name)
    }
}

impl Styled for Icon {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Icon {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        let text = window.text_style();
        let size = text.font_size.to_pixels(window.rem_size());
        let mut el = svg()
            .path(self.path)
            .flex_none()
            .size(size)
            .text_color(self.color.unwrap_or(text.color));
        el.style().refine(&self.style);
        el
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_are_found_by_name_and_served_by_path() {
        assert_eq!(IconName::named("chevron-left"), Some(IconName::ChevronLeft));
        assert_eq!(IconName::named("no-such-icon"), None);
        assert_eq!(
            IconName::ChevronRight.path().as_ref(),
            "icons/chevron-right.svg"
        );
        let svg = Icons.load("icons/chevron-left.svg").unwrap().unwrap();
        assert!(svg.starts_with(b"<svg"));
        assert!(Icons.load("icons/../secret.svg").unwrap().is_none());
        assert!(Icons.load("other/chevron-left.svg").unwrap().is_none());
        assert!(IconName::all().len() > 1000);
        // Strictly sorted by name, so every one can be found: an icon with variants
        // ("mail", "mail-check") sorts before them.
        let names: Vec<_> = IconName::all().map(IconName::name).collect();
        for pair in names.windows(2) {
            assert!(pair[0] < pair[1], "{} before {}", pair[0], pair[1]);
        }
        for name in &names {
            assert_eq!(IconName::named(name).map(IconName::name), Some(*name));
        }
        assert_eq!(IconName::named("mail"), Some(IconName::Mail));
        // Old names find the icon they're now called, in names and in asset paths.
        assert_eq!(
            IconName::named("circle-alert"),
            IconName::named("circle-exclamation-point")
        );
        assert_eq!(
            IconName::named("alert-circle"),
            IconName::named("circle-exclamation-point")
        );
        assert!(IconName::named("triangle-alert").is_some());
        assert!(IconName::named("building-2").is_some());
        assert!(Icons.load("icons/circle-alert.svg").unwrap().is_some());
        for pair in super::lucide::ALIASES.windows(2) {
            assert!(pair[0].0 < pair[1].0, "{} before {}", pair[0].0, pair[1].0);
        }
        for (old, new) in super::lucide::ALIASES {
            assert_eq!(
                IconName::named(old).map(IconName::name),
                Some(*new),
                "{old}"
            );
        }
        assert_eq!(IconName::named("bell"), Some(IconName::Bell));
    }
}
