//! PopupMenu's reorder groups: Alt-Up and Alt-Down move the highlighted entry (and the
//! highlight goes with it), a drag past a few pixels moves an entry to where it's dropped, a
//! press that doesn't move still picks, a long press drags by touch, Escape or a drop outside
//! the group or in place moves nothing, entries outside the group neither move nor take a drop,
//! a drag never opens a submenu, and nothing redraws while idle.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AppContext as _, Bounds, Context, Entity, IntoElement, KeyDownEvent, KeyUpEvent, Keystroke,
    Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement as _,
    Pixels, Point, Render, SharedString, Styled as _, TestAppContext, TouchEvent, TouchId,
    TouchPhase, VisualTestContext, Window, div, point, px, size,
};
use gpui_core::{
    DismissReason, MenuReorder, MenuSelect, Popover, PopupMenu, PopupMenuItem, Press, Root,
    SUBMENU_DELAY,
};

/// Past GPUI's long-press delay (half a second).
const LONG: Duration = Duration::from_millis(600);

struct Host {
    open: bool,
    /// The group's entries, in the host's order.
    order: Vec<&'static str>,
    /// Each of the group's entries opens a submenu, as a select's options do.
    subs: bool,
    moves: Rc<RefCell<Vec<MenuReorder>>>,
    picked: Rc<RefCell<Vec<SharedString>>>,
    dismissed: Rc<RefCell<Vec<DismissReason>>>,
    draws: Rc<Cell<usize>>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let (moves, picked, dismissed) = (
            self.moves.clone(),
            self.picked.clone(),
            self.dismissed.clone(),
        );
        let this = cx.entity().downgrade();
        let subs = self.subs;
        let group = self.order.iter().map(|id| {
            let item = PopupMenuItem::new(*id, *id).reorder_group("options");
            if subs {
                item.submenu(
                    PopupMenu::new(format!("{id}-menu"))
                        .item(PopupMenuItem::new(format!("{id}-rename"), "Rename")),
                )
            } else {
                item
            }
        });
        let menu =
            PopupMenu::new("menu")
                .item(PopupMenuItem::new("top", "Top"))
                .heading("Options")
                .items(group)
                .item(PopupMenuItem::new("more", "More").submenu(
                    PopupMenu::new("more-menu").item(PopupMenuItem::new("inner", "Inner")),
                ))
                .on_select(move |p: &MenuSelect, _, _| picked.borrow_mut().push(p.id.clone()))
                .on_reorder(move |m: &MenuReorder, _, cx| {
                    moves.borrow_mut().push(m.clone());
                    let m = m.clone();
                    this.update(cx, |h, cx| {
                        let id = h.order.remove(m.from);
                        h.order.insert(m.to, id);
                        cx.notify();
                    })
                    .ok();
                });
        div().size_full().p(px(20.)).child(
            Popover::new("pop")
                .open(self.open)
                .on_dismiss(cx.listener(move |this, why: &DismissReason, _, cx| {
                    dismissed.borrow_mut().push(*why);
                    this.open = false;
                    cx.notify();
                }))
                .trigger(Press::new("trigger").w(px(120.)).h(px(30.)).child("Menu"))
                .p(px(4.))
                .w(px(220.))
                .child(menu),
        )
    }
}

struct Opened<'a> {
    host: Entity<Host>,
    moves: Rc<RefCell<Vec<MenuReorder>>>,
    picked: Rc<RefCell<Vec<SharedString>>>,
    dismissed: Rc<RefCell<Vec<DismissReason>>>,
    draws: Rc<Cell<usize>>,
    vcx: &'a mut VisualTestContext,
}

fn open(cx: &mut TestAppContext, subs: bool) -> Opened<'_> {
    cx.update(gpui_core::init);
    let moves: Rc<RefCell<Vec<MenuReorder>>> = Rc::default();
    let picked: Rc<RefCell<Vec<SharedString>>> = Rc::default();
    let dismissed: Rc<RefCell<Vec<DismissReason>>> = Rc::default();
    let draws = Rc::new(Cell::new(0));
    let made = Rc::new(RefCell::new(None));
    let (m, p, d, n, h) = (
        moves.clone(),
        picked.clone(),
        dismissed.clone(),
        draws.clone(),
        made.clone(),
    );
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let host = cx.new(|_| Host {
            open: true,
            order: vec!["a", "b", "c", "d"],
            subs,
            moves: m,
            picked: p,
            dismissed: d,
            draws: n,
        });
        *h.borrow_mut() = Some(host.clone());
        Root::new(host, window, cx)
    });
    vcx.simulate_resize(size(px(600.), px(500.)));
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let host = made.borrow().clone().unwrap();
    let o = Opened {
        host,
        moves,
        picked,
        dismissed,
        draws,
        vcx,
    };
    assert!(
        o.vcx.debug_bounds("menu-item-a").is_some(),
        "the menu is open"
    );
    o
}

impl Opened<'_> {
    fn order(&mut self) -> Vec<&'static str> {
        self.host.read_with(self.vcx, |h, _| h.order.clone())
    }

    /// The moves heard, as (id, from, to).
    fn moves(&self) -> Vec<(String, usize, usize)> {
        self.moves
            .borrow()
            .iter()
            .map(|m| {
                assert_eq!(m.group.as_ref(), "options");
                (m.id.to_string(), m.from, m.to)
            })
            .collect()
    }

    fn picked(&self) -> Vec<String> {
        self.picked.borrow().iter().map(|s| s.to_string()).collect()
    }

    fn row(&mut self, id: &str) -> Bounds<Pixels> {
        let selector: &'static str = format!("menu-item-{id}").leak();
        self.vcx.debug_bounds(selector).unwrap()
    }

    fn shown(&mut self, selector: &str) -> bool {
        let selector: &'static str = selector.to_owned().leak();
        self.vcx.debug_bounds(selector).is_some()
    }

    /// A point in entry `id`'s row, `frac` of the way down it.
    fn in_row(&mut self, id: &str, frac: f32) -> Point<Pixels> {
        let b = self.row(id);
        point(b.center().x, b.top() + b.size.height * frac)
    }

    fn keys(&mut self, keys: &'static str) {
        for k in keys.split(' ') {
            let keystroke = Keystroke::parse(k).unwrap();
            self.vcx.simulate_event(KeyDownEvent {
                keystroke: keystroke.clone(),
                is_held: false,
                prefer_character_input: false,
            });
            self.vcx.simulate_event(KeyUpEvent { keystroke });
            self.vcx.run_until_parked();
        }
    }

    fn down(&mut self, at: Point<Pixels>) {
        self.vcx.simulate_event(MouseMoveEvent {
            position: at,
            pressed_button: None,
            modifiers: Modifiers::none(),
        });
        self.vcx.simulate_event(MouseDownEvent {
            button: MouseButton::Left,
            position: at,
            modifiers: Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        self.vcx.run_until_parked();
    }

    fn drag_to(&mut self, at: Point<Pixels>) {
        self.vcx.simulate_event(MouseMoveEvent {
            position: at,
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::none(),
        });
        self.vcx.run_until_parked();
    }

    fn up(&mut self, at: Point<Pixels>) {
        self.vcx.simulate_event(MouseUpEvent {
            button: MouseButton::Left,
            position: at,
            modifiers: Modifiers::none(),
            click_count: 1,
        });
        self.vcx.run_until_parked();
    }

    /// Presses entry `from`'s middle, drags in steps to `to`, and lets go there.
    fn drag(&mut self, from: &str, to: Point<Pixels>) {
        let start = self.in_row(from, 0.5);
        self.down(start);
        self.drag_to(point(start.x, start.y + px(6.)));
        self.drag_to(to);
        self.up(to);
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
}

#[gpui::test]
fn alt_up_and_down_move_the_highlighted_entry_and_the_highlight_follows(cx: &mut TestAppContext) {
    let mut o = open(cx, false);
    // It opens on Top; Down passes the heading to a, then b.
    o.keys("down down alt-down");
    assert_eq!(o.moves(), [("b".into(), 1, 2)]);
    assert_eq!(o.order(), ["a", "c", "b", "d"]);
    // The highlight went with b: once more puts it last, and Enter picks it.
    o.keys("alt-down");
    assert_eq!(o.order(), ["a", "c", "d", "b"]);
    o.keys("enter");
    assert_eq!(o.picked(), ["b"]);
    o.keys("alt-up alt-up alt-up");
    assert_eq!(o.order(), ["b", "a", "c", "d"]);
    assert_eq!(
        o.moves()[2..],
        [("b".into(), 3, 2), ("b".into(), 2, 1), ("b".into(), 1, 0)]
    );
    o.keys("enter");
    assert_eq!(o.picked(), ["b", "b"]);
}

#[gpui::test]
fn the_keys_do_nothing_at_the_groups_ends_or_outside_it(cx: &mut TestAppContext) {
    let mut o = open(cx, false);
    // a is first: Alt-Up has nowhere to go.
    o.keys("down alt-up");
    // d is last: Alt-Down has nowhere to go.
    o.keys("down down down alt-down");
    // Top and More aren't in the group.
    o.keys("home alt-down end alt-up");
    assert!(o.moves().is_empty());
    assert_eq!(o.order(), ["a", "b", "c", "d"]);
    // And the highlight didn't move off More: Enter opens its submenu.
    o.keys("enter");
    assert!(o.shown("menu-submenu-more"));
    assert!(o.dismissed.borrow().is_empty());
}

#[gpui::test]
fn a_drag_moves_the_entry_to_where_its_dropped(cx: &mut TestAppContext) {
    let mut o = open(cx, false);
    // a into the middle: c's lower half is the gap between c and d.
    let to = o.in_row("c", 0.75);
    o.drag("a", to);
    assert_eq!(o.moves(), [("a".into(), 0, 2)]);
    assert_eq!(o.order(), ["b", "c", "a", "d"]);
    // d to the first slot: the top half of the first row.
    let to = o.in_row("b", 0.25);
    o.drag("d", to);
    assert_eq!(o.order(), ["d", "b", "c", "a"]);
    // d to the last slot: the bottom half of the last row.
    let to = o.in_row("a", 0.75);
    o.drag("d", to);
    assert_eq!(o.order(), ["b", "c", "a", "d"]);
    assert_eq!(
        o.moves(),
        [("a".into(), 0, 2), ("d".into(), 3, 0), ("d".into(), 0, 3)]
    );
    // None of the releases was a pick.
    assert!(o.picked().is_empty());
    assert!(o.dismissed.borrow().is_empty());
}

#[gpui::test]
fn mid_drag_the_line_marks_the_drop_and_moves_with_the_pointer(cx: &mut TestAppContext) {
    let mut o = open(cx, false);
    let start = o.in_row("a", 0.5);
    o.down(start);
    for id in ["a", "b", "c", "d"] {
        assert!(
            !o.shown(&format!("menu-drop-line-{id}")),
            "no line before the drag"
        );
    }
    let to = o.in_row("c", 0.75);
    o.drag_to(point(start.x, start.y + px(6.)));
    o.drag_to(to);
    // The line is in the gap below c, where a would land (drawn on d's top edge), and 2
    // pixels tall.
    let line = o.vcx.debug_bounds("menu-drop-line-d").unwrap();
    assert_eq!(line.size.height, px(2.));
    let c = o.row("c");
    assert!(
        (line.center().y - c.bottom()).abs() <= px(1.),
        "{line:?} {c:?}"
    );
    // The lifted entry shows its grip.
    assert!(o.shown("menu-grip-a"));
    // Over b's lower half: the gap above c.
    let to = o.in_row("b", 0.75);
    o.drag_to(to);
    assert!(!o.shown("menu-drop-line-d"));
    let line = o.vcx.debug_bounds("menu-drop-line-c").unwrap();
    assert!(
        (line.center().y - c.top()).abs() <= px(1.),
        "{line:?} {c:?}"
    );
    // Over d's lower half: below the last entry.
    let to = o.in_row("d", 0.75);
    o.drag_to(to);
    let d = o.row("d");
    let line = o.vcx.debug_bounds("menu-drop-line-d").unwrap();
    assert!(
        (line.center().y - d.bottom()).abs() <= px(1.),
        "{line:?} {d:?}"
    );
    // Outside the group: no line.
    let to = o.in_row("top", 0.5);
    o.drag_to(to);
    for id in ["a", "b", "c", "d"] {
        assert!(!o.shown(&format!("menu-drop-line-{id}")), "{id}");
    }
    // Over a itself: in place, no line.
    let to = o.in_row("a", 0.5);
    o.drag_to(to);
    for id in ["a", "b", "c", "d"] {
        assert!(!o.shown(&format!("menu-drop-line-{id}")), "{id}");
    }
    o.up(to);
    assert!(o.moves().is_empty());
}

#[gpui::test]
fn a_press_that_hardly_moves_is_a_click_that_picks(cx: &mut TestAppContext) {
    let mut o = open(cx, false);
    let start = o.in_row("b", 0.5);
    o.down(start);
    o.drag_to(point(start.x + px(2.), start.y + px(2.)));
    o.up(point(start.x + px(2.), start.y + px(2.)));
    assert_eq!(o.picked(), ["b"]);
    assert!(o.moves().is_empty());
    // And a plain click after a drag picks too.
    let to = o.in_row("d", 0.75);
    o.drag("a", to);
    assert_eq!(o.moves(), [("a".into(), 0, 3)]);
    let at = o.in_row("c", 0.5);
    o.down(at);
    o.up(at);
    assert_eq!(o.picked(), ["b", "c"]);
}

#[gpui::test]
fn escape_mid_drag_drops_it_back_and_leaves_the_menu_open(cx: &mut TestAppContext) {
    let mut o = open(cx, false);
    let start = o.in_row("a", 0.5);
    o.down(start);
    o.drag_to(point(start.x, start.y + px(6.)));
    let to = o.in_row("c", 0.75);
    o.drag_to(to);
    assert!(o.shown("menu-drop-line-d"));
    o.keys("escape");
    assert!(!o.shown("menu-drop-line-d"), "the drag ended");
    assert!(o.shown("menu-item-a"), "the menu stays open");
    // Letting go afterwards moves nothing, and picks nothing, even back on a.
    o.drag_to(start);
    o.up(start);
    assert!(o.moves().is_empty());
    assert!(o.picked().is_empty());
    assert!(o.dismissed.borrow().is_empty());
    // Escape without a drag still closes the popover.
    o.keys("escape");
    assert_eq!(*o.dismissed.borrow(), [DismissReason::Escape]);
}

#[gpui::test]
fn a_drop_outside_the_group_or_in_place_moves_nothing(cx: &mut TestAppContext) {
    let mut o = open(cx, false);
    // On an entry outside the group, on the heading's side, and outside the menu.
    for to in [
        o.in_row("top", 0.5),
        o.in_row("more", 0.25),
        o.in_row("a", 0.5) - point(px(0.), o.row("a").size.height),
        point(px(560.), px(400.)),
    ] {
        o.drag("b", to);
    }
    // Back in place: on itself, and in the gaps either side of it.
    for (id, frac) in [("b", 0.5), ("a", 0.75), ("c", 0.25)] {
        let to = o.in_row(id, frac);
        o.drag("b", to);
    }
    assert!(o.moves().is_empty());
    assert_eq!(o.order(), ["a", "b", "c", "d"]);
    assert!(o.picked().is_empty());
}

#[gpui::test]
fn entries_outside_the_group_dont_move(cx: &mut TestAppContext) {
    let mut o = open(cx, false);
    // A plain entry, and one with a submenu, dragged into the group.
    let to = o.in_row("b", 0.75);
    o.drag("top", to);
    let to = o.in_row("b", 0.25);
    o.drag("more", to);
    assert!(o.moves().is_empty());
    assert_eq!(o.order(), ["a", "b", "c", "d"]);
    for id in ["a", "b", "c", "d"] {
        assert!(!o.shown(&format!("menu-drop-line-{id}")), "{id}");
    }
    // Only the group's rows have a grip.
    let at = o.in_row("top", 0.5);
    o.vcx.simulate_mouse_move(at, None, Modifiers::none());
    o.vcx.run_until_parked();
    assert!(!o.shown("menu-grip-top"));
    let at = o.in_row("c", 0.5);
    o.vcx.simulate_mouse_move(at, None, Modifiers::none());
    o.vcx.run_until_parked();
    assert!(o.shown("menu-grip-c"));
}

#[gpui::test]
fn a_drag_never_opens_a_submenu_and_closes_an_open_one(cx: &mut TestAppContext) {
    let mut o = open(cx, true);
    // A click still opens an entry's submenu.
    let at = o.in_row("b", 0.5);
    o.down(at);
    o.up(at);
    assert!(o.shown("menu-submenu-b"));
    // A drag from c closes it, and resting on d mid-drag doesn't open d's.
    let start = o.in_row("c", 0.5);
    o.down(start);
    o.drag_to(point(start.x, start.y + px(6.)));
    assert!(!o.shown("menu-submenu-b"), "closed when the drag started");
    let to = o.in_row("d", 0.75);
    o.drag_to(to);
    o.wait(SUBMENU_DELAY * 3);
    assert!(!o.shown("menu-submenu-c") && !o.shown("menu-submenu-d"));
    o.up(to);
    assert_eq!(o.moves(), [("c".into(), 2, 3)]);
    o.wait(SUBMENU_DELAY * 3);
    for id in ["a", "b", "c", "d"] {
        assert!(!o.shown(&format!("menu-submenu-{id}")), "{id}");
    }
    assert!(o.picked().is_empty());
}

#[gpui::test]
fn a_long_press_drags_by_touch(cx: &mut TestAppContext) {
    let mut o = open(cx, false);
    let start = o.in_row("a", 0.5);
    o.touch(TouchPhase::Started, start);
    o.wait(LONG);
    assert!(o.shown("menu-grip-a"), "lifted");
    let to = o.in_row("c", 0.75);
    o.touch(TouchPhase::Moved, to);
    assert!(o.shown("menu-drop-line-d"));
    o.touch(TouchPhase::Ended, to);
    assert_eq!(o.moves(), [("a".into(), 0, 2)]);
    assert_eq!(o.order(), ["b", "c", "a", "d"]);
    assert!(o.picked().is_empty(), "lifting the finger isn't a tap");
    // A tap still picks.
    let at = o.in_row("d", 0.5);
    o.touch(TouchPhase::Started, at);
    o.touch(TouchPhase::Ended, at);
    assert_eq!(o.picked(), ["d"]);
}

#[gpui::test]
fn nothing_redraws_while_idle_or_while_a_drag_holds_still(cx: &mut TestAppContext) {
    let mut o = open(cx, true);
    o.wait(Duration::from_secs(2));
    let idle = o.draws.get();
    o.wait(Duration::from_secs(5));
    assert_eq!(o.draws.get(), idle, "idle");
    // Mid-drag, moves within one gap don't redraw, and holding still doesn't either.
    let start = o.in_row("a", 0.5);
    o.down(start);
    o.drag_to(point(start.x, start.y + px(6.)));
    let to = o.in_row("c", 0.6);
    o.drag_to(to);
    o.wait(Duration::from_secs(1));
    let held = o.draws.get();
    for frac in [0.8, 0.9] {
        let at = o.in_row("c", frac);
        o.drag_to(at);
    }
    o.wait(Duration::from_secs(5));
    assert_eq!(o.draws.get(), held, "a drag holding still");
    o.up(to);
    o.wait(Duration::from_secs(1));
    let after = o.draws.get();
    o.wait(Duration::from_secs(5));
    assert_eq!(o.draws.get(), after, "idle after the drop");
}
