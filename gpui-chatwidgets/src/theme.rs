//! How a card looks, all of it from the host: colours, fonts, text sizes, radii and spacing.
//! Nothing the renderer draws with is a constant of its own; [`CardTheme::light`] and
//! [`CardTheme::dark`] are starting points a host changes to match itself.
//!
//! Sizes are CSS pixels, as OpenUI's own stylesheet gives them, and they're drawn as rems:
//! `rem_base` is how many CSS pixels the window's rem stands for, so a host that zooms by
//! changing its rem size zooms the cards with it.

use gpui_kit::{Hsla, Rems, rems, rgb, rgba};

/// Every colour a card draws with.
#[derive(Clone, Debug, PartialEq)]
pub struct CardColors {
    /// The frame around the card (when [`CardTheme::frame`] is on) and the ground the colours
    /// below are seen on.
    pub background: Hsla,
    /// The frame's border.
    pub frame_border: Hsla,
    /// Text.
    pub text: Hsla,
    /// Secondary text: subtitles, labels, axis labels.
    pub text_muted: Hsla,
    /// The quietest text: placeholders, a step's number.
    pub text_faint: Hsla,
    /// Lines between rows, a table's border, a separator.
    pub line: Hsla,
    /// The edge of something that can be pressed or typed in.
    pub line_strong: Hsla,
    /// A recessed area: a sunk card, the tab strip, a list's number box.
    pub sunk: Hsla,
    /// A raised area: the chosen tab, a step's circle.
    pub raised: Hsla,
    /// Every other table row, a pressable row's hover.
    pub highlight: Hsla,
    /// A raised card's fill (Card, MarkDownRenderer "card").
    pub surface: Hsla,
    /// A primary button, a chosen chip's edge, a checked box.
    pub accent: Hsla,
    /// Text on `accent`.
    pub on_accent: Hsla,
    /// A destructive primary button.
    pub destructive: Hsla,
    /// Text on `destructive`.
    pub on_destructive: Hsla,
    pub info: ToneColors,
    pub success: ToneColors,
    pub warning: ToneColors,
    pub danger: ToneColors,
    /// The modal's backdrop.
    pub overlay: Hsla,
    /// A modal's own fill.
    pub popover: Hsla,
    /// A code block's ground and its text colours.
    pub code: CodeColors,
    /// Chart series, picked from the middle out as OpenUI's default palette is.
    pub chart: Vec<Hsla>,
    /// A rising metric and a falling one.
    pub up: Hsla,
    pub down: Hsla,
}

/// A tone's text, its soft background, and its strong mark (a callout's bar).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToneColors {
    pub text: Hsla,
    pub background: Hsla,
    pub emphasis: Hsla,
}

/// A code block's colours, by kind of token.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CodeColors {
    pub background: Hsla,
    pub text: Hsla,
    pub string: Hsla,
    pub number: Hsla,
    pub comment: Hsla,
    pub keyword: Hsla,
    pub command: Hsla,
    pub flag: Hsla,
}

/// Font families by name; the host loads the fonts.
#[derive(Clone, Debug, PartialEq)]
pub struct CardFonts {
    pub body: gpui_kit::SharedString,
    pub mono: gpui_kit::SharedString,
}

/// Text sizes in CSS pixels, with their line heights as a multiple of the size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextSizes {
    /// Small print: a chart's ticks, a tag's small size.
    pub xs: f32,
    /// Small text, table cells.
    pub sm: f32,
    /// Body text.
    pub md: f32,
    /// Large text.
    pub lg: f32,
    /// A card header's title.
    pub heading: f32,
    /// Code.
    pub code: f32,
    /// Line height of body text.
    pub body_leading: f32,
    /// Line height of labels and one-line text (buttons, tabs, tags).
    pub label_leading: f32,
    /// Medium weight (500 on the web, 560 in OpenUI's variable Inter).
    pub medium: f32,
    /// Semibold (headings).
    pub semibold: f32,
}

/// Corner radii in CSS pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Radii {
    /// Tags, small buttons, the chosen tab.
    pub sm: f32,
    /// Buttons, inputs, code blocks.
    pub md: f32,
    /// The tab strip, large buttons.
    pub lg: f32,
    /// Tables, images, small cards.
    pub xl: f32,
    /// Cards.
    pub card: f32,
}

/// Spacing in CSS pixels: OpenUI's named gaps ("xs" to "2xl"), and the paddings inside things.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spacing {
    pub xs: f32,
    pub s: f32,
    pub m: f32,
    pub l: f32,
    pub xl: f32,
    pub xxl: f32,
    /// Inside the frame (the web's p-3 in its 1-pixel border).
    pub frame: f32,
    /// Inside a sunk card, a tab's content, a modal.
    pub card: f32,
    /// Inside a table cell.
    pub cell: f32,
}

/// The card's look. Build one from [`CardTheme::dark`] or [`CardTheme::light`] and change
/// what the host's own look says.
#[derive(Clone, Debug, PartialEq)]
pub struct CardTheme {
    pub colors: CardColors,
    pub fonts: CardFonts,
    pub text: TextSizes,
    pub radii: Radii,
    pub space: Spacing,
    /// CSS pixels per rem of the window: every size here is drawn as `rems(size / rem_base)`.
    /// GPUI's own rem is 16 pixels; GPUI Kit's Root sets it to its theme's font size.
    pub rem_base: f32,
    /// Draws the card in its own rounded, bordered frame, as a chat bubble (the web's
    /// UIMessage). Off, the card is drawn on whatever the host puts behind it.
    pub frame: bool,
    /// Whether the host's surroundings are dark (picks shadows and the code block's look).
    pub dark: bool,
}

fn hex(v: u32) -> Hsla {
    rgb(v).into()
}

fn hexa(v: u32) -> Hsla {
    rgba(v).into()
}

/// A colour at an opacity.
fn alpha(c: Hsla, a: f32) -> Hsla {
    Hsla { a, ..c }
}

/// OpenUI's default chart palette ("ocean"), dark to light.
const OCEAN: [u32; 11] = [
    0x0d47a1, 0x1565c0, 0x1976d2, 0x1e88e5, 0x2196f3, 0x42a5f5, 0x64b5f6, 0x90caf9, 0xbbdefb,
    0xe3f2fd, 0xeff8ff,
];

/// VS Code's Dark+ colours, which OpenUI's code block uses in either theme.
fn code_colors() -> CodeColors {
    CodeColors {
        background: hex(0x1e1e1e),
        text: hex(0xd4d4d4),
        string: hex(0xce9178),
        number: hex(0xb5cea8),
        comment: hex(0x6a9955),
        keyword: hex(0x569cd6),
        command: hex(0xdcdcaa),
        flag: hex(0x9cdcfe),
    }
}

impl CardTheme {
    /// OpenUI's look on a charcoal chat: its neutrals at their opacities over the ground, its
    /// status colours, and an ink accent (near-white buttons with dark text).
    pub fn dark() -> CardTheme {
        let ink = hex(0xfafafa);
        let line = hex(0xfdfdfd);
        CardTheme {
            colors: CardColors {
                background: hex(0x18181b),
                frame_border: hex(0x28282d),
                text: ink,
                text_muted: alpha(ink, 0.6),
                text_faint: alpha(ink, 0.35),
                line: alpha(line, 0.08),
                line_strong: alpha(line, 0.14),
                sunk: alpha(line, 0.04),
                raised: alpha(line, 0.08),
                highlight: alpha(line, 0.025),
                surface: alpha(line, 0.04),
                accent: hex(0xf0f0f4),
                on_accent: hex(0x010507),
                destructive: hex(0xdc2626),
                on_destructive: hex(0xffffff),
                info: ToneColors {
                    text: hex(0x93c5fd),
                    background: hexa(0x3b82f61f),
                    emphasis: hex(0x2563eb),
                },
                success: ToneColors {
                    text: hex(0x86efac),
                    background: hexa(0x16a34a1f),
                    emphasis: hex(0x16a34a),
                },
                warning: ToneColors {
                    text: hex(0xfde046),
                    background: hexa(0xeab30529),
                    emphasis: hex(0xca8a06),
                },
                danger: ToneColors {
                    text: hex(0xfca5a5),
                    background: hexa(0xdc26261f),
                    emphasis: hex(0xdc2626),
                },
                overlay: hexa(0x00000099),
                popover: hex(0x1f1f23),
                code: code_colors(),
                chart: OCEAN.iter().map(|c| hex(*c)).collect(),
                up: hex(0x4ade80),
                down: hex(0xf87171),
            },
            ..CardTheme::base()
        }
    }

    /// OpenUI's look on white, with an ink accent (near-black buttons with white text).
    pub fn light() -> CardTheme {
        let ink = hex(0x030303);
        CardTheme {
            colors: CardColors {
                background: hex(0xffffff),
                frame_border: hex(0xe4e4e7),
                text: ink,
                text_muted: alpha(ink, 0.6),
                text_faint: alpha(ink, 0.35),
                line: alpha(ink, 0.08),
                line_strong: alpha(ink, 0.14),
                sunk: alpha(ink, 0.04),
                raised: hex(0xffffff),
                highlight: alpha(ink, 0.025),
                surface: hex(0xfdfdfd),
                accent: hex(0x010507),
                on_accent: hex(0xffffff),
                destructive: hex(0xdc2626),
                on_destructive: hex(0xffffff),
                info: ToneColors {
                    text: hex(0x1e40af),
                    background: hexa(0x3b82f61f),
                    emphasis: hex(0x2563eb),
                },
                success: ToneColors {
                    text: hex(0x166534),
                    background: hexa(0x16a34a1f),
                    emphasis: hex(0x16a34a),
                },
                warning: ToneColors {
                    text: hex(0x854d0f),
                    background: hexa(0xeab30529),
                    emphasis: hex(0xca8a06),
                },
                danger: ToneColors {
                    text: hex(0xb91c1c),
                    background: hexa(0xdc26261f),
                    emphasis: hex(0xdc2626),
                },
                overlay: hexa(0x00000066),
                popover: hex(0xffffff),
                code: code_colors(),
                chart: OCEAN.iter().map(|c| hex(*c)).collect(),
                up: hex(0x16a34a),
                down: hex(0xdc2626),
            },
            dark: false,
            ..CardTheme::base()
        }
    }

    /// The sizes both defaults share: OpenUI's own (Inter, 16 px body text, 2 px spacing steps).
    fn base() -> CardTheme {
        CardTheme {
            colors: CardTheme::placeholder_colors(),
            fonts: CardFonts {
                body: "Inter".into(),
                mono: "Menlo".into(),
            },
            text: TextSizes {
                xs: 12.,
                sm: 14.,
                md: 16.,
                lg: 18.,
                heading: 18.,
                code: 13.,
                body_leading: 1.5,
                label_leading: 1.25,
                medium: 500.,
                semibold: 600.,
            },
            radii: Radii {
                sm: 6.,
                md: 8.,
                lg: 10.,
                xl: 14.,
                card: 16.,
            },
            space: Spacing {
                xs: 6.,
                s: 8.,
                m: 12.,
                l: 18.,
                xl: 24.,
                xxl: 36.,
                frame: 13.,
                card: 18.,
                cell: 12.,
            },
            rem_base: 16.,
            frame: true,
            dark: true,
        }
    }

    fn placeholder_colors() -> CardColors {
        let c = Hsla::default();
        let tone = ToneColors {
            text: c,
            background: c,
            emphasis: c,
        };
        CardColors {
            background: c,
            frame_border: c,
            text: c,
            text_muted: c,
            text_faint: c,
            line: c,
            line_strong: c,
            sunk: c,
            raised: c,
            highlight: c,
            surface: c,
            accent: c,
            on_accent: c,
            destructive: c,
            on_destructive: c,
            info: tone,
            success: tone,
            warning: tone,
            danger: tone,
            overlay: c,
            popover: c,
            code: code_colors(),
            chart: Vec::new(),
            up: c,
            down: c,
        }
    }

    /// A size in CSS pixels as rems of the window.
    pub fn px(&self, css: f32) -> Rems {
        rems(css / self.rem_base.max(1.))
    }

    /// `n` chart colours: one from the middle, two either side of it, more around it, as
    /// OpenUI's palette picks them; a theme with no chart colours draws with its text colour.
    pub fn series_colors(&self, n: usize) -> Vec<Hsla> {
        let p = &self.colors.chart;
        if p.is_empty() {
            return vec![self.colors.text; n];
        }
        let len = p.len() as isize;
        let mid = len / 2;
        match n {
            0 => Vec::new(),
            1 => vec![p[mid as usize]],
            2 => vec![
                p[(mid - 1).max(0) as usize],
                p[(mid + 1).min(len - 1) as usize],
            ],
            _ => {
                let offset = (n as isize) / 2;
                (0..n as isize)
                    .map(|i| p[((mid + i - offset) % len + len) as usize % len as usize])
                    .collect()
            }
        }
    }
}

impl Default for CardTheme {
    fn default() -> CardTheme {
        CardTheme::dark()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn series_colors_come_from_the_middle_out() {
        let t = CardTheme::dark();
        assert_eq!(t.series_colors(1), vec![t.colors.chart[5]]);
        assert_eq!(
            t.series_colors(2),
            vec![t.colors.chart[4], t.colors.chart[6]]
        );
        assert_eq!(
            t.series_colors(30).len(),
            30,
            "more series than colours wrap around"
        );
        let bare = CardTheme {
            colors: CardColors {
                chart: Vec::new(),
                ..t.colors.clone()
            },
            ..t.clone()
        };
        assert_eq!(bare.series_colors(2), vec![t.colors.text; 2]);
    }

    #[test]
    fn sizes_are_rems_of_the_host_s_base() {
        let t = CardTheme {
            rem_base: 15.,
            ..CardTheme::dark()
        };
        assert_eq!(t.px(15.), rems(1.));
        assert_eq!(t.px(7.5), rems(0.5));
    }
}
