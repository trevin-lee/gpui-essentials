//! A light card in a dark app: gpui-core's text field and slider, which the card embeds, take
//! the card's colours, not the app's global gpui-core `Theme`. Read from the scene the window
//! drew, with Core Text laying the text out (so the placeholder has glyphs to read).

#![cfg(all(target_os = "macos", feature = "gpui"))]

mod common;

use std::rc::Rc;
use std::sync::Arc;

use ::gpui::TestDispatcher;
use common::*;
use gpui::*;
use gpui_chatwidgets::CardTheme;
// The built-in #[test], not GPUI's, which gpui::* brings in.
use std::prelude::v1::test;

const PROGRAM: &str = r#"root = Form("f", Buttons([]), [FormControl("Name", Input("name", "Your name")), FormControl("Budget", Slider("budget", "continuous", 0, 100, 1, [50]))])"#;

fn app_with_text() -> TestAppContext {
    TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        Arc::new(gpui_apple::AppleTextSystem::new()),
    )
}

/// A light card's host, in an app whose gpui-core theme is dark.
fn light_host() -> Rc<TestHost> {
    let mut host = Rc::into_inner(TestHost::new(Images::Refuse)).unwrap();
    host.theme = CardTheme {
        rem_base: 16.,
        ..CardTheme::light()
    };
    Rc::new(host)
}

struct Drawn {
    /// Each solid quad's fill, size and border colour.
    quads: Vec<(Hsla, Size<Pixels>, Hsla)>,
    /// Each glyph's colour.
    glyphs: Vec<Hsla>,
}

fn scene(cx: &mut VisualTestContext) -> Drawn {
    draw(cx);
    cx.update(|window, _| {
        let scale = window.scale_factor();
        let scene = window.rendered_scene();
        let quads = scene
            .quads
            .iter()
            .filter_map(|q| {
                let size = size(
                    px(q.bounds.size.width.as_f32() / scale),
                    px(q.bounds.size.height.as_f32() / scale),
                );
                Some((q.background.as_solid()?, size, q.border_color))
            })
            .collect();
        let glyphs = scene
            .monochrome_sprites
            .iter()
            .map(|g| g.color)
            .chain(scene.subpixel_sprites.iter().map(|g| g.color))
            .collect();
        Drawn { quads, glyphs }
    })
}

fn near(a: Hsla, b: Hsla) -> bool {
    let (a, b) = (Rgba::from(a), Rgba::from(b));
    (a.r - b.r).abs() < 0.01
        && (a.g - b.g).abs() < 0.01
        && (a.b - b.b).abs() < 0.01
        && (a.a - b.a).abs() < 0.01
}

fn opens_light_in_dark(cx: &mut TestAppContext) -> (Rc<TestHost>, &mut VisualTestContext) {
    cx.update(|cx| gpui_core::Theme::set(gpui_core::Theme::dark(), cx));
    let host = light_host();
    let (_, vcx) = open(cx, PROGRAM, &host, Some(400.));
    (host, vcx)
}

#[test]
fn a_light_card_s_text_field_takes_the_card_s_colours_in_a_dark_app() {
    let mut cx = app_with_text();
    let (host, vcx) = opens_light_in_dark(&mut cx);
    let card = host.theme.colors.clone();
    let global = gpui_core::Theme::dark();

    // The placeholder, before anything is typed.
    let drawn = scene(vcx);
    assert!(
        drawn.glyphs.iter().any(|c| near(*c, card.text_muted)),
        "the placeholder in the card's muted text: {:?}",
        drawn.glyphs
    );
    assert!(
        !drawn
            .glyphs
            .iter()
            .any(|c| near(*c, global.muted_foreground)),
        "nothing in the dark theme's muted foreground"
    );

    // The caret, once the field has the focus.
    let field = vcx
        .debug_bounds("field:t:r/f0.field")
        .expect("the field")
        .center();
    vcx.simulate_click(field, Modifiers::none());
    vcx.simulate_input("Ann");
    let drawn = scene(vcx);
    let caret = |c: Hsla| {
        drawn
            .quads
            .iter()
            .any(|(fill, size, _)| near(*fill, c) && size.width <= px(3.) && size.height > px(8.))
    };
    assert!(caret(card.text), "a caret in the card's text colour");
    assert!(
        !caret(global.foreground),
        "no caret in the dark theme's near-white"
    );

    // The selection, over all of it.
    vcx.simulate_keystrokes("cmd-a");
    let drawn = scene(vcx);
    let selection = card.accent.opacity(0.3);
    assert!(
        drawn.quads.iter().any(|(fill, ..)| near(*fill, selection)),
        "the selection in the card's accent at 30%"
    );
    assert!(
        !drawn
            .quads
            .iter()
            .any(|(fill, ..)| near(*fill, global.selection())),
        "no selection in the dark theme's"
    );
}

#[test]
fn a_light_card_s_slider_takes_the_card_s_colours_in_a_dark_app() {
    let mut cx = app_with_text();
    let (host, vcx) = opens_light_in_dark(&mut cx);
    let card = host.theme.colors.clone();
    let global = gpui_core::Theme::dark();
    let drawn = scene(vcx);
    // The track and its fill are 6 CSS pixels tall; the thumb 16 across (gpui-core's CSS
    // pixels are fifteenths of a rem).
    let about = |a: Pixels, css: f32| (f32::from(a) - css * 16. / 15.).abs() < 0.5;
    let bar = |c: Hsla| {
        drawn
            .quads
            .iter()
            .any(|(fill, size, _)| near(*fill, c) && about(size.height, 6.))
    };
    assert!(bar(card.line_strong), "the track in the card's strong line");
    assert!(bar(card.accent), "the fill in the card's accent");
    // GPUI draws a box's fill and its border as two quads.
    let thumb = |pick: &dyn Fn(Hsla, Hsla) -> bool| {
        drawn
            .quads
            .iter()
            .any(|(fill, size, edge)| pick(*fill, *edge) && about(size.width, 16.))
    };
    assert!(
        thumb(&|fill, _| near(fill, card.background)),
        "the thumb in the card's background"
    );
    assert!(
        thumb(&|_, edge| near(edge, card.accent)),
        "the thumb edged in the card's accent"
    );
    assert!(
        !bar(global.input),
        "no track in the dark theme's input colour"
    );
    assert!(!bar(global.primary), "no fill in the dark theme's primary");
}
