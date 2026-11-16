//! PopupMenu's submenus: Right, Enter, Space or a press opens one beside its entry and gives it
//! the keyboard, Left or Escape closes only it, its picks go to the menu's handler, a press in it
//! isn't a press outside the popover, the pointer resting on an entry opens or closes one (and
//! crossing other entries on the way in doesn't), it opens to the left where the right has no
//! room, and nothing redraws while idle.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AppContext as _, Bounds, Context, InteractiveElement as _, IntoElement, KeyDownEvent,
    KeyUpEvent, Keystroke, Modifiers, ParentElement as _, Pixels, Render, SharedString,
    Styled as _, TestAppContext, VisualTestContext, Window, div, prelude::FluentBuilder as _, px,
    size,
};
use gpui_core::{
    DismissReason, MenuSelect, Popover, PopupMenu, PopupMenuItem, Press, Root, SUBMENU_DELAY,
};

struct Panel {
    open: bool,
    /// The trigger at the window's right edge rather than its left.
    at_right: bool,
    picked: Rc<RefCell<Vec<SharedString>>>,
    dismissed: Rc<RefCell<Vec<DismissReason>>>,
    draws: Rc<Cell<usize>>,
}

impl Render for Panel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let picked = self.picked.clone();
        let dismissed = self.dismissed.clone();
        div()
            .size_full()
            .p(px(20.))
            .flex()
            .when(self.at_right, |d| d.justify_end())
            .child(
                Popover::new("pop")
                    .open(self.open)
                    .on_dismiss(cx.listener(move |this, why: &DismissReason, _, cx| {
                        dismissed.borrow_mut().push(*why);
                        this.open = false;
                        cx.notify();
                    }))
                    .trigger(
                        Press::new("trigger")
                            .debug_selector(|| "trigger".into())
                            .w(px(120.))
                            .h(px(30.))
                            .child("Menu")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.open = !this.open;
                                cx.notify();
                            })),
                    )
                    .p(px(4.))
                    .child(
                        PopupMenu::new("menu")
                            .item(PopupMenuItem::new("copy", "Copy"))
                            .item(
                                PopupMenuItem::new("move", "Move to").submenu(
                                    PopupMenu::new("move-menu")
                                        .item(PopupMenuItem::new("inbox", "Inbox"))
                                        .item(PopupMenuItem::new("archive", "Archive")),
                                ),
                            )
                            .item(
                                PopupMenuItem::new("share", "Share").submenu(
                                    PopupMenu::new("share-menu")
                                        .item(PopupMenuItem::new("mail", "Mail")),
                                ),
                            )
                            .item(PopupMenuItem::new("delete", "Delete"))
                            .on_select(cx.listener(move |this, pick: &MenuSelect, _, cx| {
                                picked.borrow_mut().push(pick.id.clone());
                                this.open = false;
                                cx.notify();
                            })),
                    ),
            )
    }
}

struct Opened<'a> {
    picked: Rc<RefCell<Vec<SharedString>>>,
    dismissed: Rc<RefCell<Vec<DismissReason>>>,
    draws: Rc<Cell<usize>>,
    vcx: &'a mut VisualTestContext,
}

impl Opened<'_> {
    fn picked(&self) -> Vec<String> {
        self.picked.borrow().iter().map(|s| s.to_string()).collect()
    }

    fn shown(&mut self, selector: &'static str) -> Option<Bounds<Pixels>> {
        self.vcx.debug_bounds(selector)
    }

    fn point_at(&mut self, selector: &'static str) {
        let at = self.shown(selector).unwrap().center();
        self.vcx.simulate_mouse_move(at, None, Modifiers::none());
        self.vcx.run_until_parked();
    }

    fn press(&mut self, selector: &'static str) {
        let at = self.shown(selector).unwrap().center();
        self.vcx.simulate_click(at, Modifiers::none());
        self.vcx.run_until_parked();
    }

    fn wait(&mut self, time: Duration) {
        self.vcx.executor().advance_clock(time);
        self.vcx.run_until_parked();
    }
}

fn open(cx: &mut TestAppContext, at_right: bool, width: f32) -> Opened<'_> {
    cx.update(gpui_core::init);
    let picked: Rc<RefCell<Vec<SharedString>>> = Rc::default();
    let dismissed: Rc<RefCell<Vec<DismissReason>>> = Rc::default();
    let draws = Rc::new(Cell::new(0));
    let (p, d, n) = (picked.clone(), dismissed.clone(), draws.clone());
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let panel = cx.new(|_| Panel {
            open: false,
            at_right,
            picked: p,
            dismissed: d,
            draws: n,
        });
        Root::new(panel, window, cx)
    });
    vcx.simulate_resize(size(px(width), px(500.)));
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let mut o = Opened {
        picked,
        dismissed,
        draws,
        vcx,
    };
    o.press("trigger");
    assert!(o.shown("menu-item-copy").is_some(), "the menu opened");
    o
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

#[gpui::test]
fn right_opens_a_submenu_with_the_keys_and_its_pick_goes_to_the_menu(cx: &mut TestAppContext) {
    let mut o = open(cx, false, 600.);
    keys(o.vcx, "down right");
    assert!(o.shown("menu-submenu-move").is_some(), "Move to's submenu");
    // It has the keyboard, on its first entry: Down then Enter picks Archive.
    keys(o.vcx, "down enter");
    assert_eq!(o.picked(), ["archive"]);
    assert!(o.shown("menu-item-copy").is_none(), "closed on the pick");
    assert!(o.dismissed.borrow().is_empty());
}

#[gpui::test]
fn enter_and_space_open_a_submenu_rather_than_picking(cx: &mut TestAppContext) {
    let mut o = open(cx, false, 600.);
    keys(o.vcx, "down enter");
    assert!(o.shown("menu-submenu-move").is_some());
    assert!(o.picked().is_empty());
    keys(o.vcx, "left down space");
    assert!(o.shown("menu-submenu-share").is_some());
    assert!(o.shown("menu-submenu-move").is_none());
    assert!(o.picked().is_empty());
}

#[gpui::test]
fn left_and_escape_close_only_the_submenu(cx: &mut TestAppContext) {
    let mut o = open(cx, false, 600.);
    keys(o.vcx, "down right left");
    assert!(o.shown("menu-submenu-move").is_none(), "Left closed it");
    assert!(o.shown("menu-item-copy").is_some(), "the menu stays");
    // The menu has the keyboard back, on Move to: Down goes to Share.
    keys(o.vcx, "down right");
    assert!(o.shown("menu-submenu-share").is_some());
    keys(o.vcx, "escape");
    assert!(o.shown("menu-submenu-share").is_none(), "Escape closed it");
    assert!(o.shown("menu-item-copy").is_some(), "but not the popover");
    assert!(o.dismissed.borrow().is_empty());
    keys(o.vcx, "escape");
    assert!(
        o.shown("menu-item-copy").is_none(),
        "a second Escape closes it"
    );
    assert_eq!(*o.dismissed.borrow(), [DismissReason::Escape]);
}

#[gpui::test]
fn right_on_an_entry_without_a_submenu_does_nothing(cx: &mut TestAppContext) {
    let o = open(cx, false, 600.);
    keys(o.vcx, "right enter");
    assert_eq!(o.picked(), ["copy"]);
}

#[gpui::test]
fn a_press_opens_one_and_a_press_in_it_picks_without_closing_the_popover_first(
    cx: &mut TestAppContext,
) {
    let mut o = open(cx, false, 600.);
    o.press("menu-item-move");
    assert!(o.shown("menu-submenu-move").is_some());
    assert!(o.picked().is_empty(), "a press on it opens, not picks");
    // Outside the popover's panel, but not a press outside it.
    o.press("menu-item-archive");
    assert_eq!(o.picked(), ["archive"]);
    assert!(
        o.dismissed.borrow().is_empty(),
        "{:?}",
        o.dismissed.borrow()
    );
}

#[gpui::test]
fn a_press_outside_both_still_closes_the_popover(cx: &mut TestAppContext) {
    let mut o = open(cx, false, 600.);
    keys(o.vcx, "down right");
    o.vcx
        .simulate_click(gpui::point(px(580.), px(480.)), Modifiers::none());
    o.vcx.run_until_parked();
    assert_eq!(*o.dismissed.borrow(), [DismissReason::ClickOutside]);
    assert!(o.shown("menu-item-copy").is_none());
}

#[gpui::test]
fn the_pointer_resting_opens_one_and_resting_elsewhere_closes_it(cx: &mut TestAppContext) {
    let mut o = open(cx, false, 600.);
    o.point_at("menu-item-move");
    assert!(o.shown("menu-submenu-move").is_none(), "not at once");
    o.wait(SUBMENU_DELAY + Duration::from_millis(50));
    assert!(o.shown("menu-submenu-move").is_some(), "after a rest");
    // Opened by the pointer, nothing in it is highlighted: Enter picks nothing.
    keys(o.vcx, "enter");
    assert!(o.picked().is_empty());
    o.point_at("menu-item-delete");
    o.wait(SUBMENU_DELAY + Duration::from_millis(50));
    assert!(
        o.shown("menu-submenu-move").is_none(),
        "resting on Delete closed it"
    );
    // The menu has the keyboard back, on Delete.
    keys(o.vcx, "enter");
    assert_eq!(o.picked(), ["delete"]);
}

#[gpui::test]
fn crossing_entries_on_the_way_into_a_submenu_keeps_it(cx: &mut TestAppContext) {
    let mut o = open(cx, false, 600.);
    o.point_at("menu-item-move");
    o.wait(SUBMENU_DELAY + Duration::from_millis(50));
    // Over Share on the way, then into Move to's submenu before the delay's up.
    o.point_at("menu-item-share");
    o.wait(Duration::from_millis(50));
    o.point_at("menu-item-archive");
    o.wait(SUBMENU_DELAY * 2);
    assert!(o.shown("menu-submenu-move").is_some(), "still Move to's");
    assert!(o.shown("menu-submenu-share").is_none());
    keys(o.vcx, "enter");
    assert_eq!(o.picked(), ["archive"], "the pointer highlighted Archive");
}

#[gpui::test]
fn it_opens_beside_its_entry_and_to_the_left_where_the_right_has_no_room(cx: &mut TestAppContext) {
    let mut o = open(cx, false, 600.);
    keys(o.vcx, "down right");
    let row = o.shown("menu-item-move").unwrap();
    let sub = o.shown("menu-submenu-move").unwrap();
    assert!(sub.left() >= row.right(), "{row:?} {sub:?}");
    let first = o.shown("menu-item-inbox").unwrap();
    assert!(
        (first.top() - row.top()).abs() <= px(1.),
        "its first entry is level with Move to: {row:?} {first:?}"
    );

    let mut o = open(cx, true, 600.);
    keys(o.vcx, "down right");
    let row = o.shown("menu-item-move").unwrap();
    let sub = o.shown("menu-submenu-move").unwrap();
    assert!(sub.right() <= row.left(), "{row:?} {sub:?}");
    assert!(sub.left() >= px(8.));
}

#[gpui::test]
fn nothing_redraws_while_a_submenu_is_open_and_idle(cx: &mut TestAppContext) {
    let mut o = open(cx, false, 600.);
    o.point_at("menu-item-move");
    o.wait(SUBMENU_DELAY * 2);
    assert!(o.shown("menu-submenu-move").is_some());
    let before = o.draws.get();
    o.wait(Duration::from_secs(5));
    assert_eq!(o.draws.get(), before);
}
