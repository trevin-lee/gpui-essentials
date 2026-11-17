//! TextView::blocks: several keyed Markdown sources drawn as the one document they make (the
//! same height and copied text as a `markdown` view of them one after another), links resolved
//! across them, a selection kept while only later sources change, and a trailing caret that's
//! drawn but never laid out. Laid out with Core Text, as on screen.

#![cfg(target_os = "macos")]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    AppContext as _, Context, Entity, Hsla, InteractiveElement as _, IntoElement, Modifiers,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement as _, Render,
    SharedString, Styled as _, TestAppContext, TestDispatcher, VisualTestContext, Window, div,
    point, px,
};
use gpui_core::{Root, TextView};

const RED: Hsla = Hsla {
    h: 0.,
    s: 0.9,
    l: 0.5,
    a: 1.,
};

struct Doc {
    sources: Vec<(u64, SharedString)>,
    /// Also draws them as one `markdown` view, below, to compare with.
    compare: bool,
    caret: Option<Hsla>,
    links: Rc<RefCell<Vec<String>>>,
}

impl Render for Doc {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let links = self.links.clone();
        let joined: String = self
            .sources
            .iter()
            .map(|(_, s)| s.as_ref())
            .collect::<Vec<_>>()
            .join("\n\n");
        div()
            .p(px(10.))
            .w(px(420.))
            .child(
                div().debug_selector(|| "blocks".into()).child(
                    TextView::blocks("blocks", self.sources.clone())
                        .selectable(true)
                        .trailing_caret(self.caret)
                        .on_link_click(move |url, _, _, _| {
                            links.borrow_mut().push(url.to_string())
                        }),
                ),
            )
            .when(self.compare, |d| {
                d.child(
                    div()
                        .debug_selector(|| "one".into())
                        .child(TextView::markdown("one", joined)),
                )
            })
    }
}

use gpui::prelude::FluentBuilder as _;

fn app() -> TestAppContext {
    TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        Arc::new(gpui_apple::AppleTextSystem::new()),
    )
}

struct Opened<'a> {
    doc: Entity<Doc>,
    links: Rc<RefCell<Vec<String>>>,
    vcx: &'a mut VisualTestContext,
}

fn open<'a>(cx: &'a mut TestAppContext, sources: &[(u64, &str)], compare: bool) -> Opened<'a> {
    cx.update(gpui_core::init);
    let links = Rc::new(RefCell::new(Vec::new()));
    let l = links.clone();
    let sources: Vec<(u64, SharedString)> = sources
        .iter()
        .map(|(k, s)| (*k, SharedString::from(*s)))
        .collect();
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let doc = cx.new(|_| Doc {
            sources,
            compare,
            caret: None,
            links: l,
        });
        *m.borrow_mut() = Some(doc.clone());
        Root::new(doc, window, cx)
    });
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let doc = made.borrow().clone().unwrap();
    Opened { doc, links, vcx }
}

impl Opened<'_> {
    fn set(&mut self, sources: &[(u64, &str)]) {
        let sources: Vec<(u64, SharedString)> = sources
            .iter()
            .map(|(k, s)| (*k, SharedString::from(*s)))
            .collect();
        self.vcx.update(|_, cx| {
            self.doc.update(cx, |d, cx| {
                d.sources = sources;
                cx.notify();
            })
        });
        self.vcx.run_until_parked();
    }

    fn caret(&mut self, caret: Option<Hsla>) {
        self.vcx.update(|_, cx| {
            self.doc.update(cx, |d, cx| {
                d.caret = caret;
                cx.notify();
            })
        });
        self.vcx.run_until_parked();
    }

    fn copied(&mut self) -> String {
        self.vcx.simulate_keystrokes("cmd-c");
        self.vcx
            .update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()))
            .unwrap_or_default()
    }

    fn drag(&mut self, from: gpui::Point<gpui::Pixels>, to: gpui::Point<gpui::Pixels>) {
        self.vcx.simulate_event(MouseDownEvent {
            button: MouseButton::Left,
            position: from,
            modifiers: Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        self.vcx.simulate_event(MouseMoveEvent {
            position: to,
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::none(),
        });
        self.vcx.simulate_event(MouseUpEvent {
            button: MouseButton::Left,
            position: to,
            modifiers: Modifiers::none(),
            click_count: 1,
        });
        self.vcx.run_until_parked();
    }

    /// The red quads drawn: the caret.
    fn carets(&mut self) -> Vec<gpui::Bounds<f32>> {
        self.vcx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
            let k = window.scale_factor();
            window
                .rendered_scene()
                .quads
                .iter()
                .filter(|q| q.background.as_solid() == Some(RED))
                .map(|q| {
                    gpui::Bounds::new(
                        gpui::point(q.bounds.origin.x.0 / k, q.bounds.origin.y.0 / k),
                        gpui::size(q.bounds.size.width.0 / k, q.bounds.size.height.0 / k),
                    )
                })
                .collect()
        })
    }

    /// The right edge of the rightmost glyph drawn between `top` and `bottom`.
    fn last_glyph_right(&mut self, top: f32, bottom: f32) -> f32 {
        self.vcx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
            let k = window.scale_factor();
            let scene = window.rendered_scene();
            scene
                .monochrome_sprites
                .iter()
                .map(|g| g.bounds)
                .chain(scene.subpixel_sprites.iter().map(|g| g.bounds))
                .filter(|g| {
                    let y = (g.origin.y.0 + g.size.height.0 / 2.) / k;
                    y >= top && y <= bottom
                })
                .map(|g| (g.origin.x.0 + g.size.width.0) / k)
                .fold(0., f32::max)
        })
    }
}

const PARTS: [(u64, &str); 5] = [
    (
        1,
        "# A heading\n\nA first paragraph that runs long enough to wrap in the view.",
    ),
    (2, "## A smaller heading"),
    (3, "- one\n- two"),
    (4, "```\ncode here\n```"),
    (5, "| a | b |\n|---|---|\n| 1 | 2 |\n\nThe end."),
];

#[test]
fn keyed_sources_are_laid_out_and_copied_as_one_document() {
    let mut cx = app();
    let mut o = open(&mut cx, &PARTS, true);
    let blocks = o.vcx.debug_bounds("blocks").unwrap();
    let one = o.vcx.debug_bounds("one").unwrap();
    assert_eq!(blocks.size.height, one.size.height);
    // Select everything and copy it: the blocks joined as one view's are.
    o.vcx.simulate_click(
        point(blocks.left() + px(4.), blocks.top() + px(4.)),
        Modifiers::none(),
    );
    o.vcx.simulate_keystrokes("cmd-a");
    let copied = o.copied();
    assert!(
        copied.starts_with("A heading\n\nA first paragraph"),
        "{copied:?}"
    );
    assert!(copied.contains("wrap in the view.\n\nA smaller heading\n\none\ntwo\n\ncode here\n\na\tb\n1\t2\n\nThe end."), "{copied:?}");
}

#[test]
fn a_link_resolves_to_a_definition_in_another_source_even_one_that_comes_later() {
    let mut cx = app();
    let mut o = open(&mut cx, &[(1, "See [the docs][d] here.")], false);
    let b = o.vcx.debug_bounds("blocks").unwrap();
    let press = |o: &mut Opened<'_>| {
        let mut x = b.left() + px(2.);
        while o.links.borrow().is_empty() && x < b.right() {
            o.vcx
                .simulate_click(point(x, b.top() + px(8.)), Modifiers::none());
            x += px(6.);
        }
    };
    // No definition yet: it's text as written, with no link.
    press(&mut o);
    assert!(o.links.borrow().is_empty());
    o.set(&[
        (1, "See [the docs][d] here."),
        (2, "[d]: https://x.test/docs"),
    ]);
    press(&mut o);
    assert_eq!(*o.links.borrow(), ["https://x.test/docs"]);
}

#[test]
fn one_markdown_view_resolves_its_own_references_too() {
    let mut cx = app();
    let o = open(
        &mut cx,
        &[(0, "Read [this][1].\n\n[1]: https://x.test/one")],
        false,
    );
    let b = o.vcx.debug_bounds("blocks").unwrap();
    let mut x = b.left() + px(2.);
    while o.links.borrow().is_empty() && x < b.right() {
        o.vcx
            .simulate_click(point(x, b.top() + px(8.)), Modifiers::none());
        x += px(6.);
    }
    assert_eq!(*o.links.borrow(), ["https://x.test/one"]);
}

#[test]
fn a_selection_in_settled_sources_stays_while_the_last_one_streams() {
    let mut cx = app();
    let mut o = open(
        &mut cx,
        &[(1, "Settled text to copy."), (2, "The tail")],
        false,
    );
    let b = o.vcx.debug_bounds("blocks").unwrap();
    o.drag(
        point(b.left() + px(1.), b.top() + px(8.)),
        point(b.left() + px(60.), b.top() + px(8.)),
    );
    let first = o.copied();
    assert!(
        !first.is_empty() && "Settled text to copy.".starts_with(&first),
        "{first:?}"
    );
    o.vcx
        .update(|_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string("x".into())));
    o.set(&[
        (1, "Settled text to copy."),
        (2, "The tail grows, and grows"),
    ]);
    o.set(&[
        (1, "Settled text to copy."),
        (2, "The tail grows, and grows\n\n- into a list"),
    ]);
    assert_eq!(o.copied(), first, "kept");
    // A change at or before it drops it.
    o.vcx
        .update(|_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string("x".into())));
    o.set(&[(1, "Settled text, edited."), (2, "The tail")]);
    assert_eq!(o.copied(), "x", "dropped");
}

#[test]
fn the_trailing_caret_follows_the_last_source_and_never_moves_the_text() {
    let mut cx = app();
    let mut o = open(&mut cx, &[(1, "Settled."), (2, "The tail so far")], false);
    let without = o.vcx.debug_bounds("blocks").unwrap();
    assert!(o.carets().is_empty());
    o.caret(Some(RED));
    let with = o.vcx.debug_bounds("blocks").unwrap();
    assert_eq!(with, without, "drawn, not laid out");
    let carets = o.carets();
    assert_eq!(carets.len(), 1, "{carets:?}");
    let c = carets[0];
    let right = o.last_glyph_right(c.origin.y, c.origin.y + c.size.height);
    assert!(
        c.origin.x >= right - 1. && c.origin.x <= right + 4.,
        "after the tail's last glyph: {c:?} {right}"
    );
    assert!(
        c.origin.y > f32::from(with.top()) + 10.,
        "on the tail's line, below the first"
    );
    // A tail that ends in a code block: the caret is in it, after its text.
    o.set(&[(1, "Settled."), (2, "```\nlet x = 1;\n```")]);
    let c = o.carets()[0];
    let right = o.last_glyph_right(c.origin.y, c.origin.y + c.size.height);
    assert!(
        c.origin.x >= right - 1. && c.origin.x <= right + 4.,
        "{c:?} {right}"
    );
    // An empty tail: after the text before it, inside the view.
    o.set(&[(1, "Settled."), (2, "")]);
    let c = o.carets()[0];
    let b = o.vcx.debug_bounds("blocks").unwrap();
    let right = o.last_glyph_right(c.origin.y, c.origin.y + c.size.height);
    assert!(
        c.origin.x >= right - 1. && c.origin.x <= right + 4.,
        "after \"Settled.\": {c:?} {right}"
    );
    assert!(
        c.origin.y + c.size.height <= f32::from(b.bottom()) + 1.,
        "{c:?} {b:?}"
    );
    // Nothing at all: at the start, the view a line tall to hold it.
    o.set(&[(2, "")]);
    let c = o.carets()[0];
    let b = o.vcx.debug_bounds("blocks").unwrap();
    assert!(b.size.height >= px(10.), "a line tall: {b:?}");
    assert!((c.origin.x - f32::from(b.left())).abs() < 1., "{c:?} {b:?}");
    assert!(
        c.origin.y + c.size.height <= f32::from(b.bottom()) + 1.,
        "{c:?} {b:?}"
    );
    // And without the caret, an empty view takes no room.
    o.caret(None);
    assert_eq!(o.vcx.debug_bounds("blocks").unwrap().size.height, px(0.));
}

#[test]
fn of_two_definitions_in_different_sources_the_first_wins_and_footnotes_resolve() {
    let mut cx = app();
    let mut o = open(
        &mut cx,
        &[
            (1, "See [the docs][x], and a note[^n]."),
            (2, "[x]: https://first.test\n\n[^n]: The note."),
            (3, "[x]: https://second.test"),
        ],
        false,
    );
    let b = o.vcx.debug_bounds("blocks").unwrap();
    let mut x = b.left() + px(2.);
    while o.links.borrow().is_empty() && x < b.right() {
        o.vcx
            .simulate_click(point(x, b.top() + px(8.)), Modifiers::none());
        x += px(6.);
    }
    assert_eq!(*o.links.borrow(), ["https://first.test"]);
    // The note once, where it's written; the reference as its number.
    o.vcx.simulate_click(
        point(b.left() + px(4.), b.top() + px(4.)),
        Modifiers::none(),
    );
    o.vcx.simulate_keystrokes("cmd-a");
    let copied = o.copied();
    assert_eq!(copied.matches("The note.").count(), 1, "{copied:?}");
    assert!(copied.contains("a note[n]."), "{copied:?}");
}
