//! A label's descenders and accents aren't cut. Buttons, badges, toggles and keys set their text
//! at a line height of 1, so a "p" or "g" reaches below the line box and an "É" above it; their
//! label slots clip (for the ellipsis), so they must leave the ink room above and below. Laid
//! out with Core Text, as on screen, at 50%, 100% and 200% zoom: a glyph cut by its clip at the
//! top or bottom fails the test with where it is.

#![cfg(target_os = "macos")]

use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    AnyElement, AppContext as _, Context, IntoElement, ParentElement as _, Render, Styled as _,
    TestAppContext, TestDispatcher, Window, div, px, size,
};
use gpui_core::{
    Badge, Button, ButtonVariant, ButtonVariants as _, ControlSize, Kbd, Root, Sizable as _, Theme,
    Toggle,
};

/// Descenders (g, j, p, q, y) and a capital with an accent above the cap height.
const TEXT: &str = "Upgrade page gjpqy ÉÅ";

type Make = Rc<dyn Fn() -> AnyElement>;

fn components() -> Vec<(String, Make)> {
    let mut out: Vec<(String, Make)> = Vec::new();
    for v in [
        ButtonVariant::Primary,
        ButtonVariant::Outline,
        ButtonVariant::Ghost,
        ButtonVariant::Link,
    ] {
        for s in [
            ControlSize::XSmall,
            ControlSize::Small,
            ControlSize::Medium,
            ControlSize::Large,
            ControlSize::Touch,
        ] {
            out.push((
                format!("Button {v:?} {s:?}"),
                Rc::new(move || {
                    Button::new("b")
                        .label(TEXT)
                        .with_variant(v)
                        .with_size(s)
                        .into_any_element()
                }),
            ));
        }
    }
    out.push((
        "Badge".into(),
        Rc::new(|| Badge::new(TEXT).into_any_element()),
    ));
    out.push((
        "Toggle".into(),
        Rc::new(|| Toggle::new("t").label(TEXT).into_any_element()),
    ));
    out.push(("Kbd".into(), Rc::new(|| Kbd::new(TEXT).into_any_element())));
    out
}

struct Gallery {
    items: Vec<Make>,
}

impl Render for Gallery {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().flex().flex_col().gap(px(8.)).p(px(8.)).children(
            self.items
                .iter()
                .map(|make| div().flex().flex_none().child(make())),
        )
    }
}

/// Every glyph the component draws that its clip cuts at the top or bottom.
fn cut_glyphs(zoom: f32, make: Make) -> (usize, Vec<String>) {
    let mut cx = TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        Arc::new(gpui_apple::AppleTextSystem::new()),
    );
    cx.update(|cx| {
        gpui_core::init(cx);
        let mut t = Theme::dark();
        t.font_size = px(15. * zoom);
        Theme::set(t, cx);
    });
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let gallery = cx.new(|_| Gallery { items: vec![make] });
        Root::new(gallery, window, cx)
    });
    vcx.simulate_resize(size(px(1200.), px(400.)));
    vcx.run_until_parked();
    vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let scale = window.scale_factor();
        let scene = window.rendered_scene();
        // Antialiasing's half pixel either way.
        let slack = scale;
        let mut drawn = 0;
        let mut cut = Vec::new();
        for s in &scene.monochrome_sprites {
            let (b, m) = (&s.bounds, &s.content_mask.bounds);
            let top = b.origin.y.as_f32();
            let bottom = top + b.size.height.as_f32();
            let clip_top = m.origin.y.as_f32();
            let clip_bottom = clip_top + m.size.height.as_f32();
            if bottom <= clip_top || top >= clip_bottom {
                continue;
            }
            drawn += 1;
            if top < clip_top - slack || bottom > clip_bottom + slack {
                cut.push(format!(
                    "glyph at y {:.1}–{:.1} (clip {:.1}–{:.1})",
                    top / scale,
                    bottom / scale,
                    clip_top / scale,
                    clip_bottom / scale
                ));
            }
        }
        (drawn, cut)
    })
}

#[test]
fn labels_keep_their_descenders_and_accents() {
    let mut found = Vec::new();
    for zoom in [0.5, 1., 2.] {
        for (name, make) in components() {
            let (drawn, cut) = cut_glyphs(zoom, make);
            assert!(drawn > 0, "{name} at {}% drew no text", zoom * 100.);
            if let Some(first) = cut.first() {
                found.push(format!(
                    "{name} at {}%: {} glyphs cut, first {first}",
                    zoom * 100.,
                    cut.len()
                ));
            }
        }
    }
    assert!(
        found.is_empty(),
        "{} labels cut at the top or bottom:\n{}",
        found.len(),
        found.join("\n")
    );
}
