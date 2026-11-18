//! Cards driven by a finger: on macOS with the touch settings forced (`CardTheme::with_touch`),
//! and with real touch events. GPUI recognizes taps and pans from touches itself, the same on
//! every platform, so these go through what an iPhone's touches go through: a tap becomes a
//! mouse press and release, a pan becomes scroll events locked to the way it started.

#![cfg(all(target_os = "macos", feature = "gpui"))]

mod common;

use std::rc::Rc;
use std::time::Duration;

use common::*;
use gpui::*;
use gpui_agents::CardTheme;
use serde_json::json;
// The built-in #[test] (which GPUI's test attribute expands to), not GPUI's, which
// gpui::* brings in.
use std::prelude::v1::test;

/// A test host whose theme is for touch, or for a mouse.
fn host(touch: bool) -> Rc<TestHost> {
    let mut host = TestHost::new(Images::Refuse);
    let theme = host.theme.clone().with_touch(touch);
    Rc::get_mut(&mut host).expect("a new host").theme = theme;
    host
}

fn bounds(cx: &mut VisualTestContext, what: &str) -> Bounds<Pixels> {
    let sel: &'static str = Box::leak(format!("t:{what}").into_boxed_str());
    cx.debug_bounds(sel)
        .unwrap_or_else(|| panic!("{what} isn't drawn"))
}

fn touch(cx: &mut VisualTestContext, phase: TouchPhase, at: Point<Pixels>) {
    cx.simulate_event(TouchEvent {
        id: TouchId(1),
        phase,
        position: at,
        predicted_position: None,
        force: None,
    });
}

/// A finger down and up again at `at`.
fn tap(cx: &mut VisualTestContext, at: Point<Pixels>) {
    touch(cx, TouchPhase::Started, at);
    touch(cx, TouchPhase::Ended, at);
    draw(cx);
}

/// A tap in the middle of what's marked `what`.
fn tap_on(cx: &mut VisualTestContext, what: &str) {
    let at = bounds(cx, what).center();
    tap(cx, at);
}

/// A finger dragged `by` from `from` in steps a frame apart, held still, then lifted (so it
/// doesn't fling).
fn pan(cx: &mut VisualTestContext, from: Point<Pixels>, by: Point<Pixels>) {
    touch(cx, TouchPhase::Started, from);
    let steps = 6;
    for i in 1..=steps {
        cx.executor().advance_clock(Duration::from_millis(16));
        touch(cx, TouchPhase::Moved, from + by * (i as f32 / steps as f32));
    }
    cx.executor().advance_clock(Duration::from_millis(200));
    touch(cx, TouchPhase::Ended, from + by);
    draw(cx);
}

/// Just outside a control's drawn box, still inside a `min_hit` square centred on it: beside it
/// when it's narrower than that, else above it.
fn beside(b: Bounds<Pixels>) -> Point<Pixels> {
    let side = px(CardTheme::TOUCH_HIT);
    if b.size.width < side - px(4.) {
        point(
            b.right() + (side - b.size.width) / 2. - px(2.),
            b.center().y,
        )
    } else {
        assert!(
            b.size.height < side - px(4.),
            "the control is {:?}, no smaller than a touch target",
            b.size
        );
        point(b.center().x, b.top() - (side - b.size.height) / 2. + px(2.))
    }
}

#[gpui::test]
fn a_small_control_takes_a_tap_beside_it_when_the_theme_is_for_touch(cx: &mut TestAppContext) {
    let host = host(true);
    let (_, cx) = open(cx, &sample("steps-carousel-separator"), &host, Some(400.));
    // The carousel's arrows are 28 points square.
    let next = bounds(cx, "r.c2:next");
    assert!(next.size.width < px(40.), "{:?}", next.size);
    tap(cx, beside(next));
    assert!(drawn(cx, "r.c2:slide1"), "a tap beside the arrow turns it");
    let previous = bounds(cx, "r.c2:previous");
    tap(cx, beside(previous));
    assert!(drawn(cx, "r.c2:slide0"));

    // A small icon button: a tap beside it presses it, once.
    let host = self::host(true);
    let (_, cx) = open(cx, &sample("buttons"), &host, Some(400.));
    let at = beside(bounds(cx, "button:Refresh"));
    tap(cx, at);
    assert_eq!(host.sent.borrow().len(), 1, "{:?}", host.sent.borrow());
    assert!(host.sent.borrow()[0].starts_with("Refresh it"));
    // A tap on the button itself still presses it, once.
    let at = bounds(cx, "button:Refresh").center();
    tap(cx, at);
    assert_eq!(host.sent.borrow().len(), 2, "{:?}", host.sent.borrow());
}

#[gpui::test]
fn with_a_mouse_a_control_takes_presses_only_where_it_is_drawn(cx: &mut TestAppContext) {
    let host = host(false);
    let (_, cx) = open(cx, &sample("steps-carousel-separator"), &host, Some(400.));
    let next = bounds(cx, "r.c2:next");
    cx.simulate_click(beside(next), Modifiers::none());
    draw(cx);
    assert!(
        drawn(cx, "r.c2:slide0"),
        "a click beside the arrow does nothing"
    );
    cx.simulate_click(next.center(), Modifiers::none());
    draw(cx);
    assert!(drawn(cx, "r.c2:slide1"));

    let host = self::host(false);
    let (_, cx) = open(cx, &sample("buttons"), &host, Some(400.));
    let at = beside(bounds(cx, "button:Refresh"));
    cx.simulate_click(at, Modifiers::none());
    draw(cx);
    assert!(host.sent.borrow().is_empty(), "{:?}", host.sent.borrow());
}

/// A key pressed and let go.
fn key(cx: &mut VisualTestContext, key: &'static str) {
    let keystroke = Keystroke::parse(key).unwrap();
    cx.simulate_event(KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(KeyUpEvent { keystroke });
    draw(cx);
}

#[gpui::test]
fn a_tap_on_a_tab_an_arrow_a_chip_or_copy_leaves_the_focus_in_a_text_field(
    cx: &mut TestAppContext,
) {
    let host = host(true);
    let program = r#"root = Stack([q, tabs, tools, carousel])
q = Input("q", "Search")
tabs = Tabs([TabItem("a", "One", [TextContent("First")]), TabItem("b", "Two", [CodeBlock("bash", "ls -la")])])
tools = Chips("tools", "multiple", [ChipItem("figma", "Figma"), ChipItem("slack", "Slack")])
carousel = Carousel([[TextContent("Slide one")], [TextContent("Slide two")]])"#;
    let (card, cx) = open(cx, program, &host, Some(400.));
    let field = cx
        .debug_bounds("field:t:r.c0.field")
        .expect("the text field");
    tap(cx, field.center());
    // Each one does what it does, and the field keeps the focus (on a phone, the keyboard
    // stays up).
    tap_on(cx, "tab:Two");
    tap_on(cx, "copy:r.c1.s1.0");
    assert_eq!(
        cx.read_from_clipboard().and_then(|c| c.text()).as_deref(),
        Some("ls -la")
    );
    tap_on(cx, "chip:Figma");
    assert_eq!(
        card.read_with(cx, |c, _| c.store().field(None, "tools")),
        Some(json!(["figma"]))
    );
    tap_on(cx, "r.c3:next");
    assert!(drawn(cx, "r.c3:slide1"));
    cx.simulate_input("still here");
    draw(cx);
    assert_eq!(
        card.read_with(cx, |c, _| c.store().field(None, "q")),
        Some(json!("still here")),
        "the typing went to the field"
    );
    // The keyboard still reaches them: Tab from the field to the first tab, and Enter picks it.
    key(cx, "tab");
    key(cx, "enter");
    assert!(
        !drawn(cx, "copy:r.c1.s1.0"),
        "the first tab is chosen again"
    );
}

#[gpui::test]
fn a_tap_at_the_edge_of_a_select_option_picks_that_option(cx: &mut TestAppContext) {
    let host = host(true);
    let (card, cx) = open(cx, &sample("form-contact"), &host, Some(400.));
    tap_on(cx, "select:r.c1/f2");
    let first = bounds(cx, "pick:United States");
    assert!(first.size.height < px(CardTheme::TOUCH_HIT));
    tap(cx, point(first.center().x, first.bottom() - px(2.)));
    assert_eq!(
        card.read_with(cx, |c, _| c.store().field(Some("contact"), "country")),
        Some(json!("us")),
        "not the option under it"
    );
}

#[gpui::test]
fn touch_targets_change_nothing_drawn(cx: &mut TestAppContext) {
    let marks = [
        "r.c2:next",
        "r.c2:previous",
        "r.c2:slide0",
        "card",
        "button:Refresh",
        "button:Open docs",
    ];
    let mut seen: Vec<Vec<Option<Bounds<Pixels>>>> = Vec::new();
    for touch in [false, true] {
        let mut found = Vec::new();
        for program in ["steps-carousel-separator", "buttons"] {
            let host = host(touch);
            let (_, cx) = open(cx, &sample(program), &host, Some(400.));
            for m in marks {
                let sel: &'static str = Box::leak(format!("t:{m}").into_boxed_str());
                found.push(cx.debug_bounds(sel));
            }
        }
        seen.push(found);
    }
    assert!(seen[0].iter().flatten().count() >= marks.len());
    assert_eq!(
        seen[0], seen[1],
        "the same boxes, with touch targets or without"
    );
}

#[gpui::test]
fn a_swipe_turns_a_carousel_and_a_short_or_upward_one_does_not(cx: &mut TestAppContext) {
    let host = host(true);
    let (_, cx) = open(cx, &sample("steps-carousel-separator"), &host, Some(400.));
    let at = bounds(cx, "r.c2:slide0").center();
    pan(cx, at, point(px(-80.), px(0.)));
    assert!(drawn(cx, "r.c2:slide1"), "a swipe left turns to the next");
    pan(cx, at, point(px(80.), px(0.)));
    assert!(drawn(cx, "r.c2:slide0"), "a swipe right turns back");
    pan(cx, at, point(px(80.), px(0.)));
    assert!(drawn(cx, "r.c2:slide0"), "there's nothing before the first");
    pan(cx, at, point(px(-20.), px(0.)));
    assert!(drawn(cx, "r.c2:slide0"), "a short swipe eases back");
    pan(cx, at, point(px(0.), px(-80.)));
    assert!(drawn(cx, "r.c2:slide0"), "up and down is the chat's");
    run_frames(cx);
    pan(cx, at, point(px(-80.), px(0.)));
    pan(cx, at, point(px(-80.), px(0.)));
    assert!(drawn(cx, "r.c2:slide2"));
}

#[gpui::test]
fn a_wide_table_scrolls_sideways_only_with_a_sideways_pan(cx: &mut TestAppContext) {
    let host = host(true);
    let (_, cx) = open(cx, &sample("editable-table"), &host, Some(280.));
    let start = bounds(cx, "cell:0:0");
    pan(cx, start.center(), point(px(0.), px(-60.)));
    assert_eq!(
        bounds(cx, "cell:0:0").origin.x,
        start.origin.x,
        "an upward pan leaves the table where it was"
    );
    pan(cx, start.center(), point(px(-60.), px(0.)));
    assert!(
        bounds(cx, "cell:0:0").origin.x < start.origin.x,
        "a sideways pan scrolls it"
    );
}

#[gpui::test]
fn long_code_scrolls_sideways_with_a_pan(cx: &mut TestAppContext) {
    let host = host(true);
    let line = "x".repeat(120);
    let program = format!("root = Stack([CodeBlock(\"bash\", \"echo {line}\")])");
    let (_, cx) = open(cx, &program, &host, Some(280.));
    let code = bounds(cx, "code:r.c0");
    assert!(
        code.size.width > px(400.),
        "the code is as wide as its line"
    );
    let inside = point(code.left() + px(40.), code.center().y);
    pan(cx, inside, point(px(0.), px(-40.)));
    assert_eq!(bounds(cx, "code:r.c0").origin.x, code.origin.x);
    pan(cx, inside, point(px(-60.), px(0.)));
    assert_eq!(bounds(cx, "code:r.c0").origin.x, code.origin.x - px(60.));
}

/// Frames until the card stops moving.
fn run_frames(cx: &mut VisualTestContext) -> usize {
    let mut n = 0;
    for _ in 0..60 {
        cx.executor().advance_clock(Duration::from_millis(16));
        draw(cx);
        n += 1;
    }
    n
}
