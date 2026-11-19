//! CommandDialog: it opens with the keyboard in its field, typing filters the commands and
//! shows only the headings of groups with a match, the arrows pass over headings, Enter and a
//! press run a command, nothing matching says so, Escape closes it and gives the keyboard back,
//! and nothing redraws while idle.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, ParentElement as _, Render, SharedString,
    Styled as _, TestAppContext, VisualTestContext, Window, div, px, size,
};
use gpui_core::{CommandDialog, DismissReason, IconName, MenuSelect, PopupMenuItem, Press, Root};

struct Panel {
    open: bool,
    opener: FocusHandle,
    picked: Rc<RefCell<Vec<SharedString>>>,
    reasons: Rc<RefCell<Vec<DismissReason>>>,
    draws: Rc<Cell<usize>>,
}

impl Render for Panel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let (picked, reasons) = (self.picked.clone(), self.reasons.clone());
        div()
            .size_full()
            .p(px(10.))
            .child(
                Press::new("opener")
                    .track_focus(&self.opener)
                    .debug_selector(|| "opener".into())
                    .size(px(40.))
                    .child("⌘K")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.open = true;
                        cx.notify();
                    })),
            )
            .child(
                CommandDialog::new("commands")
                    .open(self.open)
                    .group(
                        "Suggestions",
                        [
                            PopupMenuItem::new("calendar", "Calendar").icon(IconName::Calendar),
                            PopupMenuItem::new("search", "Search emoji"),
                        ],
                    )
                    .group(
                        "Settings",
                        [
                            PopupMenuItem::new("profile", "Profile").shortcut("⌘P"),
                            PopupMenuItem::new("billing", "Billing").shortcut("⌘B"),
                        ],
                    )
                    .on_select(cx.listener(move |this, pick: &MenuSelect, _, cx| {
                        picked.borrow_mut().push(pick.id.clone());
                        this.open = false;
                        cx.notify();
                    }))
                    .on_dismiss(cx.listener(move |this, reason: &DismissReason, _, cx| {
                        reasons.borrow_mut().push(*reason);
                        this.open = false;
                        cx.notify();
                    })),
            )
    }
}

struct Opened<'a> {
    panel: Entity<Panel>,
    picked: Rc<RefCell<Vec<SharedString>>>,
    reasons: Rc<RefCell<Vec<DismissReason>>>,
    draws: Rc<Cell<usize>>,
    vcx: &'a mut VisualTestContext,
}

fn open(cx: &mut TestAppContext) -> Opened<'_> {
    cx.update(gpui_core::init);
    let picked: Rc<RefCell<Vec<SharedString>>> = Rc::default();
    let reasons: Rc<RefCell<Vec<DismissReason>>> = Rc::default();
    let draws = Rc::new(Cell::new(0));
    let (p, r, d) = (picked.clone(), reasons.clone(), draws.clone());
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let panel = cx.new(|cx| Panel {
            open: false,
            opener: cx.focus_handle(),
            picked: p,
            reasons: r,
            draws: d,
        });
        *m.borrow_mut() = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    vcx.simulate_resize(size(px(800.), px(600.)));
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let panel = made.borrow().clone().unwrap();
    let o = Opened {
        panel,
        picked,
        reasons,
        draws,
        vcx,
    };
    // Tab to the opener and press it, so the keyboard has somewhere to go back to.
    key(o.vcx, "tab space");
    o
}

fn key(vcx: &mut VisualTestContext, keys: &'static str) {
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

fn shown(vcx: &mut VisualTestContext, selector: String) -> bool {
    vcx.debug_bounds(Box::leak(selector.into_boxed_str()))
        .is_some()
}

fn item(vcx: &mut VisualTestContext, id: &str) -> bool {
    shown(vcx, format!("menu-item-{id}"))
}

fn heading(vcx: &mut VisualTestContext, title: &str) -> bool {
    shown(vcx, format!("menu-heading-{title}"))
}

fn typing(vcx: &mut VisualTestContext, text: &str) {
    vcx.simulate_input(text);
    vcx.run_until_parked();
}

#[gpui::test]
fn it_opens_with_every_group_and_the_field_taking_keys(cx: &mut TestAppContext) {
    let o = open(cx);
    assert!(o.vcx.debug_bounds("gpui-core-dialog").is_some());
    for id in ["calendar", "search", "profile", "billing"] {
        assert!(item(o.vcx, id), "{id}");
    }
    assert!(heading(o.vcx, "Suggestions") && heading(o.vcx, "Settings"));
    // A heading is above its group.
    let (h, first) = (
        o.vcx.debug_bounds("menu-heading-Settings").unwrap(),
        o.vcx.debug_bounds("menu-item-profile").unwrap(),
    );
    assert!(h.bottom() <= first.top());
    // The field has the keyboard: typing filters.
    typing(o.vcx, "bill");
    assert!(item(o.vcx, "billing"));
    assert!(!item(o.vcx, "profile") && !item(o.vcx, "calendar"));
    assert!(heading(o.vcx, "Settings"), "its group's heading stays");
    assert!(
        !heading(o.vcx, "Suggestions"),
        "a group with no match has none"
    );
}

#[gpui::test]
fn the_arrows_pass_over_headings_and_enter_runs_it(cx: &mut TestAppContext) {
    let o = open(cx);
    // Calendar first; down past Search emoji and the Settings heading to Profile.
    key(o.vcx, "down down enter");
    assert_eq!(*o.picked.borrow(), ["profile"]);
    assert!(o.vcx.debug_bounds("gpui-core-dialog").is_none());
}

#[gpui::test]
fn a_press_runs_a_command(cx: &mut TestAppContext) {
    let o = open(cx);
    let at = o.vcx.debug_bounds("menu-item-billing").unwrap().center();
    o.vcx.simulate_click(at, Modifiers::none());
    o.vcx.run_until_parked();
    assert_eq!(*o.picked.borrow(), ["billing"]);
}

#[gpui::test]
fn nothing_matching_says_so(cx: &mut TestAppContext) {
    let o = open(cx);
    typing(o.vcx, "zzz");
    for id in ["calendar", "search", "profile", "billing"] {
        assert!(!item(o.vcx, id), "{id}");
    }
    assert!(!heading(o.vcx, "Suggestions") && !heading(o.vcx, "Settings"));
    key(o.vcx, "enter");
    assert!(o.picked.borrow().is_empty());
}

#[gpui::test]
fn escape_closes_it_and_the_keyboard_goes_back(cx: &mut TestAppContext) {
    let o = open(cx);
    key(o.vcx, "escape");
    assert_eq!(*o.reasons.borrow(), [DismissReason::Escape]);
    let opener = o.panel.read_with(o.vcx, |p, _| p.opener.clone());
    assert!(o.vcx.update(|w, _| opener.is_focused(w)));
    // Opened again, it starts afresh.
    key(o.vcx, "space");
    assert!(item(o.vcx, "calendar") && item(o.vcx, "billing"));
}

#[gpui::test]
fn open_and_idle_it_doesnt_redraw(cx: &mut TestAppContext) {
    let o = open(cx);
    let before = o.draws.get();
    for _ in 0..30 {
        o.vcx.update(|window, cx| window.simulate_next_frame(cx));
        o.vcx
            .executor()
            .advance_clock(std::time::Duration::from_millis(16));
        o.vcx.run_until_parked();
    }
    assert_eq!(o.draws.get(), before);
}

/// A palette of many commands: its list is as tall as its visible rows at most, not the
/// window's, the keys keep the highlighted command in view as they go past them, and its
/// field is a palette's 48 points tall.
#[gpui::test]
fn a_long_palette_scrolls_and_keeps_the_highlight_in_view(cx: &mut TestAppContext) {
    struct Long;
    impl Render for Long {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .child(CommandDialog::new("long").open(true).group(
                    "Pages",
                    (0..40).map(|i| PopupMenuItem::new(format!("c{i}"), format!("Page {i}"))),
                ))
        }
    }
    cx.update(gpui_core::init);
    let (_, vcx) = cx.add_window_view(|window, cx| {
        let v = cx.new(|_| Long);
        Root::new(v, window, cx)
    });
    vcx.simulate_resize(size(px(800.), px(600.)));
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    vcx.update(|window, cx| window.draw(cx).clear(cx));
    let list = vcx.debug_bounds("picker-rows").expect("the list");
    assert!(list.size.height <= px(10. * 32. + 1.), "capped: {list:?}");
    let field = vcx.debug_bounds("picker-field").expect("the field");
    assert!(field.size.height >= px(47.), "a palette's field: {field:?}");
    for i in 1..=20 {
        key(vcx, "down");
        vcx.update(|window, cx| window.draw(cx).clear(cx));
        vcx.update(|window, cx| window.draw(cx).clear(cx));
        let name: &'static str = Box::leak(format!("menu-item-c{i}").into_boxed_str());
        let row = vcx.debug_bounds(name).expect("the row");
        let list = vcx.debug_bounds("picker-rows").unwrap();
        assert!(
            row.top() >= list.top() - px(1.) && row.bottom() <= list.bottom() + px(1.),
            "Page {i}, highlighted, is in view: {row:?} in {list:?}"
        );
    }
}
