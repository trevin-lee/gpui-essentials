//! Input's maximum length (typing stops, a paste is cut at a whole character, an input method
//! composes freely and commits what fits) and a one-line field's text alignment. Laid out with
//! Core Text, as on screen.

#![cfg(target_os = "macos")]

use std::sync::Arc;

use gpui::{
    AppContext as _, ClipboardItem, Context, Entity, EntityInputHandler as _,
    InteractiveElement as _, IntoElement, Modifiers, ParentElement as _, Render, Styled as _,
    TestAppContext, TestDispatcher, TextAlign, VisualTestContext, Window, div, point, px,
};
use gpui_core::{Input, InputState, Root};

struct Form {
    field: Entity<InputState>,
    align: TextAlign,
}

impl Render for Form {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().p(px(10.)).child(
            div()
                .w(px(240.))
                .debug_selector(|| "field".into())
                .child(Input::new(&self.field).text_align(self.align)),
        )
    }
}

fn open(
    cx: &mut TestAppContext,
    align: TextAlign,
    make: impl FnOnce(&mut Window, &mut Context<InputState>) -> InputState + 'static,
) -> (Entity<InputState>, &mut VisualTestContext) {
    cx.update(gpui_core::init);
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let field = cx.new(|cx| make(window, cx));
        *m.borrow_mut() = Some(field.clone());
        let form = cx.new(|_| Form { field, align });
        Root::new(form, window, cx)
    });
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let field = made.borrow().clone().unwrap();
    vcx.update(|window, cx| field.update(cx, |f, cx| f.focus(window, cx)));
    vcx.run_until_parked();
    (field, vcx)
}

use std::cell::RefCell;
use std::rc::Rc;

fn app() -> TestAppContext {
    TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        Arc::new(gpui_apple::AppleTextSystem::new()),
    )
}

fn value(field: &Entity<InputState>, vcx: &mut VisualTestContext) -> String {
    field.read_with(vcx, |f, _| f.value().to_string())
}

fn limited(
    max: usize,
    start: &'static str,
) -> impl FnOnce(&mut Window, &mut Context<InputState>) -> InputState {
    move |w, cx| InputState::new(w, cx).default_value(start).max_chars(max)
}

#[test]
fn typing_stops_at_the_limit() {
    let mut cx = app();
    let (field, vcx) = open(&mut cx, TextAlign::Left, limited(5, ""));
    vcx.simulate_input("abcdefg");
    assert_eq!(value(&field, vcx), "abcde");
}

#[test]
fn a_paste_is_cut_to_fit_at_a_whole_character() {
    let mut cx = app();
    let (field, vcx) = open(&mut cx, TextAlign::Left, limited(4, "ab"));
    // The thumbs-up with a skin tone is two characters, and only two are left after "c": it
    // isn't split.
    vcx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("c👍🏽d".into())));
    vcx.simulate_keystrokes("cmd-v");
    assert_eq!(value(&field, vcx), "abc");
}

#[test]
fn replacing_a_selection_counts_the_room_it_frees() {
    let mut cx = app();
    let (field, vcx) = open(&mut cx, TextAlign::Left, limited(5, "abcde"));
    vcx.simulate_keystrokes("cmd-a");
    vcx.simulate_input("xyz");
    assert_eq!(value(&field, vcx), "xyz");
}

#[test]
fn an_input_method_composes_freely_and_commits_what_fits() {
    let mut cx = app();
    let (field, vcx) = open(&mut cx, TextAlign::Left, limited(3, "a"));
    let f = field.clone();
    vcx.update(|window, cx| {
        f.update(cx, |f, cx| {
            f.replace_and_mark_text_in_range(None, "nihao", None, window, cx);
            assert_eq!(f.value(), "anihao", "composing isn't cut");
            f.replace_text_in_range(None, "你好吗", window, cx);
        })
    });
    assert_eq!(value(&field, vcx), "a你好");
}

#[test]
fn the_hosts_own_value_isnt_cut() {
    let mut cx = app();
    let (field, vcx) = open(&mut cx, TextAlign::Left, limited(2, ""));
    vcx.update(|window, cx| field.update(cx, |f, cx| f.set_value("longer", window, cx)));
    assert_eq!(value(&field, vcx), "longer");
    // But typing adds nothing past the limit.
    vcx.simulate_input("x");
    assert_eq!(value(&field, vcx), "longer");
}

/// The drawn glyphs' left and right edges, in window pixels.
fn ink(vcx: &mut VisualTestContext) -> (f32, f32) {
    vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let k = window.scale_factor();
        let scene = window.rendered_scene();
        let xs: Vec<(f32, f32)> = scene
            .monochrome_sprites
            .iter()
            .map(|g| g.bounds)
            .chain(scene.subpixel_sprites.iter().map(|g| g.bounds))
            .map(|b| (b.origin.x.0 / k, (b.origin.x.0 + b.size.width.0) / k))
            .collect();
        assert!(!xs.is_empty(), "text drawn");
        (
            xs.iter().map(|x| x.0).fold(f32::MAX, f32::min),
            xs.iter().map(|x| x.1).fold(f32::MIN, f32::max),
        )
    })
}

fn with_text(
    text: &'static str,
) -> impl FnOnce(&mut Window, &mut Context<InputState>) -> InputState {
    move |w, cx| InputState::new(w, cx).default_value(text)
}

#[test]
fn right_aligned_text_sits_at_the_right_and_a_press_finds_it_there() {
    let mut cx = app();
    let (field, vcx) = open(&mut cx, TextAlign::Right, with_text("12"));
    let b = vcx.debug_bounds("field").unwrap();
    let (left, right) = ink(vcx);
    let mid = f32::from(b.center().x);
    assert!(left > mid, "{left} {mid}");
    // Within the padding and the caret's room of the right edge.
    assert!(f32::from(b.right()) - right < 20., "{right} {:?}", b);
    // A press in the middle is before the text, so the caret goes to its start.
    vcx.simulate_click(b.center(), Modifiers::none());
    vcx.simulate_input("!");
    assert_eq!(value(&field, vcx), "!12");
}

#[test]
fn centred_text_sits_in_the_middle() {
    let mut cx = app();
    let (_, vcx) = open(&mut cx, TextAlign::Center, with_text("1234"));
    let b = vcx.debug_bounds("field").unwrap();
    let (left, right) = ink(vcx);
    let mid = f32::from(b.center().x);
    assert!(
        ((left + right) / 2. - mid).abs() < 4.,
        "{left} {right} {mid}"
    );
}

#[test]
fn text_wider_than_the_field_scrolls_from_the_left_as_any() {
    let mut cx = app();
    let (field, vcx) = open(&mut cx, TextAlign::Right, with_text(""));
    vcx.simulate_input(&"9".repeat(60));
    let b = vcx.debug_bounds("field").unwrap();
    let (_, right) = ink(vcx);
    // The end is in view, inside the field.
    assert!(right <= f32::from(b.right()), "{right} {:?}", b);
    assert!(f32::from(b.right()) - right < 20., "{right} {:?}", b);
    vcx.simulate_keystrokes("cmd-left");
    vcx.simulate_input("x");
    assert!(value(&field, vcx).starts_with('x'));
    // The start is in view again, at the left.
    let (left, _) = ink(vcx);
    assert!(
        left >= f32::from(b.left()) && left - f32::from(b.left()) < 20.,
        "{left}"
    );
}

#[test]
fn a_press_at_the_right_of_a_right_aligned_field_is_the_end() {
    let mut cx = app();
    let (field, vcx) = open(&mut cx, TextAlign::Right, with_text("12"));
    let b = vcx.debug_bounds("field").unwrap();
    vcx.simulate_click(point(b.right() - px(4.), b.center().y), Modifiers::none());
    vcx.simulate_input("!");
    assert_eq!(value(&field, vcx), "12!");
}

#[test]
fn the_host_can_select_all() {
    let mut cx = app();
    let (field, vcx) = open(&mut cx, TextAlign::Left, with_text("hello"));
    vcx.update(|window, cx| field.update(cx, |f, cx| f.select_all(window, cx)));
    assert_eq!(field.read_with(vcx, |f, _| f.selection()), 0..5);
    vcx.simulate_input("x");
    assert_eq!(value(&field, vcx), "x");
}

#[test]
fn the_host_can_reveal_and_mask_a_field() {
    let mut cx = app();
    let (field, vcx) = open(&mut cx, TextAlign::Left, |w, cx| {
        InputState::new(w, cx).default_value("secret").masked(true)
    });
    let copied = |vcx: &mut VisualTestContext| {
        vcx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("before".into())));
        vcx.simulate_keystrokes("cmd-a cmd-c");
        vcx.update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()))
    };
    assert_eq!(copied(vcx).as_deref(), Some("before"), "masked: no copy");
    vcx.update(|window, cx| field.update(cx, |f, cx| f.set_masked(false, window, cx)));
    assert_eq!(
        copied(vcx).as_deref(),
        Some("secret"),
        "revealed: it copies"
    );
    vcx.update(|window, cx| field.update(cx, |f, cx| f.set_masked(true, window, cx)));
    assert_eq!(copied(vcx).as_deref(), Some("before"), "masked again");
    assert_eq!(value(&field, vcx), "secret");
}
