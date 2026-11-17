//! Card: its header puts the action at the top right beside the title and description, it
//! draws only the parts it's given, it's painted in the theme's card colours, its text wraps
//! inside it in a narrow box (laid out with Core Text, as on screen), and nothing redraws
//! while it sits there.

#![cfg(target_os = "macos")]

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    AnyElement, AppContext as _, Bounds, Context, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Render, Styled as _, TestAppContext, TestDispatcher,
    VisualTestContext, Window, div, px,
};
use gpui_core::{Button, ButtonVariants as _, Card, Root, Sizable as _, Theme};

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

fn full_card() -> AnyElement {
    Card::new()
        .title("Team plan")
        .description("Five seats, billed monthly.")
        .action(
            div()
                .debug_selector(|| "edit".into())
                .child(Button::new("edit").label("Edit").outline().small()),
        )
        .child(div().debug_selector(|| "body".into()).child("Next charge"))
        .footer(Button::new("upgrade").label("Upgrade"))
        .into_any_element()
}

#[gpui::test]
fn the_action_sits_at_the_top_right_beside_the_title(cx: &mut TestAppContext) {
    let (_, vcx) = open(cx, 360., full_card);
    let card = bounds(vcx, "card");
    let title = bounds(vcx, "card-title");
    let description = bounds(vcx, "card-description");
    let action = bounds(vcx, "edit");
    // The card's 16 CSS pixels of padding (15 pixel rems: 16 pixels here) and its border.
    assert!(
        (card.right() - action.right() - px(17.)).abs() < px(1.),
        "{action:?} in {card:?}"
    );
    assert!(
        (action.top() - title.top()).abs() < px(1.),
        "{action:?} {title:?}"
    );
    assert!(title.right() <= action.left(), "{title:?} {action:?}");
    assert!(description.top() >= title.bottom(), "{description:?}");
    let body = bounds(vcx, "body");
    let footer = bounds(vcx, "card-footer");
    assert!(body.top() > description.bottom());
    assert!(footer.top() > body.bottom());
    // The footer runs to the card's edges and its bottom.
    assert!(
        (footer.bottom() - card.bottom()).abs() <= px(1.),
        "{footer:?}"
    );
    assert!((footer.left() - card.left()).abs() <= px(1.), "{footer:?}");
}

#[gpui::test]
fn it_draws_only_the_parts_it_is_given(cx: &mut TestAppContext) {
    let (_, vcx) = open(cx, 360., || {
        Card::new()
            .child(div().debug_selector(|| "body".into()).child("Only content"))
            .into_any_element()
    });
    assert!(vcx.debug_bounds("card-header").is_none());
    assert!(vcx.debug_bounds("card-footer").is_none());
    let card = bounds(vcx, "card");
    let body = bounds(vcx, "body");
    // Padded the same above and below.
    let above = body.top() - card.top();
    let below = card.bottom() - body.bottom();
    assert!((above - below).abs() < px(1.), "{above:?} {below:?}");
}

#[test]
fn small_tightens_the_spacing() {
    let gap = |small: bool| {
        let mut cx = app();
        let (_, vcx) = open(&mut cx, 360., move || {
            let card = Card::new()
                .title("Title")
                .child(div().debug_selector(|| "body".into()).child("Body"));
            if small { card.small() } else { card }.into_any_element()
        });
        let card = bounds(vcx, "card");
        let title = bounds(vcx, "card-title");
        title.top() - card.top()
    };
    let (default, small) = (gap(false), gap(true));
    assert!(
        (default - px(17.)).abs() < px(0.5),
        "16 and the border: {default:?}"
    );
    assert!(
        (small - px(13.)).abs() < px(0.5),
        "12 and the border: {small:?}"
    );
}

#[test]
fn it_is_painted_in_the_themes_card_colours() {
    let mut cx = app();
    let colors = Theme::dark().colors;
    let (_, vcx) = open(&mut cx, 360., full_card);
    let (fills, borders) = vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let scene = window.rendered_scene();
        let fills: Vec<Hsla> = scene
            .quads
            .iter()
            .filter_map(|q| q.background.as_solid())
            .collect();
        let borders: Vec<Hsla> = scene.quads.iter().map(|q| q.border_color).collect();
        (fills, borders)
    });
    assert!(fills.contains(&colors.card), "{fills:?}");
    assert!(fills.contains(&colors.muted.opacity(0.5)), "the footer");
    assert!(borders.contains(&colors.border), "{borders:?}");
}

/// Every glyph drawn outside `inside`, or cut off at what clips it (but for a slot narrower
/// than one glyph, which cuts its ellipsis).
fn glyphs_out(vcx: &mut VisualTestContext, inside: Bounds<Pixels>) -> Vec<String> {
    vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let k = window.scale_factor();
        let scene = window.rendered_scene();
        let slack = 2.;
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
                let slack = slack + pad;
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
fn long_text_wraps_inside_a_narrow_card() {
    for width in [60., 120., 240.] {
        let mut cx = app();
        let (_, vcx) = open(&mut cx, width, || {
            Card::new()
                .title(LONG)
                .description(LONG)
                .action(Button::new("edit").label("Edit").outline().small())
                .child(LONG)
                .footer(div().min_w_0().truncate().child(LONG))
                .into_any_element()
        });
        let card = bounds(vcx, "card");
        let b = bounds(vcx, "box");
        assert!(card.right() <= b.right() + px(0.5), "{card:?} in {b:?}");
        let bad = glyphs_out(vcx, card);
        assert!(bad.is_empty(), "{width}: {bad:?}");
        let title = bounds(vcx, "card-title");
        assert!(
            title.size.height > px(30.),
            "{width}: the title wraps onto more lines: {title:?}"
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
    let (draws, vcx) = open(cx, 360., full_card);
    frame(vcx);
    let settled = draws.get();
    for _ in 0..30 {
        assert_eq!(frame(vcx), 0, "asked for a frame while idle");
    }
    assert_eq!(draws.get(), settled);
}
