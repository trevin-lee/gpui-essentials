//! More of PopupMenu's submenus: a submenu's panel in the colours of the popover it's drawn
//! from, the open submenu kept by its entry's id while entries come and go, a menu opened with
//! a submenu open and the keyboard in it, and a submenu made only when it opens.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{
    AppContext as _, Context, Entity, Hsla, InteractiveElement as _, IntoElement, KeyDownEvent,
    KeyUpEvent, Keystroke, Modifiers, ParentElement as _, Render, SharedString, Styled as _,
    TestAppContext, VisualTestContext, Window, div, px, size,
};
use gpui_core::{MenuSelect, Popover, PopoverColors, PopupMenu, PopupMenuItem, Press, Root};

const RED: Hsla = Hsla {
    h: 0.,
    s: 0.9,
    l: 0.4,
    a: 1.,
};
const BLUE: Hsla = Hsla {
    h: 0.6,
    s: 0.9,
    l: 0.5,
    a: 1.,
};

#[derive(Default)]
struct Panel {
    open: bool,
    /// An entry added above Move to.
    added: bool,
    /// Move to taken away.
    removed: bool,
    /// The host's popover colours.
    colored: bool,
    /// Opens with Move to's submenu open.
    start_in_move: bool,
    picked: Rc<RefCell<Vec<SharedString>>>,
    /// How many times the lazy submenu has been made.
    made: Rc<Cell<usize>>,
}

impl Render for Panel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let picked = self.picked.clone();
        let made = self.made.clone();
        let mut menu = PopupMenu::new("menu").item(PopupMenuItem::new("copy", "Copy"));
        if self.added {
            menu = menu.item(PopupMenuItem::new("new", "New entry"));
        }
        if !self.removed {
            menu = menu.item(
                PopupMenuItem::new("move", "Move to").submenu(
                    PopupMenu::new("move-menu")
                        .item(PopupMenuItem::new("inbox", "Inbox"))
                        .item(PopupMenuItem::new("archive", "Archive")),
                ),
            );
        }
        menu = menu
            .item(PopupMenuItem::new("share", "Share").submenu_with(move || {
                made.set(made.get() + 1);
                PopupMenu::new("share-menu").item(PopupMenuItem::new("mail", "Mail"))
            }))
            .when(self.start_in_move, |m| m.open_submenu("move"))
            .on_select(cx.listener(move |this, pick: &MenuSelect, _, cx| {
                picked.borrow_mut().push(pick.id.clone());
                this.open = false;
                cx.notify();
            }));
        div().size_full().p(px(20.)).child(
            Popover::new("pop")
                .open(self.open)
                .when(self.colored, |p| {
                    p.colors(PopoverColors {
                        background: RED,
                        foreground: BLUE,
                        border: BLUE,
                    })
                })
                .on_dismiss(cx.listener(|this, _, _, cx| {
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
                .child(menu),
        )
    }
}

use gpui::prelude::FluentBuilder as _;

fn open(
    cx: &mut TestAppContext,
    setup: impl FnOnce(&mut Panel),
) -> (Entity<Panel>, &mut VisualTestContext) {
    cx.update(gpui_core::init);
    let mut panel = Panel::default();
    setup(&mut panel);
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let p = cx.new(|_| panel);
        *m.borrow_mut() = Some(p.clone());
        Root::new(p, window, cx)
    });
    vcx.simulate_resize(size(px(600.), px(500.)));
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let p = made.borrow().clone().unwrap();
    let at = vcx.debug_bounds("trigger").unwrap().center();
    vcx.simulate_click(at, Modifiers::none());
    vcx.run_until_parked();
    (p, vcx)
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

fn change(vcx: &mut VisualTestContext, p: &Entity<Panel>, f: impl FnOnce(&mut Panel)) {
    vcx.update(|_, cx| {
        p.update(cx, |p, cx| {
            f(p);
            cx.notify();
        })
    });
    vcx.run_until_parked();
}

#[gpui::test]
fn a_submenu_is_drawn_in_its_popovers_colours(cx: &mut TestAppContext) {
    let (_, vcx) = open(cx, |p| p.colored = true);
    keys(vcx, "down right");
    let sub = vcx.debug_bounds("menu-submenu-move").unwrap();
    // GPUI draws a box's fill and its border as two quads at its place.
    let (fills, borders) = vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let k = window.scale_factor();
        let at_sub: Vec<_> = window
            .rendered_scene()
            .quads
            .iter()
            .filter(|q| {
                (q.bounds.origin.x.0 / k - f32::from(sub.left())).abs() < 1.
                    && (q.bounds.origin.y.0 / k - f32::from(sub.top())).abs() < 1.
            })
            .cloned()
            .collect();
        (
            at_sub
                .iter()
                .filter_map(|q| q.background.as_solid())
                .collect::<Vec<_>>(),
            at_sub
                .iter()
                .filter(|q| q.border_widths.left.0 > 0.)
                .map(|q| q.border_color)
                .collect::<Vec<_>>(),
        )
    });
    assert!(fills.contains(&RED), "{fills:?}");
    assert!(borders.contains(&BLUE), "{borders:?}");
}

#[gpui::test]
fn the_open_submenu_stays_its_entrys_while_entries_come_and_go(cx: &mut TestAppContext) {
    let (p, vcx) = open(cx, |_| {});
    keys(vcx, "down right");
    assert!(vcx.debug_bounds("menu-submenu-move").is_some());
    // An entry above it: Move to moves down a place, and its submenu stays open.
    change(vcx, &p, |p| p.added = true);
    assert!(vcx.debug_bounds("menu-submenu-move").is_some());
    assert!(vcx.debug_bounds("menu-submenu-share").is_none());
    // Move to taken away: its submenu closes, and no neighbour's opens in its place.
    change(vcx, &p, |p| p.removed = true);
    assert!(vcx.debug_bounds("menu-submenu-move").is_none());
    assert!(vcx.debug_bounds("menu-submenu-share").is_none());
    assert!(
        vcx.debug_bounds("menu-item-copy").is_some(),
        "the menu stays"
    );
}

#[gpui::test]
fn a_menu_can_open_with_a_submenu_open_and_the_keyboard_in_it(cx: &mut TestAppContext) {
    let (p, vcx) = open(cx, |p| p.start_in_move = true);
    assert!(vcx.debug_bounds("menu-submenu-move").is_some());
    keys(vcx, "down enter");
    assert_eq!(
        *p.read_with(vcx, |p, _| p.picked.borrow().clone()),
        ["archive"]
    );
}

#[gpui::test]
fn a_lazy_submenu_is_made_only_when_it_opens(cx: &mut TestAppContext) {
    let (p, vcx) = open(cx, |_| {});
    let made = p.read_with(vcx, |p, _| p.made.clone());
    assert_eq!(made.get(), 0, "not while closed");
    keys(vcx, "down down right");
    assert!(vcx.debug_bounds("menu-submenu-share").is_some());
    assert!(made.get() >= 1);
    keys(vcx, "enter");
    assert_eq!(
        *p.read_with(vcx, |p, _| p.picked.borrow().clone()),
        ["mail"]
    );
}
