//! Alert: a destructive one is an alert to assistive technology and the default one a status
//! (or what the host says), named by its title; it's drawn in the variant's colours with the
//! icon level with the title; its text wraps inside it in a narrow box (laid out with Core
//! Text, as on screen); and nothing redraws while it sits there.

#![cfg(target_os = "macos")]

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    AnyElement, AppContext as _, Bounds, Context, Element as _, Hsla, InteractiveElement as _,
    IntoElement, ParentElement as _, Pixels, Render, RenderOnce as _, Role, Styled as _,
    TestAppContext, TestDispatcher, VisualTestContext, Window, accesskit, div, px,
};
use gpui_core::{Alert, IconName, Root, Theme};

const LONG: &str = "Pneumonoultramicroscopicsilicovolcanoconiosis-Antidisestablishmentarianism \
                    職務経歴書と志望動機を来週の月曜日までに提出してください 🎓📚✅🚀";

type Make = Rc<dyn Fn() -> AnyElement>;

struct Host {
    width: f32,
    make: Make,
    draws: Rc<Cell<usize>>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        div().p(px(20.)).child(
            div()
                .w(px(self.width))
                .debug_selector(|| "box".into())
                .child((self.make)()),
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
    width: f32,
    make: impl Fn() -> AnyElement + 'static,
) -> (Rc<Cell<usize>>, &mut VisualTestContext) {
    cx.update(gpui_core::init);
    let draws = Rc::new(Cell::new(0));
    let d = draws.clone();
    let make: Make = Rc::new(make);
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let host = cx.new(|_| Host {
            width,
            make,
            draws: d,
        });
        Root::new(host, window, cx)
    });
    vcx.run_until_parked();
    (draws, vcx)
}

fn bounds(vcx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    vcx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} drawn"))
}

fn alert(destructive: bool) -> Alert {
    let a = Alert::new("a")
        .icon(IconName::CircleExclamationPoint)
        .title("Sync failed")
        .description("Check your connection and try again.");
    if destructive { a.destructive() } else { a }
}

/// The alert's role and its node as assistive technology gets it.
fn node(vcx: &mut VisualTestContext, alert: Alert) -> (Option<Role>, accesskit::Node) {
    vcx.update(|window, cx| {
        let el = alert.render(window, cx).into_element();
        let role = el.a11y_role();
        let mut node = accesskit::Node::new(role.unwrap_or(Role::Unknown));
        el.write_a11y_info(&mut node);
        (role, node)
    })
}

#[gpui::test]
fn destructive_is_an_alert_and_default_a_status(cx: &mut TestAppContext) {
    let (_, vcx) = open(cx, 300., || div().into_any_element());
    let (role, n) = node(vcx, alert(true));
    assert_eq!(role, Some(Role::Alert));
    assert_eq!(n.label(), Some("Sync failed"));
    assert_eq!(
        n.description(),
        Some("Check your connection and try again.")
    );
    assert_eq!(node(vcx, alert(false)).0, Some(Role::Status));
    assert_eq!(
        node(vcx, alert(false).role(Role::Note)).0,
        Some(Role::Note),
        "the host's role wins"
    );
}

/// The colours of the glyphs drawn, and of the filled boxes.
fn colours(vcx: &mut VisualTestContext) -> (Vec<Hsla>, Vec<Hsla>) {
    vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let scene = window.rendered_scene();
        let glyphs = scene
            .monochrome_sprites
            .iter()
            .map(|g| g.color)
            .chain(scene.subpixel_sprites.iter().map(|g| g.color))
            .collect();
        let fills = scene
            .quads
            .iter()
            .filter_map(|q| q.background.as_solid())
            .collect();
        (glyphs, fills)
    })
}

#[test]
fn each_variant_draws_in_its_colours() {
    let t = Theme::dark().colors;
    let mut cx = app();
    let (_, vcx) = open(&mut cx, 300., || alert(false).into_any_element());
    let (glyphs, fills) = colours(vcx);
    assert!(fills.contains(&t.card), "{fills:?}");
    assert!(glyphs.contains(&t.card_foreground), "the title");
    assert!(glyphs.contains(&t.muted_foreground), "the description");
    assert!(!glyphs.contains(&t.destructive));

    let mut cx = app();
    let (_, vcx) = open(&mut cx, 300., || alert(true).into_any_element());
    let (glyphs, _) = colours(vcx);
    assert!(glyphs.contains(&t.destructive), "the title and the icon");
    assert!(
        glyphs.contains(&t.destructive.opacity(0.9)),
        "the description"
    );
    assert!(!glyphs.contains(&t.muted_foreground));
}

#[test]
fn the_icon_sits_level_with_the_title_and_the_text_beside_it() {
    let mut cx = app();
    let (_, vcx) = open(&mut cx, 300., || alert(false).into_any_element());
    let icon = bounds(vcx, "alert-icon");
    let title = bounds(vcx, "alert-title");
    let description = bounds(vcx, "alert-description");
    assert!(
        (icon.top() - title.top()).abs() < px(0.5),
        "{icon:?} {title:?}"
    );
    assert!(
        (icon.size.height - title.size.height).abs() < px(0.5),
        "one line: {icon:?} {title:?}"
    );
    assert!(icon.right() < title.left());
    assert_eq!(description.left(), title.left());
    assert!(description.top() >= title.bottom());
}

/// Every glyph drawn outside `inside`, or cut off at what clips it (but for a slot narrower
/// than one glyph, which cuts its ellipsis).
fn glyphs_out(vcx: &mut VisualTestContext, inside: Bounds<Pixels>) -> Vec<String> {
    vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let k = window.scale_factor();
        let scene = window.rendered_scene();
        scene
            .monochrome_sprites
            .iter()
            .map(|s| (s.bounds, s.content_mask.bounds, 0.))
            .chain(
                scene
                    .subpixel_sprites
                    .iter()
                    .map(|s| (s.bounds, s.content_mask.bounds, 0.)),
            )
            .chain(scene.polychrome_sprites.iter().map(|s| {
                // An emoji's bitmap is padded either side of its advance.
                (
                    s.bounds,
                    s.content_mask.bounds,
                    s.bounds.size.width.as_f32() / k / 7.,
                )
            }))
            .filter_map(|(b, clip, pad)| {
                let (l, r) = (
                    b.origin.x.as_f32() / k,
                    (b.origin.x + b.size.width).as_f32() / k,
                );
                let (cl, cr) = (
                    clip.origin.x.as_f32() / k,
                    (clip.origin.x + clip.size.width).as_f32() / k,
                );
                if r <= cl || l >= cr {
                    return None;
                }
                let slack = 2. + pad;
                // A slot narrower than one glyph (a label squeezed to nothing) cuts its ellipsis, as
                // browsers do.
                let squeezed = cr - cl < r - l;
                let cut = !squeezed && (l < cl - slack || r > cr + slack);
                let out = l < f32::from(inside.left()) - slack
                    || r.min(cr) > f32::from(inside.right()) + slack;
                (cut || out).then(|| format!("glyph {l:.0}–{r:.0} clip {cl:.0}–{cr:.0}"))
            })
            .collect()
    })
}

#[test]
fn long_text_wraps_inside_a_narrow_alert() {
    for width in [80., 160., 320.] {
        let mut cx = app();
        let (_, vcx) = open(&mut cx, width, || {
            Alert::new("a")
                .icon(IconName::Info)
                .title(LONG)
                .description(LONG)
                .child(LONG)
                .into_any_element()
        });
        let alert = bounds(vcx, "alert");
        let b = bounds(vcx, "box");
        assert!(alert.right() <= b.right() + px(0.5), "{alert:?} in {b:?}");
        let bad = glyphs_out(vcx, alert);
        assert!(bad.is_empty(), "{width}: {bad:?}");
        let title = bounds(vcx, "alert-title");
        assert!(
            title.size.height > px(40.),
            "{width}: the title wraps: {title:?}"
        );
    }
}

/// Delivers one display frame and says how many things asked for it.
fn frame(vcx: &mut VisualTestContext) -> usize {
    vcx.executor().advance_clock(Duration::from_millis(16));
    let asked = vcx.update(|window, cx| window.simulate_next_frame(cx));
    vcx.run_until_parked();
    asked
}

#[gpui::test]
fn nothing_redraws_while_idle(cx: &mut TestAppContext) {
    let (draws, vcx) = open(cx, 300., || alert(true).into_any_element());
    frame(vcx);
    let settled = draws.get();
    for _ in 0..30 {
        assert_eq!(frame(vcx), 0, "asked for a frame while idle");
    }
    assert_eq!(draws.get(), settled);
}
