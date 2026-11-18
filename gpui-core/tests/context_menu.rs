//! ContextMenu: a right-click opens its menu at the pointer, a finger resting on the element
//! opens it (and a short or moving touch doesn't), Shift-F10 and the Menu key open it under the
//! focused element, a pick runs the menu's handler and closes it, Escape, a press outside and
//! the element scrolling away close it, a plain click is the element's, and nothing redraws
//! while idle.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AppContext as _, Bounds, Context, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, MouseButton, ParentElement as _, Pixels, Point,
    Render, ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestAppContext, TouchEvent, TouchId, TouchPhase, VisualTestContext, Window, div, point, px,
    size,
};
use gpui_core::{ContextMenu, MenuSelect, PopupMenu, PopupMenuItem, Press, Root};

/// Past GPUI's long-press delay (half a second).
const LONG: Duration = Duration::from_millis(600);

struct List {
    focus: Vec<FocusHandle>,
    scroll: ScrollHandle,
    picked: Rc<RefCell<Vec<SharedString>>>,
    clicks: Rc<Cell<usize>>,
    draws: Rc<Cell<usize>>,
    /// What each row's on_open_change was told, in order.
    opens: Rc<RefCell<Vec<(usize, bool)>>>,
}

impl Render for List {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let rows = self.focus.iter().enumerate().map(|(i, focus)| {
            let clicks = self.clicks.clone();
            let picked = self.picked.clone();
            let opens = self.opens.clone();
            ContextMenu::new(
                ("row", i),
                Press::new(("press", i))
                    .track_focus(focus)
                    .debug_selector(move || format!("row-{i}"))
                    .w_full()
                    .h(px(40.))
                    .child(format!("Row {i}"))
                    .on_click(move |_, _, _| clicks.set(clicks.get() + 1)),
            )
            .on_open_change(move |open, _| opens.borrow_mut().push((i, open)))
            .menu(move |_, _| {
                let picked = picked.clone();
                PopupMenu::new("menu")
                    .item(PopupMenuItem::new("copy", "Copy"))
                    .item(PopupMenuItem::new("rename", "Rename"))
                    .item(PopupMenuItem::new("move", "Move to").submenu(
                        PopupMenu::new("move-menu").item(PopupMenuItem::new("inbox", "Inbox")),
                    ))
                    .on_select(move |pick: &MenuSelect, _, _| {
                        picked.borrow_mut().push(pick.id.clone())
                    })
            })
        });
        div().size_full().child(
            div()
                .id("list")
                .track_scroll(&self.scroll)
                .overflow_y_scroll()
                .size_full()
                .flex()
                .flex_col()
                .pt(px(20.))
                .px(px(20.))
                .children(rows)
                .child(div().flex_none().h(px(2000.))),
        )
    }
}

struct Opened<'a> {
    list: gpui::Entity<List>,
    picked: Rc<RefCell<Vec<SharedString>>>,
    clicks: Rc<Cell<usize>>,
    draws: Rc<Cell<usize>>,
    opens: Rc<RefCell<Vec<(usize, bool)>>>,
    vcx: &'a mut VisualTestContext,
}

impl Opened<'_> {
    fn picked(&self) -> Vec<String> {
        self.picked.borrow().iter().map(|s| s.to_string()).collect()
    }

    fn shown(&mut self, selector: &'static str) -> Option<Bounds<Pixels>> {
        self.vcx.debug_bounds(selector)
    }

    fn row(&mut self, i: usize) -> Bounds<Pixels> {
        let name: &'static str = Box::leak(format!("row-{i}").into_boxed_str());
        self.shown(name).expect("the row is drawn")
    }

    fn menu(&mut self) -> Option<Bounds<Pixels>> {
        self.shown("gpui-core-popover")
    }

    fn right_click(&mut self, at: Point<Pixels>) {
        self.vcx
            .simulate_mouse_down(at, MouseButton::Right, Modifiers::none());
        self.vcx
            .simulate_mouse_up(at, MouseButton::Right, Modifiers::none());
        self.vcx.run_until_parked();
    }

    fn click(&mut self, at: Point<Pixels>) {
        self.vcx.simulate_click(at, Modifiers::none());
        self.vcx.run_until_parked();
    }

    fn touch(&mut self, phase: TouchPhase, at: Point<Pixels>) {
        self.vcx.simulate_event(TouchEvent {
            id: TouchId(1),
            phase,
            position: at,
            predicted_position: None,
            force: None,
        });
        self.vcx.run_until_parked();
    }

    fn wait(&mut self, time: Duration) {
        self.vcx.executor().advance_clock(time);
        self.vcx.run_until_parked();
    }

    fn key(&mut self, keys: &'static str) {
        for k in keys.split(' ') {
            let keystroke = Keystroke::parse(k).unwrap();
            self.vcx.simulate_event(KeyDownEvent {
                keystroke: keystroke.clone(),
                is_held: false,
                prefer_character_input: false,
            });
            self.vcx.simulate_event(KeyUpEvent { keystroke });
        }
        self.vcx.run_until_parked();
    }

    fn focus_row(&mut self, i: usize) {
        let handle = self.list.read_with(self.vcx, |l, _| l.focus[i].clone());
        self.vcx.update(|window, cx| window.focus(&handle, cx));
        self.vcx.run_until_parked();
    }

    fn row_focused(&mut self, i: usize) -> bool {
        let handle = self.list.read_with(self.vcx, |l, _| l.focus[i].clone());
        self.vcx.update(|window, _| handle.is_focused(window))
    }
}

fn open(cx: &mut TestAppContext) -> Opened<'_> {
    cx.update(gpui_core::init);
    let picked: Rc<RefCell<Vec<SharedString>>> = Rc::default();
    let clicks = Rc::new(Cell::new(0));
    let draws = Rc::new(Cell::new(0));
    let opens: Rc<RefCell<Vec<(usize, bool)>>> = Rc::default();
    let (p, c, n, os) = (picked.clone(), clicks.clone(), draws.clone(), opens.clone());
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let list = cx.new(|cx| List {
            focus: (0..4).map(|_| cx.focus_handle()).collect(),
            scroll: ScrollHandle::new(),
            picked: p,
            clicks: c,
            draws: n,
            opens: os,
        });
        *m.borrow_mut() = Some(list.clone());
        Root::new(list, window, cx)
    });
    vcx.simulate_resize(size(px(400.), px(300.)));
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let list = made.borrow().clone().unwrap();
    Opened {
        list,
        picked,
        clicks,
        draws,
        opens,
        vcx,
    }
}

#[gpui::test]
fn a_right_click_opens_it_at_the_pointer(cx: &mut TestAppContext) {
    let mut o = open(cx);
    let row = o.row(1);
    let at = point(row.left() + px(60.), row.center().y);
    assert!(o.menu().is_none());
    o.right_click(at);
    let m = o.menu().expect("open");
    assert_eq!(m.left(), at.x, "{m:?} {at:?}");
    assert!(m.top() >= at.y && m.top() <= at.y + px(8.), "{m:?} {at:?}");
    assert!(o.shown("menu-item-copy").is_some());
    assert_eq!(o.clicks.get(), 0, "a right-click isn't the row's click");
}

#[gpui::test]
fn a_right_click_on_another_row_moves_it_there(cx: &mut TestAppContext) {
    let mut o = open(cx);
    let first = o.row(0).center();
    let third = o.row(2).center();
    o.right_click(first);
    o.right_click(third);
    let m = o.menu().expect("open");
    assert_eq!(m.left(), third.x);
    assert!(m.top() >= third.y && m.top() <= third.y + px(8.), "{m:?}");
}

#[gpui::test]
fn a_long_press_opens_it_with_rows_for_touch(cx: &mut TestAppContext) {
    let mut o = open(cx);
    let at = o.row(1).center();
    o.touch(TouchPhase::Started, at);
    o.wait(Duration::from_millis(300));
    assert!(o.menu().is_none(), "not before the delay");
    o.wait(LONG);
    let m = o.menu().expect("open");
    assert_eq!(m.left(), at.x);
    assert!(m.top() >= at.y && m.top() <= at.y + px(8.), "{m:?} {at:?}");
    let row = o.shown("menu-item-copy").expect("the menu's rows");
    assert!(row.size.height >= px(44.), "{row:?}");
    // Lifting the finger is the long press's end, not a tap on the row.
    o.touch(TouchPhase::Ended, at);
    assert!(o.menu().is_some());
    assert_eq!(o.clicks.get(), 0);
}

#[gpui::test]
fn a_short_touch_is_a_tap_not_a_long_press(cx: &mut TestAppContext) {
    let mut o = open(cx);
    let at = o.row(1).center();
    o.touch(TouchPhase::Started, at);
    o.wait(Duration::from_millis(200));
    o.touch(TouchPhase::Ended, at);
    o.wait(LONG);
    assert!(o.menu().is_none());
    assert_eq!(o.clicks.get(), 1, "the tap is the row's");
}

#[gpui::test]
fn moving_during_the_press_cancels_it(cx: &mut TestAppContext) {
    let mut o = open(cx);
    let at = o.row(1).center();
    o.touch(TouchPhase::Started, at);
    o.wait(Duration::from_millis(200));
    o.touch(TouchPhase::Moved, at + point(px(0.), px(30.)));
    o.wait(LONG);
    assert!(o.menu().is_none());
    o.touch(TouchPhase::Ended, at + point(px(0.), px(30.)));
    assert!(o.menu().is_none());
}

#[gpui::test]
fn shift_f10_opens_it_under_the_focused_element(cx: &mut TestAppContext) {
    let mut o = open(cx);
    o.focus_row(2);
    o.key("shift-f10");
    let row = o.row(2);
    let m = o.menu().expect("open");
    assert_eq!(m.left(), row.left(), "{m:?} {row:?}");
    assert!(
        m.top() >= row.bottom() && m.top() <= row.bottom() + px(8.),
        "{m:?} {row:?}"
    );
    // The menu has the keyboard: Down and Enter pick its second entry.
    o.key("down enter");
    assert_eq!(o.picked(), ["rename"]);
    assert!(o.menu().is_none());
    assert!(o.row_focused(2), "the focus goes back to the row");
}

#[gpui::test]
fn the_menu_key_opens_it_too(cx: &mut TestAppContext) {
    let mut o = open(cx);
    o.focus_row(0);
    o.key("menu");
    let row = o.row(0);
    let m = o.menu().expect("open");
    assert_eq!(m.left(), row.left());
}

#[gpui::test]
fn the_keys_do_nothing_without_the_focus_inside(cx: &mut TestAppContext) {
    let mut o = open(cx);
    o.key("shift-f10");
    assert!(o.menu().is_none());
}

#[gpui::test]
fn a_pick_runs_on_select_and_closes_it(cx: &mut TestAppContext) {
    let mut o = open(cx);
    let at = o.row(0).center();
    o.right_click(at);
    let copy = o.shown("menu-item-copy").unwrap().center();
    o.click(copy);
    assert_eq!(o.picked(), ["copy"]);
    assert!(o.menu().is_none());
    assert_eq!(o.clicks.get(), 0);
}

#[gpui::test]
fn a_pick_in_a_submenu_closes_it_too(cx: &mut TestAppContext) {
    let mut o = open(cx);
    let at = o.row(0).center();
    o.right_click(at);
    let mv = o.shown("menu-item-move").unwrap().center();
    o.click(mv);
    let inbox = o.shown("menu-item-inbox").expect("the submenu").center();
    o.click(inbox);
    assert_eq!(o.picked(), ["inbox"]);
    assert!(o.menu().is_none());
}

#[gpui::test]
fn escape_closes_it(cx: &mut TestAppContext) {
    let mut o = open(cx);
    o.focus_row(1);
    let at = o.row(1).center();
    o.right_click(at);
    assert!(o.menu().is_some());
    o.key("escape");
    assert!(o.menu().is_none());
    assert!(o.picked().is_empty());
    assert!(o.row_focused(1), "the focus goes back to what had it");
}

#[gpui::test]
fn a_press_outside_closes_it(cx: &mut TestAppContext) {
    let mut o = open(cx);
    let at = o.row(0).center();
    o.right_click(at);
    assert!(o.menu().is_some());
    o.click(point(px(380.), px(290.)));
    assert!(o.menu().is_none());
}

#[gpui::test]
fn it_closes_when_its_element_scrolls_away(cx: &mut TestAppContext) {
    let mut o = open(cx);
    let at = o.row(0).center();
    o.right_click(at);
    assert!(o.menu().is_some());
    o.list.update(o.vcx, |l, cx| {
        l.scroll.set_offset(point(px(0.), px(-500.)));
        cx.notify();
    });
    o.vcx.run_until_parked();
    assert!(o.menu().is_none());
}

#[gpui::test]
fn a_plain_click_is_the_elements_own(cx: &mut TestAppContext) {
    let mut o = open(cx);
    let at = o.row(1).center();
    o.click(at);
    assert!(o.menu().is_none());
    assert_eq!(o.clicks.get(), 1);
}

#[gpui::test]
fn nothing_redraws_while_idle(cx: &mut TestAppContext) {
    let mut o = open(cx);
    let before = o.draws.get();
    o.wait(Duration::from_secs(5));
    assert_eq!(o.draws.get(), before, "closed");
    let at = o.row(0).center();
    o.right_click(at);
    assert!(o.menu().is_some());
    let before = o.draws.get();
    o.wait(Duration::from_secs(5));
    assert_eq!(o.draws.get(), before, "open");
    o.key("escape");
    // A tap, then rest: the long press's timer is gone with the touch.
    o.touch(TouchPhase::Started, at);
    o.touch(TouchPhase::Ended, at);
    let before = o.draws.get();
    o.wait(Duration::from_secs(5));
    assert_eq!(o.draws.get(), before, "after a tap");
    assert!(o.menu().is_none());
}

/// A host is told when a menu opens and closes, by every way it does: a right-click and Escape,
/// a pick, a press outside, the keys, and a right-click on another row (that row's opens after
/// this one's closes, so a host counting them never sees none open in between).
#[gpui::test]
fn the_host_is_told_when_it_opens_and_closes(cx: &mut TestAppContext) {
    let mut o = open(cx);
    let told = |o: &Opened| o.opens.borrow().clone();
    let at = o.row(1).center();
    o.right_click(at);
    assert_eq!(told(&o), [(1, true)]);
    o.key("escape");
    assert_eq!(told(&o), [(1, true), (1, false)]);
    o.opens.borrow_mut().clear();

    o.right_click(at);
    let copy = o.shown("menu-item-copy").unwrap().center();
    o.click(copy);
    assert_eq!(told(&o), [(1, true), (1, false)], "a pick");
    o.opens.borrow_mut().clear();

    o.right_click(at);
    o.click(point(px(380.), px(290.)));
    assert_eq!(told(&o), [(1, true), (1, false)], "a press outside");
    o.opens.borrow_mut().clear();

    o.focus_row(2);
    o.key("shift-f10");
    assert_eq!(told(&o), [(2, true)], "the keys");
    o.key("escape");
    o.opens.borrow_mut().clear();

    // Open on one row, then on another: told of both, and never told twice of the same.
    o.right_click(at);
    let other = o.row(0).center();
    o.right_click(other);
    let t = told(&o);
    assert!(
        t.contains(&(1, true)) && t.contains(&(1, false)) && t.contains(&(0, true)),
        "{t:?}"
    );
    assert_eq!(t.len(), 3, "{t:?}");
    assert!(o.menu().is_some());
    // Nothing more while it stays open.
    o.vcx.update(|window, cx| window.draw(cx).clear(cx));
    o.vcx.run_until_parked();
    assert_eq!(told(&o).len(), 3);
}
