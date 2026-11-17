//! A code editor's marks: the line the server said is wrong (a band, its number and its message,
//! gone once the text changes) and the text's colours (worked out again as it's typed, a host's
//! bad ranges passed over, and YAML with the `highlight` feature). Laid out with Core Text.

#![cfg(target_os = "macos")]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    AppContext as _, Context, Entity, HighlightStyle, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Styled as _, TestAppContext, TestDispatcher, VisualTestContext,
    Window, div, px,
};
use gpui_core::text::HighlightFn;
use gpui_core::{ActiveTheme as _, Input, InputState, Root};

const RED: Hsla = Hsla {
    h: 0.,
    s: 0.9,
    l: 0.5,
    a: 1.,
};

struct Form {
    code: Entity<InputState>,
}

impl Render for Form {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().p(px(10.)).child(
            div()
                .w(px(420.))
                .h(px(200.))
                .debug_selector(|| "code".into())
                .child(Input::new(&self.code).size_full()),
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

/// Every word before a colon, in red: a host's own highlighter.
fn keys_in_red() -> HighlightFn {
    Arc::new(|_: &str, code: &str| {
        let mut out = Vec::new();
        let mut start = 0;
        for line in code.split_inclusive('\n') {
            let indent = line.len() - line.trim_start().len();
            if let Some(colon) = line.find(':')
                && colon > indent
            {
                out.push((
                    start + indent..start + colon,
                    HighlightStyle {
                        color: Some(RED),
                        ..Default::default()
                    },
                ));
            }
            start += line.len();
        }
        out
    })
}

fn open<'a>(
    cx: &'a mut TestAppContext,
    text: &str,
    highlight: Option<HighlightFn>,
) -> (Entity<InputState>, &'a mut VisualTestContext) {
    cx.update(gpui_core::init);
    let text = text.to_string();
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let code = cx.new(|cx| {
            let s = InputState::code(window, cx).default_value(text);
            match highlight {
                Some(f) => s.highlight(f, "yaml"),
                None => s,
            }
        });
        *m.borrow_mut() = Some(code.clone());
        let form = cx.new(|_| Form { code });
        Root::new(form, window, cx)
    });
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let code = made.borrow().clone().unwrap();
    vcx.update(|window, cx| code.update(cx, |f, cx| f.focus(window, cx)));
    vcx.run_until_parked();
    (code, vcx)
}

struct Drawn {
    /// Glyphs: their left, top and colour.
    glyphs: Vec<(f32, f32, Hsla)>,
    /// Filled quads: their top, height and colour.
    fills: Vec<(f32, f32, Hsla)>,
}

fn drawn(vcx: &mut VisualTestContext) -> Drawn {
    vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let k = window.scale_factor();
        let scene = window.rendered_scene();
        Drawn {
            glyphs: scene
                .monochrome_sprites
                .iter()
                .map(|g| (g.bounds.origin.x.0 / k, g.bounds.origin.y.0 / k, g.color))
                .collect(),
            fills: scene
                .quads
                .iter()
                .filter_map(|q| {
                    Some((
                        q.bounds.origin.y.0 / k,
                        q.bounds.size.height.0 / k,
                        q.background.as_solid()?,
                    ))
                })
                .collect(),
        }
    })
}

fn near(a: Hsla, b: Hsla) -> bool {
    (a.h - b.h).abs() < 0.02 && (a.s - b.s).abs() < 0.1 && (a.l - b.l).abs() < 0.1
}

#[test]
fn the_wrong_line_has_a_band_its_number_and_its_message_until_the_text_changes() {
    let mut cx = app();
    let (code, vcx) = open(
        &mut cx,
        "rules:\n  - name: [unclosed\n    action: allow",
        None,
    );
    let destructive = vcx.update(|_, cx| cx.theme().destructive);
    let before = drawn(vcx);
    vcx.update(|_, cx| {
        code.update(cx, |c, cx| {
            c.set_error_line(Some((1, "expected ] before the line's end".into())), cx)
        })
    });
    vcx.run_until_parked();
    let after = drawn(vcx);
    // A band on the second line.
    let band = after
        .fills
        .iter()
        .find(|(_, _, c)| near(*c, destructive) && c.a < 0.5)
        .copied()
        .expect("a band");
    let second_row = before
        .glyphs
        .iter()
        .map(|g| g.1)
        .filter(|y| *y > band.0 - 2. && *y < band.0 + band.1)
        .count();
    assert!(second_row > 0, "the band is on a row of text: {band:?}");
    // The message (and the line's number) in the error colour.
    let red: Vec<_> = after
        .glyphs
        .iter()
        .filter(|g| near(g.2, destructive))
        .collect();
    assert!(red.len() > 20, "the message is drawn: {} glyphs", red.len());
    assert_eq!(
        code.read_with(vcx, |c, _| c.error_line()),
        Some((1, "expected ] before the line's end".into()))
    );
    // Typing changes the text: the mark goes.
    vcx.simulate_input("x");
    let typed = drawn(vcx);
    assert!(
        !typed
            .fills
            .iter()
            .any(|(_, _, c)| near(*c, destructive) && c.a < 0.5)
    );
    assert!(!typed.glyphs.iter().any(|g| near(g.2, destructive)));
    assert_eq!(code.read_with(vcx, |c, _| c.error_line()), None);
}

#[test]
fn the_colours_follow_the_text_as_its_typed() {
    let mut cx = app();
    let (_, vcx) = open(&mut cx, "rules:\n  - name: a", Some(keys_in_red()));
    let red =
        |vcx: &mut VisualTestContext| drawn(vcx).glyphs.iter().filter(|g| near(g.2, RED)).count();
    // "rules" and "- name".
    let first = red(vcx);
    assert!(first >= 9, "{first}");
    vcx.simulate_keystrokes("cmd-down enter");
    vcx.simulate_input("action: allow");
    // Worked out again for the new text: "action" too.
    assert_eq!(red(vcx), first + 6);
}

#[test]
fn a_hosts_bad_ranges_are_passed_over() {
    let mut cx = app();
    #[allow(clippy::reversed_empty_ranges)]
    let bad: HighlightFn = Arc::new(|_: &str, code: &str| {
        let style = HighlightStyle {
            color: Some(RED),
            ..Default::default()
        };
        vec![
            (0..3, style),
            (1..2, style),                       // overlapping the one before
            (code.len()..code.len() + 5, style), // past the end
            (5..4, style),                       // backwards
            (7..8, style),                       // inside "é", off a character's edge
        ]
    });
    let (code, vcx) = open(&mut cx, "abc d é f", Some(bad));
    let red = drawn(vcx).glyphs.iter().filter(|g| near(g.2, RED)).count();
    assert_eq!(red, 3, "just the first range's");
    vcx.simulate_input("!");
    assert_eq!(
        code.read_with(vcx, |c, _| c.value().to_string()),
        "!abc d é f"
    );
}

#[cfg(feature = "highlight")]
#[test]
fn yaml_is_coloured_by_the_highlight_feature() {
    let mut cx = app();
    let theme = cx.update(|cx| cx.theme().clone());
    let f = gpui_core::highlight::Highlighter::new()
        .into_fn(gpui_core::highlight::SyntaxTheme::from_theme(&theme));
    let (_, vcx) = open(
        &mut cx,
        "rules:\n  - name: \"Read the shared folder\"\n    allow: true\n",
        Some(f),
    );
    let colours: Vec<Hsla> = drawn(vcx).glyphs.iter().map(|g| g.2).collect();
    let mut kinds = colours.clone();
    kinds.dedup_by(|a, b| near(*a, *b));
    let distinct = colours.iter().fold(Vec::<Hsla>::new(), |mut seen, c| {
        if !seen.iter().any(|s| near(*s, *c)) {
            seen.push(*c);
        }
        seen
    });
    assert!(
        distinct.len() >= 3,
        "keys, strings and the rest: {distinct:?}"
    );
}
