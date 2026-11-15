//! The calendar's colours. By default they come from the GPUI Kit theme, so the calendar looks
//! like the GPUI Kit app around it, and events take their calendar's colour from the host or from a
//! palette chosen for the charcoal theme.

use gpui_kit::component::Theme;
use gpui_kit::{App, Hsla, rgb};

/// Every colour the calendar draws with, other than events'.
#[derive(Clone, Copy, Debug)]
pub struct CalendarColors {
    pub background: Hsla,
    pub text: Hsla,
    pub muted_text: Hsla,
    /// Hour lines and the lines between days.
    pub line: Hsla,
    /// The half-hour lines, and other quiet lines.
    pub faint_line: Hsla,
    /// Saturday and Sunday, and days outside the month in the month view.
    pub off_day: Hsla,
    /// Today's date mark and the line at the current time.
    pub now: Hsla,
    /// Text on the `now` colour.
    pub on_now: Hsla,
    /// The outline of the selected slot.
    pub selection: Hsla,
    /// A day being dropped on in the month view, and the hour the clocks skip.
    pub highlight: Hsla,
}

impl CalendarColors {
    /// The calendar's colours for the current GPUI Kit theme.
    pub fn from_theme(cx: &App) -> Self {
        let c = &Theme::global(cx).colors;
        Self {
            background: c.background,
            text: c.foreground,
            muted_text: c.muted_foreground,
            line: c.border,
            faint_line: c.border.opacity(0.45),
            off_day: c.foreground.opacity(0.018),
            now: rgb(0xe5665b).into(),
            on_now: rgb(0xffffff).into(),
            selection: c.foreground.opacity(0.55),
            highlight: c.foreground.opacity(0.06),
        }
    }
}

/// Calendar colours for a calendar the host gave none: muted enough to sit on charcoal, far
/// enough apart to tell at a glance.
pub const PALETTE: [u32; 8] = [
    0x5b8def, // blue
    0x4cc38a, // green
    0xe2a336, // amber
    0x9b7cf0, // violet
    0x3fb6c4, // teal
    0xe06fa8, // pink
    0xe5665b, // red
    0x8b8b93, // grey
];

/// A palette colour for a calendar id, the same every time for the same id.
pub fn palette_color(calendar: &str) -> u32 {
    // FNV-1a: tiny, stable across runs and platforms (unlike the standard library's hasher).
    let mut h: u32 = 0x811c9dc5;
    for b in calendar.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x01000193);
    }
    PALETTE[(h as usize) % PALETTE.len()]
}

/// The colours an event chip is drawn with, from its calendar's colour.
#[derive(Clone, Copy, Debug)]
pub struct ChipColors {
    /// The calendar's colour itself: the bar at the chip's edge and a month row's dot.
    pub accent: Hsla,
    /// The chip's fill: the colour laid over the background, opaque so overlapping chips
    /// don't show through each other.
    pub fill: Hsla,
    pub text: Hsla,
    pub muted_text: Hsla,
}

/// `top` over `bottom` at `alpha`, as one opaque colour.
pub fn mix(bottom: Hsla, top: Hsla, alpha: f32) -> Hsla {
    let (a, b) = (bottom.to_rgb(), top.to_rgb());
    let m = |x: f32, y: f32| x + (y - x) * alpha;
    gpui_kit::Rgba {
        r: m(a.r, b.r),
        g: m(a.g, b.g),
        b: m(a.b, b.b),
        a: 1.,
    }
    .into()
}

impl ChipColors {
    pub fn new(rgb_color: u32, colors: &CalendarColors, selected: bool, past: bool) -> Self {
        let accent: Hsla = rgb(rgb_color).into();
        if selected {
            // Solid, with dark text: the selected event is the one thing that stands out.
            let text: Hsla = rgb(0x0f0f11).into();
            return Self {
                accent,
                fill: accent,
                text,
                muted_text: text.opacity(0.72),
            };
        }
        let strength = if past { 0.13 } else { 0.24 };
        let fill = mix(colors.background, accent, strength);
        let text = if past {
            colors.text.opacity(0.55)
        } else {
            colors.text
        };
        let muted_text = mix(colors.muted_text, accent, 0.35).opacity(if past { 0.6 } else { 1. });
        Self {
            accent: if past { accent.opacity(0.55) } else { accent },
            fill,
            text,
            muted_text,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palette_colors_are_stable() {
        assert_eq!(palette_color("work"), palette_color("work"));
        // Not a promise about which colour, only that ids spread over the palette.
        let picks: std::collections::HashSet<u32> =
            ["work", "home", "school", "health", "agents", "travel"]
                .iter()
                .map(|c| palette_color(c))
                .collect();
        assert!(picks.len() >= 3);
    }
}
