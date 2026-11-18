use gpui::{Hsla, SharedString};

use crate::text_mark::{LinkMark, TextMark};

/// A kind of inline mark, keyed on exactly `TextMark`'s attributes (D12).
#[non_exhaustive]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum MarkKind {
    Bold,
    Italic,
    Strikethrough,
    Underline,
    Code,
    Highlight,
    Link,
}

impl MarkKind {
    pub(crate) fn is_set(self, mark: &TextMark) -> bool {
        match self {
            Self::Bold => mark.bold,
            Self::Italic => mark.italic,
            Self::Strikethrough => mark.strikethrough,
            Self::Underline => mark.underline,
            Self::Code => mark.code,
            Self::Highlight => mark.highlight.is_some(),
            Self::Link => mark.link.is_some(),
        }
    }

    /// Whether typing at the trailing edge of this mark extends it (D6).
    pub(crate) fn extends_at_boundary(self) -> bool {
        !matches!(self, Self::Link | Self::Code)
    }
}

/// Read-only view of the marks at a position. `TextMark` stays internal.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct MarkSet(pub(crate) TextMark);

impl MarkSet {
    pub fn has(&self, kind: MarkKind) -> bool {
        kind.is_set(&self.0)
    }

    pub fn highlight(&self) -> Option<Hsla> {
        self.0.highlight
    }

    pub fn link_url(&self) -> Option<&SharedString> {
        self.0.link.as_ref().map(|link| &link.url)
    }

    pub fn is_empty(&self) -> bool {
        self.0 == TextMark::default()
    }
}

/// Whether a mark covers a whole range, none of it, or part of it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MarkState {
    Off,
    On,
    Mixed,
}

/// A change to apply over a range. Builder with private fields.
#[derive(Clone, Default, PartialEq, Debug)]
pub struct MarkChange {
    set: Vec<MarkKind>,
    clear: Vec<MarkKind>,
    link: Option<LinkMark>,
    highlight: Option<Hsla>,
}

/// The highlight colour `<mark>` gets without a colour attribute.
pub(crate) fn default_highlight() -> Hsla {
    gpui::rgb(0xfef08a).into()
}

/// One of Notion's colours: its name, how it looks in a light theme (what a document stores),
/// and how it's drawn in a dark theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaletteColor {
    pub name: &'static str,
    pub light: u32,
    pub dark: u32,
}

/// Notion's background colours, in its menu's order: a highlight's fill, stored as
/// `<mark color="#…">`.
pub const BACKGROUNDS: [PaletteColor; 9] = [
    PaletteColor {
        name: "Gray",
        light: 0xf1f1ef,
        dark: 0x373737,
    },
    PaletteColor {
        name: "Brown",
        light: 0xf4eeee,
        dark: 0x4a3228,
    },
    PaletteColor {
        name: "Orange",
        light: 0xfbecdd,
        dark: 0x5c3b23,
    },
    PaletteColor {
        name: "Yellow",
        light: 0xfbf3db,
        dark: 0x564328,
    },
    PaletteColor {
        name: "Green",
        light: 0xedf3ec,
        dark: 0x243d30,
    },
    PaletteColor {
        name: "Blue",
        light: 0xe7f3f8,
        dark: 0x143a4e,
    },
    PaletteColor {
        name: "Purple",
        light: 0xf6f3f9,
        dark: 0x3c2d49,
    },
    PaletteColor {
        name: "Pink",
        light: 0xfaf1f5,
        dark: 0x4e2c3c,
    },
    PaletteColor {
        name: "Red",
        light: 0xfdebec,
        dark: 0x522e2a,
    },
];

/// `color` as `0xrrggbb`, when it's opaque.
pub(crate) fn rgb_of(color: Hsla) -> Option<u32> {
    let rgba = gpui::Rgba::from(color);
    let byte = |value: f32| (value.clamp(0., 1.) * 255.).round() as u32;
    (byte(rgba.a) == 255).then(|| byte(rgba.r) << 16 | byte(rgba.g) << 8 | byte(rgba.b))
}

/// The background colour `color` is, if it's one of [`BACKGROUNDS`].
pub fn background_of(color: Hsla) -> Option<&'static PaletteColor> {
    let rgb = rgb_of(color)?;
    BACKGROUNDS.iter().find(|b| b.light == rgb)
}

impl PaletteColor {
    /// The colour a document stores for it.
    pub fn color(&self) -> Hsla {
        gpui::rgb(self.light).into()
    }

    /// How it's drawn in a light theme, or a dark one when `dark`.
    pub fn fill(&self, dark: bool) -> Hsla {
        gpui::rgb(if dark { self.dark } else { self.light }).into()
    }
}

/// The fill a highlight in `color` is drawn with: in a dark theme a background colour's dark
/// twin, so the page doesn't glare; else `color` itself.
pub(crate) fn drawn_highlight(color: Hsla, dark: bool) -> Hsla {
    background_of(color).map_or(color, |b| b.fill(dark))
}

impl MarkChange {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(mut self, kind: MarkKind) -> Self {
        self.clear.retain(|k| *k != kind);
        self.set.push(kind);
        self
    }

    pub fn clear(mut self, kind: MarkKind) -> Self {
        self.set.retain(|k| *k != kind);
        self.clear.push(kind);
        self
    }

    pub fn with_link(mut self, url: impl Into<SharedString>, title: Option<SharedString>) -> Self {
        self.link = Some(LinkMark {
            url: url.into(),
            identifier: None,
            title,
        });
        self.set(MarkKind::Link)
    }

    pub fn with_highlight(mut self, color: Hsla) -> Self {
        self.highlight = Some(color);
        self.set(MarkKind::Highlight)
    }

    pub(crate) fn apply(&self, mark: &TextMark) -> TextMark {
        let mut out = self.apply_raw(mark);
        // Inline code holds literal text: only a link may wrap it, so other
        // marks never apply inside code, and making text code drops them.
        if out.code {
            out.bold = false;
            out.italic = false;
            out.strikethrough = false;
            out.underline = false;
            out.highlight = None;
        }
        out
    }

    fn apply_raw(&self, mark: &TextMark) -> TextMark {
        let mut out = mark.clone();
        for kind in &self.clear {
            match kind {
                MarkKind::Bold => out.bold = false,
                MarkKind::Italic => out.italic = false,
                MarkKind::Strikethrough => out.strikethrough = false,
                MarkKind::Underline => out.underline = false,
                MarkKind::Code => out.code = false,
                MarkKind::Highlight => out.highlight = None,
                MarkKind::Link => out.link = None,
            }
        }
        for kind in &self.set {
            match kind {
                MarkKind::Bold => out.bold = true,
                MarkKind::Italic => out.italic = true,
                MarkKind::Strikethrough => out.strikethrough = true,
                MarkKind::Underline => out.underline = true,
                MarkKind::Code => out.code = true,
                MarkKind::Highlight => {
                    out.highlight = Some(self.highlight.unwrap_or_else(default_highlight))
                }
                MarkKind::Link => {
                    if let Some(link) = &self.link {
                        out.link = Some(link.clone());
                    }
                }
            }
        }
        out
    }
}
