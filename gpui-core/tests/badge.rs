//! Badge: each variant's fill, text and border; a narrow space cuts its label short; it's text
//! (no Tab stop, no press) until given `on_click`, then a button that a click, Tab and Enter
//! reach, with its focus ring; a touch badge takes presses 44 points around it; and nothing
//! redraws while it sits there. Laid out with Core Text.

#![cfg(target_os = "macos")]

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    AnyElement, App, AppContext as _, Bounds, Context, Hsla, InteractiveElement as _, IntoElement,
    KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, ParentElement as _, Pixels, Render,
    Styled as _, TestAppContext, TestDispatcher, VisualTestContext, Window, div, point, px,
};
use gpui_core::{ActiveTheme as _, Badge, BadgeVariant, IconName, Press, Root, Theme};

type Make = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

struct Host {
    width: f32,
    make: Make,
    draws: Rc<Cell<usize>>,
}

impl Render for Host {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        div().p(px(40.)).child(
            div()
                .debug_selector(|| "box".into())
                .flex()
                .w(px(self.width))
                .child((self.make)(window, cx)),
        )
    }
}

fn app() -> TestAppContext {
    TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        Arc::new(gpui_apple::AppleTextSystem::new()),
    )
}

fn open(
    cx: &mut TestAppContext,
    theme: Theme,
    width: f32,
    make: Make,
) -> (Rc<Cell<usize>>, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_core::init(cx);
        Theme::set(theme, cx);
    });
    let draws = Rc::new(Cell::new(0));
    let d = draws.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let host = cx.new(|_| Host {
            width,
            make,
            draws: d,
        });
        Root::new(host, window, cx)
    });
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    (draws, vcx)
}

/// A badge in a wrapper the tests find it by (the wrapper is a flex row, so it fits the badge).
fn found(badge: Badge) -> AnyElement {
    div()
        .flex()
        .min_w_0()
        .debug_selector(|| "badge".into())
        .child(badge)
        .into_any_element()
}

fn close(a: Hsla, b: Hsla) -> bool {
    let (a, b) = (gpui::Rgba::from(a), gpui::Rgba::from(b));
    (a.r - b.r).abs() < 0.01
        && (a.g - b.g).abs() < 0.01
        && (a.b - b.b).abs() < 0.01
        && (a.a - b.a).abs() < 0.01
}

/// What was drawn inside `within`: the quads' fills and borders, the glyphs' colours and
/// right edges, and whether a shadow (the focus ring) was drawn around it.
struct Drawn {
    fills: Vec<Hsla>,
    borders: Vec<Hsla>,
    glyphs: Vec<(Hsla, f32)>,
    shadows: Vec<Hsla>,
}

fn drawn(vcx: &mut VisualTestContext, within: Bounds<Pixels>) -> Drawn {
    vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let k = window.scale_factor();
        let scene = window.rendered_scene();
        let inside = |b: Bounds<gpui::ScaledPixels>| {
            let c = point(px(b.center().x.0 / k), px(b.center().y.0 / k));
            within.contains(&c)
        };
        Drawn {
            fills: scene
                .quads
                .iter()
                .filter(|q| inside(q.bounds))
                .filter_map(|q| q.background.as_solid())
                .filter(|c| c.a > 0.)
                .collect(),
            borders: scene
                .quads
                .iter()
                .filter(|q| inside(q.bounds) && q.border_widths.top.0 > 0.)
                .map(|q| q.border_color)
                .filter(|c| c.a > 0.)
                .collect(),
            glyphs: scene
                .monochrome_sprites
                .iter()
                .filter(|g| inside(g.bounds) && g.content_mask.bounds.size.width.0 > 0.)
                .map(|g| {
                    let shown = (g.bounds.origin.x.0 + g.bounds.size.width.0)
                        .min(g.content_mask.bounds.origin.x.0 + g.content_mask.bounds.size.width.0);
                    (g.color, shown / k)
                })
                .collect(),
            shadows: scene
                .shadows
                .iter()
                .filter(|s| inside(s.bounds))
                .map(|s| s.color)
                .collect(),
        }
    })
}

#[gpui::test]
fn each_variant_draws_in_its_colours() {
    for theme in [Theme::dark(), Theme::light()] {
        let c = theme.colors;
        let dark = theme.mode.is_dark();
        let cases = [
            (
                BadgeVariant::Default,
                Some(c.primary),
                c.primary_foreground,
                None,
            ),
            (
                BadgeVariant::Secondary,
                Some(c.secondary),
                c.secondary_foreground,
                None,
            ),
            (
                BadgeVariant::Destructive,
                Some(c.destructive.opacity(if dark { 0.2 } else { 0.1 })),
                c.destructive,
                None,
            ),
            (BadgeVariant::Outline, None, c.foreground, Some(c.border)),
        ];
        for (variant, fill, text, border) in cases {
            let mut cx = app();
            let (_, vcx) = open(
                &mut cx,
                theme.clone(),
                300.,
                Rc::new(move |_, _| found(Badge::new("Badge").variant(variant))),
            );
            let b = vcx.debug_bounds("badge").unwrap();
            let d = drawn(vcx, b);
            match fill {
                Some(fill) => assert!(
                    d.fills.iter().any(|f| close(*f, fill)),
                    "{variant:?}: no {fill:?} fill in {:?}",
                    d.fills
                ),
                None => assert!(d.fills.is_empty(), "{variant:?} is filled: {:?}", d.fills),
            }
            match border {
                Some(border) => assert!(
                    d.borders.iter().any(|f| close(*f, border)),
                    "{variant:?}: no {border:?} border in {:?}",
                    d.borders
                ),
                None => assert!(d.borders.is_empty(), "{variant:?}: {:?}", d.borders),
            }
            assert!(!d.glyphs.is_empty(), "{variant:?}: no label drawn");
            assert!(
                d.glyphs.iter().all(|(g, _)| close(*g, text)),
                "{variant:?}: label not in {text:?}"
            );
            // shadcn's: 20 pixels tall, its 15-pixel rem's.
            assert_eq!(b.size.height, px(20.), "{variant:?}");
        }
    }
}

#[gpui::test]
fn a_narrow_space_cuts_the_label_short() {
    let mut cx = app();
    let (_, vcx) = open(
        &mut cx,
        Theme::dark(),
        70.,
        Rc::new(|_, _| {
            found(
                Badge::new("A label far too long for its seventy pixels")
                    .icon(IconName::Check)
                    .outline(),
            )
        }),
    );
    let bx = vcx.debug_bounds("box").unwrap();
    let b = vcx.debug_bounds("badge").unwrap();
    assert!(b.right() <= bx.right() + px(0.5), "{b:?} out of {bx:?}");
    let d = drawn(vcx, bx);
    assert!(!d.glyphs.is_empty());
    for (_, right) in d.glyphs {
        assert!(
            right <= f32::from(bx.right()) + 1.,
            "a glyph ends at {right} past {bx:?}"
        );
    }
}

fn keys(vcx: &mut VisualTestContext, keys: &'static str) {
    for k in keys.split(' ') {
        let keystroke = Keystroke::parse(k).unwrap();
        vcx.simulate_event(KeyDownEvent {
            keystroke: keystroke.clone(),
            is_held: false,
            prefer_character_input: false,
        });
        vcx.simulate_event(KeyUpEvent { keystroke });
        vcx.run_until_parked();
    }
}

/// A badge (pressable or not, for touch or not) and a Press after it, counting presses of each.
fn with_after(
    pressable: bool,
    touch: bool,
    pressed: Rc<Cell<usize>>,
    after: Rc<Cell<usize>>,
) -> Make {
    Rc::new(move |_, _| {
        let (pressed, after) = (pressed.clone(), after.clone());
        let mut badge = Badge::new("Beta").touch(touch);
        if pressable {
            badge = badge.on_click(move |_, _, _| pressed.set(pressed.get() + 1));
        }
        div()
            .flex()
            .gap(px(60.))
            .child(found(badge))
            .child(
                Press::new("after")
                    .debug_selector(|| "after".into())
                    .child("After")
                    .on_click(move |_, _, _| after.set(after.get() + 1)),
            )
            .into_any_element()
    })
}

#[gpui::test]
fn without_on_click_it_is_text(cx: &mut TestAppContext) {
    let (pressed, after) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (_, vcx) = open(
        cx,
        Theme::dark(),
        300.,
        with_after(false, false, pressed.clone(), after.clone()),
    );
    // Tab passes it by: the first stop is the Press after it.
    keys(vcx, "tab enter");
    assert_eq!(after.get(), 1);
}

#[gpui::test]
fn with_on_click_a_click_tab_and_enter_press_it(cx: &mut TestAppContext) {
    let (pressed, after) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (_, vcx) = open(
        cx,
        Theme::dark(),
        300.,
        with_after(true, false, pressed.clone(), after.clone()),
    );
    // Tab stops on it first, and Enter presses it.
    keys(vcx, "tab enter");
    assert_eq!(
        (pressed.get(), after.get()),
        (1, 0),
        "Tab didn't stop on the badge"
    );
    let at = vcx.debug_bounds("badge").unwrap().center();
    vcx.simulate_click(at, Modifiers::none());
    vcx.run_until_parked();
    assert_eq!(pressed.get(), 2);
}

#[gpui::test]
fn focused_from_the_keyboard_it_shows_the_ring() {
    let mut cx = app();
    let (pressed, after) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (_, vcx) = open(
        &mut cx,
        Theme::dark(),
        300.,
        with_after(true, false, pressed, after),
    );
    let b = vcx.debug_bounds("badge").unwrap();
    assert!(
        drawn(vcx, b).shadows.is_empty(),
        "a ring before it had the focus"
    );
    keys(vcx, "tab");
    let ring = vcx.update(|_, cx| cx.theme().focus_ring());
    let shadows = drawn(vcx, b).shadows;
    assert!(
        shadows.iter().any(|s| close(*s, ring)),
        "no ring with the focus: {shadows:?}"
    );
}

#[gpui::test]
fn a_touch_badge_takes_presses_44_points_around_it(cx: &mut TestAppContext) {
    for touch in [false, true] {
        let (pressed, after) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
        let (_, vcx) = open(
            cx,
            Theme::dark(),
            300.,
            with_after(true, touch, pressed.clone(), after),
        );
        let b = vcx.debug_bounds("badge").unwrap();
        // 20 points tall: 15 below its middle is past it, but inside a 44-point square.
        vcx.simulate_click(
            point(b.center().x, b.center().y + px(15.)),
            Modifiers::none(),
        );
        vcx.run_until_parked();
        assert_eq!(pressed.get(), usize::from(touch), "touch {touch}");
    }
}

/// Delivers one display frame and says how many things had asked for it.
fn frame(vcx: &mut VisualTestContext) -> usize {
    vcx.executor().advance_clock(Duration::from_millis(16));
    let asked = vcx.update(|window, cx| window.simulate_next_frame(cx));
    vcx.run_until_parked();
    asked
}

#[gpui::test]
fn nothing_redraws_while_it_sits_there(cx: &mut TestAppContext) {
    let (pressed, after) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (draws, vcx) = open(
        cx,
        Theme::dark(),
        300.,
        with_after(true, false, pressed, after),
    );
    let settled = draws.get();
    for _ in 0..30 {
        assert_eq!(frame(vcx), 0, "asked for a frame");
    }
    assert_eq!(draws.get(), settled);
}
