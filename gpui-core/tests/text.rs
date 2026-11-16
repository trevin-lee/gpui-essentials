//! Text drawn with the default theme shows: its fonts are ones every Mac has, so a host that
//! loads none still gets text. Laid out with Core Text, as on screen.

#![cfg(target_os = "macos")]

use std::sync::Arc;

use gpui::{
    AppContext as _, Context, IntoElement, ParentElement as _, Render, TestAppContext,
    TestDispatcher, Window, div,
};
use gpui_core::Root;

struct Hello;

impl Render for Hello {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().child("Hello, world")
    }
}

#[test]
fn the_default_theme_draws_text_without_loading_fonts() {
    let mut cx = TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        Arc::new(gpui_apple::AppleTextSystem::new()),
    );
    cx.update(gpui_core::init);
    let (_, vcx) = cx.add_window_view(|window, cx| {
        let hello = cx.new(|_| Hello);
        Root::new(hello, window, cx)
    });
    vcx.run_until_parked();
    vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let scene = window.rendered_scene();
        let glyphs = scene.monochrome_sprites.len() + scene.subpixel_sprites.len();
        assert!(glyphs >= 10, "drew {glyphs} glyphs for \"Hello, world\"");
    });
}
