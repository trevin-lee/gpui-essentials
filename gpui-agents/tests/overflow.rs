//! No text runs out of its place, at any width. Every golden program, the same programs with the
//! worst text an agent could write in every label (a word with nowhere to break, a 300-character
//! address, Japanese, Arabic and emoji, very long numbers), and cards made of nothing but such
//! text, are laid out with Core Text and the bundled Inter (as on screen) at 280, 400, 720 and 736
//! wide. Any glyph or icon drawn past what clips it, past the card, or past the window fails the
//! test with where it is. What scrolls sideways (a wide table, code, an input's own text) clips
//! its glyphs on purpose and says so ([`CardHost::scroller`]), so that's left alone.

#![cfg(all(target_os = "macos", feature = "gpui"))]

mod common;

use std::rc::Rc;
use std::sync::Arc;

use ::gpui::TestDispatcher;
use common::*;
use gpui::*;
// The built-in #[test], not GPUI's, which gpui::* brings in.
use std::prelude::v1::test;

/// One word, with nowhere to break.
const WORD: &str = "Pneumonoultramicroscopicsilicovolcanoconiosis-Antidisestablishmentarianism-Floccinaucinihilipilification";

fn url() -> String {
    format!("https://example.com/apply/{}", "x".repeat(260))
}

fn words() -> String {
    format!(
        "職務経歴書と志望動機を来週の月曜日までに提出してください。يرجى إرسال خطة الدراسة المحدثة قبل التسجيل 🎓📚✅🚀 {WORD} {}",
        url()
    )
}

/// The program with the worst text added to every string that reads as words (one with a
/// capital or a space), leaving keywords ("row", "small"), names and addresses as they are.
fn worsened(src: &str) -> String {
    let worst = format!(" {WORD} 1234567890123456789012345678901234567890 日本語 العربية 🎓");
    let mut out = String::new();
    let mut chars = src.chars().peekable();
    while let Some(c) = chars.next() {
        out.push(c);
        if c != '"' {
            continue;
        }
        let mut s = String::new();
        let mut closed = false;
        while let Some(d) = chars.next() {
            if d == '\\' {
                s.push(d);
                if let Some(e) = chars.next() {
                    s.push(e);
                }
                continue;
            }
            if d == '"' {
                closed = true;
                break;
            }
            s.push(d);
        }
        let wordy = (s.contains(' ') || s.chars().any(|c| c.is_uppercase()))
            && !s.starts_with("http")
            && !s.starts_with('/');
        out.push_str(&s);
        if wordy {
            out.push_str(&worst);
        }
        if closed {
            out.push('"');
        }
    }
    out
}

/// Cards of nothing but the worst text, in every component that shows text.
fn worst_cards() -> Vec<(String, String)> {
    let w = format!("\"{}\"", words().replace('"', ""));
    let u = format!("\"{}\"", url());
    let n = "\"123456789012345678901234567890123456789012345\"";
    vec![
        (
            "worst-text".into(),
            format!(
                r#"root = Stack([CardHeader({w}, {u}), InlineHeader({w}, {w}), TextContent({w}), TextContent({w}, "large-heavy"), MarkDownRenderer({w}, "card"), Callout("info", {w}, {w}), TextCallout("danger", {w}, {w}), Label({w}), Separator(), CodeBlock("bash", {w}), Steps([StepsItem({w}, {w})])])"#
            ),
        ),
        (
            "worst-row".into(),
            format!(
                r#"root = Stack([Stack([TextContent({w}), TextContent({u}), Card([TextContent({n})])], "row"), Stack([Tag({w}), Tag({u}, null, "lg", "success"), Button({w})], "row", "s", "center", "between", true), Stack([Card([TextContent({w})], "sunk"), Card([TextContent({w})], "clear")], "row", "2xl")])"#
            ),
        ),
        (
            "worst-data".into(),
            format!(
                r#"root = Stack([TagBlock([{w}, {u}, {n}], "lg"), EntityList([{{left: {w}, right: {n}, rightVariant: "number"}}], "default", {{left: {w}, right: {w}}}, {{left: {w}, right: {n}}}), ListBlock([ListItem({w}, {w}, null, {w}, @ToAssistant("x")), ListItem({u})], "number"), Table([Col({w}, [{w}, {u}]), Col({n}, [{n}, 1], "number"), Col("C", ["a"]), Col("D", ["a"]), Col("E", ["a"]), Col("F", ["a"]), Col("G", ["a"]), Col("H", ["a"]), Col("I", ["a"]), Col("J", ["a"]), Col("K", ["a"]), Col("L", ["a"])])])"#
            ),
        ),
        (
            "worst-blocks".into(),
            format!(
                r#"root = Stack([SnippetCardBlock([SnippetCardItem("a", IconText(Icon("users"), "neutral", "m", {w}, {w}), BoldText("number", {n}, {w}, "metric"))], "grid", true, @ToAssistant("x")), OverviewCardBlock([OverviewCardItem("o", Text("text", {w}, {w}, "number", "lg"), MetricIndicatorInline({n}, {w}, {{direction: "up", value: 123456789}}))]), ContextCardBlock([ContextCardItem("c", {w}, {w}, "gray")]), VisualCardBlock([VisualCardItem(BoldText("text", {w}, {w}), "v")]), CompositeCardBlock([CompositeCardItem("p", IconText(Icon("box"), "neutral", "m", {w}, {w}, true), [MetricIndicatorWithStrikethrough({n}, {w}, {n}, {{direction: "down", value: 4}})], {{price: BoldText("text", {n}), button: Button({w})}})], "carousel"), ImageText("/x.png", {w}, {w}, {w}, true), ImageTextLarge("/x.png", {w}, {w}, {w})])"#
            ),
        ),
        (
            "worst-form".into(),
            format!(
                r#"root = Stack([Form("f", Buttons([Button({w}), Button({w}, @ToAssistant("x"), "secondary")]), [FormControl({w}, Input("a", {w}), {w}), FormControl({w}, TextArea("b", {w}, 3)), FormControl({w}, Select("c", [SelectItem("x", {w})], {w})), FormControl({w}, CheckBoxGroup("d", [CheckBoxItem({w}, {w}, "x")])), FormControl({w}, RadioGroup("e", [RadioItem({w}, {w}, "x")])), FormControl({w}, SwitchGroup("g", [SwitchItem({w}, {w}, "x")], "card")), FormControl({w}, Chips("h", "multiple", [ChipItem("x", {w})])), FormControl({w}, OptionCards("i", "single", [OptionCard("x", {w}, {w})])), FormControl({w}, Slider("j", "continuous", 0, 100, 1, [5], {w}))])])"#
            ),
        ),
        (
            "worst-charts".into(),
            format!(
                r#"root = Stack([BarChart([{w}, {u}, "c"], [Series({w}, [1, 1234567890123, 3])], "grouped", {w}, {w}), HorizontalBarChart([{w}, {u}], [Series({w}, [1, 2])]), LineChart([{w}, "b"], [Series({w}, [1, 2])]), RadarChart([{w}, {u}, "c"], [Series({w}, [1, 2, 3])]), PieChart([{w}, {u}], [123456789012345, 2], "donut"), RadialChart([{w}], [3]), SingleStackedBarChart([{w}, "b"], [1, 2]), ScatterChart([ScatterSeries({w}, [Point(1, 123456789012), Point(2, 3)])], {w}, {w})])"#
            ),
        ),
        (
            "worst-layout".into(),
            format!(
                r#"root = Stack([Tabs([TabItem("a", {w}, [TextContent({w})]), TabItem("b", {w}, [TextContent("b")])]), Accordion([AccordionItem("a", {w}, [TextContent({w})])]), Carousel([[TextContent({w})], [TextContent({u})]]), Buttons([Button({w}), Button({u})], "column"), IconButton({w}, Icon("settings"))])"#
            ),
        ),
    ]
}

/// A test app that lays text out as the screen does (GPUI's test platform otherwise measures
/// every string as nothing, so nothing could overflow).
fn app_with_text() -> TestAppContext {
    TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        Arc::new(gpui_apple::AppleTextSystem::new()),
    )
}

/// Every glyph or icon drawn past the left or right edge of what clips it, of the card or of
/// the window. What's inside a region that scrolls sideways is left alone.
fn overflows(cx: &mut VisualTestContext, host: &TestHost) -> Vec<String> {
    draw(cx);
    let marks: Vec<String> = host.scrollers.borrow().iter().cloned().collect();
    let fields: Vec<Bounds<Pixels>> = marks
        .into_iter()
        .filter_map(|k| cx.debug_bounds(Box::leak(k.into_boxed_str())))
        .collect();
    let card = cx.debug_bounds("t:card").expect("the card is drawn");
    cx.update(|window, _| {
        let scale = window.scale_factor();
        let width = f32::from(window.viewport_size().width) * scale;
        let (card_left, card_right) = (f32::from(card.origin.x) * scale, f32::from(card.right()) * scale);
        let scene = window.rendered_scene();
        // A glyph's tile has a pixel of padding either side.
        let slack = 2.0 * scale;
        let in_field = |m: &Bounds<ScaledPixels>| {
            fields.iter().any(|f| {
                let (x, w) = (f32::from(f.origin.x) * scale, f32::from(f.size.width) * scale);
                m.origin.x.as_f32() >= x - slack && m.origin.x.as_f32() + m.size.width.as_f32() <= x + w + slack
            })
        };
        let mut out = Vec::new();
        if std::env::var("OVERFLOW_DEBUG").is_ok() {
            eprintln!("sprites: {} mono, {} subpixel", scene.monochrome_sprites.len(), scene.subpixel_sprites.len());
        }
        let sprites = scene.monochrome_sprites.iter().map(|s| (&s.bounds, &s.content_mask, s.color)).chain(scene.subpixel_sprites.iter().map(|s| (&s.bounds, &s.content_mask, s.color)));
        for (b, m, color) in sprites {
            if in_field(&m.bounds) {
                continue;
            }
            let (left, right) = (b.origin.x.as_f32(), b.origin.x.as_f32() + b.size.width.as_f32());
            let (clip_left, clip_right) = (m.bounds.origin.x.as_f32(), m.bounds.origin.x.as_f32() + m.bounds.size.width.as_f32());
            if left < clip_left - slack || right > clip_right + slack || right > width + slack || left < card_left - slack || right > card_right + slack {
                out.push(format!(
                    "at x {:.0}–{:.0}, y {:.0} (clip x {:.0}–{:.0}, card {:.0}–{:.0}; lightness {:.2})",
                    left / scale,
                    right / scale,
                    b.origin.y.as_f32() / scale,
                    clip_left / scale,
                    clip_right / scale,
                    card_left / scale,
                    card_right / scale,
                    color.l
                ));
            }
        }
        out
    })
}

#[test]
fn every_card_keeps_its_text_in_its_place() {
    let mut programs = goldens();
    let worse: Vec<(String, String)> = programs
        .iter()
        .map(|(n, s)| (format!("{n} (worst text)"), worsened(s)))
        .collect();
    programs.extend(worse);
    programs.extend(worst_cards());
    let mut found = Vec::new();
    for width in [280., 400., 720., 736.] {
        let mut app = app_with_text();
        for (name, source) in &programs {
            let host = TestHost::new(Images::Ready);
            let (card, cx) = open(&mut app, source, &host, Some(width));
            // Every accordion open, so what's inside is looked at too.
            cx.update(|_, cx| card.update(cx, |c, cx| c.expand_all(cx)));
            for b in overflows(cx, &host) {
                found.push(format!("{width}: {name}: {b}"));
            }
        }
    }
    found.dedup();
    assert!(
        found.is_empty(),
        "{} glyphs past their place:\n{}",
        found.len(),
        found
            .iter()
            .take(60)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// A text that can't wrap, running out of a box too narrow for it and past the card, is found;
/// one that wraps isn't. (A glyph wholly outside what clips it isn't drawn at all, so text cut
/// off by a clip shows up only as the glyph across its edge; text that escapes everything is
/// what the card's and the window's edges catch.)
#[test]
fn the_check_finds_text_past_its_box() {
    // A card whose one Text is drawn without wrapping, through a theme-less wrapper: the
    // check must see past the card's edge.
    struct Boxed(bool, Entity<gpui_agents::CardView>);
    impl Render for Boxed {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let text = format!("Too long for this box: {WORD}");
            let inner = if self.0 {
                div().whitespace_nowrap().child(text)
            } else {
                div().min_w_0().child(text)
            };
            div()
                .w(px(300.))
                .font_family("Inter")
                .child(self.1.clone())
                .child(div().w(px(200.)).child(div().flex().child(inner)))
        }
    }
    for (nowrap, should_find) in [(true, true), (false, false)] {
        let mut app = app_with_text();
        setup(&mut app);
        let host = TestHost::new(Images::Refuse);
        let h: Rc<dyn gpui_agents::CardHost> = host.clone();
        let (_, cx) = app.add_window_view(move |window, cx| {
            let card = cx.new(|cx| {
                gpui_agents::CardView::new(
                    "root = Stack([TextContent(\"ok\")])",
                    "t",
                    h,
                    window,
                    cx,
                )
            });
            Boxed(nowrap, card)
        });
        let bad = overflows(cx, &host);
        assert_eq!(!bad.is_empty(), should_find, "nowrap {nowrap}: {bad:?}");
    }
}
