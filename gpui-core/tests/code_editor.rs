//! InputState::code: lines that don't wrap, scrolled sideways to the caret; line numbers in a
//! column the text never draws over; Tab and Shift-Tab indenting, Enter keeping the indent,
//! Ctrl-Tab moving the keyboard on; a long file typed into without delay; and the wheel's
//! scroll kept (a textarea's too). Laid out with Core Text.

#![cfg(target_os = "macos")]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    AppContext as _, Bounds, Context, Entity, InteractiveElement as _, IntoElement, Modifiers,
    ParentElement as _, Pixels, Render, ScrollDelta, ScrollEvent, Styled as _, TestAppContext,
    TestDispatcher, VisualTestContext, Window, div, point, px,
};
use gpui_core::{Input, InputState, Root};

struct Form {
    code: Entity<InputState>,
    other: Entity<InputState>,
}

impl Render for Form {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .p(px(10.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                div()
                    .w(px(360.))
                    .h(px(200.))
                    .debug_selector(|| "code".into())
                    .child(Input::new(&self.code).size_full()),
            )
            .child(div().w(px(200.)).child(Input::new(&self.other)))
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
    text: String,
    textarea: bool,
) -> (
    Entity<InputState>,
    Entity<InputState>,
    &mut VisualTestContext,
) {
    cx.update(gpui_core::init);
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let code = cx.new(|cx| {
            let s = if textarea {
                InputState::textarea(window, cx).auto_grow(8, 8)
            } else {
                InputState::code(window, cx)
            };
            s.default_value(text)
        });
        let other = cx.new(|cx| InputState::new(window, cx));
        *m.borrow_mut() = Some((code.clone(), other.clone()));
        let form = cx.new(|_| Form { code, other });
        Root::new(form, window, cx)
    });
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let (code, other) = made.borrow().clone().unwrap();
    vcx.update(|window, cx| code.update(cx, |f, cx| f.focus(window, cx)));
    vcx.run_until_parked();
    (code, other, vcx)
}

fn value(field: &Entity<InputState>, vcx: &mut VisualTestContext) -> String {
    field.read_with(vcx, |f, _| f.value().to_string())
}

/// The glyphs drawn: their bounds, in window pixels.
fn glyphs(vcx: &mut VisualTestContext) -> Vec<Bounds<f32>> {
    vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let k = window.scale_factor();
        let scene = window.rendered_scene();
        scene
            .monochrome_sprites
            .iter()
            .map(|g| g.bounds)
            .chain(scene.subpixel_sprites.iter().map(|g| g.bounds))
            .map(|b| {
                gpui::Bounds::new(
                    gpui::point(b.origin.x.0 / k, b.origin.y.0 / k),
                    gpui::size(b.size.width.0 / k, b.size.height.0 / k),
                )
            })
            .collect()
    })
}

fn inside(g: &Bounds<f32>, b: Bounds<Pixels>) -> bool {
    let (x, y) = (
        g.origin.x + g.size.width / 2.,
        g.origin.y + g.size.height / 2.,
    );
    x >= f32::from(b.left())
        && x <= f32::from(b.right())
        && y >= f32::from(b.top())
        && y <= f32::from(b.bottom())
}

const LONG: &str = "rules:\n  - name: a rule whose line is much longer than the editor is wide, so it has to scroll sideways to show its end\n    action: allow";

#[test]
fn long_lines_dont_wrap_and_the_editor_follows_the_caret_sideways() {
    let mut cx = app();
    let (code, _, vcx) = open(&mut cx, LONG.into(), false);
    let b = vcx.debug_bounds("code").unwrap();
    // Three lines, three rows of glyphs: nothing wrapped.
    let mut rows: Vec<i32> = glyphs(vcx)
        .iter()
        .filter(|g| inside(g, b))
        .map(|g| (g.origin.y / 4.).round() as i32)
        .collect();
    rows.sort();
    rows.dedup_by(|a, b| (*a - *b).abs() <= 2);
    assert!(rows.len() <= 4, "{rows:?}"); // three lines and their line numbers' rows
    // To the end of the long line: it scrolls so the end shows.
    vcx.simulate_keystrokes("cmd-up down cmd-right");
    vcx.simulate_input("!");
    assert!(value(&code, vcx).contains("its end!\n"));
    let right_edge = glyphs(vcx)
        .iter()
        .filter(|g| inside(g, b))
        .map(|g| g.origin.x + g.size.width)
        .fold(0f32, f32::max);
    assert!(
        right_edge > f32::from(b.right()) - 40.,
        "the line's end is in view: {right_edge} in {b:?}"
    );
}

#[test]
fn line_numbers_have_a_column_the_text_never_draws_over() {
    let mut cx = app();
    let (code, _, vcx) = open(&mut cx, "one\ntwo\nthree".into(), false);
    let b = vcx.debug_bounds("code").unwrap();
    let all = glyphs(vcx);
    let mine: Vec<_> = all.iter().filter(|g| inside(g, b)).collect();
    // The numbers 1, 2 and 3, then the text after a column.
    let left = mine.iter().map(|g| g.origin.x).fold(f32::MAX, f32::min);
    let text_left = mine
        .iter()
        .map(|g| g.origin.x)
        .filter(|x| *x > left + 12.)
        .fold(f32::MAX, f32::min);
    assert!(
        text_left > left + 12.,
        "a column of numbers before the text"
    );
    // A press just after the column puts the caret at the line's start.
    vcx.simulate_click(
        point(px(text_left + 1.), px(mine[0].origin.y + 4.)),
        Modifiers::none(),
    );
    vcx.simulate_input("> ");
    assert_eq!(value(&code, vcx), "> one\ntwo\nthree");
}

#[test]
fn tab_indents_shift_tab_outdents_and_enter_keeps_the_indent() {
    let mut cx = app();
    let (code, _, vcx) = open(&mut cx, "rules:".into(), false);
    vcx.simulate_keystrokes("cmd-down enter tab");
    vcx.simulate_input("- name: a");
    vcx.simulate_keystrokes("enter");
    vcx.simulate_input("action: allow");
    assert_eq!(value(&code, vcx), "rules:\n  - name: a\n  action: allow");
    // Every selected line moves in, then out.
    vcx.simulate_keystrokes("cmd-a tab");
    assert_eq!(
        value(&code, vcx),
        "  rules:\n    - name: a\n    action: allow"
    );
    vcx.simulate_keystrokes("shift-tab shift-tab");
    assert_eq!(value(&code, vcx), "rules:\n- name: a\naction: allow");
    // And undo takes each back.
    vcx.simulate_keystrokes("cmd-z");
    assert_eq!(value(&code, vcx), "rules:\n  - name: a\n  action: allow");
}

#[test]
fn ctrl_tab_moves_the_keyboard_on() {
    let mut cx = app();
    let (code, other, vcx) = open(&mut cx, "a".into(), false);
    vcx.simulate_keystrokes("ctrl-tab");
    vcx.simulate_input("b");
    assert_eq!(value(&code, vcx), "a");
    assert_eq!(value(&other, vcx), "b");
}

#[test]
fn a_long_file_is_typed_into_without_delay() {
    let mut cx = app();
    let text: String = (0..2000)
        .map(|i| format!("  - name: rule number {i}\n    action: ask\n"))
        .collect();
    let (code, _, vcx) = open(&mut cx, text, false);
    vcx.simulate_keystrokes("cmd-down");
    let start = Instant::now();
    for _ in 0..10 {
        vcx.simulate_input("x");
    }
    let each = start.elapsed() / 10;
    assert!(value(&code, vcx).ends_with("xxxxxxxxxx"));
    // Generous, for a debug build on a busy runner: a keystroke must never take a second.
    assert!(each < Duration::from_millis(1000), "{each:?} a keystroke");
    eprintln!("a keystroke in 4,000 lines: {each:?}");
}

#[test]
fn the_wheel_scrolls_a_textarea_and_it_stays_there() {
    let mut cx = app();
    let text: String = (0..60).map(|i| format!("line {i}\n")).collect();
    let (code, _, vcx) = open(&mut cx, text, true);
    let b = vcx.debug_bounds("code").unwrap();
    // The caret at the top; then the wheel scrolls down, away from it.
    vcx.simulate_keystrokes("cmd-up");
    vcx.simulate_event(ScrollEvent {
        position: b.center(),
        delta: ScrollDelta::Pixels(point(px(0.), px(-300.))),
        ..Default::default()
    });
    vcx.run_until_parked();
    // Drawn again: still scrolled, so a press at the top is on a line well down.
    let _ = glyphs(vcx);
    vcx.simulate_click(
        point(b.left() + px(20.), b.top() + px(14.)),
        Modifiers::none(),
    );
    vcx.simulate_input("@");
    let v = value(&code, vcx);
    let line = v.lines().position(|l| l.contains('@')).unwrap();
    assert!(line > 5, "the press landed on line {line}");
}

#[test]
fn nothing_is_drawn_outside_the_editor() {
    let mut cx = app();
    let text: String = (0..60).map(|i| format!("line {i}\n")).collect();
    let (_, _, vcx) = open(&mut cx, text, false);
    // Scrolled to the end: the lines and their numbers above are out of view, and drawn nowhere.
    vcx.simulate_keystrokes("cmd-down");
    let b = vcx.debug_bounds("code").unwrap();
    let outside: Vec<_> = glyphs(vcx).into_iter().filter(|g| !inside(g, b)).collect();
    assert!(outside.is_empty(), "{outside:?} outside {b:?}");
}
