use crate::{Family, Font};

/// One of the font files laid out with and drawn in: KaTeX's, renamed (`fonts/rename.py`).
/// They are licensed under the SIL Open Font License 1.1 (`fonts/OFL.txt`), which applies when
/// you redistribute them.
///
/// Each file is a family of its own, regular and upright (`name`, e.g. `GPUI Math Main
/// Italic` for KaTeX_Main-Italic): KaTeX's own files all claim to be upright and regular, so
/// a font system that matches on family, weight and style couldn't tell KaTeX_Main Regular from
/// Italic, and the OFL keeps the KaTeX_* names for KaTeX's unmodified files.
///
/// (gpui-essentials: changed from katex-native 0.1.1, which embeds every face KaTeX ships under
/// KaTeX's names.)
#[derive(Clone, Copy, Debug)]
pub struct FontFile {
    pub font: Font,
    /// The family the file declares.
    pub name: &'static str,
    pub data: &'static [u8],
}

macro_rules! font {
    ($family:ident, $bold:literal, $italic:literal, $name:literal, $file:literal) => {
        FontFile {
            font: Font {
                family: Family::$family,
                bold: $bold,
                italic: $italic,
            },
            name: $name,
            data: include_bytes!(concat!("../fonts/GPUIMath", $file, ".ttf")),
        }
    };
}

/// The faces drawn, about 0.35 MB in all. A formula in a face not here (Fraktur, Script, sans
/// serif, typewriter, bold calligraphic) is for the renderer to show another way.
pub static FONTS: &[FontFile] = &[
    font!(Ams, false, false, "GPUI Math AMS", "AMS"),
    font!(
        Caligraphic,
        false,
        false,
        "GPUI Math Caligraphic",
        "Caligraphic"
    ),
    font!(Main, false, false, "GPUI Math Main", "Main"),
    font!(Main, true, false, "GPUI Math Main Bold", "MainBold"),
    font!(Main, false, true, "GPUI Math Main Italic", "MainItalic"),
    font!(
        Main,
        true,
        true,
        "GPUI Math Main Bold Italic",
        "MainBoldItalic"
    ),
    font!(Math, false, true, "GPUI Math Math Italic", "MathItalic"),
    font!(
        Math,
        true,
        true,
        "GPUI Math Math Bold Italic",
        "MathBoldItalic"
    ),
    font!(Size1, false, false, "GPUI Math Size1", "Size1"),
    font!(Size2, false, false, "GPUI Math Size2", "Size2"),
    font!(Size3, false, false, "GPUI Math Size3", "Size3"),
    font!(Size4, false, false, "GPUI Math Size4", "Size4"),
];

/// The file a face is drawn from: its own, or (as a browser synthesizes a style KaTeX doesn't
/// ship) its family's bold or regular one. None for a family not here.
pub fn file_for(font: Font) -> Option<&'static FontFile> {
    let find = |f: Font| FONTS.iter().find(|file| file.font == f);
    find(font)
        .or_else(|| {
            find(Font {
                italic: false,
                ..font
            })
        })
        .or_else(|| {
            find(Font {
                bold: false,
                italic: false,
                ..font
            })
        })
        .or_else(|| FONTS.iter().find(|file| file.font.family == font.family))
}

#[cfg(test)]
mod tests {
    use super::FONTS;

    /// GPUI won't load a font without an "m" (it measures text with one), so every face has
    /// one (fonts/rename.py gives one to those KaTeX ships without).
    #[test]
    fn every_face_has_an_m() {
        for file in FONTS {
            assert!(
                crate::font_data::advance(file.font, 'm').is_some(),
                "{} has no m",
                file.name
            );
        }
    }
}
