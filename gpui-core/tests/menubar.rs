//! Menubar: a press opens a menu and a press on another title switches to it, the pointer
//! moving onto another title while one is open switches too, the keys move along the bar (one
//! Tab stop), open a menu and switch between open menus (Right opening a submenu first), a pick
//! is told with its menu and closes it, Escape closes it and gives the keyboard back to its
//! title, touch titles are 44 points, a narrow box holds the bar, and nothing redraws while idle.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, ParentElement as _, Render, Styled as _,
    TestAppContext, VisualTestContext, Window, div, px,
};
use gpui_core::{Menubar, MenubarMenu, PopupMenu, PopupMenuItem, Press, Root};

struct Host {
    touch: bool,
    width: f32,
    picked: Rc<RefCell<Vec<(String, String)>>>,
    draws: Rc<Cell<usize>>,
    before: FocusHandle,
    after: FocusHandle,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let picked = self.picked.clone();
        let menu = |id: &str, entries: &[&str]| {
            PopupMenu::new(format!("{id}-menu"))
                .items(entries.iter().map(|e| PopupMenuItem::new(*e, *e)))
        };
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
                    .w(px(self.width))
                    .debug_selector(|| "box".into())
                    .child(
                        Menubar::new("bar")
                            .touch(self.touch)
                            .menu(MenubarMenu::new(
                                "file",
                                "File",
                                menu("file", &["new", "open"]),
                            ))
                            .menu(MenubarMenu::new(
                                "edit",
                                "Edit",
                                menu("edit", &["undo", "redo"]).item(
                                    PopupMenuItem::new("find", "Find").submenu(
                                        PopupMenu::new("find-menu")
                                            .item(PopupMenuItem::new("find-next", "Find Next")),
                                    ),
                                ),
                            ))
                            .menu(
                                MenubarMenu::new("help", "Help", menu("help", &["about"]))
                                    .disabled(true),
                            )
                            .menu(MenubarMenu::new("view", "View", menu("view", &["zoom"])))
                            .on_select(move |(menu, pick), _, _| {
                                picked
                                    .borrow_mut()
                                    .push((menu.to_string(), pick.id.to_string()))
                            }),
                    ),
            )
            .child(Press::new("after").track_focus(&self.after).child("After"))
    }
}

fn open(
    cx: &mut TestAppContext,
    touch: bool,
    width: f32,
) -> (Entity<Host>, &mut VisualTestContext) {
    cx.update(gpui_core::init);
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let h = cx.new(|cx| Host {
            touch,
            width,
            picked: Rc::default(),
            draws: Rc::default(),
            before: cx.focus_handle(),
            after: cx.focus_handle(),
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

/// Which menu is open, by an entry of each.
fn open_menu(vcx: &mut VisualTestContext) -> Option<&'static str> {
    [
        ("file", "new"),
        ("edit", "undo"),
        ("view", "zoom"),
        ("help", "about"),
    ]
    .into_iter()
    .filter(|(_, entry)| {
        vcx.debug_bounds(Box::leak(format!("menu-item-{entry}").into_boxed_str()))
            .is_some()
    })
    .map(|(menu, _)| menu)
    .next()
}

fn picked(h: &Entity<Host>, vcx: &mut VisualTestContext) -> Vec<(String, String)> {
    h.read_with(vcx, |h, _| h.picked.borrow().clone())
}

fn tab_in(h: &Entity<Host>, vcx: &mut VisualTestContext) {
    let before = h.read_with(vcx, |h, _| h.before.clone());
    vcx.update(|window, cx| window.focus(&before, cx));
    keys(vcx, "tab");
}

#[gpui::test]
fn a_press_opens_a_menu_and_another_title_switches(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, false, 600.);
    assert_eq!(open_menu(vcx), None);
    click(vcx, "menubar-file");
    assert_eq!(open_menu(vcx), Some("file"));
    click(vcx, "menubar-edit");
    assert_eq!(open_menu(vcx), Some("edit"));
    click(vcx, "menubar-help");
    assert_eq!(open_menu(vcx), None, "a disabled title opens nothing");
    click(vcx, "menubar-edit");
    click(vcx, "menu-item-redo");
    assert_eq!(picked(&h, vcx), [("edit".into(), "redo".into())]);
    assert_eq!(open_menu(vcx), None, "a pick closes it");
    click(vcx, "menubar-view");
    click(vcx, "menubar-view");
    assert_eq!(
        open_menu(vcx),
        None,
        "a second press on its title closes it"
    );
}

#[gpui::test]
fn the_pointer_on_another_title_switches_while_one_is_open(cx: &mut TestAppContext) {
    let (_, vcx) = open(cx, false, 600.);
    let edit = vcx.debug_bounds("menubar-edit").unwrap().center();
    vcx.simulate_mouse_move(edit, None, Modifiers::none());
    vcx.run_until_parked();
    assert_eq!(open_menu(vcx), None, "nothing opens by the pointer alone");
    click(vcx, "menubar-file");
    vcx.simulate_mouse_move(edit, None, Modifiers::none());
    vcx.run_until_parked();
    assert_eq!(open_menu(vcx), Some("edit"));
}

#[gpui::test]
fn the_keys_move_along_the_bar_and_between_open_menus(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, false, 600.);
    tab_in(&h, vcx);
    keys(vcx, "right");
    assert_eq!(open_menu(vcx), None, "with none open, Right only moves");
    keys(vcx, "down");
    assert_eq!(open_menu(vcx), Some("edit"));
    keys(vcx, "right");
    assert_eq!(open_menu(vcx), Some("view"), "Help, disabled, passed over");
    keys(vcx, "right");
    assert_eq!(open_menu(vcx), Some("file"), "round from the last");
    keys(vcx, "left left");
    assert_eq!(open_menu(vcx), Some("edit"));
    // The keyboard is in the open menu: Down and Enter pick its second entry.
    keys(vcx, "down enter");
    assert_eq!(picked(&h, vcx), [("edit".into(), "redo".into())]);
    assert_eq!(open_menu(vcx), None);
    // Back on Edit's title: Enter opens it again, Escape closes it.
    keys(vcx, "enter");
    assert_eq!(open_menu(vcx), Some("edit"));
    keys(vcx, "escape");
    assert_eq!(open_menu(vcx), None);
    keys(vcx, "space");
    assert_eq!(
        open_menu(vcx),
        Some("edit"),
        "Escape gave the keyboard back to Edit"
    );
}

#[gpui::test]
fn right_opens_a_submenu_before_the_next_menu(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, false, 600.);
    tab_in(&h, vcx);
    keys(vcx, "right down up right");
    assert_eq!(open_menu(vcx), Some("edit"));
    assert!(
        vcx.debug_bounds("menu-item-find-next").is_some(),
        "its submenu"
    );
    keys(vcx, "enter");
    assert_eq!(picked(&h, vcx), [("edit".into(), "find-next".into())]);
}

#[gpui::test]
fn the_bar_is_one_tab_stop(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, false, 600.);
    tab_in(&h, vcx);
    keys(vcx, "tab");
    let after = h.read_with(vcx, |h, _| h.after.clone());
    assert!(vcx.update(|window, _| after.is_focused(window)));
}

#[gpui::test]
fn touch_titles_are_44_points_and_a_narrow_box_holds_the_bar(cx: &mut TestAppContext) {
    let (_, vcx) = open(cx, true, 120.);
    let t = vcx.debug_bounds("menubar-file").unwrap();
    assert!(t.size.height >= px(44.), "{t:?}");
    let b = vcx.debug_bounds("box").unwrap();
    let bar = vcx.debug_bounds("menubar").unwrap();
    assert!(bar.right() <= b.right() + px(0.5), "{bar:?} in {b:?}");
}

#[gpui::test]
fn open_and_idle_nothing_redraws(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, false, 600.);
    click(vcx, "menubar-file");
    let before = h.read_with(vcx, |h, _| h.draws.get());
    for _ in 0..30 {
        vcx.update(|window, cx| window.simulate_next_frame(cx));
        vcx.executor()
            .advance_clock(std::time::Duration::from_millis(16));
        vcx.run_until_parked();
    }
    assert_eq!(h.read_with(vcx, |h, _| h.draws.get()), before);
}
