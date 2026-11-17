//! A menu's and a picker's highlight stays on its entry while the host's entries come, go and
//! move: Enter picks the entry shown highlighted, a picker whose entries come after it opens
//! highlights one, and when the highlighted entry goes the first takes it.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    AppContext as _, Context, Entity, IntoElement, KeyDownEvent, KeyUpEvent, Keystroke,
    ParentElement as _, Render, SharedString, Styled as _, TestAppContext, VisualTestContext,
    Window, div, px,
};
use gpui_core::{MenuSelect, Picker, PopupMenu, PopupMenuItem, Root};

struct Host {
    picker: bool,
    items: Vec<&'static str>,
    picked: Rc<RefCell<Vec<SharedString>>>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let picked = self.picked.clone();
        let pick = move |p: &MenuSelect, _: &mut Window, _: &mut gpui::App| {
            picked.borrow_mut().push(p.id.clone())
        };
        let items = self.items.iter().map(|t| PopupMenuItem::new(*t, *t));
        let body = if self.picker {
            Picker::new("p")
                .items(items)
                .on_select(pick)
                .into_any_element()
        } else {
            PopupMenu::new("m")
                .items(items)
                .on_select(pick)
                .into_any_element()
        };
        div().p(px(20.)).w(px(260.)).child(body)
    }
}

fn open<'a>(
    cx: &'a mut TestAppContext,
    picker: bool,
    items: Vec<&'static str>,
) -> (Entity<Host>, &'a mut VisualTestContext) {
    cx.update(gpui_core::init);
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let h = cx.new(|_| Host {
            picker,
            items,
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

fn set(vcx: &mut VisualTestContext, h: &Entity<Host>, items: Vec<&'static str>) {
    vcx.update(|_, cx| {
        h.update(cx, |h, cx| {
            h.items = items;
            cx.notify();
        })
    });
    vcx.run_until_parked();
}

fn picked(vcx: &mut VisualTestContext, h: &Entity<Host>) -> Vec<String> {
    h.read_with(vcx, |h, _| {
        h.picked.borrow().iter().map(|s| s.to_string()).collect()
    })
}

#[gpui::test]
fn a_menus_highlight_follows_its_entry_when_one_comes_above_it(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, false, vec!["a", "b", "c"]);
    keys(vcx, "down");
    set(vcx, &h, vec!["new", "a", "b", "c"]);
    keys(vcx, "enter");
    assert_eq!(picked(vcx, &h), ["b"]);
}

#[gpui::test]
fn when_the_highlighted_entry_goes_the_first_takes_it(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, false, vec!["a", "b", "c"]);
    keys(vcx, "down");
    set(vcx, &h, vec!["a", "c"]);
    keys(vcx, "enter");
    assert_eq!(picked(vcx, &h), ["a"]);
}

#[gpui::test]
fn a_pickers_highlight_follows_its_entry_when_the_host_sorts_again(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, true, vec!["a", "b", "c"]);
    keys(vcx, "down");
    set(vcx, &h, vec!["c", "b", "a"]);
    set(vcx, &h, vec!["b", "c", "a"]);
    keys(vcx, "enter");
    assert_eq!(picked(vcx, &h), ["b"]);
}

#[gpui::test]
fn a_picker_whose_entries_come_after_it_opens_highlights_one(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, true, vec![]);
    set(vcx, &h, vec!["a", "b"]);
    keys(vcx, "enter");
    assert_eq!(picked(vcx, &h), ["a"]);
}
