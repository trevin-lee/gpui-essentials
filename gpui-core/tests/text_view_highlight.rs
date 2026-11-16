//! TextView's code blocks in the host's highlighting: a fenced block in a language takes the
//! highlighter's colours, a block with none doesn't, a host's bad ranges are passed over, a new
//! highlighter lays the view out again, and gpui-core's own highlighter works end to end.

#![cfg(target_os = "macos")]

use std::sync::Arc;

use gpui::{
    AppContext as _, Context, Entity, HighlightStyle, Hsla, IntoElement, ParentElement as _,
    Render, Styled as _, TestAppContext, TestDispatcher, VisualTestContext, Window, div, px,
};
use gpui_core::text::HighlightFn;
use gpui_core::{Root, TextView, TextViewStyle};

const RED: Hsla = Hsla {
    h: 0.,
    s: 0.9,
    l: 0.5,
    a: 1.,
};

fn near(a: Hsla, b: Hsla) -> bool {
    (a.h - b.h).abs() < 0.02 && (a.l - b.l).abs() < 0.06 && a.s > 0.5
}

/// Colours its first two bytes red, whatever the language.
fn first_two_red() -> HighlightFn {
    Arc::new(|_, _| {
        vec![(
            0..2,
            HighlightStyle {
                color: Some(RED),
                ..Default::default()
            },
        )]
    })
}

struct Doc {
    source: &'static str,
    highlight: Option<HighlightFn>,
}

impl Render for Doc {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(400.))
            .child(TextView::markdown("d", self.source).style(TextViewStyle {
                highlight: self.highlight.clone(),
                ..Default::default()
            }))
    }
}

fn open<'a>(
    cx: &'a mut TestAppContext,
    source: &'static str,
    highlight: Option<HighlightFn>,
) -> (Entity<Doc>, &'a mut VisualTestContext) {
    cx.update(gpui_core::init);
    let made = std::rc::Rc::new(std::cell::RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let doc = cx.new(|_| Doc { source, highlight });
        *m.borrow_mut() = Some(doc.clone());
        Root::new(doc, window, cx)
    });
    vcx.run_until_parked();
    let doc = made.borrow().clone().unwrap();
    (doc, vcx)
}

fn app() -> TestAppContext {
    TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        Arc::new(gpui_apple::AppleTextSystem::new()),
    )
}

fn glyphs(vcx: &mut VisualTestContext) -> Vec<Hsla> {
    vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let scene = window.rendered_scene();
        scene
            .monochrome_sprites
            .iter()
            .map(|g| g.color)
            .chain(scene.subpixel_sprites.iter().map(|g| g.color))
            .collect()
    })
}

#[test]
fn a_fenced_block_takes_the_highlighters_colours() {
    let mut cx = app();
    let (_, vcx) = open(&mut cx, "```rust\nfn main() {}\n```", Some(first_two_red()));
    assert!(glyphs(vcx).iter().any(|c| near(*c, RED)));
}

#[test]
fn a_block_with_no_language_isnt_highlighted() {
    let mut cx = app();
    let (_, vcx) = open(&mut cx, "```\nfn main() {}\n```", Some(first_two_red()));
    assert!(!glyphs(vcx).iter().any(|c| near(*c, RED)));
}

#[test]
fn a_hosts_bad_ranges_are_passed_over() {
    let red = HighlightStyle {
        color: Some(RED),
        ..Default::default()
    };
    // Off a character's edge (inside "é"), past the end, backwards, and overlapping; then one
    // that's fine.
    let f: HighlightFn = Arc::new(move |_, code| {
        vec![
            (1..2, red),
            (0..9999, red),
            (std::ops::Range { start: 5, end: 3 }, red),
            (3..6, red),
            (4..5, red),
            (code.len() - 2..code.len(), red),
        ]
    });
    let mut cx = app();
    let (_, vcx) = open(&mut cx, "```rust\né abc de\n```", Some(f));
    // It drew, and the good ranges came through.
    assert!(glyphs(vcx).iter().any(|c| near(*c, RED)));
}

#[test]
fn a_new_highlighter_lays_the_view_out_again() {
    let mut cx = app();
    let (doc, vcx) = open(&mut cx, "```rust\nfn main() {}\n```", None);
    assert!(!glyphs(vcx).iter().any(|c| near(*c, RED)));
    doc.update(vcx, |d, cx| {
        d.highlight = Some(first_two_red());
        cx.notify();
    });
    vcx.run_until_parked();
    assert!(glyphs(vcx).iter().any(|c| near(*c, RED)));
}

#[cfg(feature = "highlight")]
#[test]
fn gpui_cores_highlighter_colours_a_rust_block() {
    use gpui_core::highlight::{Highlighter, SyntaxTheme};
    let theme = gpui_core::Theme::dark();
    let keyword = theme.colors.syntax_keyword;
    let f = Highlighter::new().into_fn(SyntaxTheme::from_theme(&theme));
    let mut cx = app();
    let (_, vcx) = open(&mut cx, "```rust\nfn main() { let x = 1; }\n```", Some(f));
    let near_kw = |c: &Hsla| (c.h - keyword.h).abs() < 0.02 && (c.l - keyword.l).abs() < 0.06;
    assert!(glyphs(vcx).iter().any(near_kw));
}
