//! Hosts with palettes of their own, and hosts that never call `init`: a field's keys work
//! without it, and a field's, slider's and tooltip's colours can be given per element.

#![cfg(target_os = "macos")]

use std::sync::Arc;

use gpui::{
    AnyView, AppContext as _, Context, Entity, Hsla, IntoElement, ParentElement as _, Render,
    Styled as _, TestAppContext, TestDispatcher, VisualTestContext, Window, div, hsla, px,
};
use gpui_core::{Input, InputState, Slider, SliderState, Tooltip};

fn app() -> TestAppContext {
    TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        Arc::new(gpui_apple::AppleTextSystem::new()),
    )
}

const RED: Hsla = Hsla {
    h: 0.,
    s: 0.9,
    l: 0.5,
    a: 1.,
};
const GREEN: Hsla = Hsla {
    h: 0.33,
    s: 0.9,
    l: 0.4,
    a: 1.,
};
const BLUE: Hsla = Hsla {
    h: 0.6,
    s: 0.9,
    l: 0.5,
    a: 1.,
};

struct Card {
    field: Entity<InputState>,
    slider: Entity<SliderState>,
    tip: Option<AnyView>,
}

impl Render for Card {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(300.))
            .child(
                Input::new(&self.field)
                    .placeholder_color(RED)
                    .caret_color(GREEN),
            )
            .child(
                Slider::new(&self.slider)
                    .track_color(BLUE)
                    .fill_color(GREEN)
                    .w_full(),
            )
            .children(self.tip.clone())
    }
}

/// A card in a window with no Root and no `init`: as a host embedding a crate would.
fn open(cx: &mut TestAppContext) -> (Entity<InputState>, &mut VisualTestContext) {
    let made = std::rc::Rc::new(std::cell::RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let field = cx.new(|cx| InputState::new(window, cx).placeholder("Name"));
        let slider = cx.new(|_| SliderState::new().default_value(50.));
        let tip = Some(
            Tooltip::new("Tip")
                .background(BLUE)
                .text_color(RED)
                .build(window, cx),
        );
        *m.borrow_mut() = Some(field.clone());
        Card { field, slider, tip }
    });
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let field = made.borrow().clone().unwrap();
    (field, vcx)
}

#[test]
fn a_field_works_without_init() {
    let mut cx = app();
    let (field, vcx) = open(&mut cx);
    vcx.update(|window, cx| field.update(cx, |f, cx| f.focus(window, cx)));
    vcx.simulate_input("ab");
    vcx.simulate_keystrokes("backspace");
    assert_eq!(field.read_with(vcx, |f, _| f.value().to_string()), "a");
}

#[test]
fn colours_given_per_element_are_drawn() {
    let mut cx = app();
    let (field, vcx) = open(&mut cx);
    vcx.update(|window, cx| field.update(cx, |f, cx| f.focus(window, cx)));
    let (glyphs, fills) = vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let scene = window.rendered_scene();
        let glyphs: Vec<Hsla> = scene
            .monochrome_sprites
            .iter()
            .map(|g| g.color)
            .chain(scene.subpixel_sprites.iter().map(|g| g.color))
            .collect();
        let fills: Vec<Hsla> = scene
            .quads
            .iter()
            .filter_map(|q| q.background.as_solid())
            .collect();
        (glyphs, fills)
    });
    let near = |a: Hsla, b: Hsla| (a.h - b.h).abs() < 0.02 && (a.l - b.l).abs() < 0.05;
    assert!(
        glyphs.iter().any(|c| near(*c, RED)),
        "the placeholder (and the tooltip's text) in the given colour"
    );
    assert!(
        fills.iter().any(|c| near(*c, GREEN)),
        "the caret and the slider's fill in the given colour"
    );
    assert!(
        fills.iter().any(|c| near(*c, BLUE)),
        "the slider's track and the tooltip in the given colour"
    );
}

#[test]
fn the_default_track_is_the_input_colour_in_the_dark() {
    let mut cx = app();
    let (_, vcx) = cx.add_window_view(|_, cx| {
        let slider = cx.new(|_| SliderState::new().default_value(50.));
        Plain { slider }
    });
    vcx.run_until_parked();
    let fills = vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        window
            .rendered_scene()
            .quads
            .iter()
            .filter_map(|q| q.background.as_solid())
            .collect::<Vec<_>>()
    });
    let input = gpui_core::Theme::dark().input;
    assert!(fills.contains(&input), "{fills:?}");
    let _ = hsla;
}

struct Plain {
    slider: Entity<SliderState>,
}

impl Render for Plain {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().w(px(200.)).child(Slider::new(&self.slider).w_full())
    }
}
