//! PopupMenu::move_highlight: the host moves an open menu's highlight to an entry, once per new
//! id (or, with move_highlight_request, per new request), and the keys move it on from there.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    AppContext as _, Context, Entity, IntoElement, KeyDownEvent, KeyUpEvent, Keystroke,
    ParentElement as _, Render, SharedString, Styled as _, TestAppContext, VisualTestContext,
    Window, div, px,
};
use gpui_core::{MenuSelect, PopupMenu, PopupMenuItem, Root};

struct Host {
    moved: Option<&'static str>,
    request: Option<u64>,
    picked: Rc<RefCell<Vec<SharedString>>>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let picked = self.picked.clone();
        let mut menu = PopupMenu::new("m")
            .items(["none", "day", "week", "month"].map(|t| PopupMenuItem::new(t, t)))
            .on_select(move |p: &MenuSelect, _, _| picked.borrow_mut().push(p.id.clone()));
        match (self.moved, self.request) {
            (Some(id), Some(n)) => menu = menu.move_highlight_request(id, n),
            (Some(id), None) => menu = menu.move_highlight(id),
            _ => {}
        }
        div().p(px(20.)).w(px(260.)).child(menu)
    }
}

fn open(cx: &mut TestAppContext) -> (Entity<Host>, &mut VisualTestContext) {
    cx.update(gpui_core::init);
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let h = cx.new(|_| Host {
            moved: None,
            request: None,
            picked: Rc::default(),
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

fn picked(h: &Entity<Host>, vcx: &mut VisualTestContext) -> Vec<String> {
    h.read_with(vcx, |h, _| {
        h.picked.borrow().iter().map(|s| s.to_string()).collect()
    })
}

fn moved(vcx: &mut VisualTestContext, h: &Entity<Host>, to: &'static str) {
    vcx.update(|_, cx| {
        h.update(cx, |h, cx| {
            h.moved = Some(to);
            cx.notify();
        })
    });
    vcx.run_until_parked();
}

#[gpui::test]
fn the_host_moves_an_open_menus_highlight_and_the_keys_go_on_from_there(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx);
    moved(vcx, &h, "week");
    keys(vcx, "enter");
    assert_eq!(picked(&h, vcx), ["week"]);
    // The same id again doesn't pull the highlight back once the keys have moved it.
    keys(vcx, "down");
    vcx.update(|_, cx| h.update(cx, |_, cx| cx.notify()));
    vcx.run_until_parked();
    keys(vcx, "enter");
    assert_eq!(picked(&h, vcx), ["week", "month"]);
    // A new id moves it again.
    moved(vcx, &h, "day");
    keys(vcx, "enter");
    assert_eq!(picked(&h, vcx), ["week", "month", "day"]);
}

fn requested(vcx: &mut VisualTestContext, h: &Entity<Host>, to: &'static str, n: u64) {
    vcx.update(|_, cx| {
        h.update(cx, |h, cx| {
            h.moved = Some(to);
            h.request = Some(n);
            cx.notify();
        })
    });
    vcx.run_until_parked();
}

#[gpui::test]
fn a_new_request_moves_the_highlight_to_the_same_entry_again(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx);
    requested(vcx, &h, "week", 1);
    keys(vcx, "down");
    keys(vcx, "enter");
    assert_eq!(picked(&h, vcx), ["month"]);
    // The same request again leaves the highlight where the keys put it.
    keys(vcx, "up up");
    requested(vcx, &h, "week", 1);
    keys(vcx, "enter");
    assert_eq!(picked(&h, vcx), ["month", "day"]);
    // The next request for the same entry moves it there again.
    requested(vcx, &h, "week", 2);
    keys(vcx, "enter");
    assert_eq!(picked(&h, vcx), ["month", "day", "week"]);
}
