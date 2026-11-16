//! A menu entry's hint (its shortcut's text) gives way to its label: a long hint is cut short
//! with an ellipsis while the label keeps its text, down to the hint's least width, and neither
//! runs out of the row. Laid out with Core Text.

#![cfg(target_os = "macos")]

use std::sync::Arc;

use gpui::{
    AppContext as _, Bounds, Context, IntoElement, ParentElement as _, Pixels, Render, Styled as _,
    TestAppContext, TestDispatcher, VisualTestContext, Window, div, px,
};
use gpui_core::{PopupMenu, PopupMenuItem, Root};

const LONG_HINT: &str = "Sorted by the date it was last modified, newest first";

struct Panel;

impl Render for Panel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().p(px(20.)).w(px(260.)).child(
            PopupMenu::new("menu")
                .take_focus(false)
                .item(PopupMenuItem::new("short", "Status").shortcut(LONG_HINT))
                .item(PopupMenuItem::new("keys", "Status").shortcut("⌘S"))
                .item(
                    PopupMenuItem::new(
                        "both",
                        "A label long enough to need the whole row and more besides",
                    )
                    .shortcut(LONG_HINT),
                ),
        )
    }
}

fn open(cx: &mut TestAppContext) -> &mut VisualTestContext {
    cx.update(gpui_core::init);
    let (_, vcx) = cx.add_window_view(|window, cx| {
        let v = cx.new(|_| Panel);
        Root::new(v, window, cx)
    });
    vcx.run_until_parked();
    vcx
}

fn app() -> TestAppContext {
    TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        Arc::new(gpui_apple::AppleTextSystem::new()),
    )
}

/// How many glyphs are drawn inside `b`.
fn glyphs_in(vcx: &mut VisualTestContext, b: Bounds<Pixels>) -> usize {
    vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let k = window.scale_factor();
        let scene = window.rendered_scene();
        scene
            .monochrome_sprites
            .iter()
            .map(|g| g.bounds)
            .chain(scene.subpixel_sprites.iter().map(|g| g.bounds))
            .filter(|g| {
                let x = (g.origin.x.0 + g.size.width.0 / 2.) / k;
                let y = (g.origin.y.0 + g.size.height.0 / 2.) / k;
                x >= f32::from(b.left())
                    && x <= f32::from(b.right())
                    && y >= f32::from(b.top())
                    && y <= f32::from(b.bottom())
            })
            .count()
    })
}

fn inside(inner: Bounds<Pixels>, row: Bounds<Pixels>) -> bool {
    inner.left() >= row.left() - px(4.) && inner.right() <= row.right() + px(4.)
}

#[test]
fn a_long_hint_gives_way_to_the_label() {
    let mut cx = app();
    let vcx = open(&mut cx);
    let row = vcx.debug_bounds("menu-item-short").unwrap();
    let label = vcx.debug_bounds("menu-label-short").unwrap();
    let hint = vcx.debug_bounds("menu-hint-short").unwrap();
    // "Status" whole: six glyphs, none cut for an ellipsis.
    assert_eq!(glyphs_in(vcx, label), 6, "{label:?}");
    // The hint has the rest of the row, cut short, and stays in it.
    assert!(inside(hint, row), "{hint:?} in {row:?}");
    assert!(hint.left() >= label.right() - px(4.), "{label:?} {hint:?}");
    assert!(hint.size.width >= px(28.), "{hint:?}");
    assert!(
        glyphs_in(vcx, hint) < LONG_HINT.chars().filter(|c| *c != ' ').count(),
        "the hint is cut short"
    );
}

#[test]
fn a_short_hint_is_drawn_whole() {
    let mut cx = app();
    let vcx = open(&mut cx);
    let hint = vcx.debug_bounds("menu-hint-keys").unwrap();
    assert_eq!(glyphs_in(vcx, hint), 2, "{hint:?}");
}

#[test]
fn with_a_long_label_too_the_hint_keeps_its_least_and_both_stay_in_the_row() {
    let mut cx = app();
    let vcx = open(&mut cx);
    let row = vcx.debug_bounds("menu-item-both").unwrap();
    let label = vcx.debug_bounds("menu-label-both").unwrap();
    let hint = vcx.debug_bounds("menu-hint-both").unwrap();
    assert!(
        inside(label, row) && inside(hint, row),
        "{label:?} {hint:?} in {row:?}"
    );
    let least = hint.size.width;
    assert!(least >= px(28.) && least < px(40.), "{hint:?}");
    assert!(
        glyphs_in(vcx, hint) >= 1,
        "the hint shows a character or two"
    );
}
