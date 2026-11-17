//! A picker's field: a full touch target on touch as its rows are, starting with the host's
//! text (a step come back to keeps its name), stopping at the host's length, and with its
//! magnifying glass only while it filters unless the host says. Laid out with Core Text.

#![cfg(target_os = "macos")]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    AppContext as _, Context, IntoElement, KeyDownEvent, KeyUpEvent, Keystroke, ParentElement as _,
    Render, Styled as _, TestAppContext, TestDispatcher, VisualTestContext, Window, div, px,
};
use gpui_core::{MenuSelect, Picker, PopupMenuItem, Root};

type Log = Rc<RefCell<Vec<MenuSelect>>>;

#[derive(Clone, Copy)]
enum Kind {
    Plain,
    Touch,
    /// A naming field come back to, with what was typed.
    Back,
    Limited,
    Naming,
    NamingWithGlass,
}

struct Panel(Kind, Log);

impl Render for Panel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let log = self.1.clone();
        let items = ["Text", "Number", "Date"].map(|t| PopupMenuItem::new(t.to_lowercase(), t));
        let p = Picker::new("p")
            .items(items)
            .on_select(move |p: &MenuSelect, _, _| log.borrow_mut().push(p.clone()));
        let p = match self.0 {
            Kind::Plain => p,
            Kind::Touch => p.touch(true),
            Kind::Back => p.filter(false).query("Budget"),
            Kind::Limited => p.filter(false).max_chars(5),
            Kind::Naming => p.filter(false),
            Kind::NamingWithGlass => p.filter(false).search_icon(true),
        };
        div().p(px(20.)).w(px(260.)).child(p)
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

fn enter(vcx: &mut VisualTestContext) {
    let keystroke = Keystroke::parse("enter").unwrap();
    vcx.simulate_event(KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    vcx.simulate_event(KeyUpEvent { keystroke });
    vcx.run_until_parked();
}

fn picked_query(log: &Log) -> Option<String> {
    log.borrow()
        .last()
        .and_then(|p| p.query.as_ref().map(|q| q.to_string()))
}

#[test]
fn on_touch_the_field_is_as_tall_as_a_row() {
    let mut cx = app();
    let (_, vcx) = open(&mut cx, Kind::Touch);
    let field = vcx.debug_bounds("picker-field").unwrap();
    let row = vcx.debug_bounds("menu-item-text").unwrap();
    assert!(field.size.height >= px(44.), "{field:?}");
    assert!(row.size.height >= px(44.), "{row:?}");
    let mut cx = app();
    let (_, vcx) = open(&mut cx, Kind::Plain);
    let field = vcx.debug_bounds("picker-field").unwrap();
    assert!(field.size.height < px(44.), "not on a pointer: {field:?}");
}

#[test]
fn the_field_starts_with_the_hosts_text_and_typing_goes_on_from_it() {
    let mut cx = app();
    let (log, vcx) = open(&mut cx, Kind::Back);
    vcx.simulate_input(" 2027");
    enter(vcx);
    assert_eq!(picked_query(&log).as_deref(), Some("Budget 2027"));
}

#[test]
fn the_field_stops_at_the_hosts_length() {
    let mut cx = app();
    let (log, vcx) = open(&mut cx, Kind::Limited);
    vcx.simulate_input("Quarterly");
    enter(vcx);
    assert_eq!(picked_query(&log).as_deref(), Some("Quart"));
}

#[test]
fn the_glass_shows_while_the_field_filters_unless_the_host_says() {
    let mut cx = app();
    let (_, vcx) = open(&mut cx, Kind::Plain);
    assert!(vcx.debug_bounds("picker-glass").is_some());
    let mut cx = app();
    let (_, vcx) = open(&mut cx, Kind::Naming);
    assert!(vcx.debug_bounds("picker-glass").is_none());
    let mut cx = app();
    let (_, vcx) = open(&mut cx, Kind::NamingWithGlass);
    assert!(vcx.debug_bounds("picker-glass").is_some());
}
