//! Input: typing through the platform's input path, an input method composing, the keys,
//! undo, Enter, masking, a textarea growing with its text, the pointer, and no frames while idle.
//! Laid out with Core Text, as on screen.

#![cfg(target_os = "macos")]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    AppContext as _, ClipboardItem, Context, Entity, EntityInputHandler as _,
    InteractiveElement as _, IntoElement, Modifiers, ParentElement as _, Render, Styled as _,
    TestAppContext, TestDispatcher, VisualTestContext, Window, div, point, px,
};
use gpui_core::{Input, InputEvent, InputState, Root};

struct Form {
    field: Entity<InputState>,
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
                    .w(px(240.))
                    .debug_selector(|| "field".into())
                    .child(Input::new(&self.field)),
            )
            .child(div().w(px(240.)).child(Input::new(&self.other)))
    }
}

type Events = Rc<RefCell<Vec<InputEvent>>>;

struct Opened<'a> {
    field: Entity<InputState>,
    events: Events,
    vcx: &'a mut VisualTestContext,
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
    make: impl FnOnce(&mut Window, &mut Context<InputState>) -> InputState + 'static,
) -> Opened<'_> {
    cx.update(gpui_core::init);
    let events: Events = Rc::default();
    let e = events.clone();
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let field = cx.new(|cx| make(window, cx));
        let other = cx.new(|cx| InputState::new(window, cx));
        cx.subscribe(&field, move |_, _, ev: &InputEvent, _| {
            e.borrow_mut().push(ev.clone())
        })
        .detach();
        *m.borrow_mut() = Some(field.clone());
        let form = cx.new(|_| Form { field, other });
        Root::new(form, window, cx)
    });
    // Focus events come only to the active window, as on screen.
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let field = made.borrow().clone().unwrap();
    vcx.update(|window, cx| field.update(cx, |f, cx| f.focus(window, cx)));
    vcx.run_until_parked();
    Opened { field, events, vcx }
}

fn value(o: &mut Opened) -> String {
    o.field.read_with(o.vcx, |f, _| f.value().to_string())
}

#[test]
fn typing_comes_through_the_platform_input_path() {
    let mut cx = app();
    let o = open(&mut cx, InputState::new);
    o.vcx.simulate_input("héllo 🎓");
    let mut o = o;
    assert_eq!(value(&mut o), "héllo 🎓");
    assert!(o.events.borrow().contains(&InputEvent::Change));
    assert!(o.events.borrow().contains(&InputEvent::Focus));
}

#[test]
fn an_input_method_composes_then_commits() {
    let mut cx = app();
    let mut o = open(&mut cx, InputState::new);
    o.vcx.simulate_input("a");
    let field = o.field.clone();
    o.vcx.update(|window, cx| {
        field.update(cx, |f, cx| {
            f.replace_and_mark_text_in_range(None, "ni", None, window, cx);
            assert_eq!(f.marked_text_range(window, cx), Some(1..3));
            f.replace_and_mark_text_in_range(None, "nih", None, window, cx);
            f.replace_text_in_range(None, "你好", window, cx);
            assert_eq!(f.marked_text_range(window, cx), None);
        })
    });
    assert_eq!(value(&mut o), "a你好");
    // UTF-16 offsets for the platform: "a" is one unit, each Han character one more.
    let sel = o
        .vcx
        .update(|window, cx| field.update(cx, |f, cx| f.selected_text_range(false, window, cx)));
    assert_eq!(sel.unwrap().range, 3..3);
}

#[test]
fn the_keys_move_select_and_delete() {
    let mut cx = app();
    let mut o = open(&mut cx, InputState::new);
    o.vcx.simulate_input("one two three");
    o.vcx.simulate_keystrokes("alt-backspace");
    assert_eq!(value(&mut o), "one two ");
    o.vcx.simulate_keystrokes("cmd-left shift-alt-right");
    o.vcx.simulate_input("ONE");
    assert_eq!(value(&mut o), "ONE two ");
    o.vcx.simulate_keystrokes("cmd-a backspace");
    assert_eq!(value(&mut o), "");
    o.vcx.simulate_input("ab");
    o.vcx.simulate_keystrokes("left left delete");
    assert_eq!(value(&mut o), "b");
}

#[test]
fn undo_takes_back_a_typing_run_at_once() {
    let mut cx = app();
    let mut o = open(&mut cx, InputState::new);
    for c in ["h", "e", "l", "l", "o"] {
        o.vcx.simulate_input(c);
    }
    o.vcx.simulate_keystrokes("cmd-a backspace");
    o.vcx.simulate_keystrokes("cmd-z");
    assert_eq!(value(&mut o), "hello");
    o.vcx.simulate_keystrokes("cmd-z");
    assert_eq!(value(&mut o), "");
    o.vcx.simulate_keystrokes("cmd-shift-z");
    assert_eq!(value(&mut o), "hello");
}

#[test]
fn enter_submits_a_line_and_breaks_a_textarea() {
    let mut cx = app();
    let mut o = open(&mut cx, InputState::new);
    o.vcx.simulate_input("x");
    o.vcx.simulate_keystrokes("enter shift-enter");
    assert_eq!(value(&mut o), "x");
    let ev = o.events.borrow().clone();
    assert!(ev.contains(&InputEvent::PressEnter { secondary: false }));
    assert!(ev.contains(&InputEvent::PressEnter { secondary: true }));

    let mut cx = app();
    let mut o = open(&mut cx, InputState::textarea);
    o.vcx.simulate_input("a");
    o.vcx.simulate_keystrokes("enter");
    o.vcx.simulate_input("b");
    assert_eq!(value(&mut o), "a\nb");

    let mut cx = app();
    let mut o = open(&mut cx, |w, cx| {
        InputState::textarea(w, cx).submit_on_enter(true)
    });
    o.vcx.simulate_input("a");
    o.vcx.simulate_keystrokes("enter shift-enter");
    o.vcx.simulate_input("b");
    assert_eq!(value(&mut o), "a\nb");
    assert!(
        o.events
            .borrow()
            .contains(&InputEvent::PressEnter { secondary: false })
    );
}

#[test]
fn a_line_takes_pasted_line_breaks_as_spaces() {
    let mut cx = app();
    let mut o = open(&mut cx, InputState::new);
    o.vcx.update(|_, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string(
            "12 Main St\r\nSpringfield".into(),
        ))
    });
    o.vcx.simulate_keystrokes("cmd-v");
    assert_eq!(value(&mut o), "12 Main St Springfield");
}

#[test]
fn a_masked_field_wont_copy() {
    let mut cx = app();
    let mut o = open(&mut cx, |w, cx| InputState::new(w, cx).masked(true));
    o.vcx
        .update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("before".into())));
    o.vcx.simulate_input("secret");
    o.vcx.simulate_keystrokes("cmd-a cmd-c cmd-x");
    assert_eq!(value(&mut o), "secret");
    let clip = o
        .vcx
        .update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()));
    assert_eq!(clip.as_deref(), Some("before"));
}

#[test]
fn a_textarea_grows_with_its_text_between_its_rows() {
    let mut cx = app();
    let mut o = open(&mut cx, |w, cx| InputState::textarea(w, cx).auto_grow(2, 4));
    let h = |o: &mut Opened| o.vcx.debug_bounds("field").unwrap().size.height;
    let two = h(&mut o);
    o.vcx.simulate_input("a\nb\nc");
    o.vcx.run_until_parked();
    let three = h(&mut o);
    assert!(three > two, "{two:?} then {three:?}");
    o.vcx.simulate_input("\nd\ne\nf\ng");
    o.vcx.run_until_parked();
    let most = h(&mut o);
    assert!(most > three);
    o.vcx.simulate_input("\nh\ni");
    o.vcx.run_until_parked();
    assert_eq!(h(&mut o), most, "past its most rows it scrolls");
    // A long line wraps instead of running out of the box.
    let mut cx = app();
    let mut o = open(&mut cx, |w, cx| {
        InputState::textarea(w, cx).auto_grow(1, 10)
    });
    let one = h(&mut o);
    o.vcx.simulate_input(&"word ".repeat(40));
    o.vcx.run_until_parked();
    assert!(h(&mut o) > one * 2.);
}

#[test]
fn up_and_down_move_between_lines() {
    let mut cx = app();
    let mut o = open(&mut cx, InputState::textarea);
    o.vcx.simulate_input("first\nsecond");
    o.vcx.simulate_keystrokes("up");
    o.vcx.simulate_input("!");
    assert_eq!(value(&mut o), "first!\nsecond");
    o.vcx.simulate_keystrokes("cmd-up");
    o.vcx.simulate_input("^");
    assert_eq!(value(&mut o), "^first!\nsecond");
}

#[test]
fn a_click_places_the_caret_and_a_double_click_takes_a_word() {
    let mut cx = app();
    let mut o = open(&mut cx, |w, cx| {
        InputState::new(w, cx).default_value("alpha beta")
    });
    let b = o.vcx.debug_bounds("field").unwrap();
    // Far right of the field: the end.
    o.vcx
        .simulate_click(point(b.right() - px(4.), b.center().y), Modifiers::none());
    o.vcx.simulate_input("!");
    assert_eq!(value(&mut o), "alpha beta!");
    // The left padding is 12 pixels; a double click near the start takes "alpha".
    let at = point(b.left() + px(16.), b.center().y);
    o.vcx.simulate_event(gpui::MouseDownEvent {
        button: gpui::MouseButton::Left,
        position: at,
        modifiers: Modifiers::none(),
        click_count: 2,
        first_mouse: false,
    });
    o.vcx.simulate_event(gpui::MouseUpEvent {
        button: gpui::MouseButton::Left,
        position: at,
        modifiers: Modifiers::none(),
        click_count: 2,
    });
    o.vcx.run_until_parked();
    assert_eq!(o.field.read_with(o.vcx, |f, _| f.selection()), 0..5);
}

#[test]
fn tab_moves_on_and_the_field_says_so() {
    let mut cx = app();
    let o = open(&mut cx, InputState::new);
    o.vcx.simulate_keystrokes("tab");
    assert!(o.events.borrow().contains(&InputEvent::Blur));
}

#[test]
fn a_focused_field_asks_for_no_frames_while_idle() {
    let mut cx = app();
    let o = open(&mut cx, InputState::new);
    o.vcx.simulate_input("typed");
    for _ in 0..60 {
        o.vcx.executor().advance_clock(Duration::from_millis(16));
        let asked = o.vcx.update(|window, cx| window.simulate_next_frame(cx));
        assert_eq!(asked, 0, "a frame was asked for while nothing changed");
        o.vcx.run_until_parked();
    }
}

/// Runs `f` on the field's input handler, as the platform's input method would.
fn ime<R>(
    o: &mut Opened,
    f: impl FnOnce(&mut InputState, &mut Window, &mut Context<InputState>) -> R,
) -> R {
    let field = o.field.clone();
    o.vcx
        .update(|window, cx| field.update(cx, |f2, cx| f(f2, window, cx)))
}

#[test]
fn japanese_composes_updates_and_commits() {
    let mut cx = app();
    let mut o = open(&mut cx, InputState::new);
    // か, then かん, as the romaji are typed; the marked text is in the field all along.
    ime(&mut o, |f, w, cx| {
        f.replace_and_mark_text_in_range(None, "か", None, w, cx)
    });
    assert_eq!(value(&mut o), "か");
    ime(&mut o, |f, w, cx| {
        f.replace_and_mark_text_in_range(None, "かん", None, w, cx)
    });
    assert_eq!(value(&mut o), "かん");
    assert_eq!(
        ime(&mut o, |f, w, cx| f.marked_text_range(w, cx)),
        Some(0..2)
    );
    // The candidate is chosen: 漢, committed.
    ime(&mut o, |f, w, cx| {
        f.replace_text_in_range(None, "漢", w, cx)
    });
    assert_eq!(value(&mut o), "漢");
    assert_eq!(ime(&mut o, |f, w, cx| f.marked_text_range(w, cx)), None);
    // And the caret is after it, in UTF-16 units.
    let sel = ime(&mut o, |f, w, cx| f.selected_text_range(false, w, cx)).unwrap();
    assert_eq!(sel.range, 1..1);
}

#[test]
fn unmarking_keeps_the_composed_text() {
    let mut cx = app();
    let mut o = open(&mut cx, InputState::new);
    ime(&mut o, |f, w, cx| {
        f.replace_and_mark_text_in_range(None, "にほん", Some(3..3), w, cx)
    });
    ime(&mut o, |f, w, cx| f.unmark_text(w, cx));
    assert_eq!(value(&mut o), "にほん");
    assert_eq!(ime(&mut o, |f, w, cx| f.marked_text_range(w, cx)), None);
    o.vcx.simulate_input("ご");
    assert_eq!(value(&mut o), "にほんご");
}

#[test]
fn chinese_pinyin_replaces_its_marked_letters() {
    let mut cx = app();
    let mut o = open(&mut cx, InputState::new);
    o.vcx.simulate_input("说");
    ime(&mut o, |f, w, cx| {
        f.replace_and_mark_text_in_range(None, "zhong", None, w, cx)
    });
    ime(&mut o, |f, w, cx| {
        f.replace_and_mark_text_in_range(None, "zhong guo", Some(9..9), w, cx)
    });
    assert_eq!(value(&mut o), "说zhong guo");
    // The new selection is inside the marked text, counted from its start.
    assert_eq!(
        ime(&mut o, |f, w, cx| f.selected_text_range(false, w, cx))
            .unwrap()
            .range,
        10..10
    );
    ime(&mut o, |f, w, cx| {
        f.replace_text_in_range(None, "中国", w, cx)
    });
    assert_eq!(value(&mut o), "说中国");
}

#[test]
fn ranges_are_utf16_for_the_platform() {
    let mut cx = app();
    let mut o = open(&mut cx, InputState::new);
    o.vcx.simulate_input("a🎓b");
    // 🎓 is two UTF-16 units.
    assert_eq!(ime(&mut o, |f, w, cx| f.text_length_utf16(w, cx)), Some(4));
    assert_eq!(
        ime(&mut o, |f, w, cx| f.text_input_editable_range(w, cx)),
        Some(0..4)
    );
    let mut actual = None;
    let t = ime(&mut o, |f, w, cx| {
        f.text_for_range(1..3, &mut actual, w, cx)
    });
    assert_eq!(t.as_deref(), Some("🎓"));
    ime(&mut o, |f, w, cx| {
        f.replace_text_in_range(Some(1..3), "x", w, cx)
    });
    assert_eq!(value(&mut o), "axb");
    assert!(ime(&mut o, |f, w, cx| f.accepts_text_input(w, cx)));
}

#[test]
fn the_keyboard_is_told_what_the_field_is() {
    use gpui::{Autocapitalize, TextInputAction};
    let mut cx = app();
    let mut o = open(&mut cx, InputState::new);
    let k = ime(&mut o, |f, w, cx| f.text_input_configuration(w, cx));
    assert!(k.autocorrect && k.suggestions);
    assert_eq!(k.autocapitalize, Autocapitalize::Sentences);
    assert_eq!(k.input_action, TextInputAction::Done);

    let mut cx = app();
    let mut o = open(&mut cx, |w, cx| InputState::new(w, cx).masked(true));
    let k = ime(&mut o, |f, w, cx| f.text_input_configuration(w, cx));
    assert!(
        !k.autocorrect && !k.suggestions,
        "no suggestions learned from a password"
    );
    assert_eq!(k.autocapitalize, Autocapitalize::None);

    let mut cx = app();
    let mut o = open(&mut cx, |w, cx| {
        InputState::textarea(w, cx).submit_on_enter(true)
    });
    assert_eq!(
        ime(&mut o, |f, w, cx| f.text_input_configuration(w, cx)).input_action,
        TextInputAction::Send
    );
}

#[test]
fn a_selection_has_a_band_per_row_for_the_platforms_handles() {
    let mut cx = app();
    let mut o = open(&mut cx, InputState::textarea);
    o.vcx.simulate_input("first\nsecond\nthird");
    let field_bounds = o.vcx.debug_bounds("field").unwrap();
    let bands = ime(&mut o, |f, w, cx| {
        f.selection_bounds_for_range(2..15, field_bounds, w, cx)
    });
    assert_eq!(bands.len(), 3);
    assert!(bands.iter().all(|b| b.size.height > px(0.)));
}

#[test]
fn composing_text_is_drawn_while_it_is_marked() {
    let mut cx = app();
    let mut o = open(&mut cx, InputState::new);
    let glyphs = |o: &mut Opened| {
        let b = o.vcx.debug_bounds("field").unwrap();
        o.vcx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
            let s = window.scale_factor();
            window
                .rendered_scene()
                .monochrome_sprites
                .iter()
                .map(|g| &g.bounds)
                .chain(
                    window
                        .rendered_scene()
                        .subpixel_sprites
                        .iter()
                        .map(|g| &g.bounds),
                )
                .filter(|g| {
                    let x = g.origin.x.as_f32() / s;
                    let y = g.origin.y.as_f32() / s;
                    x >= f32::from(b.left())
                        && x <= f32::from(b.right())
                        && y >= f32::from(b.top())
                        && y <= f32::from(b.bottom())
                })
                .count()
        })
    };
    let empty = glyphs(&mut o);
    ime(&mut o, |f, w, cx| {
        f.replace_and_mark_text_in_range(None, "かんじ", None, w, cx)
    });
    assert!(glyphs(&mut o) >= empty + 3, "the marked text isn't drawn");
}
