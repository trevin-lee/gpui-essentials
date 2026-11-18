//! How a table looks, all of it from the host: colours, fonts, text sizes and the sizes of rows,
//! headers and cells. [`TableTheme::dark`], [`TableTheme::light`] and [`TableTheme::from_core`]
//! are starting points a host changes to match itself.
//!
//! Sizes are points at 100% zoom, drawn as rems: `rem_base` is how many points the window's rem
//! stands for (gpui-core's Root makes the rem its theme's font size, 15 by default), so a host
//! that zooms by changing its rem zooms the table with it. Touch targets ([`TableTheme::min_hit`])
//! are window points and don't zoom.

use gpui::{Hsla, Pixels, Rems, SharedString, px, rems, rgb, rgba};

use crate::model::Color;

/// An option's chip: its fill and its text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChipColors {
    pub background: Hsla,
    pub text: Hsla,
}

/// What each of the ten option colours looks like.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OptionColors {
    pub default: ChipColors,
    pub gray: ChipColors,
    pub brown: ChipColors,
    pub orange: ChipColors,
    pub yellow: ChipColors,
    pub green: ChipColors,
    pub blue: ChipColors,
    pub purple: ChipColors,
    pub pink: ChipColors,
    pub red: ChipColors,
}

impl OptionColors {
    /// The chip for an option colour; one this version doesn't know draws as the default.
    pub fn get(&self, color: &Color) -> ChipColors {
        match color {
            Color::Gray => self.gray,
            Color::Brown => self.brown,
            Color::Orange => self.orange,
            Color::Yellow => self.yellow,
            Color::Green => self.green,
            Color::Blue => self.blue,
            Color::Purple => self.purple,
            Color::Pink => self.pink,
            Color::Red => self.red,
            Color::Default | Color::Other(_) => self.default,
        }
    }
}

/// Every colour the table draws with.
#[derive(Clone, Debug, PartialEq)]
pub struct TableColors {
    /// Behind the rows.
    pub background: Hsla,
    /// Cell text.
    pub text: Hsla,
    /// Header text, counts, icons, read-only times.
    pub text_muted: Hsla,
    /// Placeholders: an empty title, "New".
    pub text_faint: Hsla,
    /// Lines between rows and columns.
    pub line: Hsla,
    /// The header's lower edge, a column being resized, a card that's lifted or hovered.
    pub line_strong: Hsla,
    /// A control's boundary, which shows it's there: an unticked checkbox's outline and the
    /// open button's edge, at least 3:1 against the background (WCAG 1.4.11).
    pub control: Hsla,
    /// Behind the header row.
    pub header: Hsla,
    /// Behind a group's header.
    pub group: Hsla,
    /// A row under the pointer, a highlighted menu item.
    pub hover: Hsla,
    /// The focused cell's edge.
    pub focus: Hsla,
    /// The focused cell's fill.
    pub selection: Hsla,
    /// A checked box, a link, where a dragged column will land.
    pub accent: Hsla,
    /// The check on `accent`.
    pub on_accent: Hsla,
    /// A menu's or picker's fill.
    pub popover: Hsla,
    /// A menu's or picker's edge.
    pub popover_border: Hsla,
    /// A shadow under menus (and under a board's card while it's dragged).
    pub shadow: Hsla,
    /// A board's cards.
    pub card: Hsla,
    /// The ten option colours.
    pub options: OptionColors,
}

/// Font families by name; the host loads the fonts.
#[derive(Clone, Debug, PartialEq)]
pub struct TableFonts {
    pub body: SharedString,
    /// Numbers can be set in it; the default theme uses the body font with tabular figures
    /// where the platform has them.
    pub mono: SharedString,
}

/// Sizes in points at 100% zoom.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TableSizes {
    /// Cell text.
    pub text: f32,
    /// Header text, chips, counts.
    pub small: f32,
    /// A row's height (and a group header's, and the New row's).
    pub row_height: f32,
    /// The header row's height.
    pub header_height: f32,
    /// Inside a cell, left and right.
    pub cell_padding: f32,
    /// A column without a width.
    pub column_width: f32,
    /// The title column without a width.
    pub title_width: f32,
    /// The narrowest a column can be dragged.
    pub min_column_width: f32,
    /// The widest a column can be dragged.
    pub max_column_width: f32,
    /// Chips' corners.
    pub chip_radius: f32,
    /// Menus' corners.
    pub radius: f32,
    /// Icons in headers and buttons.
    pub icon: f32,
    /// A menu's or picker's width.
    pub menu_width: f32,
    /// Medium weight, for headers and group names.
    pub medium: f32,
    /// A board's column, cards and all.
    pub board_column_width: f32,
}

/// The table's look. Build one from [`TableTheme::dark`], [`TableTheme::light`] or
/// [`TableTheme::from_core`] and change what the host's own look says.
#[derive(Clone, Debug, PartialEq)]
pub struct TableTheme {
    pub colors: TableColors,
    pub fonts: TableFonts,
    pub sizes: TableSizes,
    /// Points per rem of the window: every size is drawn as `rems(size / rem_base)`. gpui-core's
    /// Root sets the rem to its theme's font size (15 by default).
    pub rem_base: f32,
    /// The smallest target anything pressable takes presses in, in window points (it doesn't
    /// zoom): rows, headers, the Open button, checkboxes, menu items and resize handles. 44 on
    /// iOS, Apple's minimum; 0 elsewhere.
    pub min_hit: f32,
    /// Whether people use the table with a finger: the Open button shows on every row (there's
    /// no hover), and column edges take a drag from a finger.
    pub touch: bool,
    /// Whether the surroundings are dark (picks the shadow's strength).
    pub dark: bool,
}

fn hex(v: u32) -> Hsla {
    rgb(v).into()
}

fn hexa(v: u32) -> Hsla {
    rgba(v).into()
}

fn chip(bg: u32, text: u32) -> ChipColors {
    ChipColors {
        background: hexa(bg),
        text: hex(text),
    }
}

impl TableTheme {
    /// Apple's minimum touch target, in points per side.
    pub const TOUCH_HIT: f32 = 44.;

    /// The theme for touch (44-point targets, the Open button always shown) or for a mouse and
    /// keyboard, whatever the platform.
    pub fn with_touch(self, touch: bool) -> TableTheme {
        TableTheme {
            min_hit: if touch { TableTheme::TOUCH_HIT } else { 0. },
            touch,
            ..self
        }
    }

    /// A charcoal look, matching gpui-core's dark theme.
    pub fn dark() -> TableTheme {
        let ink = hex(0xededef);
        TableTheme {
            colors: TableColors {
                background: hex(0x0f0f11),
                text: ink,
                text_muted: hex(0x8b8b93),
                text_faint: hex(0x5c5c64),
                line: hex(0x222226),
                line_strong: hex(0x34343a),
                control: hex(0x656571),
                header: hex(0x0f0f11),
                group: hex(0x141417),
                hover: hexa(0xffffff0a),
                focus: hex(0x5b8def),
                selection: hexa(0x5b8def1f),
                accent: hex(0x5b8def),
                on_accent: hex(0xffffff),
                popover: hex(0x1b1b1f),
                popover_border: hex(0x2e2e34),
                shadow: hexa(0x00000080),
                card: hex(0x1b1b1f),
                options: OptionColors {
                    default: chip(0xffffff1a, 0xdcdce0),
                    gray: chip(0x8b8b9340, 0xd0d0d6),
                    brown: chip(0x8a5a3c55, 0xe6c3ad),
                    orange: chip(0xd9731a4d, 0xf3bf8f),
                    yellow: chip(0xc9a2274d, 0xecd68c),
                    green: chip(0x2f9e6a4d, 0x9fdcbc),
                    blue: chip(0x2f6fd64d, 0xa9c6f5),
                    purple: chip(0x8a55d14d, 0xcdb5f2),
                    pink: chip(0xc94f8d4d, 0xf0b3d2),
                    red: chip(0xd2453d4d, 0xf3aaa5),
                },
            },
            ..TableTheme::base()
        }
    }

    /// The same on white.
    pub fn light() -> TableTheme {
        let ink = hex(0x1d1d1f);
        TableTheme {
            colors: TableColors {
                background: hex(0xffffff),
                text: ink,
                text_muted: hex(0x6b6b73),
                text_faint: hex(0xa3a3aa),
                line: hex(0xececef),
                line_strong: hex(0xdcdce0),
                control: hex(0x8f8f9e),
                header: hex(0xffffff),
                group: hex(0xf7f7f8),
                hover: hexa(0x0000000a),
                focus: hex(0x2f6fd6),
                selection: hexa(0x2f6fd614),
                accent: hex(0x2f6fd6),
                on_accent: hex(0xffffff),
                popover: hex(0xffffff),
                popover_border: hex(0xdcdce0),
                shadow: hexa(0x00000026),
                card: hex(0xffffff),
                options: OptionColors {
                    default: chip(0x0000000f, 0x3a3a40),
                    gray: chip(0xe3e2e0ff, 0x32302c),
                    brown: chip(0xeee0daff, 0x442a1e),
                    orange: chip(0xfadec9ff, 0x49290e),
                    yellow: chip(0xfbf3dbff, 0x402c1b),
                    green: chip(0xdbeddbff, 0x1c3829),
                    blue: chip(0xd3e5efff, 0x183347),
                    purple: chip(0xe8deeeff, 0x412454),
                    pink: chip(0xf5e0e9ff, 0x4c2337),
                    red: chip(0xffe2ddff, 0x5d1715),
                },
            },
            dark: false,
            ..TableTheme::base()
        }
    }

    /// The table in a gpui-core theme's colours and fonts: its lines, muted text, popover and
    /// ring, with the option colours of [`TableTheme::dark`] or [`TableTheme::light`] by mode,
    /// and the rem base at its font size.
    pub fn from_core(core: &gpui_core::Theme) -> TableTheme {
        let start = if core.mode.is_dark() {
            TableTheme::dark()
        } else {
            TableTheme::light()
        };
        TableTheme {
            colors: TableColors {
                background: core.background,
                text: core.foreground,
                text_muted: core.muted_foreground,
                text_faint: core.muted_foreground.opacity(0.65),
                line: core.border,
                // gpui-core's quiet control colour for lines, its boundary colour (3:1) for
                // controls' edges.
                line_strong: core.control,
                control: core.input,
                header: core.background,
                group: core.muted.opacity(0.5),
                hover: core.accent,
                popover: core.popover,
                popover_border: core.border,
                card: if core.mode.is_dark() {
                    core.popover
                } else {
                    core.card
                },
                ..start.colors
            },
            fonts: TableFonts {
                body: core.font_family.clone(),
                mono: core.mono_font_family.clone(),
            },
            rem_base: f32::from(core.font_size).max(1.),
            ..start
        }
    }

    /// The sizes every default shares: 13-point text in 34-point rows.
    fn base() -> TableTheme {
        let touch = cfg!(any(target_os = "ios", target_os = "android"));
        TableTheme {
            colors: TableTheme::dark_colors_placeholder(),
            fonts: TableFonts {
                body: ".SystemUIFont".into(),
                mono: "Menlo".into(),
            },
            sizes: TableSizes {
                text: 13.,
                small: 12.,
                row_height: 34.,
                header_height: 34.,
                cell_padding: 8.,
                column_width: 180.,
                title_width: 260.,
                min_column_width: 56.,
                max_column_width: 1200.,
                chip_radius: 4.,
                radius: 8.,
                icon: 14.,
                menu_width: 240.,
                medium: 500.,
                board_column_width: 264.,
            },
            rem_base: 15.,
            min_hit: if touch { TableTheme::TOUCH_HIT } else { 0. },
            touch,
            dark: true,
        }
    }

    fn dark_colors_placeholder() -> TableColors {
        let c = Hsla::default();
        let ch = ChipColors {
            background: c,
            text: c,
        };
        TableColors {
            background: c,
            text: c,
            text_muted: c,
            text_faint: c,
            line: c,
            line_strong: c,
            control: c,
            header: c,
            group: c,
            hover: c,
            focus: c,
            selection: c,
            accent: c,
            on_accent: c,
            popover: c,
            popover_border: c,
            shadow: c,
            card: c,
            options: OptionColors {
                default: ch,
                gray: ch,
                brown: ch,
                orange: ch,
                yellow: ch,
                green: ch,
                blue: ch,
                purple: ch,
                pink: ch,
                red: ch,
            },
        }
    }

    /// A size in points as rems of the window.
    pub fn px(&self, points: f32) -> Rems {
        rems(points / self.rem_base.max(1.))
    }

    /// A size in points as the window's pixels, at the window's rem.
    pub fn pixels(&self, points: f32, rem: Pixels) -> Pixels {
        px(points / self.rem_base.max(1.) * f32::from(rem))
    }

    /// A pressable target's least size, in window pixels: `size` points, or the touch minimum.
    pub fn hit(&self, points: f32, rem: Pixels) -> Pixels {
        self.pixels(points, rem).max(px(self.min_hit))
    }
}

impl Default for TableTheme {
    fn default() -> Self {
        Self::dark()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touch_and_rems() {
        let t = TableTheme::dark().with_touch(true);
        assert_eq!((t.min_hit, t.touch), (44., true));
        let m = TableTheme::light().with_touch(false);
        assert_eq!((m.min_hit, m.touch), (0., false));
        assert_eq!(t.px(30.), rems(2.));
        assert_eq!(t.pixels(30., px(30.)), px(60.));
        assert_eq!(t.hit(20., px(15.)), px(44.));
        assert_eq!(m.hit(20., px(15.)), px(20.));
        let core = TableTheme::from_core(&gpui_core::Theme::light());
        assert!(!core.dark);
        assert_eq!(core.rem_base, 15.);
        assert_eq!(
            TableTheme::dark()
                .colors
                .options
                .get(&Color::Other("x".into())),
            TableTheme::dark().colors.options.default
        );
    }
}
