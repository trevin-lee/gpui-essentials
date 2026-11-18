//! Italic stays italic in a family without an italic face: the run is set in the system UI
//! font's italic instead of drawing upright. A stand-in text system has "Inter" with only an
//! upright face and the system font with both, and records which faces text is shaped in.

use std::borrow::Cow;
use std::sync::{Arc, Mutex};

use gpui::{
    AppContext as _, Bounds, Context, DevicePixels, Font, FontId, FontMetrics, FontRun, FontStyle,
    GlyphId, IntoElement, LineLayout, NoopTextSystem, ParentElement as _, Pixels,
    PlatformTextSystem, Render, RenderGlyphParams, Result, Size, Styled as _, TestAppContext,
    TestDispatcher, TextRenderingMode, TextSystem, Window, div, font, px, size,
};
use gpui_core::text::italic::{ITALIC_FALLBACK_FAMILY, italic_font};
use gpui_core::{Root, TextView, Theme};

const INTER: FontId = FontId(1);
const SYSTEM: FontId = FontId(2);
const SYSTEM_ITALIC: FontId = FontId(3);

/// Inter (and any other family) upright only; the system font upright and italic, unless
/// `system_italic` is off.
struct Fonts {
    noop: NoopTextSystem,
    system_italic: bool,
    shaped: Mutex<Vec<FontId>>,
}

impl Fonts {
    fn new(system_italic: bool) -> Self {
        Fonts {
            noop: NoopTextSystem::new(),
            system_italic,
            shaped: Mutex::default(),
        }
    }
}

impl PlatformTextSystem for Fonts {
    fn add_fonts(&self, fonts: Vec<Cow<'static, [u8]>>) -> Result<()> {
        self.noop.add_fonts(fonts)
    }
    fn all_font_names(&self) -> Vec<String> {
        vec!["Inter".into(), ITALIC_FALLBACK_FAMILY.into()]
    }
    fn font_id(&self, d: &Font) -> Result<FontId> {
        Ok(if d.family.as_ref() == ITALIC_FALLBACK_FAMILY {
            if d.style == FontStyle::Italic && self.system_italic {
                SYSTEM_ITALIC
            } else {
                SYSTEM
            }
        } else {
            INTER
        })
    }
    fn font_metrics(&self, f: FontId) -> FontMetrics {
        self.noop.font_metrics(f)
    }
    fn typographic_bounds(&self, f: FontId, g: GlyphId) -> Result<Bounds<f32>> {
        self.noop.typographic_bounds(f, g)
    }
    fn advance(&self, f: FontId, g: GlyphId) -> Result<Size<f32>> {
        self.noop.advance(f, g)
    }
    fn glyph_for_char(&self, f: FontId, c: char) -> Option<GlyphId> {
        self.noop.glyph_for_char(f, c)
    }
    fn glyph_raster_bounds(&self, p: &RenderGlyphParams) -> Result<Bounds<DevicePixels>> {
        self.noop.glyph_raster_bounds(p)
    }
    fn rasterize_glyph(
        &self,
        p: &RenderGlyphParams,
        b: Bounds<DevicePixels>,
    ) -> Result<(Size<DevicePixels>, Vec<u8>)> {
        self.noop.rasterize_glyph(p, b)
    }
    fn layout_line(&self, text: &str, size: Pixels, runs: &[FontRun]) -> LineLayout {
        self.shaped
            .lock()
            .unwrap()
            .extend(runs.iter().map(|r| r.font_id));
        self.noop.layout_line(text, size, runs)
    }
    fn recommended_rendering_mode(&self, f: FontId, s: Pixels) -> TextRenderingMode {
        self.noop.recommended_rendering_mode(f, s)
    }
}

#[test]
fn an_italic_the_family_lacks_comes_from_the_system_font() {
    let ts = TextSystem::new(Arc::new(Fonts::new(true)));
    let asked = font("Inter").italic();
    let got = italic_font(asked.clone(), &ts);
    assert_eq!(got.family.as_ref(), ITALIC_FALLBACK_FAMILY);
    assert_eq!(got.style, FontStyle::Italic);
    assert_eq!(got.weight, asked.weight);
    assert_eq!(ts.resolve_font(&got), SYSTEM_ITALIC);
    // Bold italic keeps its weight.
    let bold = italic_font(font("Inter").bold().italic(), &ts);
    assert_eq!(bold.weight, font("Inter").bold().weight);
    // Upright text, and an italic the family has, are left alone.
    assert_eq!(italic_font(font("Inter"), &ts), font("Inter"));
    let system = font(ITALIC_FALLBACK_FAMILY).italic();
    assert_eq!(italic_font(system.clone(), &ts), system);
}

#[test]
fn without_any_italic_it_draws_upright_as_before() {
    let ts = TextSystem::new(Arc::new(Fonts::new(false)));
    let asked = font("Inter").italic();
    assert_eq!(italic_font(asked.clone(), &ts), asked);
}

struct Doc;

impl Render for Doc {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(400.))
            .child(TextView::markdown("doc", "upright *slanted* upright"))
    }
}

#[test]
fn markdown_emphasis_in_a_family_without_italic_is_italic() {
    let fonts = Arc::new(Fonts::new(true));
    let mut cx =
        TestAppContext::build_with_text_system(TestDispatcher::new(0), None, fonts.clone());
    cx.update(|cx| {
        gpui_core::init(cx);
        let mut theme = Theme::dark();
        theme.font_family = "Inter".into();
        Theme::set(theme, cx);
    });
    let (_, vcx) = cx.add_window_view(|window, cx| Root::new(cx.new(|_| Doc), window, cx));
    vcx.simulate_resize(size(px(600.), px(300.)));
    vcx.run_until_parked();
    let shaped = fonts.shaped.lock().unwrap().clone();
    assert!(
        shaped.contains(&SYSTEM_ITALIC),
        "the emphasis was shaped in {shaped:?}, not the system font's italic"
    );
    assert!(shaped.contains(&INTER), "the rest stays in Inter");
}
