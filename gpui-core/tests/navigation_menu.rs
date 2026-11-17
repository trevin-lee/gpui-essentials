//! NavigationMenu: a press opens a trigger's panel and a press on a link navigates and closes
//! it; from the keyboard Down opens a panel with its first link focused, the arrows move between
//! the links and Escape closes it, giving the keyboard back to the trigger; Left and Right move
//! along the row, switching panels while one is open; a press outside closes it; a narrow window
//! gets fewer columns and nothing spills; touch items are 44 points; and nothing redraws while
//! idle.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, ParentElement as _, Render, Styled as _,
    TestAppContext, VisualTestContext, Window, div, px, size,
};
use gpui_core::{NavigationMenu, NavigationMenuItem, NavigationMenuLink, Press, Root};

struct Host {
    touch: bool,
    went: Rc<RefCell<Vec<String>>>,
    draws: Rc<Cell<usize>>,
    before: FocusHandle,
}

const COMPONENTS: [(&str, &str); 4] = [
    (
        "dialog",
        "A window over the page that takes the keyboard until it's closed.",
    ),
    ("tabs", "Panels shown one at a time."),
    ("tooltip", "A word about what's under the pointer."),
    ("progress", "How far along a task is."),
];

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let went = self.went.clone();
        let mut components = NavigationMenuItem::panel("components", "Components").columns(2);
        for (id, d) in COMPONENTS {
            components = components.with_link(NavigationMenuLink::new(id, id).description(d));
        }
        div()
            .size_full()
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
                NavigationMenu::new("site")
                    .touch(self.touch)
                    .item(
                        NavigationMenuItem::panel("start", "Getting started")
                            .with_link(NavigationMenuLink::new("intro", "Introduction"))
                            .with_link(NavigationMenuLink::new("install", "Installation")),
                    )
                    .item(components)
                    .item(NavigationMenuItem::link("docs", "Docs"))
                    .on_navigate(move |id, _, _| went.borrow_mut().push(id.to_string())),
            )
            .child(
                div()
                    .debug_selector(|| "outside".into())
                    .h(px(40.))
                    .child("Page"),
            )
    }
}

fn open(cx: &mut TestAppContext, touch: bool) -> (Entity<Host>, &mut VisualTestContext) {
    cx.update(gpui_core::init);
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let h = cx.new(|cx| Host {
            touch,
            went: Rc::default(),
            draws: Rc::default(),
            before: cx.focus_handle(),
        });
        *m.borrow_mut() = Some(h.clone());
        Root::new(h, window, cx)
    });
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

fn click(vcx: &mut VisualTestContext, what: &'static str) {
    let at = vcx.debug_bounds(what).expect(what).center();
    vcx.simulate_click(at, Modifiers::none());
    vcx.run_until_parked();
}

fn panel(vcx: &mut VisualTestContext) -> Option<&'static str> {
    ["start", "components"].into_iter().find(|id| {
        vcx.debug_bounds(Box::leak(format!("nav-panel-{id}").into_boxed_str()))
            .is_some()
    })
}

fn went(h: &Entity<Host>, vcx: &mut VisualTestContext) -> Vec<String> {
    h.read_with(vcx, |h, _| h.went.borrow().clone())
}

fn tab_in(h: &Entity<Host>, vcx: &mut VisualTestContext) {
    let before = h.read_with(vcx, |h, _| h.before.clone());
    vcx.update(|window, cx| window.focus(&before, cx));
    keys(vcx, "tab");
}

#[gpui::test]
fn a_press_opens_a_panel_and_a_link_navigates(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, false);
    click(vcx, "nav-components");
    assert_eq!(panel(vcx), Some("components"));
    click(vcx, "nav-start");
    assert_eq!(panel(vcx), Some("start"), "another trigger switches");
    click(vcx, "nav-link-install");
    assert_eq!(went(&h, vcx), ["install"]);
    assert_eq!(panel(vcx), None, "following a link closes it");
    click(vcx, "nav-docs");
    assert_eq!(went(&h, vcx), ["install", "docs"]);
    click(vcx, "nav-start");
    click(vcx, "outside");
    assert_eq!(panel(vcx), None, "a press outside closes it");
}

#[gpui::test]
fn the_keyboard_opens_a_panel_and_moves_through_its_links(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, false);
    tab_in(&h, vcx);
    keys(vcx, "right down");
    assert_eq!(panel(vcx), Some("components"));
    // The first link has the keyboard; Down, Right and End move on, Up back.
    keys(vcx, "down right up enter");
    assert_eq!(went(&h, vcx), ["tabs"]);
    keys(vcx, "enter end enter");
    assert_eq!(went(&h, vcx), ["tabs", "progress"]);
    // Escape closes it and the keyboard is back on its trigger.
    keys(vcx, "space escape");
    assert_eq!(panel(vcx), None);
    keys(vcx, "enter enter");
    assert_eq!(went(&h, vcx), ["tabs", "progress", "dialog"]);
}

#[gpui::test]
fn left_and_right_move_along_the_row_and_switch_panels(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, false);
    tab_in(&h, vcx);
    keys(vcx, "down");
    assert_eq!(panel(vcx), Some("start"));
    keys(vcx, "escape right");
    assert_eq!(panel(vcx), None);
    keys(vcx, "right enter");
    assert_eq!(went(&h, vcx), ["docs"], "Right reached the Docs link");
    keys(vcx, "right");
    keys(vcx, "space");
    assert_eq!(panel(vcx), Some("start"), "round from the last");
    // With a panel open from its trigger, Right opens the next trigger's, and a link closes it.
    keys(vcx, "escape");
    click(vcx, "nav-start");
    keys(vcx, "right");
    assert_eq!(panel(vcx), Some("components"));
    keys(vcx, "right");
    assert_eq!(panel(vcx), None, "on the Docs link");
}

#[gpui::test]
fn a_narrow_window_gets_fewer_columns(cx: &mut TestAppContext) {
    let (_, vcx) = open(cx, false);
    click(vcx, "nav-components");
    let a = vcx.debug_bounds("nav-link-dialog").unwrap();
    let b = vcx.debug_bounds("nav-link-tabs").unwrap();
    assert_eq!(a.top(), b.top(), "two columns: {a:?} {b:?}");
    vcx.simulate_resize(size(px(300.), px(600.)));
    vcx.run_until_parked();
    let a = vcx.debug_bounds("nav-link-dialog").unwrap();
    let b = vcx.debug_bounds("nav-link-tabs").unwrap();
    assert!(b.top() > a.top(), "one column: {a:?} {b:?}");
    let p = vcx.debug_bounds("gpui-core-popover").unwrap();
    assert!(p.left() >= px(0.) && p.right() <= px(300.), "{p:?}");
    for (id, _) in COMPONENTS {
        let l = vcx
            .debug_bounds(Box::leak(format!("nav-link-{id}").into_boxed_str()))
            .unwrap();
        assert!(l.right() <= p.right(), "{id} {l:?} in {p:?}");
    }
}

#[gpui::test]
fn touch_items_are_44_points(cx: &mut TestAppContext) {
    let (_, vcx) = open(cx, true);
    for what in ["nav-start", "nav-docs"] {
        let b = vcx.debug_bounds(what).unwrap();
        assert!(b.size.height >= px(44.), "{what} {b:?}");
    }
    click(vcx, "nav-start");
    let l = vcx.debug_bounds("nav-link-intro").unwrap();
    assert!(l.size.height >= px(44.), "{l:?}");
}

#[gpui::test]
fn open_and_idle_nothing_redraws(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, false);
    tab_in(&h, vcx);
    keys(vcx, "down");
    let before = h.read_with(vcx, |h, _| h.draws.get());
    for _ in 0..30 {
        vcx.update(|window, cx| window.simulate_next_frame(cx));
        vcx.executor()
            .advance_clock(std::time::Duration::from_millis(16));
        vcx.run_until_parked();
    }
    assert_eq!(h.read_with(vcx, |h, _| h.draws.get()), before);
}
