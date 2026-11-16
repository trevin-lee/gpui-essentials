//! Picker: the field has the keyboard as it opens, typing filters the entries (every word,
//! case aside), a new query highlights the first match, Up and Down move through the matches
//! without moving the field's caret, Enter picks, nothing matching shows a note, disabled
//! entries are passed over, and Escape closes its popover. Laid out with Core Text.

#![cfg(target_os = "macos")]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    AppContext as _, Context, Entity, FocusHandle, IntoElement, KeyDownEvent, KeyUpEvent,
    Keystroke, ParentElement as _, Render, SharedString, Styled as _, TestAppContext,
    TestDispatcher, VisualTestContext, Window, div, px,
};
use gpui_core::{Picker, Popover, PopupMenuItem, Press, Root};

struct Panel {
    open: bool,
    /// Whether Home is the current option.
    current: bool,
    trigger: FocusHandle,
    picked: Rc<RefCell<Vec<SharedString>>>,
}

impl Render for Panel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let picked = self.picked.clone();
        div().size_full().p(px(20.)).child(
            Popover::new("pop")
                .open(self.open)
                .on_dismiss(cx.listener(|this, _, _, cx| {
                    this.open = false;
                    cx.notify();
                }))
                .trigger(
                    Press::new("trigger")
                        .track_focus(&self.trigger)
                        .w(px(120.))
                        .h(px(30.))
                        .child("Tag")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.open = !this.open;
                            cx.notify();
                        })),
                )
                .w(px(240.))
                .p(px(4.))
                .child(
                    Picker::new("picker")
                        .placeholder("Find a tag")
                        .item(PopupMenuItem::new("work", "Work"))
                        .item(PopupMenuItem::new("home", "Home").checked(self.current))
                        .item(PopupMenuItem::new("archived", "Archived").disabled(true))
                        .item(PopupMenuItem::new("errands", "Errands and shopping"))
                        .on_select(cx.listener(move |this, id: &SharedString, _, cx| {
                            picked.borrow_mut().push(id.clone());
                            this.open = false;
                            cx.notify();
                        })),
                ),
        )
    }
}

struct Opened<'a> {
    panel: Entity<Panel>,
    picked: Rc<RefCell<Vec<SharedString>>>,
    vcx: &'a mut VisualTestContext,
}

fn open(cx: &mut TestAppContext) -> Opened<'_> {
    open_with(cx, false)
}

fn open_with(cx: &mut TestAppContext, current: bool) -> Opened<'_> {
    cx.update(gpui_core::init);
    let picked: Rc<RefCell<Vec<SharedString>>> = Rc::default();
    let p = picked.clone();
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let panel = cx.new(|cx| Panel {
            open: false,
            current,
            trigger: cx.focus_handle(),
            picked: p,
        });
        *m.borrow_mut() = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let panel = made.borrow().clone().unwrap();
    keys(vcx, "tab space");
    Opened { panel, picked, vcx }
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

fn shown(vcx: &mut VisualTestContext) -> Vec<&'static str> {
    ["work", "home", "archived", "errands"]
        .into_iter()
        .filter(|id| {
            let sel: &'static str = match *id {
                "work" => "menu-item-work",
                "home" => "menu-item-home",
                "archived" => "menu-item-archived",
                _ => "menu-item-errands",
            };
            vcx.debug_bounds(sel).is_some()
        })
        .collect()
}

fn picked(o: &Opened<'_>) -> Vec<String> {
    o.picked.borrow().iter().map(|s| s.to_string()).collect()
}

#[test]
fn typing_filters_and_enter_picks_the_first_match() {
    let mut cx = app();
    let o = open(&mut cx);
    assert_eq!(shown(o.vcx), ["work", "home", "archived", "errands"]);
    o.vcx.simulate_input("HOM");
    assert_eq!(shown(o.vcx), ["home"]);
    keys(o.vcx, "enter");
    assert_eq!(picked(&o), ["home"]);
}

#[test]
fn every_word_of_the_query_has_to_match() {
    let mut cx = app();
    let o = open(&mut cx);
    o.vcx.simulate_input("shop err");
    assert_eq!(shown(o.vcx), ["errands"]);
}

#[test]
fn the_arrows_move_through_the_matches_not_the_caret() {
    let mut cx = app();
    let o = open(&mut cx);
    // Work is highlighted; down passes Home... and Archived, which is disabled.
    keys(o.vcx, "down down enter");
    assert_eq!(picked(&o), ["errands"]);
    // Typing, then Up, then typing again: the caret stayed at the end.
    keys(o.vcx, "space");
    o.vcx.simulate_input("o");
    keys(o.vcx, "up");
    o.vcx.simulate_input("r");
    assert_eq!(shown(o.vcx), ["work"], "the query is \"or\", not \"ro\"");
}

#[test]
fn nothing_matching_shows_a_note_and_enter_picks_nothing() {
    let mut cx = app();
    let o = open(&mut cx);
    o.vcx.simulate_input("zzz");
    assert!(shown(o.vcx).is_empty());
    keys(o.vcx, "enter");
    assert!(o.picked.borrow().is_empty());
}

#[test]
fn escape_closes_it_and_the_focus_goes_back() {
    let mut cx = app();
    let o = open(&mut cx);
    o.vcx.simulate_input("wo");
    keys(o.vcx, "escape");
    assert!(shown(o.vcx).is_empty(), "closed");
    assert!(o.picked.borrow().is_empty());
    let trigger = o.panel.read_with(o.vcx, |p, _| p.trigger.clone());
    assert!(o.vcx.update(|w, _| trigger.is_focused(w)));
}

#[test]
fn it_opens_on_the_current_option() {
    let mut cx = app();
    let o = open_with(&mut cx, true);
    keys(o.vcx, "enter");
    assert_eq!(picked(&o), ["home"]);
}
