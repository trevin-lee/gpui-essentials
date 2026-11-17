//! SearchList: typing filters the rows (every word, case aside), Up and Down move the highlight
//! while the field keeps the keyboard, Enter and a press open a row, the selected row is drawn
//! selected, Escape clears the search (and then is left to what's around it), the highlight
//! stays on its row when the host re-sorts, a list of 5,000 draws only the rows in view and keeps
//! the highlight in view, touch rows are 44 points, and nothing redraws while idle. Laid out with
//! Core Text.

#![cfg(target_os = "macos")]

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui::{
    AppContext as _, Context, Entity, InteractiveElement as _, IntoElement, KeyBinding,
    KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, ParentElement as _, Render, SharedString,
    Styled as _, TestAppContext, TestDispatcher, VisualTestContext, Window, actions, div, px,
};
use gpui_core::{Root, SearchList};

actions!(search_list_test, [OuterEscape]);

const VAULT: [(&str, &str); 5] = [
    ("gh", "GitHub trevin@example.com"),
    ("gm", "Google Mail trevin"),
    ("bank", "Chase Bank card"),
    ("ssh", "Server SSH key"),
    ("gl", "GitLab work"),
];

struct Host {
    items: Vec<(SharedString, SharedString)>,
    selected: Option<SharedString>,
    touch: bool,
    row_height: Option<f32>,
    list_height: f32,
    opened: Rc<RefCell<Vec<SharedString>>>,
    /// Each row's (highlighted, selected), as drawn last frame.
    rows: Rc<RefCell<HashMap<SharedString, (bool, bool)>>>,
    /// How many rows the last frame asked the host to draw.
    row_calls: Rc<Cell<usize>>,
    renders: Rc<Cell<usize>>,
    /// Escapes that reached past the list.
    escapes: Rc<Cell<usize>>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        self.rows.borrow_mut().clear();
        self.row_calls.set(0);
        let (rows, calls, escapes) = (
            self.rows.clone(),
            self.row_calls.clone(),
            self.escapes.clone(),
        );
        let mut list = SearchList::new("vault")
            .placeholder("Search the vault")
            .accessibility_label("Vault")
            .empty_text("Nothing in the vault matches")
            .items(self.items.clone())
            .touch(self.touch)
            .render_row(move |id, highlighted, selected, _, _| {
                calls.set(calls.get() + 1);
                rows.borrow_mut()
                    .insert(id.clone(), (highlighted, selected));
                div().child(id.clone())
            })
            .on_open(cx.listener(|this, id: &SharedString, _, cx| {
                this.opened.borrow_mut().push(id.clone());
                this.selected = Some(id.clone());
                cx.notify();
            }));
        if let Some(s) = self.selected.clone() {
            list = list.selected(s);
        }
        if let Some(h) = self.row_height {
            list = list.row_height(px(h));
        }
        div()
            .key_context("Outer")
            .on_action(move |_: &OuterEscape, _, _| escapes.set(escapes.get() + 1))
            .size_full()
            .p(px(20.))
            .child(div().w(px(300.)).h(px(self.list_height)).child(list))
    }
}

struct Opened<'a> {
    host: Entity<Host>,
    vcx: &'a mut VisualTestContext,
}

fn app() -> TestAppContext {
    TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        Arc::new(gpui_apple::AppleTextSystem::new()),
    )
}

fn vault() -> Vec<(SharedString, SharedString)> {
    VAULT
        .iter()
        .map(|(i, t)| ((*i).into(), (*t).into()))
        .collect()
}

fn open(cx: &mut TestAppContext, items: Vec<(SharedString, SharedString)>) -> Opened<'_> {
    open_with(cx, items, |_| {})
}

fn open_with(
    cx: &mut TestAppContext,
    items: Vec<(SharedString, SharedString)>,
    set: impl FnOnce(&mut Host) + 'static,
) -> Opened<'_> {
    cx.update(gpui_core::init);
    cx.update(|cx| cx.bind_keys([KeyBinding::new("escape", OuterEscape, Some("Outer"))]));
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let host = cx.new(|_| {
            let mut h = Host {
                items,
                selected: None,
                touch: false,
                row_height: None,
                list_height: 300.,
                opened: Rc::default(),
                rows: Rc::default(),
                row_calls: Rc::default(),
                renders: Rc::default(),
                escapes: Rc::default(),
            };
            set(&mut h);
            h
        });
        *m.borrow_mut() = Some(host.clone());
        Root::new(host, window, cx)
    });
    vcx.simulate_resize(gpui::size(px(400.), px(800.)));
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let host = made.borrow().clone().unwrap();
    Opened { host, vcx }
}

fn keys(vcx: &mut VisualTestContext, keys: &str) {
    for k in keys.split(' ') {
        let k: &'static str = Box::leak(k.to_string().into_boxed_str());
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

fn row(id: &str) -> &'static str {
    Box::leak(format!("search-row-{id}").into_boxed_str())
}

/// The vault's rows drawn, in order.
fn shown(vcx: &mut VisualTestContext) -> Vec<&'static str> {
    VAULT
        .iter()
        .map(|(id, _)| *id)
        .filter(|id| vcx.debug_bounds(row(id)).is_some())
        .collect()
}

fn highlighted(o: &mut Opened<'_>) -> Option<String> {
    o.host.read_with(o.vcx, |h, _| {
        h.rows
            .borrow()
            .iter()
            .find(|(_, (on, _))| *on)
            .map(|(id, _)| id.to_string())
    })
}

fn selected_rows(o: &mut Opened<'_>) -> Vec<String> {
    o.host.read_with(o.vcx, |h, _| {
        h.rows
            .borrow()
            .iter()
            .filter(|(_, (_, sel))| *sel)
            .map(|(id, _)| id.to_string())
            .collect()
    })
}

fn opened(o: &mut Opened<'_>) -> Vec<String> {
    o.host.read_with(o.vcx, |h, _| {
        h.opened.borrow().iter().map(|s| s.to_string()).collect()
    })
}

fn click(vcx: &mut VisualTestContext, which: &'static str, count: usize) {
    let at = vcx.debug_bounds(which).expect("drawn").center();
    vcx.simulate_event(gpui::MouseDownEvent {
        button: gpui::MouseButton::Left,
        position: at,
        modifiers: Modifiers::none(),
        click_count: count,
        first_mouse: false,
    });
    vcx.simulate_event(gpui::MouseUpEvent {
        button: gpui::MouseButton::Left,
        position: at,
        modifiers: Modifiers::none(),
        click_count: count,
    });
    vcx.run_until_parked();
}

#[test]
fn typing_filters_by_every_word_case_aside() {
    let mut cx = app();
    let o = open(&mut cx, vault());
    assert_eq!(shown(o.vcx), ["gh", "gm", "bank", "ssh", "gl"]);
    keys(o.vcx, "tab");
    o.vcx.simulate_input("GIT");
    assert_eq!(shown(o.vcx), ["gh", "gl"]);
    o.vcx.simulate_input(" work");
    assert_eq!(
        shown(o.vcx),
        ["gl"],
        "both words, in either order of the text"
    );
    o.vcx.simulate_input(" zzz");
    assert!(shown(o.vcx).is_empty());
    assert!(o.vcx.debug_bounds("search-list-empty").is_some());
}

#[test]
fn up_and_down_move_the_highlight_while_the_field_keeps_the_keyboard() {
    let mut cx = app();
    let mut o = open(&mut cx, vault());
    // Without the keyboard the highlight isn't shown.
    assert_eq!(highlighted(&mut o), None);
    keys(o.vcx, "tab");
    assert_eq!(highlighted(&mut o).as_deref(), Some("gh"));
    keys(o.vcx, "down down");
    assert_eq!(highlighted(&mut o).as_deref(), Some("bank"));
    keys(o.vcx, "up");
    assert_eq!(highlighted(&mut o).as_deref(), Some("gm"));
    // It stops at the ends.
    keys(o.vcx, "up up up");
    assert_eq!(highlighted(&mut o).as_deref(), Some("gh"));
    // Typing still goes to the field, the caret where it was: "gi", Up, "t" is "git".
    o.vcx.simulate_input("gi");
    keys(o.vcx, "down up");
    o.vcx.simulate_input("t");
    assert_eq!(shown(o.vcx), ["gh", "gl"], "the query is \"git\"");
}

#[test]
fn enter_opens_the_highlighted_row() {
    let mut cx = app();
    let mut o = open(&mut cx, vault());
    keys(o.vcx, "tab down enter");
    assert_eq!(opened(&mut o), ["gm"]);
    // A new search highlights its first match.
    o.vcx.simulate_input("ssh");
    keys(o.vcx, "enter");
    assert_eq!(opened(&mut o), ["gm", "ssh"]);
    // Nothing matching: Enter opens nothing.
    o.vcx.simulate_input("zzz");
    keys(o.vcx, "enter");
    assert_eq!(opened(&mut o), ["gm", "ssh"]);
}

#[test]
fn a_press_opens_a_row_once_and_the_keys_go_on_from_it() {
    let mut cx = app();
    let mut o = open(&mut cx, vault());
    click(o.vcx, row("bank"), 1);
    assert_eq!(opened(&mut o), ["bank"]);
    // The second press of a double press doesn't open it again.
    click(o.vcx, row("bank"), 2);
    assert_eq!(opened(&mut o), ["bank"]);
    // The list has the keyboard now, from the row pressed.
    assert_eq!(highlighted(&mut o).as_deref(), Some("bank"));
    keys(o.vcx, "down enter");
    assert_eq!(opened(&mut o), ["bank", "ssh"]);
}

#[test]
fn the_selected_row_is_drawn_selected_and_highlighted() {
    let mut cx = app();
    let mut o = open_with(&mut cx, vault(), |h| h.selected = Some("bank".into()));
    assert_eq!(selected_rows(&mut o), ["bank"]);
    keys(o.vcx, "tab");
    assert_eq!(highlighted(&mut o).as_deref(), Some("bank"));
    // Opening another moves both.
    keys(o.vcx, "down enter");
    assert_eq!(selected_rows(&mut o), ["ssh"]);
    assert_eq!(highlighted(&mut o).as_deref(), Some("ssh"));
    // The host selecting one moves the highlight to it.
    o.host.update(o.vcx, |h, cx| {
        h.selected = Some("gh".into());
        cx.notify();
    });
    o.vcx.run_until_parked();
    assert_eq!(highlighted(&mut o).as_deref(), Some("gh"));
}

#[test]
fn escape_clears_the_search_then_is_left_to_whats_around() {
    let mut cx = app();
    let mut o = open_with(&mut cx, vault(), |h| h.selected = Some("ssh".into()));
    keys(o.vcx, "tab");
    o.vcx.simulate_input("git");
    assert_eq!(shown(o.vcx), ["gh", "gl"]);
    keys(o.vcx, "escape");
    assert_eq!(shown(o.vcx), ["gh", "gm", "bank", "ssh", "gl"]);
    // Cleared, the highlight is back on the open row.
    assert_eq!(highlighted(&mut o).as_deref(), Some("ssh"));
    let escapes = o.host.read_with(o.vcx, |h, _| h.escapes.clone());
    assert_eq!(escapes.get(), 0, "the list took the first");
    keys(o.vcx, "escape");
    assert_eq!(escapes.get(), 1, "with nothing typed it went on");
    // The field still has the keyboard.
    o.vcx.simulate_input("bank");
    assert_eq!(shown(o.vcx), ["bank"]);
}

#[test]
fn the_highlight_stays_on_its_row_when_the_host_re_sorts() {
    let mut cx = app();
    let mut o = open(&mut cx, vault());
    keys(o.vcx, "tab down down");
    assert_eq!(highlighted(&mut o).as_deref(), Some("bank"));
    // Sorted by title, Chase comes first.
    o.host.update(o.vcx, |h, cx| {
        h.items.sort_by(|a, b| a.1.cmp(&b.1));
        cx.notify();
    });
    o.vcx.run_until_parked();
    assert_eq!(highlighted(&mut o).as_deref(), Some("bank"));
    keys(o.vcx, "enter");
    assert_eq!(opened(&mut o), ["bank"]);
    // And from there the keys go by the new order: below Chase is GitHub now.
    keys(o.vcx, "down enter");
    assert_eq!(opened(&mut o), ["bank", "gh"]);
}

fn many(n: usize) -> Vec<(SharedString, SharedString)> {
    (0..n)
        .map(|i| (format!("p{i}").into(), format!("Login {i}").into()))
        .collect()
}

fn drawn(vcx: &mut VisualTestContext, i: usize) -> bool {
    vcx.debug_bounds(row(&format!("p{i}"))).is_some()
}

#[test]
fn five_thousand_rows_draw_only_those_in_view_and_keep_the_highlight_there() {
    let mut cx = app();
    let mut o = open(&mut cx, many(5000));
    let made = (0..5000).filter(|i| drawn(o.vcx, *i)).count();
    // 300 pixels less the field: about seven rows of 32.
    assert!((5..=12).contains(&made), "{made} rows drawn");
    let calls = o.host.read_with(o.vcx, |h, _| h.row_calls.get());
    assert!(calls <= made + 3, "{calls} rows made for {made} drawn");
    keys(o.vcx, "tab");
    keys(o.vcx, &["down"; 300].join(" "));
    assert_eq!(highlighted(&mut o).as_deref(), Some("p300"));
    assert!(drawn(o.vcx, 300), "the highlight is in view");
    assert!(!drawn(o.vcx, 0));
    let made = (0..5000).filter(|i| drawn(o.vcx, *i)).count();
    assert!(
        (5..=12).contains(&made),
        "{made} rows drawn after scrolling"
    );
    keys(o.vcx, "enter");
    assert_eq!(opened(&mut o), ["p300"]);
    // A search far down the list brings its first match into view.
    o.vcx.simulate_input("4999");
    assert!(drawn(o.vcx, 4999));
    keys(o.vcx, "enter");
    assert_eq!(opened(&mut o), ["p300", "p4999"]);
}

#[test]
fn touch_rows_are_at_least_44_points() {
    let mut cx = app();
    let o = open_with(&mut cx, vault(), |h| {
        h.touch = true;
        h.row_height = Some(20.);
    });
    let b = o.vcx.debug_bounds(row("gh")).unwrap();
    assert!(b.size.height >= px(44.), "{b:?}");
    let f = o.vcx.debug_bounds("search-list-field").unwrap();
    assert!(f.size.height >= px(44.), "{f:?}");
}

#[test]
fn a_set_row_height_is_every_rows() {
    let mut cx = app();
    let o = open_with(&mut cx, vault(), |h| h.row_height = Some(48.));
    for (id, _) in VAULT {
        let b = o.vcx.debug_bounds(row(id)).unwrap();
        assert_eq!(b.size.height, px(48.), "{id}");
    }
}

#[test]
fn nothing_redraws_while_idle() {
    let mut cx = app();
    let o = open(&mut cx, many(5000));
    keys(o.vcx, "tab down down");
    o.vcx.simulate_input("Login 1");
    let renders = o.host.read_with(o.vcx, |h, _| h.renders.clone());
    let before = renders.get();
    for _ in 0..60 {
        o.vcx.executor().advance_clock(Duration::from_millis(16));
        let asked = o.vcx.update(|window, cx| window.simulate_next_frame(cx));
        assert_eq!(asked, 0, "a frame was asked for while nothing changed");
        o.vcx.run_until_parked();
    }
    assert_eq!(renders.get(), before);
}
