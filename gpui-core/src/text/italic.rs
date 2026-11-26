//! Italics that stay italic. GPUI picks the closest face a family has, so asking a family with
//! no italic face (an app that bundles only upright Inter) for italic silently draws it
//! upright. Before text is shaped, a run asking for italic in such a family is set in the system
//! UI font's italic at the same weight instead, when that has one: a different face, but still
//! visibly italic. GPUI can't slant glyphs (no synthetic oblique), so a host that wants its own
//! family's italics bundles them.
//!
//! In a browser there's no system font to stand in: GPUI's web platform has only the fonts the
//! page gives it, and it panics on a family it can't find. There, such a run stays upright.

use gpui::{Font, FontStyle, TextRun, TextSystem};

/// The family whose italic stands in for a family without one.
pub const ITALIC_FALLBACK_FAMILY: &str = ".SystemUIFont";

/// Whether `font`'s family has a face for its italic, other than its upright one.
fn has_italic(font: &Font, ts: &TextSystem) -> bool {
    let mut upright = font.clone();
    upright.style = FontStyle::Normal;
    ts.resolve_font(font) != ts.resolve_font(&upright)
}

/// `font`, or the fallback family's italic at its weight when `font` asks for italic its family
/// doesn't have. Anything not italic is returned as it is.
pub fn italic_font(font: Font, ts: &TextSystem) -> Font {
    if font.style != FontStyle::Italic || cfg!(target_family = "wasm") || has_italic(&font, ts) {
        return font;
    }
    let mut fallback = font.clone();
    fallback.family = ITALIC_FALLBACK_FAMILY.into();
    fallback.features = Default::default();
    fallback.fallbacks = None;
    if has_italic(&fallback, ts) {
        fallback
    } else {
        // Nothing here has an italic: it draws upright, as it would have.
        font
    }
}

/// [`italic_font`] for each run.
pub fn italic_runs(runs: &mut [TextRun], ts: &TextSystem) {
    for run in runs {
        if run.font.style == FontStyle::Italic {
            run.font = italic_font(run.font.clone(), ts);
        }
    }
}
