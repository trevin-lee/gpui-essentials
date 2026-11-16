# Drawing the document without GPUI Kit

The engine draws each top-level block through GPUI Kit's Markdown renderer (`BlockNode::render`
over a projection built in `direct.rs`), and takes each text leaf's geometry for the caret and hit
testing from GPUI Kit's `collect_leaf_geometry` (`frame.rs`). This is the plan for drawing with
gpui-core's paragraph layer (`gpui_core::text::paragraph`) instead, a block kind at a time, with
the editor working after every step.

## How it goes

1. **Geometry of its own** (done). `frame.rs` keeps each leaf's geometry as either GPUI Kit's
   `LeafGeometry` or the engine's own (a `ParagraphLayout` and where it was drawn). The caret,
   hit testing, selection and the table grips read either.
2. **Blocks drawn by the engine**, top-level kind by kind. A top-level block is drawn by the
   engine when everything in it can be (`draw::tree`); otherwise the whole block still goes
   through GPUI Kit. A block the engine draws reports its
   leaves' geometry itself; the rest still go through GPUI Kit, and both kinds of geometry make
   one frame. In order:
   - paragraphs and headings without pictures (done);
   - dividers, custom blocks (extensions) and source atoms (done);
   - quotes, callouts and toggles (with `surfaces.rs`' containers) (done);
   - lists, with their markers and task boxes (done);
   - tables, with the grips (done);
   - pictures, inline and alone (through the host's image loader);
   - code blocks, once syntax highlighting has a home outside GPUI Kit.
3. **The projection goes.** When nothing is drawn through GPUI Kit, `direct.rs`' projection to
   `BlockNode`, the projection cache and `collect_leaf_geometry` go with it.

## What GPUI Kit draws today, to match

Sizes are relative to the body font size (the editor's inherited text size).

| | |
|---|---|
| Paragraph gap (below a block, unless last or in a list) | 0.75 rem |
| Heading sizes, levels 1 to 6 | 1.8, 4/3, 17/15, 1, 14/15, 13/15 × body |
| Heading line heights | 1.25 for level 1, else 1.35 |
| Heading weight | semibold; level 6 in the muted colour |
| Gap above a heading | its size × section gap (1.1 for level 1, 1.6 for level 2, else 1.4), less the gap the previous block leaves (a rule leaves 1.6 × body); 0.25 × size after another heading; none when first or nested |
| Gap below a heading | 0.35 × its size, unless last |
| Bold, italic | weight bold, italic style |
| Strike, underline | 1 px lines |
| Link | the link colour, a 1 px underline at 40% opacity |
| Inline code | mono family, 0.875 × size, the inline code background and style |
| Highlight (`<mark>`) | its colour behind, the text in a readable colour on it |
| Quote | muted text, a 2 px left border, padding 0.9 × body |
| Table | 1 px rules and frame on the surface colour, the header row on the code background; cells padded 12 px sides and 7 px top and bottom; a column is as wide as its widest line (at least 48 px), and when the table is short of room its text wraps down to 4 rem; room left over goes beside each cell by its alignment |
| Rule | 1.6 × body above and below |
| Code block | mono, line height 1.6, padding 16 px sides, 14 px top (28 with a language label) and bottom, the code background and a border |

The engine's rows are its line height exactly. GPUI Kit's come out up to one device pixel taller
(49 device pixels for a 24 px line at 2x), so a page the engine draws is a little shorter; the
caret and hit testing follow what's drawn either way.
