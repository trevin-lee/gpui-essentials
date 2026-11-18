//! Drawing a demo in a hidden window, and saving the picture small: cropped to the content's
//! height, at most 1600 pixels wide, in 256 colours.

use std::collections::HashMap;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use color_quant::NeuQuant;
use gpui::{
    AnyElement, AnyView, AnyWindowHandle, App, AppContext as _, AsyncApp, Bounds, Context,
    IntoElement, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ParentElement as _, Pixels, PlatformInput, Point, Render, Styled as _, Window, WindowBounds,
    WindowOptions, div, point, px, size,
};
use gpui_core::Theme;
use image::RgbaImage;

use crate::site::{Act, Demo};

/// The stage's width in points: every picture is this wide, at the screen's scale.
pub const STAGE_WIDTH: f32 = 720.;
/// The stage's height in points: room for the tallest demo, cropped away after. It fits a
/// 1024×768 screen (a CI runner's) with the title bar: macOS shrinks a window too tall for the
/// screen, and the light and dark pictures would differ.
const STAGE_HEIGHT: f32 = 600.;
/// Room left above and below the content, in points.
const MARGIN: f32 = 40.;
/// The least height of a picture, in points, so a lone switch isn't a sliver.
const MIN_HEIGHT: f32 = 180.;
/// The widest picture saved, in pixels.
const MAX_WIDTH: u32 = 1600;
/// How different from the background a pixel must be to count as content (per channel).
const TOLERANCE: u8 = 4;

/// The window's view: the demo, in the middle of the stage.
struct Stage {
    demo: Staged,
}

enum Staged {
    Element(fn(&mut Window, &mut App) -> AnyElement),
    View(AnyView),
}

impl Render for Stage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let demo = match &self.demo {
            Staged::Element(render) => render(window, cx),
            Staged::View(view) => view.clone().into_any_element(),
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .p(px(MARGIN))
            .child(demo)
    }
}

/// Draws `demo` in the light or the dark theme in a hidden window, has the pointer `act` on
/// it, and returns the picture.
pub async fn capture(demo: Demo, act: Act, dark: bool, cx: &mut AsyncApp) -> Result<RgbaImage> {
    let window = cx.update(|cx| {
        Theme::set(if dark { Theme::dark() } else { Theme::light() }, cx);
        let bounds = Bounds::centered(None, size(px(STAGE_WIDTH), px(STAGE_HEIGHT)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            focus: false,
            show: false,
            ..Default::default()
        };
        gpui_core::open_window(options, cx, |window, cx| {
            let demo = match demo {
                Demo::Element(render) => Staged::Element(render),
                Demo::View(build) => Staged::View(build(window, cx)),
            };
            cx.new(|_| Stage { demo })
        })
        .map(|(window, _)| window)
    })?;
    // A few frames, for icons and fonts to load and anything that settles on the next frame.
    settle(window, Duration::from_millis(60), cx).await?;
    if act != Act::None {
        // The pointer goes to the middle of what was drawn, in points.
        let drawn = picture(window, cx)?;
        let Some([top, bottom, left, right]) = content_box(&drawn) else {
            bail!("nothing was drawn to act on");
        };
        let scale = drawn.width() as f32 / STAGE_WIDTH;
        let at = point(
            px((left + right) as f32 / 2. / scale),
            px((top + bottom) as f32 / 2. / scale),
        );
        cx.update_window(window, |_, window, cx| perform(act, at, window, cx))?;
        // Long enough for a hover card's or a tooltip's delay, and for anything opened to draw.
        let wait = if act == Act::Hover { 400 } else { 60 };
        settle(window, Duration::from_millis(wait), cx).await?;
    }
    let image = picture(window, cx)?;
    cx.update_window(window, |_, window, _| window.remove_window())?;
    Ok(image)
}

/// Draws three frames, `pause` apart.
async fn settle(window: AnyWindowHandle, pause: Duration, cx: &mut AsyncApp) -> Result<()> {
    for _ in 0..3 {
        cx.update_window(window, |_, window, cx| {
            window.refresh();
            let _ = window.draw(cx);
        })?;
        cx.background_executor().timer(pause).await;
    }
    Ok(())
}

/// Draws a frame and returns it.
fn picture(window: AnyWindowHandle, cx: &mut AsyncApp) -> Result<RgbaImage> {
    cx.update_window(window, |_, window, cx| {
        window.refresh();
        let _ = window.draw(cx);
        window.render_to_image()
    })?
}

/// Moves the pointer to `at` and, for a click, presses and releases a button there.
fn perform(act: Act, at: Point<Pixels>, window: &mut Window, cx: &mut App) {
    let modifiers = Modifiers::default();
    let moved = MouseMoveEvent {
        position: at,
        pressed_button: None,
        modifiers,
    };
    let _ = window.dispatch_event(PlatformInput::MouseMove(moved), cx);
    let button = match act {
        Act::Click => MouseButton::Left,
        Act::RightClick => MouseButton::Right,
        Act::None | Act::Hover => return,
    };
    let down = MouseDownEvent {
        button,
        position: at,
        modifiers,
        click_count: 1,
        first_mouse: false,
    };
    let _ = window.dispatch_event(PlatformInput::MouseDown(down), cx);
    let up = MouseUpEvent {
        button,
        position: at,
        modifiers,
        click_count: 1,
    };
    let _ = window.dispatch_event(PlatformInput::MouseUp(up), cx);
}

/// The first and last rows (inclusive) and columns holding content: pixels unlike the
/// top-left one, which is the background.
fn content_box(image: &RgbaImage) -> Option<[u32; 4]> {
    let bg = image.get_pixel(0, 0).0;
    let mut found: Option<[u32; 4]> = None;
    for (x, y, p) in image.enumerate_pixels() {
        let differs = p.0.iter().zip(bg).any(|(a, b)| a.abs_diff(b) > TOLERANCE);
        if differs {
            let b = found.get_or_insert([y, y, x, x]);
            b[0] = b[0].min(y);
            b[1] = b[1].max(y);
            b[2] = b[2].min(x);
            b[3] = b[3].max(x);
        }
    }
    found
}

/// Crops a variant's light and dark pictures to the same rows, those holding the content in
/// either plus a margin, and scales them down if they're wider than [`MAX_WIDTH`]. Warns when
/// the content reaches an edge of the stage, where it was probably cut.
pub fn crop_pair(light: RgbaImage, dark: RgbaImage, what: &str) -> Result<[RgbaImage; 2]> {
    // A window's height can round a pixel differently from one window to the next (on a 1×
    // screen), so a few pixels' difference is cut off the bottom; more is a real mismatch.
    let (w, h) = (
        light.width().min(dark.width()),
        light.height().min(dark.height()),
    );
    if light.width().abs_diff(dark.width()) > 4 || light.height().abs_diff(dark.height()) > 4 {
        bail!(
            "{what}: the light picture is {:?} pixels and the dark one {:?}",
            light.dimensions(),
            dark.dimensions()
        );
    }
    let light = image::imageops::crop_imm(&light, 0, 0, w, h).to_image();
    let dark = image::imageops::crop_imm(&dark, 0, 0, w, h).to_image();
    let scale = w as f32 / STAGE_WIDTH;
    let boxes: Vec<[u32; 4]> = [&light, &dark]
        .into_iter()
        .filter_map(content_box)
        .collect();
    let Some(content) = boxes.into_iter().reduce(|a, b| {
        [
            a[0].min(b[0]),
            a[1].max(b[1]),
            a[2].min(b[2]),
            a[3].max(b[3]),
        ]
    }) else {
        bail!("{what}: nothing was drawn");
    };
    let edge = (2. * scale) as u32;
    if content[0] < edge || content[1] + edge >= h || content[2] < edge || content[3] + edge >= w {
        eprintln!("warning: {what} reaches the edge of the {STAGE_WIDTH}×{STAGE_HEIGHT} stage");
    }
    let margin = (MARGIN * scale) as u32;
    let mut top = content[0].saturating_sub(margin);
    let mut bottom = (content[1] + 1 + margin).min(h);
    let least = ((MIN_HEIGHT * scale) as u32).min(h);
    if bottom - top < least {
        let more = least - (bottom - top);
        top = top.saturating_sub(more / 2);
        bottom = (top + least).min(h);
        top = bottom - least;
    }
    Ok([light, dark]
        .map(|image| fit(image::imageops::crop_imm(&image, 0, top, w, bottom - top).to_image())))
}

/// `image`, scaled down to [`MAX_WIDTH`] if it's wider.
fn fit(image: RgbaImage) -> RgbaImage {
    let (w, h) = image.dimensions();
    if w <= MAX_WIDTH {
        return image;
    }
    let height = (h as u64 * MAX_WIDTH as u64 / w as u64) as u32;
    image::imageops::resize(
        &image,
        MAX_WIDTH,
        height.max(1),
        image::imageops::FilterType::Lanczos3,
    )
}

/// Saves a crate's picture from its `docs/` folder as the site's are: at most [`MAX_WIDTH`]
/// wide, in 256 colours.
pub fn copy_picture(from: &Path, to: &Path) -> Result<()> {
    let image = image::open(from)
        .with_context(|| format!("reading {}", from.display()))?
        .into_rgba8();
    save_png(&fit(image), to)
}

/// Saves `image` as an indexed PNG of at most 256 colours: exactly its own colours when it has
/// that few, or NeuQuant's palette otherwise, with the background kept exact.
pub fn save_png(image: &RgbaImage, path: &Path) -> Result<()> {
    let (w, h) = image.dimensions();
    let (pixels, _) = image.as_raw().as_chunks::<4>();
    // Its own colours, if it has no more than 256.
    let mut colours: HashMap<[u8; 4], u8> = HashMap::new();
    let mut exact = true;
    for p in pixels {
        let next = colours.len();
        if let std::collections::hash_map::Entry::Vacant(slot) = colours.entry(*p) {
            if next == 256 {
                exact = false;
                break;
            }
            slot.insert(next as u8);
        }
    }
    let mut palette = vec![[0u8; 4]; colours.len()];
    for (colour, &i) in &colours {
        palette[i as usize] = *colour;
    }
    let indices: Vec<u8> = if exact {
        pixels.iter().map(|p| colours[p]).collect()
    } else {
        let quant = NeuQuant::new(3, 256, image.as_raw());
        palette = quant.color_map_rgba().as_chunks::<4>().0.to_vec();
        let bg = image.get_pixel(0, 0).0;
        palette[quant.index_of(&bg)] = bg;
        let mut seen: HashMap<[u8; 4], u8> = HashMap::new();
        pixels
            .iter()
            .map(|p| *seen.entry(*p).or_insert_with(|| quant.index_of(p) as u8))
            .collect()
    };
    let file = File::create(path).with_context(|| format!("creating {}", path.display()))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), w, h);
    encoder.set_color(png::ColorType::Indexed);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_palette(
        palette
            .iter()
            .flat_map(|c| [c[0], c[1], c[2]])
            .collect::<Vec<u8>>(),
    );
    encoder.set_compression(png::Compression::High);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&indices)?;
    writer.finish()?;
    Ok(())
}
