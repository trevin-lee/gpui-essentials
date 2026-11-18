"""Makes the font files here from KaTeX's: the faces gpui-core draws math with, each renamed.

Usage: python3 -I rename.py <katex-fonts-dir> <out-dir>

`katex-fonts-dir` is KaTeX's own fonts (katex-native 0.1.1's `fonts/`, KaTeX 0.18.5's TTFs).
Needs fontTools (`pip install fonttools`).

Why rename: every KaTeX file declares itself upright and regular weight (bold ones weight 700),
so a font system that matches on family, weight and style (GPUI's) can't tell KaTeX_Main
Regular from Italic. Each face here is a family of its own, regular and upright, named for the
face: "GPUI Math Main Italic" is KaTeX_Main-Italic. The fonts are under the SIL Open Font
License 1.1 (OFL.txt), which reserves the KaTeX_* family names for KaTeX's own files, so a
modified file takes another name: none of these is called KaTeX. Nothing else in a file
changes: glyphs and metrics are KaTeX's, and the cmap is too but for an "m" in the faces that
have none (below).
"""
import sys
from pathlib import Path

from fontTools.ttLib import TTFont

# The faces drawn (the others' formulas show their TeX): KaTeX file, then the new family.
FACES = {
    "KaTeX_AMS-Regular": "GPUI Math AMS",
    "KaTeX_Caligraphic-Regular": "GPUI Math Caligraphic",
    "KaTeX_Main-Regular": "GPUI Math Main",
    "KaTeX_Main-Bold": "GPUI Math Main Bold",
    "KaTeX_Main-Italic": "GPUI Math Main Italic",
    "KaTeX_Main-BoldItalic": "GPUI Math Main Bold Italic",
    "KaTeX_Math-Italic": "GPUI Math Math Italic",
    "KaTeX_Math-BoldItalic": "GPUI Math Math Bold Italic",
    "KaTeX_Size1-Regular": "GPUI Math Size1",
    "KaTeX_Size2-Regular": "GPUI Math Size2",
    "KaTeX_Size3-Regular": "GPUI Math Size3",
    "KaTeX_Size4-Regular": "GPUI Math Size4",
}


def main():
    src, out = Path(sys.argv[1]), Path(sys.argv[2])
    out.mkdir(parents=True, exist_ok=True)
    for file, family in FACES.items():
        font = TTFont(src / f"{file}.ttf")
        postscript = family.replace(" ", "")
        name = font["name"]
        # Family, subfamily, full name, PostScript name; no typographic family or subfamily.
        for record in list(name.names):
            if record.nameID in (16, 17, 21, 22):
                name.removeNames(nameID=record.nameID)
        for name_id, value in ((1, family), (2, "Regular"), (3, f"{postscript};GPUI"), (4, family), (6, postscript)):
            name.setName(value, name_id, 3, 1, 0x409)
            name.setName(value, name_id, 1, 0, 0)
        # GPUI measures text with a font's "m" and won't load a font without one; KaTeX's AMS,
        # Caligraphic and Size faces (symbols, capitals, delimiters) have none. Such a face gets
        # an "m" that's one of its own glyphs, which math never draws.
        best = font.getBestCmap()
        if ord("m") not in best:
            stand_in = next(name for code, name in sorted(best.items()) if name != ".notdef")
            for table in font["cmap"].tables:
                if table.isUnicode():
                    table.cmap[ord("m")] = stand_in
            print(f"  gave it an m ({stand_in})")
        # Regular and upright, so its family has the one face.
        os2 = font["OS/2"]
        os2.usWeightClass = 400
        os2.fsSelection = (os2.fsSelection & ~0b1100001) | 0b1000000
        font["head"].macStyle = 0
        font.save(out / f"{postscript}.ttf")
        print(f"{file}.ttf -> {postscript}.ttf ({family})")


if __name__ == "__main__":
    main()
