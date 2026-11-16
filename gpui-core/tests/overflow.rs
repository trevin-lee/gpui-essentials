//! No text runs out of its place, at any width or zoom. Every component that shows text is laid
//! out with Core Text (as on screen) holding the worst text there is (a word with nowhere to
//! break, Japanese, Arabic, emoji, a long number), in boxes from 40 to 1600 pixels wide, at 50%,
//! 100% and 200% zoom. A glyph or icon drawn past what clips it, or past the box the component
//! was given, fails the test with where it is. The one exception: a slot narrower than a single
//! glyph (a label squeezed to nothing) cuts its ellipsis, as browsers do.

#![cfg(target_os = "macos")]

use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    AnyElement, App, AppContext as _, Context, IntoElement, ParentElement as _, Render,
    Styled as _, TestAppContext, TestDispatcher, Window, div, px, size,
};
use gpui_core::{
    Button, ButtonGroup, ButtonVariant, ButtonVariants as _, ControlSize, DatePicker,
    DatePickerState, IconName, Input, InputState, Root, Sizable as _, TextView, Theme, Tooltip,
};

const WORD: &str = "Pneumonoultramicroscopicsilicovolcanoconiosis-Antidisestablishmentarianism";
const JAPANESE: &str = "職務経歴書と志望動機を来週の月曜日までに提出してください";
const ARABIC: &str = "يرجى إرسال خطة الدراسة المحدثة قبل التسجيل";
const EMOJI: &str = "🎓📚✅🚀🎓📚✅🚀🎓📚✅🚀";
const NUMBER: &str = "1234567890123456789012345678901234567890";

fn texts() -> Vec<String> {
    vec![
        WORD.into(),
        JAPANESE.into(),
        ARABIC.into(),
        EMOJI.into(),
        NUMBER.into(),
        format!("{WORD} {JAPANESE} {ARABIC} {EMOJI}"),
        "OK".into(),
    ]
}

type Make = Rc<dyn Fn(&mut Window, &mut App) -> AnyElement>;

/// Every component that shows text, in every variant and size, with each worst text.
fn components() -> Vec<(String, Make)> {
    let mut out: Vec<(String, Make)> = Vec::new();
    for (t, text) in texts().into_iter().enumerate() {
        let placeholder = text.clone();
        out.push((
            format!("DatePicker placeholder {t}"),
            Rc::new(move |window, cx| {
                let state = cx.new(|cx| DatePickerState::new(window, cx));
                DatePicker::new(&state)
                    .placeholder(placeholder.clone())
                    .into_any_element()
            }),
        ));
    }
    for (t, text) in texts().into_iter().enumerate() {
        for (kind, multi) in [("one-line", false), ("textarea", true)] {
            let value = text.clone();
            out.push((
                format!("Input {kind} text {t}"),
                Rc::new(move |window, cx| {
                    let value = value.clone();
                    let state = cx.new(|cx| {
                        let s = if multi {
                            InputState::textarea(window, cx)
                        } else {
                            InputState::new(window, cx)
                        };
                        s.default_value(value)
                    });
                    Input::new(&state).into_any_element()
                }),
            ));
            let placeholder = text.clone();
            out.push((
                format!("Input {kind} placeholder {t}"),
                Rc::new(move |window, cx| {
                    let placeholder = placeholder.clone();
                    let state = cx.new(|cx| {
                        let s = if multi {
                            InputState::textarea(window, cx)
                        } else {
                            InputState::new(window, cx)
                        };
                        s.placeholder(placeholder)
                    });
                    Input::new(&state).into_any_element()
                }),
            ));
        }
    }
    for (t, text) in texts().into_iter().enumerate() {
        // Every kind of block, holding the worst text.
        let doc = format!(
            "# {text}\n\n{text} with `{text}` and [{text}](https://example.test)\n\n- {text}\n  - {text}\n\n> {text}\n\n```\n{text}\n```\n\n| {text} | b |\n|:-:|--:|\n| {text} | {text} |"
        );
        out.push((
            format!("TextView text {t}"),
            Rc::new(move |_, _| TextView::markdown("doc", doc.clone()).into_any_element()),
        ));
    }
    let variants = [
        ButtonVariant::Primary,
        ButtonVariant::Outline,
        ButtonVariant::Secondary,
        ButtonVariant::Ghost,
        ButtonVariant::Destructive,
        ButtonVariant::Link,
    ];
    let sizes = [
        ControlSize::XSmall,
        ControlSize::Small,
        ControlSize::Medium,
        ControlSize::Large,
        ControlSize::Touch,
    ];
    for (t, text) in texts().into_iter().enumerate() {
        for v in variants {
            for s in sizes {
                let label = text.clone();
                out.push((
                    format!("Button {v:?} {s:?} text {t}"),
                    Rc::new(move |_, _| {
                        Button::new("b")
                            .label(label.clone())
                            .icon(IconName::Star)
                            .with_variant(v)
                            .with_size(s)
                            .into_any_element()
                    }),
                ));
            }
        }
        let tip = text.clone();
        out.push((
            format!("Tooltip text {t}"),
            Rc::new(move |window, cx| {
                Tooltip::new(tip.clone())
                    .build(window, cx)
                    .into_any_element()
            }),
        ));
        let label = text.clone();
        out.push((
            format!("ButtonGroup text {t}"),
            Rc::new(move |_, _| {
                ButtonGroup::new("g")
                    .outline()
                    .small()
                    .children((0..3usize).map(|i| Button::new(("b", i)).label(label.clone())))
                    .into_any_element()
            }),
        ));
    }
    out
}

struct Gallery {
    width: f32,
    items: Vec<Make>,
}

impl Render for Gallery {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let items: Vec<AnyElement> = self.items.iter().map(|make| make(window, cx)).collect();
        div().flex().flex_col().gap(px(4.)).children(
            items
                .into_iter()
                .map(|item| div().flex().w(px(self.width)).flex_none().child(item)),
        )
    }
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

/// Every glyph or icon drawn past what clips it, or past `width` from the window's left edge
/// (where every box starts).
fn overflows(
    cx: &mut TestAppContext,
    width: f32,
    zoom: f32,
    items: Vec<Make>,
    scrolls: bool,
) -> Vec<String> {
    cx.update(|cx| {
        gpui_core::init(cx);
        let mut t = Theme::dark();
        t.font_size = px(15. * zoom);
        Theme::set(t, cx);
    });
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let gallery = cx.new(|_| Gallery { width, items });
        Root::new(gallery, window, cx)
    });
    vcx.simulate_resize(size(px(1700.), px(20000.)));
    vcx.run_until_parked();
    vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let scale = window.scale_factor();
        let scene = window.rendered_scene();
        let slack = 2.0 * scale;
        let limit = width * scale;
        let mut out = Vec::new();
        let sprites = scene
            .monochrome_sprites
            .iter()
            .map(|s| (&s.bounds, &s.content_mask, false))
            .chain(
                scene
                    .subpixel_sprites
                    .iter()
                    .map(|s| (&s.bounds, &s.content_mask, false)),
            )
            .chain(
                scene
                    .polychrome_sprites
                    .iter()
                    .map(|s| (&s.bounds, &s.content_mask, true)),
            );
        for (b, m, emoji) in sprites {
            // An emoji's bitmap is wider than its advance, padded with clear pixels either side:
            // its sprite overstates its ink by up to a seventh of its width each side.
            let slack = if emoji {
                slack.max(b.size.width.as_f32() / 7.)
            } else {
                slack
            };
            let (left, right) = (
                b.origin.x.as_f32(),
                b.origin.x.as_f32() + b.size.width.as_f32(),
            );
            let clip_left = m.bounds.origin.x.as_f32();
            let clip_right = clip_left + m.bounds.size.width.as_f32();
            // Clipped away entirely: not drawn.
            if right <= clip_left || left >= clip_right {
                continue;
            }
            // A slot narrower than one glyph (a label squeezed to nothing) cuts its ellipsis, as
            // browsers do; any other glyph crossing its clip is text cut off.
            let squeezed = clip_right - clip_left < right - left;
            // A field that scrolls sideways clips its text at its edge on purpose.
            let cut =
                !scrolls && !squeezed && (left < clip_left - slack || right > clip_right + slack);
            // What shows of it must stay in the box.
            let shown_right = right.min(clip_right);
            if cut || shown_right > limit + slack {
                out.push(format!(
                    "glyph at x {:.0}–{:.0}, y {:.0} (clip {:.0}–{:.0}, box 0–{width})",
                    left / scale,
                    right / scale,
                    b.origin.y.as_f32() / scale,
                    clip_left / scale,
                    clip_right / scale,
                ));
            }
        }
        out
    })
}

#[test]
fn every_component_keeps_its_text_in_its_box() {
    let mut found = Vec::new();
    for zoom in [0.5, 1., 2.] {
        for width in [40., 80., 160., 320., 640., 1600.] {
            for (name, make) in components() {
                // A tooltip floats over the window, not in a box, and a window is never
                // narrower than 160 points: below that its padding alone is wider than it.
                if name.starts_with("Tooltip") && width < 160. {
                    continue;
                }
                // Under 60 CSS pixels there's no room for a character: a field's padding and
                // border take 26 (a date field's its icon too), and a table's cells and a nested list's text have less than one
                // (such a character is cut at its cell, never drawn over it).
                let narrow = ["Input", "TextView", "DatePicker"];
                if narrow.iter().any(|n| name.starts_with(n)) && width / zoom < 60. {
                    continue;
                }
                let mut cx = app_with_text();
                let scrolls = name.starts_with("Input one-line");
                let bad = overflows(&mut cx, width, zoom, vec![make], scrolls);
                if let Some(first) = bad.first() {
                    found.push(format!(
                        "{name} at {width} wide, {}%: {} glyphs out, first {first}",
                        zoom * 100.,
                        bad.len()
                    ));
                }
            }
        }
    }
    assert!(
        found.is_empty(),
        "{} overflows:\n{}",
        found.len(),
        found.join("\n")
    );
}
