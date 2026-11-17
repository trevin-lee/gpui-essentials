//! Select: a press opens it and a press on an option chooses it (not a disabled one), Enter,
//! Space and Down open it from the keyboard on the chosen option, typing a letter in it moves
//! to the next option starting with it and on the closed select chooses it, Escape closes it
//! and gives the keyboard back, disabled it doesn't open, a touch one and its options are 44
//! points tall, a long choice is cut short in a narrow box, and nothing redraws while nothing
//! changes.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, ParentElement as _, Render, SharedString,
    Styled as _, TestAppContext, VisualTestContext, Window, div, px, size,
};
use gpui_core::{Disableable as _, Press, Root, Select, Sizable as _};

#[derive(Clone, Copy)]
struct Setup {
    disabled: bool,
    touch: bool,
    width: f32,
}

const PLAIN: Setup = Setup {
    disabled: false,
    touch: false,
    width: 240.,
};

struct Host {
    s: Setup,
    fruit: Option<SharedString>,
    changes: Rc<RefCell<Vec<SharedString>>>,
    draws: Rc<Cell<usize>>,
    before: FocusHandle,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let changes = self.changes.clone();
        let mut select = Select::new("fruit")
            .accessibility_label("Fruit")
            .placeholder("Pick a fruit")
            .heading("Fruit")
            .option("apple", "Apple")
            .option("banana", "Banana")
            .option("blueberry", "Blueberry")
            .option("cherry", "Cherry")
            .option("blackcurrant", "Blackcurrant")
            .disable("cherry")
            .separator()
            .heading("Vegetables")
            .option(
                "long",
                "A vegetable with a name long enough to be cut short in a narrow select",
            )
            .selected(self.fruit.clone())
            .disabled(self.s.disabled)
            .w_full()
            .on_change(cx.listener(move |this, id: &SharedString, _, cx| {
                changes.borrow_mut().push(id.clone());
                this.fruit = Some(id.clone());
                cx.notify();
            }));
        if self.s.touch {
            select = select.touch();
        }
        div()
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                Press::new("before")
                    .track_focus(&self.before)
                    .child("Before"),
            )
            .child(
                div()
                    .w(px(self.s.width))
                    .debug_selector(|| "box".into())
                    .child(select),
            )
    }
}

fn open(cx: &mut TestAppContext, s: Setup) -> (Entity<Host>, &mut VisualTestContext) {
    cx.update(gpui_core::init);
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let h = cx.new(|cx| Host {
            s,
            fruit: None,
            changes: Rc::default(),
            draws: Rc::default(),
            before: cx.focus_handle(),
        });
        *m.borrow_mut() = Some(h.clone());
        Root::new(h, window, cx)
    });
    vcx.simulate_resize(size(px(600.), px(600.)));
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let h = made.borrow().clone().unwrap();
    (h, vcx)
}

fn keys(vcx: &mut VisualTestContext, keys: &'static str) {
    for k in keys.split(' ') {
        let keystroke = Keystroke::parse(k).unwrap();
        vcx.simulate_event(KeyDownEvent {
            keystroke: keystroke.clone(),
            is_held: false,
            prefer_character_input: false,
        });
        vcx.simulate_event(KeyUpEvent { keystroke });
        vcx.run_until_parked();
    }
}

fn click(vcx: &mut VisualTestContext, selector: &'static str) {
    let at = vcx.debug_bounds(selector).unwrap().center();
    vcx.simulate_click(at, Modifiers::none());
    vcx.run_until_parked();
}

fn is_open(vcx: &mut VisualTestContext) -> bool {
    vcx.debug_bounds("gpui-core-popover").is_some()
}

fn fruit(h: &Entity<Host>, vcx: &mut VisualTestContext) -> Option<String> {
    h.read_with(vcx, |h, _| h.fruit.as_ref().map(|f| f.to_string()))
}

fn focus_before(h: &Entity<Host>, vcx: &mut VisualTestContext) {
    let before = h.read_with(vcx, |h, _| h.before.clone());
    vcx.update(|window, cx| window.focus(&before, cx));
}

#[gpui::test]
fn a_press_opens_it_and_a_press_on_an_option_chooses_it(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, PLAIN);
    assert!(!is_open(vcx));
    click(vcx, "select-fruit");
    assert!(is_open(vcx));
    click(vcx, "menu-item-banana");
    assert_eq!(fruit(&h, vcx).as_deref(), Some("banana"));
    assert!(!is_open(vcx), "choosing closes it");
    // A disabled option can't be chosen, and the menu stays open.
    click(vcx, "select-fruit");
    click(vcx, "menu-item-cherry");
    assert_eq!(fruit(&h, vcx).as_deref(), Some("banana"));
    assert!(is_open(vcx));
    // A second press on the select closes it.
    click(vcx, "select-fruit");
    assert!(!is_open(vcx));
}

#[gpui::test]
fn the_keyboard_opens_it_on_the_chosen_option(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, PLAIN);
    focus_before(&h, vcx);
    // Down opens it, on the first option (nothing chosen yet); Down and Enter choose Banana.
    keys(vcx, "tab down");
    assert!(is_open(vcx));
    keys(vcx, "down enter");
    assert_eq!(fruit(&h, vcx).as_deref(), Some("banana"));
    // Space opens it on Banana, the chosen one: Down moves to Blueberry.
    keys(vcx, "space down space");
    assert_eq!(fruit(&h, vcx).as_deref(), Some("blueberry"));
    // Enter opens it too; Down passes over Cherry, disabled.
    keys(vcx, "enter down enter");
    assert_eq!(fruit(&h, vcx).as_deref(), Some("blackcurrant"));
}

#[gpui::test]
fn escape_closes_it_and_gives_the_keyboard_back(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, PLAIN);
    focus_before(&h, vcx);
    keys(vcx, "tab enter");
    assert!(is_open(vcx));
    keys(vcx, "escape");
    assert!(!is_open(vcx));
    assert_eq!(fruit(&h, vcx), None);
    // The keyboard is back on the select: Down opens it again.
    keys(vcx, "down");
    assert!(is_open(vcx));
}

#[gpui::test]
fn typing_a_letter_moves_to_the_next_option_starting_with_it(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, PLAIN);
    focus_before(&h, vcx);
    keys(vcx, "tab enter b");
    assert!(is_open(vcx));
    keys(vcx, "enter");
    assert_eq!(fruit(&h, vcx).as_deref(), Some("banana"));
    // From Banana: b goes to Blueberry, then Blackcurrant, then round to Banana.
    keys(vcx, "enter b b enter");
    assert_eq!(fruit(&h, vcx).as_deref(), Some("blackcurrant"));
    keys(vcx, "enter b enter");
    assert_eq!(fruit(&h, vcx).as_deref(), Some("banana"));
    // c: Cherry is disabled, so nothing moves.
    keys(vcx, "enter c enter");
    assert_eq!(fruit(&h, vcx).as_deref(), Some("banana"));
}

#[gpui::test]
fn on_the_closed_select_a_letter_chooses(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, PLAIN);
    focus_before(&h, vcx);
    keys(vcx, "tab a");
    assert_eq!(fruit(&h, vcx).as_deref(), Some("apple"));
    assert!(!is_open(vcx));
    keys(vcx, "b b");
    assert_eq!(fruit(&h, vcx).as_deref(), Some("blueberry"));
}

#[gpui::test]
fn disabled_it_doesnt_open(cx: &mut TestAppContext) {
    let (h, vcx) = open(
        cx,
        Setup {
            disabled: true,
            ..PLAIN
        },
    );
    click(vcx, "select-fruit");
    assert!(!is_open(vcx));
    focus_before(&h, vcx);
    keys(vcx, "tab down a");
    assert!(!is_open(vcx));
    assert_eq!(fruit(&h, vcx), None);
}

#[gpui::test]
fn a_touch_one_is_44_points_tall(cx: &mut TestAppContext) {
    let (_, vcx) = open(
        cx,
        Setup {
            touch: true,
            ..PLAIN
        },
    );
    let b = vcx.debug_bounds("select-fruit").unwrap();
    assert!(b.size.height >= px(44.), "{b:?}");
    click(vcx, "select-fruit");
    let row = vcx.debug_bounds("menu-item-apple").unwrap();
    assert!(row.size.height >= px(44.), "{row:?}");
}

#[gpui::test]
fn a_long_choice_is_cut_short_in_a_narrow_box(cx: &mut TestAppContext) {
    let (h, vcx) = open(
        cx,
        Setup {
            width: 120.,
            ..PLAIN
        },
    );
    h.update(vcx, |h, cx| {
        h.fruit = Some("long".into());
        cx.notify();
    });
    vcx.run_until_parked();
    let outer = vcx.debug_bounds("box").unwrap();
    let b = vcx.debug_bounds("select-fruit").unwrap();
    assert!(b.right() <= outer.right() + px(1.), "{b:?} in {outer:?}");
}

fn frame(vcx: &mut VisualTestContext) -> usize {
    vcx.executor().advance_clock(Duration::from_millis(16));
    let asked = vcx.update(|window, cx| window.simulate_next_frame(cx));
    vcx.run_until_parked();
    asked
}

#[gpui::test]
fn nothing_redraws_while_nothing_changes(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, PLAIN);
    for open_it in [false, true] {
        if open_it {
            click(vcx, "select-fruit");
        }
        for _ in 0..3 {
            frame(vcx);
        }
        let settled = h.read_with(vcx, |h, _| h.draws.get());
        for _ in 0..30 {
            assert_eq!(
                frame(vcx),
                0,
                "asked for a frame while idle (open: {open_it})"
            );
        }
        assert_eq!(h.read_with(vcx, |h, _| h.draws.get()), settled);
    }
}
