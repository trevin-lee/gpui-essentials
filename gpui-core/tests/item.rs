//! Item: with on_click it's pressed by a click, Enter or Space (not when disabled, and not by a
//! press on its own action); its title is cut short on one line and its description after two
//! (laid out with Core Text, as on screen), even in a narrow box; the variants draw their
//! border and fill; small tightens it and touch makes it 44 points tall; a group's separator
//! is a rule; and nothing redraws while it sits there.

#![cfg(target_os = "macos")]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    AnyElement, AppContext as _, Bounds, Context, Hsla, InteractiveElement as _, IntoElement,
    KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, ParentElement as _, Pixels, Render,
    Styled as _, TestAppContext, TestDispatcher, VisualTestContext, Window, div, px,
};
use gpui_core::{
    Button, ButtonVariants as _, Disableable as _, IconName, Item, ItemGroup, ItemSeparator, Root,
    Sizable as _, Theme,
};

const LONG: &str = "Pneumonoultramicroscopicsilicovolcanoconiosis-Antidisestablishmentarianism \
                    and a description that runs on for a good while longer than two lines would \
                    hold in any reasonable box 職務経歴書と志望動機を来週の月曜日までに提出してください";

type Make = Rc<dyn Fn(Rc<RefCell<Vec<&'static str>>>) -> AnyElement>;

struct Host {
    width: f32,
    make: Make,
    pressed: Rc<RefCell<Vec<&'static str>>>,
    draws: Rc<Cell<usize>>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        div().p(px(20.)).child(
            div()
                .w(px(self.width))
                .debug_selector(|| "box".into())
                .child((self.make)(self.pressed.clone())),
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

struct Opened<'a> {
    pressed: Rc<RefCell<Vec<&'static str>>>,
    draws: Rc<Cell<usize>>,
    vcx: &'a mut VisualTestContext,
}

fn open(
    cx: &mut TestAppContext,
    width: f32,
    make: impl Fn(Rc<RefCell<Vec<&'static str>>>) -> AnyElement + 'static,
) -> Opened<'_> {
    cx.update(gpui_core::init);
    let pressed = Rc::new(RefCell::new(Vec::new()));
    let draws = Rc::new(Cell::new(0));
    let (p, d) = (pressed.clone(), draws.clone());
    let make: Make = Rc::new(make);
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let host = cx.new(|_| Host {
            width,
            make,
            pressed: p,
            draws: d,
        });
        Root::new(host, window, cx)
    });
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    Opened {
        pressed,
        draws,
        vcx,
    }
}

fn bounds(vcx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    vcx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} drawn"))
}

/// A pressable item with an action of its own.
fn pressable(pressed: Rc<RefCell<Vec<&'static str>>>, disabled: bool) -> AnyElement {
    let p = pressed.clone();
    Item::new("ana")
        .outline()
        .icon(IconName::User)
        .title("Ana Ruiz")
        .description("Asked to move Thursday's review.")
        .action(
            div().debug_selector(|| "reply".into()).child(
                Button::new("reply")
                    .label("Reply")
                    .outline()
                    .small()
                    .on_click(move |_, _, _| p.borrow_mut().push("reply")),
            ),
        )
        .disabled(disabled)
        .on_click(move |_, _, _| pressed.borrow_mut().push("item"))
        .into_any_element()
}

fn click(vcx: &mut VisualTestContext, selector: &'static str) {
    let at = bounds(vcx, selector).center();
    vcx.simulate_click(at, Modifiers::none());
    vcx.run_until_parked();
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

#[test]
fn a_press_or_a_key_presses_it_but_not_its_action() {
    let mut cx = app();
    let o = open(&mut cx, 400., |p| pressable(p, false));
    click(o.vcx, "item-title");
    assert_eq!(*o.pressed.borrow(), ["item"]);
    click(o.vcx, "reply");
    assert_eq!(*o.pressed.borrow(), ["item", "reply"], "the action's own");
    keys(o.vcx, "tab enter");
    keys(o.vcx, "space");
    assert_eq!(*o.pressed.borrow(), ["item", "reply", "item", "item"]);
}

#[test]
fn disabled_it_ignores_presses() {
    let mut cx = app();
    let o = open(&mut cx, 400., |p| pressable(p, true));
    click(o.vcx, "item-title");
    keys(o.vcx, "tab enter");
    assert!(!o.pressed.borrow().contains(&"item"));
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
fn the_title_is_cut_short_and_the_description_after_two_lines() {
    for width in [80., 200., 400.] {
        let mut cx = app();
        let o = open(&mut cx, width, |_| {
            Item::new("i")
                .icon(IconName::User)
                .title(LONG)
                .description(LONG)
                .into_any_element()
        });
        let item = bounds(o.vcx, "item");
        let b = bounds(o.vcx, "box");
        assert!(item.right() <= b.right() + px(0.5), "{width}: {item:?}");
        let bad = glyphs_out(o.vcx, item);
        assert!(bad.is_empty(), "{width}: {bad:?}");
        // 14 pixel text (15 pixel rems): a 19.25 pixel line for the title, 21 for the
        // description.
        let title = bounds(o.vcx, "item-title");
        assert!(title.size.height < px(21.), "{width}: one line: {title:?}");
        let description = bounds(o.vcx, "item-description");
        assert!(
            description.size.height > px(30.) && description.size.height < px(43.),
            "{width}: two lines: {description:?}"
        );
    }
}

/// The fills and border colours drawn.
fn paint(vcx: &mut VisualTestContext) -> (Vec<Hsla>, Vec<Hsla>) {
    vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let scene = window.rendered_scene();
        (
            scene
                .quads
                .iter()
                .filter_map(|q| q.background.as_solid())
                .collect(),
            scene
                .quads
                .iter()
                .filter(|q| q.border_widths.left.0 > 0.)
                .map(|q| q.border_color)
                .collect(),
        )
    })
}

#[test]
fn the_variants_draw_their_border_and_fill() {
    let t = Theme::dark().colors;
    let mut cx = app();
    let o = open(&mut cx, 300., |_| {
        Item::new("i").outline().title("Outline").into_any_element()
    });
    let (_, borders) = paint(o.vcx);
    assert!(borders.contains(&t.border), "{borders:?}");

    let mut cx = app();
    let o = open(&mut cx, 300., |_| {
        Item::new("i").muted().title("Muted").into_any_element()
    });
    let (fills, borders) = paint(o.vcx);
    assert!(fills.contains(&t.muted.opacity(0.5)), "{fills:?}");
    assert!(!borders.contains(&t.border));
}

#[test]
fn small_is_tighter_and_touch_is_44_points_tall() {
    let height = |make: fn() -> Item| {
        let mut cx = app();
        let o = open(&mut cx, 300., move |_| {
            make()
                .title("Title")
                .on_click(|_, _, _| {})
                .into_any_element()
        });
        bounds(o.vcx, "item").size.height
    };
    let (medium, small) = (height(|| Item::new("i")), height(|| Item::new("i").small()));
    assert!(small < medium - px(6.), "{small:?} {medium:?}");
    // At half zoom, where the default item is shorter than a finger.
    let mut cx = app();
    cx.update(|cx| {
        let mut t = Theme::dark();
        t.font_size = px(7.5);
        Theme::set(t, cx);
    });
    let o = open(&mut cx, 300., |_| {
        Item::new("i")
            .small()
            .touch()
            .title("Title")
            .on_click(|_, _, _| {})
            .into_any_element()
    });
    // Touch is the default size at least 44 points tall.
    assert!(bounds(o.vcx, "item").size.height >= px(44.));
}

#[test]
fn a_group_separates_its_items_with_a_rule() {
    let mut cx = app();
    let o = open(&mut cx, 300., |_| {
        ItemGroup::new("g")
            .child(Item::new("a").title("A"))
            .child(ItemSeparator::new())
            .child(Item::new("b").title("B"))
            .into_any_element()
    });
    let rule = bounds(o.vcx, "item-separator");
    let b = bounds(o.vcx, "box");
    assert_eq!(rule.size.height, px(1.));
    assert_eq!(rule.size.width, b.size.width);
}

/// Delivers one display frame and says how many things asked for it.
fn frame(vcx: &mut VisualTestContext) -> usize {
    vcx.executor().advance_clock(Duration::from_millis(16));
    let asked = vcx.update(|window, cx| window.simulate_next_frame(cx));
    vcx.run_until_parked();
    asked
}

#[test]
fn nothing_redraws_while_idle() {
    let mut cx = app();
    let o = open(&mut cx, 300., |p| pressable(p, false));
    click(o.vcx, "item-title");
    frame(o.vcx);
    let settled = o.draws.get();
    for _ in 0..30 {
        assert_eq!(frame(o.vcx), 0, "asked for a frame while idle");
    }
    assert_eq!(o.draws.get(), settled);
}
