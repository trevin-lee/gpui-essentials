//! A picker whose field names rather than filters: every entry stays as it's typed in, the
//! highlight stays where the keys put it, and a pick carries what's typed. A filtering picker's
//! picks carry their query too; a menu's carry none. Laid out with Core Text.

#![cfg(target_os = "macos")]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    AppContext as _, Context, IntoElement, KeyDownEvent, KeyUpEvent, Keystroke, ParentElement as _,
    Render, Styled as _, TestAppContext, TestDispatcher, VisualTestContext, Window, div, px,
};
use gpui_core::{MenuSelect, Picker, PopupMenu, PopupMenuItem, Root};

type Log = Rc<RefCell<Vec<MenuSelect>>>;

#[derive(Clone, Copy)]
enum Kind {
    Naming,
    Filtering,
    Menu,
}

struct Panel(Kind, Log);

impl Render for Panel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let log = self.1.clone();
        let pick = move |p: &MenuSelect, _: &mut Window, _: &mut gpui::App| {
            log.borrow_mut().push(p.clone())
        };
        let items = ["Text", "Number", "Date"].map(|t| PopupMenuItem::new(t.to_lowercase(), t));
        let body = match self.0 {
            Kind::Naming => Picker::new("p")
                .filter(false)
                .placeholder("Property name")
                .items(items)
                .on_select(pick)
                .into_any_element(),
            Kind::Filtering => Picker::new("p")
                .items(items)
                .on_select(pick)
                .into_any_element(),
            Kind::Menu => PopupMenu::new("m")
                .items(items)
                .on_select(pick)
                .into_any_element(),
        };
        div().p(px(20.)).w(px(260.)).child(body)
    }
}

fn open(cx: &mut TestAppContext, kind: Kind) -> (Log, &mut VisualTestContext) {
    cx.update(gpui_core::init);
    let log: Log = Rc::default();
    let l = log.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let v = cx.new(|_| Panel(kind, l));
        Root::new(v, window, cx)
    });
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    (log, vcx)
}

fn app() -> TestAppContext {
    TestAppContext::build_with_text_system(
        TestDispatcher::new(0),
        None,
        Arc::new(gpui_apple::AppleTextSystem::new()),
    )
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

fn shown(vcx: &mut VisualTestContext) -> usize {
    ["menu-item-text", "menu-item-number", "menu-item-date"]
        .into_iter()
        .filter(|s| vcx.debug_bounds(s).is_some())
        .count()
}

#[test]
fn a_naming_field_keeps_the_list_and_its_highlight_and_the_pick_carries_the_name() {
    let mut cx = app();
    let (log, vcx) = open(&mut cx, Kind::Naming);
    keys(vcx, "down");
    vcx.simulate_input("Budget");
    assert_eq!(shown(vcx), 3, "nothing filtered");
    keys(vcx, "enter");
    let got = log.borrow().clone();
    assert_eq!(got.len(), 1);
    // Down moved to Number before typing, and typing didn't move it back.
    assert_eq!(got[0].id.as_ref(), "number");
    assert_eq!(got[0].query.as_deref(), Some("Budget"));
}

#[test]
fn a_filtering_pickers_pick_carries_its_query() {
    let mut cx = app();
    let (log, vcx) = open(&mut cx, Kind::Filtering);
    vcx.simulate_input("dat");
    assert_eq!(shown(vcx), 1);
    keys(vcx, "enter");
    let got = log.borrow().clone();
    assert_eq!(
        (got[0].id.as_ref(), got[0].query.as_deref()),
        ("date", Some("dat"))
    );
}

#[test]
fn a_menus_pick_carries_no_query() {
    let mut cx = app();
    let (log, vcx) = open(&mut cx, Kind::Menu);
    keys(vcx, "enter");
    assert_eq!(log.borrow()[0].query, None);
}
