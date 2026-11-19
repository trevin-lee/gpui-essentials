//! Roving focus: Tab reaches the rows once and leaves once, the arrows, Home and End move
//! between them, Enter and Space press them, Tab comes back to the row that had the keyboard,
//! a row that goes hands the keyboard to its neighbour, a tree folds, opens and climbs, and the
//! rows tell assistive tech what they are.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use gpui::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, KeyUpEvent, Keystroke, ParentElement as _, Render, Role, SharedString,
    Styled as _, TestAppContext, VisualTestContext, Window, div, px, size,
};
use gpui_core::base::{PopupMenuItem, Press};
use gpui_core::{DropdownMenu, Root, RovingFocus, RowAction};

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    List,
    Row,
    Tree,
}

struct Host {
    kind: Kind,
    before: FocusHandle,
    after: FocusHandle,
    rows: Vec<&'static str>,
    selected: Option<&'static str>,
    /// Which rows (and actions, "row/action") were pressed, in order.
    pressed: Rc<RefCell<Vec<String>>>,
    /// Each row's two actions' focus handles, when it has them: Pin, then Delete (row b's
    /// second is a menu).
    acts: HashMap<&'static str, (FocusHandle, FocusHandle)>,
    /// A tree's open rows.
    open: HashSet<&'static str>,
    /// A tree's toggles: (row, open).
    toggled: Rc<RefCell<Vec<(String, bool)>>>,
}

/// The tree: a (b, c (d)), e.
const TREE: [(&str, usize, &[&str]); 5] = [
    ("a", 1, &["b", "c"]),
    ("b", 2, &[]),
    ("c", 2, &["d"]),
    ("d", 3, &[]),
    ("e", 1, &[]),
];

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let acts = self.acts.clone();
        let pressed = self.pressed.clone();
        // A row, and its actions when it has them: Pin, and Delete (or, on b, a menu).
        let press = move |id: &'static str| {
            let p = pressed.clone();
            let mut row = Press::new(id)
                .debug_selector(move || format!("row-{id}"))
                .h(px(24.))
                .flex()
                .child(id)
                .on_click(move |_, _, _| p.borrow_mut().push(id.to_string()));
            let mut actions = Vec::new();
            if let Some((pin, del)) = acts.get(id) {
                let p = pressed.clone();
                row = row.child(
                    Press::new(format!("{id}-pin"))
                        .track_focus(pin)
                        .tab_stop(false)
                        .accessibility_label(format!("Pin {id}"))
                        .child("Pin")
                        .on_click(move |e, _, cx| {
                            cx.stop_propagation();
                            let _ = e;
                            p.borrow_mut().push(format!("{id}/pin"))
                        }),
                );
                let p = pressed.clone();
                if id == "b" {
                    row = row.child(
                        DropdownMenu::new("b-menu")
                            .trigger(
                                Press::new("b-more")
                                    .track_focus(del)
                                    .tab_stop(false)
                                    .accessibility_label("More for b")
                                    .child("More"),
                            )
                            .item(PopupMenuItem::new("copy", "Copy"))
                            .on_select(move |pick, _, _| {
                                p.borrow_mut().push(format!("b/{}", pick.id))
                            }),
                    );
                    actions = vec![RowAction::new(pin), RowAction::new(del).menu()];
                } else {
                    row = row.child(
                        Press::new(format!("{id}-delete"))
                            .track_focus(del)
                            .tab_stop(false)
                            .accessibility_label(format!("Delete {id}"))
                            .child("Delete")
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                p.borrow_mut().push(format!("{id}/delete"))
                            }),
                    );
                    actions = vec![RowAction::new(pin), RowAction::new(del)];
                }
            }
            (row, actions)
        };
        let mut list = RovingFocus::new("list").accessibility_label("Things");
        if let Some(s) = self.selected {
            list = list.selected(s);
        }
        match self.kind {
            Kind::List | Kind::Row => {
                if self.kind == Kind::Row {
                    list = list.horizontal();
                }
                // A heading between the rows, which the keys pass over.
                list = list.child(div().child("Heading"));
                for id in &self.rows {
                    let (row, actions) = press(id);
                    list = list
                        .row(*id, format!("Row {id}"), row)
                        .row_actions(*id, actions);
                }
            }
            Kind::Tree => {
                let toggled = self.toggled.clone();
                let host = cx.entity().downgrade();
                list = list.tree(move |id, open, _, cx| {
                    toggled.borrow_mut().push((id.to_string(), open));
                    let id: &'static str =
                        TREE.iter().find(|(r, _, _)| *r == id.as_ref()).unwrap().0;
                    host.update(cx, |h, cx| {
                        if open {
                            h.open.insert(id);
                        } else {
                            h.open.remove(id);
                        }
                        cx.notify();
                    })
                    .ok();
                });
                // The rows in view: a folded row's children aren't drawn.
                let mut hidden_under: Option<usize> = None;
                for (id, level, children) in TREE {
                    if let Some(l) = hidden_under {
                        if level > l {
                            continue;
                        }
                        hidden_under = None;
                    }
                    let open = self.open.contains(id);
                    if !children.is_empty() && !open {
                        hidden_under = Some(level);
                    }
                    let expanded = (!children.is_empty()).then_some(open);
                    let (row, actions) = press(id);
                    list = list
                        .tree_row(id, format!("Row {id}"), level, expanded, row)
                        .row_actions(id, actions);
                }
            }
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
            .child(list)
            .child(Press::new("after").track_focus(&self.after).child("After"))
    }
}

fn open<'a>(
    cx: &'a mut TestAppContext,
    kind: Kind,
    selected: Option<&'static str>,
) -> (Entity<Host>, &'a mut VisualTestContext) {
    open_with(cx, kind, selected, false)
}

/// As `open`, each row with actions when `actions`.
fn open_with<'a>(
    cx: &'a mut TestAppContext,
    kind: Kind,
    selected: Option<&'static str>,
    actions: bool,
) -> (Entity<Host>, &'a mut VisualTestContext) {
    cx.update(gpui_core::init);
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let h = cx.new(|cx| Host {
            kind,
            before: cx.focus_handle(),
            after: cx.focus_handle(),
            rows: vec!["a", "b", "c"],
            selected,
            pressed: Default::default(),
            acts: if actions {
                ["a", "b", "c", "d", "e"]
                    .into_iter()
                    .map(|id| (id, (cx.focus_handle(), cx.focus_handle())))
                    .collect()
            } else {
                HashMap::new()
            },
            open: HashSet::new(),
            toggled: Default::default(),
        });
        *m.borrow_mut() = Some(h.clone());
        Root::new(h, window, cx)
    });
    vcx.simulate_resize(size(px(400.), px(400.)));
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let h = made.borrow().clone().unwrap();
    // The keyboard on Before.
    let before = h.read_with(vcx, |h, _| h.before.clone());
    vcx.update(|window, cx| window.focus(&before, cx));
    vcx.run_until_parked();
    (h, vcx)
}

fn key(vcx: &mut VisualTestContext, k: &'static str) {
    let keystroke = Keystroke::parse(k).unwrap();
    vcx.simulate_event(KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    vcx.simulate_event(KeyUpEvent { keystroke });
    vcx.run_until_parked();
}

/// What has the keyboard: "before", "after", or the row (found by pressing it with Enter, which
/// the host records).
fn focused(h: &Entity<Host>, vcx: &mut VisualTestContext) -> String {
    let (before, after) = h.read_with(vcx, |h, _| (h.before.clone(), h.after.clone()));
    let outside = vcx.update(|window, _| {
        if before.is_focused(window) {
            Some("before")
        } else if after.is_focused(window) {
            Some("after")
        } else {
            None
        }
    });
    match outside {
        Some(s) => s.into(),
        None => which_row(vcx),
    }
}

/// Which row has the keyboard: Enter presses it, and the press says which.
fn which_row(vcx: &mut VisualTestContext) -> String {
    let pressed = PRESSED
        .with(|p| p.borrow().clone())
        .expect("the host's presses");
    let n = pressed.borrow().len();
    key(vcx, "enter");
    let p = pressed.borrow();
    if p.len() == n + 1 {
        p[n].to_string()
    } else {
        "nothing".into()
    }
}

thread_local! {
    static PRESSED: RefCell<Option<Rc<RefCell<Vec<String>>>>> = const { RefCell::new(None) };
}

fn watch(h: &Entity<Host>, vcx: &mut VisualTestContext) {
    let p = h.read_with(vcx, |h, _| h.pressed.clone());
    PRESSED.with(|w| *w.borrow_mut() = Some(p));
}

#[gpui::test]
fn tab_enters_the_rows_once_and_leaves_once(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, Kind::List, None);
    watch(&h, vcx);
    key(vcx, "tab");
    assert_eq!(focused(&h, vcx), "a", "Tab enters on the first row");
    key(vcx, "tab");
    assert_eq!(focused(&h, vcx), "after", "the next Tab leaves the rows");
    key(vcx, "shift-tab");
    assert_eq!(focused(&h, vcx), "a", "Shift-Tab comes back to the one row");
    key(vcx, "shift-tab");
    assert_eq!(focused(&h, vcx), "before");
}

#[gpui::test]
fn the_arrows_home_and_end_move_between_the_rows(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, Kind::List, None);
    watch(&h, vcx);
    key(vcx, "tab");
    key(vcx, "down");
    assert_eq!(focused(&h, vcx), "b");
    key(vcx, "down");
    key(vcx, "down");
    assert_eq!(focused(&h, vcx), "c", "Down stops at the last row");
    key(vcx, "up");
    assert_eq!(focused(&h, vcx), "b");
    key(vcx, "home");
    assert_eq!(focused(&h, vcx), "a");
    key(vcx, "up");
    assert_eq!(focused(&h, vcx), "a", "Up stops at the first row");
    key(vcx, "end");
    assert_eq!(focused(&h, vcx), "c");
}

#[gpui::test]
fn enter_and_space_press_the_row(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, Kind::List, None);
    key(vcx, "tab");
    key(vcx, "down");
    key(vcx, "enter");
    key(vcx, "space");
    let pressed = h.read_with(vcx, |h, _| h.pressed.borrow().clone());
    assert_eq!(pressed, ["b", "b"]);
}

#[gpui::test]
fn tab_comes_back_to_the_row_that_had_the_keyboard(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, Kind::List, None);
    watch(&h, vcx);
    key(vcx, "tab");
    // The middle row: Shift-Tab would reach the last row by itself.
    key(vcx, "down");
    key(vcx, "tab");
    assert_eq!(focused(&h, vcx), "after");
    key(vcx, "shift-tab");
    assert_eq!(focused(&h, vcx), "b", "back on the row it left from");
}

#[gpui::test]
fn the_chosen_row_is_where_tab_enters_first(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, Kind::List, Some("b"));
    watch(&h, vcx);
    key(vcx, "tab");
    assert_eq!(focused(&h, vcx), "b");
}

#[gpui::test]
fn a_row_that_goes_hands_the_keyboard_to_its_neighbour(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, Kind::List, None);
    watch(&h, vcx);
    key(vcx, "tab");
    key(vcx, "down");
    // b goes while it has the keyboard: the row after it takes it.
    h.update(vcx, |h, cx| {
        h.rows.retain(|r| *r != "b");
        cx.notify();
    });
    vcx.run_until_parked();
    assert_eq!(focused(&h, vcx), "c");
    // The last row goes: the one before it takes it.
    h.update(vcx, |h, cx| {
        h.rows.retain(|r| *r != "c");
        cx.notify();
    });
    vcx.run_until_parked();
    assert_eq!(focused(&h, vcx), "a");
}

#[gpui::test]
fn a_horizontal_row_moves_with_left_and_right_and_leaves_up_and_down(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, Kind::Row, None);
    watch(&h, vcx);
    key(vcx, "tab");
    key(vcx, "right");
    assert_eq!(focused(&h, vcx), "b");
    key(vcx, "down");
    assert_eq!(focused(&h, vcx), "b", "Down isn't the row's");
    key(vcx, "left");
    assert_eq!(focused(&h, vcx), "a");
}

#[gpui::test]
fn a_tree_opens_folds_and_climbs(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, Kind::Tree, None);
    watch(&h, vcx);
    key(vcx, "tab");
    assert_eq!(focused(&h, vcx), "a");
    // a is folded: Down goes to e, its children not being rows.
    key(vcx, "down");
    assert_eq!(focused(&h, vcx), "e");
    key(vcx, "up");
    // Right opens a; Right again goes to its first child.
    key(vcx, "right");
    let toggled = h.read_with(vcx, |h, _| h.toggled.borrow().clone());
    assert_eq!(toggled, [("a".to_string(), true)]);
    assert_eq!(focused(&h, vcx), "a", "opening stays on the row");
    key(vcx, "right");
    assert_eq!(focused(&h, vcx), "b");
    // b has no children: Right does nothing; Left goes up to its parent.
    key(vcx, "right");
    assert_eq!(focused(&h, vcx), "b");
    key(vcx, "down");
    key(vcx, "right");
    key(vcx, "right");
    assert_eq!(focused(&h, vcx), "d", "c opened, then its child");
    key(vcx, "left");
    assert_eq!(focused(&h, vcx), "c", "Left on a leaf climbs to its parent");
    // Left on an open row folds it.
    key(vcx, "left");
    let toggled = h.read_with(vcx, |h, _| h.toggled.borrow().clone());
    assert_eq!(toggled.last(), Some(&("c".to_string(), false)));
    assert_eq!(focused(&h, vcx), "c");
    key(vcx, "left");
    assert_eq!(focused(&h, vcx), "a");
}

#[gpui::test]
fn the_rows_say_what_they_are(cx: &mut TestAppContext) {
    cx.update(gpui_core::init);
    for (kind, role) in [(Kind::List, Role::ListItem), (Kind::Tree, Role::TreeItem)] {
        gpui_core::audit_start();
        let (_h, vcx) = open(cx, kind, None);
        vcx.run_until_parked();
        let said = gpui_core::audit_take();
        let rows: Vec<_> = said.iter().filter(|a| a.role == role).collect();
        assert!(!rows.is_empty(), "rows with role {role:?}");
        for a in rows {
            let label: Option<&SharedString> = a.label.as_ref();
            assert!(
                label.is_some_and(|l| l.starts_with("Row ")),
                "a row named by its words"
            );
        }
    }
}

#[gpui::test]
fn right_goes_into_a_rows_actions_and_left_comes_back(cx: &mut TestAppContext) {
    let (h, vcx) = open_with(cx, Kind::List, None, true);
    watch(&h, vcx);
    key(vcx, "tab");
    key(vcx, "right");
    assert_eq!(focused(&h, vcx), "a/pin");
    key(vcx, "right");
    assert_eq!(focused(&h, vcx), "a/delete");
    key(vcx, "right");
    assert_eq!(
        focused(&h, vcx),
        "a/delete",
        "Right stops at the last action"
    );
    key(vcx, "left");
    assert_eq!(focused(&h, vcx), "a/pin");
    key(vcx, "left");
    assert_eq!(
        focused(&h, vcx),
        "a",
        "Left from the first action is the row"
    );
}

#[gpui::test]
fn up_and_down_from_an_action_land_on_the_neighbouring_row(cx: &mut TestAppContext) {
    let (h, vcx) = open_with(cx, Kind::List, None, true);
    watch(&h, vcx);
    key(vcx, "tab");
    key(vcx, "right");
    key(vcx, "down");
    assert_eq!(focused(&h, vcx), "b", "the row, not its action");
    key(vcx, "right");
    key(vcx, "up");
    assert_eq!(focused(&h, vcx), "a");
}

#[gpui::test]
fn escape_from_an_action_goes_back_to_its_row(cx: &mut TestAppContext) {
    let (h, vcx) = open_with(cx, Kind::List, None, true);
    watch(&h, vcx);
    key(vcx, "tab");
    key(vcx, "right");
    key(vcx, "right");
    key(vcx, "escape");
    assert_eq!(focused(&h, vcx), "a");
}

#[gpui::test]
fn the_actions_arent_tab_stops(cx: &mut TestAppContext) {
    let (h, vcx) = open_with(cx, Kind::List, None, true);
    watch(&h, vcx);
    key(vcx, "tab");
    assert_eq!(focused(&h, vcx), "a");
    key(vcx, "tab");
    assert_eq!(
        focused(&h, vcx),
        "after",
        "Tab leaves the list, past every action"
    );
    key(vcx, "shift-tab");
    assert_eq!(focused(&h, vcx), "a");
}

#[gpui::test]
fn shift_f10_opens_the_rows_menu_action(cx: &mut TestAppContext) {
    let (h, vcx) = open_with(cx, Kind::List, None, true);
    watch(&h, vcx);
    key(vcx, "tab");
    key(vcx, "down");
    key(vcx, "shift-f10");
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        vcx.debug_bounds("menu-item-copy").is_some(),
        "b's menu is open"
    );
    key(vcx, "enter");
    let pressed = h.read_with(vcx, |h, _| h.pressed.borrow().clone());
    assert_eq!(
        pressed.last().map(String::as_str),
        Some("b/copy"),
        "the menu's first entry, from the keys"
    );
}

#[gpui::test]
fn in_a_tree_right_opens_first_then_goes_into_the_actions(cx: &mut TestAppContext) {
    let (h, vcx) = open_with(cx, Kind::Tree, None, true);
    watch(&h, vcx);
    key(vcx, "tab");
    // a is folded: Right opens it, staying on it.
    key(vcx, "right");
    assert_eq!(focused(&h, vcx), "a");
    // Open, with actions: Right goes into them (Down reaches its children).
    key(vcx, "right");
    assert_eq!(focused(&h, vcx), "a/pin");
    key(vcx, "left");
    assert_eq!(focused(&h, vcx), "a");
    // A leaf's Right goes into its actions; Left back to the row, Left again to the parent.
    key(vcx, "down");
    key(vcx, "right");
    assert_eq!(focused(&h, vcx), "b/pin");
    key(vcx, "left");
    key(vcx, "left");
    assert_eq!(focused(&h, vcx), "a");
}

#[gpui::test]
fn the_actions_are_named_buttons(cx: &mut TestAppContext) {
    cx.update(gpui_core::init);
    gpui_core::audit_start();
    let (_h, vcx) = open_with(cx, Kind::List, None, true);
    vcx.run_until_parked();
    let said = gpui_core::audit_take();
    for label in ["Pin a", "Delete a", "More for b"] {
        assert!(
            said.iter()
                .any(|a| a.role == gpui::Role::Button && a.label.as_deref() == Some(label)),
            "{label} is a named button"
        );
    }
}
