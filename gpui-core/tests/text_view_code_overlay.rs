//! TextView::code_block_overlay: the host's element over each fenced code block (in a list or a
//! quote too), told the block's source, place, language and code; built only for blocks in
//! view; and a press its button stops doesn't start a selection. Laid out with Core Text.

#![cfg(target_os = "macos")]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    AppContext as _, Context, InteractiveElement as _, IntoElement, Modifiers, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement as _, Render, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestAppContext, TestDispatcher,
    VisualTestContext, Window, div, point, px,
};
use gpui_core::text::CodeBlock;
use gpui_core::{Root, TextView};

type Seen = Rc<RefCell<Vec<CodeBlock>>>;

struct Doc {
    sources: Vec<(u64, SharedString)>,
    seen: Seen,
    pressed: Rc<RefCell<Vec<usize>>>,
    height: f32,
}

impl Render for Doc {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (seen, pressed) = (self.seen.clone(), self.pressed.clone());
        div().p(px(10.)).child(
            div()
                .id("scroller")
                .w(px(420.))
                .h(px(self.height))
                .overflow_y_scroll()
                .child(
                    div().debug_selector(|| "doc".into()).child(
                        TextView::blocks("doc", self.sources.clone())
                            .selectable(true)
                            .code_block_overlay(move |code, _, _| {
                                seen.borrow_mut().push(code.clone());
                                let pressed = pressed.clone();
                                let index = code.index;
                                let key = code.key;
                                div()
                                    .size_full()
                                    .flex()
                                    .justify_end()
                                    .child(
                                        div()
                                            .id(("copy", (key * 100) as usize + index))
                                            .debug_selector(move || format!("copy-{key}-{index}"))
                                            .size(px(24.))
                                            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                                cx.stop_propagation();
                                            })
                                            .on_click(move |_, _, _| {
                                                pressed.borrow_mut().push(index)
                                            }),
                                    )
                                    .into_any_element()
                            }),
                    ),
                ),
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
    seen: Seen,
    pressed: Rc<RefCell<Vec<usize>>>,
    vcx: &'a mut VisualTestContext,
}

fn open<'a>(cx: &'a mut TestAppContext, sources: &[(u64, &str)], height: f32) -> Opened<'a> {
    cx.update(gpui_core::init);
    let seen: Seen = Rc::default();
    let pressed: Rc<RefCell<Vec<usize>>> = Rc::default();
    let (s, p) = (seen.clone(), pressed.clone());
    let sources: Vec<(u64, SharedString)> = sources
        .iter()
        .map(|(k, t)| (*k, SharedString::from(*t)))
        .collect();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let doc = cx.new(|_| Doc {
            sources,
            seen: s,
            pressed: p,
            height,
        });
        Root::new(doc, window, cx)
    });
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    Opened { seen, pressed, vcx }
}

impl Opened<'_> {
    /// The blocks told to the overlay on one fresh draw.
    fn drawn(&mut self) -> Vec<CodeBlock> {
        self.seen.borrow_mut().clear();
        self.vcx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        let mut v = self.seen.borrow().clone();
        v.dedup();
        v
    }
}

const SOURCES: [(u64, &str); 2] = [
    (
        1,
        "Some text.\n\n```rust\nfn main() {}\n```\n\n- an item\n\n  ```\n  in a list\n  ```\n\n> ```sh\n> in a quote\n> ```\n",
    ),
    (2, "```\nsecond source\n```"),
];

#[test]
fn every_code_block_is_told_and_covered_in_lists_and_quotes_too() {
    let mut cx = app();
    let mut o = open(&mut cx, &SOURCES, 600.);
    let drawn = o.drawn();
    let blocks: Vec<(u64, usize, Option<&str>, &str)> = drawn
        .iter()
        .map(|c| (c.key, c.index, c.language.as_deref(), c.code.as_ref()))
        .collect();
    assert_eq!(
        blocks,
        [
            (1, 0, Some("rust"), "fn main() {}"),
            (1, 1, None, "in a list"),
            (1, 2, Some("sh"), "in a quote"),
            (2, 0, None, "second source"),
        ]
    );
    // Each overlay is on its block: the button at the block's top right, inside the view.
    let doc = o.vcx.debug_bounds("doc").unwrap();
    let mut tops = Vec::new();
    for sel in ["copy-1-0", "copy-1-1", "copy-1-2", "copy-2-0"] {
        let b = o.vcx.debug_bounds(sel).unwrap_or_else(|| panic!("{sel}"));
        assert!(
            b.right() <= doc.right() + px(1.) && b.top() >= doc.top(),
            "{sel} {b:?}"
        );
        tops.push(b.top());
    }
    assert!(
        tops.windows(2).all(|w| w[0] < w[1]),
        "in order down the page: {tops:?}"
    );
}

#[test]
fn only_the_blocks_in_view_are_built() {
    let mut cx = app();
    let long: String = (0..40)
        .map(|i| format!("```\nblock {i}\n```\n\n"))
        .collect();
    let mut o = open(&mut cx, &[(0, long.as_str())], 200.);
    let drawn = o.drawn();
    assert!(
        !drawn.is_empty() && drawn.len() < 10,
        "{} of 40 built",
        drawn.len()
    );
}

#[test]
fn a_press_the_button_stops_doesnt_start_a_selection() {
    let mut cx = app();
    let mut o = open(&mut cx, &SOURCES, 600.);
    let _ = o.drawn();
    let button = o.vcx.debug_bounds("copy-1-0").unwrap().center();
    let elsewhere = point(button.x - px(200.), button.y + px(30.));
    o.vcx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: button,
        modifiers: Modifiers::none(),
        click_count: 1,
        first_mouse: false,
    });
    o.vcx.simulate_event(MouseMoveEvent {
        position: elsewhere,
        pressed_button: Some(MouseButton::Left),
        modifiers: Modifiers::none(),
    });
    o.vcx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: elsewhere,
        modifiers: Modifiers::none(),
        click_count: 1,
    });
    o.vcx.run_until_parked();
    o.vcx
        .update(|_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string("x".into())));
    o.vcx.simulate_keystrokes("cmd-c");
    let copied = o
        .vcx
        .update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()))
        .unwrap_or_default();
    assert_eq!(copied, "x", "no selection was started");
    // A plain press on the button is its own.
    o.vcx.simulate_click(button, Modifiers::none());
    o.vcx.run_until_parked();
    assert_eq!(*o.pressed.borrow(), [0]);
}
