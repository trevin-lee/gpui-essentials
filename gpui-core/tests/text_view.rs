//! TextView: it lays Markdown out to its width, selects and copies, hands link presses to the
//! host, and asks for no frames while idle. Laid out with Core Text, as on screen.

#![cfg(target_os = "macos")]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    AppContext as _, Context, InteractiveElement as _, IntoElement, Modifiers, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement as _, Render, Styled as _,
    TestAppContext, TestDispatcher, VisualTestContext, Window, div, point, px,
};
use gpui_core::{Root, TextView};

const DOC: &str = "# A heading\n\nA paragraph with [a link](https://example.test/x) and **bold** text that goes on long enough to wrap when the view is narrow.\n\n- one\n- two\n\n```\ncode here\n```";

struct Doc {
    width: f32,
    links: Rc<RefCell<Vec<String>>>,
}

impl Render for Doc {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let links = self.links.clone();
        div().p(px(10.)).child(
            div()
                .w(px(self.width))
                .debug_selector(|| "doc".into())
                .child(
                    TextView::markdown("doc", DOC)
                        .selectable(true)
                        .on_link_click(move |url, _, _, _| {
                            links.borrow_mut().push(url.to_string())
                        }),
                ),
        )
    }
}

fn open(cx: &mut TestAppContext, width: f32) -> (Rc<RefCell<Vec<String>>>, &mut VisualTestContext) {
    cx.update(gpui_core::init);
    let links = Rc::new(RefCell::new(Vec::new()));
    let l = links.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let doc = cx.new(|_| Doc { width, links: l });
        Root::new(doc, window, cx)
    });
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    (links, vcx)
}

fn app() -> TestAppContext {
    TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        Arc::new(gpui_apple::AppleTextSystem::new()),
    )
}

#[test]
fn a_narrower_view_is_taller() {
    let mut cx = app();
    let (_, vcx) = open(&mut cx, 600.);
    let wide = vcx.debug_bounds("doc").unwrap().size.height;
    let mut cx = app();
    let (_, vcx) = open(&mut cx, 160.);
    let narrow = vcx.debug_bounds("doc").unwrap().size.height;
    assert!(narrow > wide, "{narrow:?} at 160 wide, {wide:?} at 600");
}

#[test]
fn select_all_copies_the_text() {
    let mut cx = app();
    let (_, vcx) = open(&mut cx, 600.);
    let b = vcx.debug_bounds("doc").unwrap();
    // A click in the view gives it the focus (and starts a selection there).
    vcx.simulate_click(
        point(b.left() + px(4.), b.top() + px(4.)),
        Modifiers::none(),
    );
    vcx.simulate_keystrokes("cmd-a cmd-c");
    let copied = vcx
        .update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()))
        .unwrap();
    assert!(
        copied.starts_with("A heading\n\nA paragraph with a link and bold text"),
        "{copied:?}"
    );
    assert!(copied.contains("one\ntwo"), "{copied:?}");
    assert!(copied.ends_with("code here"), "{copied:?}");
    assert!(
        !copied.contains("https://"),
        "the link's address isn't its text"
    );
}

#[test]
fn a_drag_selects_and_a_press_on_a_link_is_the_hosts() {
    let mut cx = app();
    let (links, vcx) = open(&mut cx, 600.);
    let b = vcx.debug_bounds("doc").unwrap();
    // Drag across the heading: a selection, and no link.
    let (from, to) = (
        point(b.left() + px(2.), b.top() + px(10.)),
        point(b.left() + px(80.), b.top() + px(10.)),
    );
    vcx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: from,
        modifiers: Modifiers::none(),
        click_count: 1,
        first_mouse: false,
    });
    vcx.simulate_event(MouseMoveEvent {
        position: to,
        pressed_button: Some(MouseButton::Left),
        modifiers: Modifiers::none(),
    });
    vcx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: to,
        modifiers: Modifiers::none(),
        click_count: 1,
    });
    vcx.simulate_keystrokes("cmd-c");
    let copied = vcx
        .update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()))
        .unwrap_or_default();
    assert!(
        !copied.is_empty() && "A heading".starts_with(&copied),
        "{copied:?}"
    );
    assert!(links.borrow().is_empty());
    // Find the link by pressing along the paragraph's first row until the host hears of it.
    let mut x = b.left() + px(2.);
    while links.borrow().is_empty() && x < b.right() {
        for y in [px(40.), px(48.), px(56.)] {
            let at = point(x, b.top() + y);
            vcx.simulate_click(at, Modifiers::none());
            if !links.borrow().is_empty() {
                break;
            }
        }
        x += px(8.);
    }
    assert_eq!(
        links.borrow().first().map(String::as_str),
        Some("https://example.test/x")
    );
}

#[test]
fn a_view_asks_for_no_frames_while_idle() {
    let mut cx = app();
    let (_, vcx) = open(&mut cx, 400.);
    for _ in 0..30 {
        vcx.executor().advance_clock(Duration::from_millis(16));
        assert_eq!(vcx.update(|window, cx| window.simulate_next_frame(cx)), 0);
        vcx.run_until_parked();
    }
}

#[test]
fn right_to_left_text_wraps_like_any_other() {
    // Arabic runs leftwards in a shaped line; it still wraps to the view's width.
    struct Rtl;
    impl Render for Rtl {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().w(px(120.)).debug_selector(|| "rtl".into()).child(TextView::markdown(
                "rtl",
                "يرجى إرسال خطة الدراسة المحدثة قبل التسجيل يرجى إرسال خطة الدراسة المحدثة قبل التسجيل",
            ))
        }
    }
    let mut cx = app();
    cx.update(gpui_core::init);
    let (_, vcx) = cx.add_window_view(|window, cx| {
        let v = cx.new(|_| Rtl);
        Root::new(v, window, cx)
    });
    vcx.run_until_parked();
    let one_row = px(30.);
    assert!(
        vcx.debug_bounds("rtl").unwrap().size.height > one_row * 3.,
        "it didn't wrap"
    );
}

#[test]
fn it_draws_in_the_colour_of_the_text_around_it() {
    // The dark theme's text is light: no glyph of the view may be dark (as it was when its
    // style was read where GPUI doesn't apply its parents').
    let mut cx = app();
    let (_, vcx) = open(&mut cx, 600.);
    let b = vcx.debug_bounds("doc").unwrap();
    let dark = vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let s = window.scale_factor();
        let scene = window.rendered_scene();
        scene
            .monochrome_sprites
            .iter()
            .map(|g| (&g.bounds, g.color))
            .chain(scene.subpixel_sprites.iter().map(|g| (&g.bounds, g.color)))
            .filter(|(g, _)| {
                b.contains(&gpui::point(
                    px(g.origin.x.as_f32() / s),
                    px(g.origin.y.as_f32() / s),
                ))
            })
            .filter(|(_, c)| c.l < 0.5)
            .count()
    });
    assert_eq!(dark, 0, "glyphs drawn dark on the dark theme");
}
