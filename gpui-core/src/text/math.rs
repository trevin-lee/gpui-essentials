//! Math typeset as KaTeX typesets it (the `math` feature): katex-rs reads the TeX and builds
//! KaTeX's boxes, the vendored katex-native lays them out as a display list in ems, and this
//! draws that list with GPUI's text (in KaTeX's fonts, renamed per face) and paths.
//!
//! A formula that can't be typeset shows its TeX instead (the TextView's step before this):
//! one katex-rs doesn't read, one in a face that isn't shipped, one longer than
//! [`MAX_TEX`], and one whose layout took longer than [`MAX_TIME`] (remembered, so it isn't
//! tried again each frame).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    App, Bounds, Font, FontFallbacks, FontFeatures, FontStyle, FontWeight, Hsla, PathBuilder,
    Pixels, Point, SharedString, TextAlign, TextRun, Window, fill, point, px, size,
};
use katex_native::{Item, Layout, PathCommand, katex};

/// The most TeX a formula is typeset from, in bytes: past it, its TeX shows.
pub const MAX_TEX: usize = 4 * 1024;
/// The longest a formula's layout may take: past it, its TeX shows, now and later.
pub const MAX_TIME: Duration = Duration::from_millis(50);
/// How many formulas' layouts are kept (the cache is emptied when it's full).
const KEPT: usize = 512;

/// A typeset formula, in ems of the size it's drawn at.
#[derive(Debug)]
pub struct Formula {
    layout: Layout,
}

impl Formula {
    /// Its width, its extent above the baseline and below it, at `size` pixels an em.
    pub fn width(&self, size: Pixels) -> Pixels {
        size * self.layout.width as f32
    }

    pub fn ascent(&self, size: Pixels) -> Pixels {
        size * self.layout.height as f32
    }

    pub fn descent(&self, size: Pixels) -> Pixels {
        size * self.layout.depth as f32
    }

    /// Draws it with its baseline's left end at `origin`, `em` pixels an em, in `color`
    /// (where KaTeX gives no colour of its own).
    pub fn paint(
        &self,
        origin: Point<Pixels>,
        em: Pixels,
        color: Hsla,
        window: &mut Window,
        cx: &mut App,
    ) {
        let at = |x: f64, y: f64| point(origin.x + em * x as f32, origin.y + em * y as f32);
        let colour = |c: &Option<katex_native::Color>| c.as_ref().map_or(color, to_hsla);
        for item in &self.layout.items {
            match item {
                Item::Text {
                    x,
                    y,
                    text,
                    font,
                    size: ems,
                    color: c,
                } => {
                    let Some(file) = katex_native::file_for(*font) else {
                        continue;
                    };
                    let font_size = em * *ems as f32;
                    let run = TextRun {
                        len: text.len(),
                        font: Font {
                            family: file.name.into(),
                            features: FontFeatures::default(),
                            fallbacks: None::<FontFallbacks>,
                            weight: FontWeight::NORMAL,
                            style: FontStyle::Normal,
                        },
                        color: colour(c),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    let line = window.text_system().shape_line(
                        SharedString::from(text.clone()),
                        font_size,
                        &[run],
                        None,
                    );
                    let baseline = at(*x, *y);
                    let _ = line.paint(
                        point(baseline.x, baseline.y - line.ascent),
                        line.ascent + line.descent,
                        TextAlign::Left,
                        None,
                        window,
                        cx,
                    );
                }
                Item::Rect {
                    x,
                    y,
                    width,
                    height,
                    color: c,
                } => {
                    let o = at(*x, *y);
                    // A rule is at least a hairline, as a browser draws one.
                    let h = (em * *height as f32).max(px(1.) / window.scale_factor());
                    window.paint_quad(fill(Bounds::new(o, size(em * *width as f32, h)), colour(c)));
                }
                Item::Path {
                    commands,
                    clip,
                    color: c,
                } => {
                    let mut b = PathBuilder::fill();
                    for cmd in commands {
                        match *cmd {
                            PathCommand::MoveTo(x, y) => b.move_to(at(x, y)),
                            PathCommand::LineTo(x, y) => b.line_to(at(x, y)),
                            PathCommand::CubicTo(x1, y1, x2, y2, x, y) => {
                                b.cubic_bezier_to(at(x, y), at(x1, y1), at(x2, y2))
                            }
                            PathCommand::Close => b.close(),
                        }
                    }
                    let Ok(path) = b.build() else { continue };
                    let mask = gpui::ContentMask {
                        bounds: Bounds::new(
                            at(clip.x, clip.y),
                            size(em * clip.width as f32, em * clip.height as f32),
                        ),
                    };
                    let c = colour(c);
                    window.with_content_mask(Some(mask), |window| window.paint_path(path, c));
                }
                Item::Line {
                    from,
                    to,
                    thickness,
                    color: c,
                } => {
                    let (a, z) = (at(from.0, from.1), at(to.0, to.1));
                    let half = f32::from((em * *thickness as f32 / 2.).max(px(0.5)));
                    let (dx, dy) = (f32::from(z.x - a.x), f32::from(z.y - a.y));
                    let len = (dx * dx + dy * dy).sqrt().max(0.001);
                    // The stroke as the quadrilateral it covers.
                    let n = point(px(-dy * half / len), px(dx * half / len));
                    let mut b = PathBuilder::fill();
                    b.move_to(a + n);
                    b.line_to(z + n);
                    b.line_to(z - n);
                    b.line_to(a - n);
                    b.close();
                    if let Ok(path) = b.build() {
                        window.paint_path(path, colour(c));
                    }
                }
                _ => {}
            }
        }
    }
}

fn to_hsla(c: &katex_native::Color) -> Hsla {
    gpui::Rgba {
        r: c.r as f32 / 255.,
        g: c.g as f32 / 255.,
        b: c.b as f32 / 255.,
        a: c.a as f32 / 255.,
    }
    .into()
}

/// Formulas by their TeX and whether they're display ones: typeset, or None to show their TeX.
type Cache = HashMap<(String, bool), Option<Rc<Formula>>>;

thread_local! {
    static CONTEXT: katex::KatexContext = katex::KatexContext::default();
    static CACHE: RefCell<Cache> = RefCell::new(HashMap::new());
    /// The time cap, which a test lowers.
    static CAP: std::cell::Cell<Duration> = const { std::cell::Cell::new(MAX_TIME) };
}

/// `tex` typeset (`display`: as a display equation, else inline), or None when it's to show as
/// its TeX. Each formula is laid out once and kept.
pub fn typeset(tex: &str, display: bool) -> Option<Rc<Formula>> {
    if tex.len() > MAX_TEX || tex.trim().is_empty() {
        return None;
    }
    let key = (tex.to_string(), display);
    if let Some(hit) = CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return hit;
    }
    // The context's tables are built once, outside the time the layout is held to.
    let (laid, took) = CONTEXT.with(|ctx| {
        let settings = katex::Settings::builder()
            .display_mode(display)
            // Macros can't expand without end, and nothing's sized past what a chat shows.
            .max_expand(1000)
            .max_size(20.)
            .build();
        let start = Instant::now();
        let laid = katex_native::layout_with(ctx, tex, &settings).ok();
        (laid, start.elapsed())
    });
    let formula = laid
        .filter(|_| took <= CAP.with(|c| c.get()))
        .filter(drawable)
        .map(|layout| Rc::new(Formula { layout }));
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() >= KEPT {
            c.clear();
        }
        c.insert(key, formula.clone());
    });
    formula
}

/// Whether every face it's set in is one that's shipped.
fn drawable(l: &Layout) -> bool {
    l.items.iter().all(|i| match i {
        Item::Text { font, .. } => {
            katex_native::file_for(*font).is_some_and(|f| f.font.family == font.family)
        }
        _ => true,
    }) && l.tag.is_none()
}

/// KaTeX's faces, for the window's text system: each a family of its own.
pub fn fonts() -> Vec<std::borrow::Cow<'static, [u8]>> {
    katex_native::FONTS
        .iter()
        .map(|f| std::borrow::Cow::Borrowed(f.data))
        .collect()
}

#[cfg(test)]
pub(crate) fn set_time_cap(cap: Duration) {
    CAP.with(|c| c.set(cap));
    CACHE.with(|c| c.borrow_mut().clear());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trevins_cases_are_typeset() {
        for (tex, display) in [
            (r"x_{k+1} = F x_k + w", false),
            (r"J_{ij}", false),
            (r"2n+1", false),
            (r"m \times n", false),
            (r"\frac{\partial f_i}{\partial x_j}", true),
            (r"a \qquad b", true),
            (r"f(x + \Delta x) \approx f(x) + J\,\Delta x", true),
            (
                r"f(v,\theta) = \begin{pmatrix} v\cos\theta \\ v\sin\theta \end{pmatrix} \quad\Rightarrow\quad J = \begin{pmatrix} \cos\theta & -v\sin\theta \\ \sin\theta & v\cos\theta \end{pmatrix}",
                true,
            ),
        ] {
            let f = typeset(tex, display).unwrap_or_else(|| panic!("{tex}"));
            assert!(f.width(px(16.)) > px(0.), "{tex}");
        }
    }

    #[test]
    fn what_cant_be_typeset_shows_its_tex() {
        // Unknown to KaTeX, in a face that isn't shipped, empty.
        assert!(typeset(r"\unknowncommand{x}", true).is_none());
        assert!(typeset(r"\mathfrak{g}", false).is_none());
        assert!(typeset("  ", false).is_none());
    }

    #[test]
    fn a_formula_past_the_size_cap_shows_its_tex_without_being_read() {
        let long = "x + ".repeat(MAX_TEX / 4 + 1);
        let start = Instant::now();
        assert!(typeset(&long, true).is_none());
        assert!(start.elapsed() < Duration::from_millis(5), "it isn't read");
        assert!(
            typeset(&"x + ".repeat(100), true).is_some(),
            "under it, typeset"
        );
    }

    #[test]
    fn a_formula_past_the_time_cap_shows_its_tex_and_stays_so() {
        set_time_cap(Duration::ZERO);
        assert!(typeset(r"\frac{a}{b}", true).is_none());
        // Remembered: not laid out again, even once the cap is back.
        CAP.with(|c| c.set(MAX_TIME));
        assert!(typeset(r"\frac{a}{b}", true).is_none());
        set_time_cap(MAX_TIME);
        assert!(typeset(r"\frac{a}{b}", true).is_some());
    }
}
