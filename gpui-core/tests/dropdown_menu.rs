//! DropdownMenu: a click, a tap, Enter, Space or Down on the trigger opens it (from the keys with
//! the first entry highlighted, from the pointer with none), a second press closes it, a pick runs
//! on_select and closes it, Escape closes it and gives the keyboard back to the trigger, a press
//! outside closes it, it lines up with each side and alignment, a tap gives rows for touch, a
//! disabled one doesn't open, and nothing redraws while idle. (That the trigger is told expanded
//! is the crate's own test: GPUI builds no tree to read here.)

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AppContext as _, Bounds, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, ParentElement as _, Pixels, Point, Render,
    Styled as _, TestAppContext, TouchEvent, TouchId, TouchPhase, VisualTestContext, Window, div,
    px, size,
};
use gpui_core::{
    Button, ButtonVariants as _, Disableable as _, DropdownMenu, PopoverAlign, PopoverSide,
    PopupMenuItem, Press, Root,
};

struct Host {
    side: PopoverSide,
    align: PopoverAlign,
    touch: bool,
    disabled: bool,
    picked: Rc<RefCell<Vec<String>>>,
    draws: Rc<Cell<usize>>,
    before: FocusHandle,
    trigger: FocusHandle,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let picked = self.picked.clone();
        let picked_too = self.picked.clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(10.))
            .child(
                Press::new("before")
                    .track_focus(&self.before)
                    .debug_selector(|| "before".into())
                    .child("Before"),
            )
            .child(
                div().flex().items_start().child(
                    div().debug_selector(|| "button-box".into()).child(
                        DropdownMenu::new("account")
                            .trigger(Button::new("account-button").outline().label("Open"))
                            .heading("My Account")
                            .items([
                                PopupMenuItem::new("profile", "Profile").shortcut("⇧⌘P"),
                                PopupMenuItem::new("billing", "Billing"),
                                PopupMenuItem::new("team", "Team").disabled(true),
                            ])
                            .separator()
                            .item(PopupMenuItem::new("log-out", "Log out"))
                            .touch(self.touch)
                            .disabled(self.disabled)
                            .on_select(move |pick, _, _| {
                                picked.borrow_mut().push(pick.id.to_string())
                            }),
                    ),
                ),
            )
            .child(
                DropdownMenu::new("placed")
                    .trigger(
                        Press::new("placed-trigger")
                            .track_focus(&self.trigger)
                            .debug_selector(|| "placed-trigger".into())
                            .w(px(160.))
                            .h(px(36.))
                            .child("A wide trigger"),
                    )
                    .item(PopupMenuItem::new("one", "One"))
                    .item(PopupMenuItem::new("two", "Two"))
                    .side(self.side)
                    .align(self.align)
                    .on_select(move |pick, _, _| picked_too.borrow_mut().push(pick.id.to_string())),
            )
    }
}

struct Opened<'a> {
    host: Entity<Host>,
    vcx: &'a mut VisualTestContext,
}

fn open(cx: &mut TestAppContext) -> Opened<'_> {
    cx.update(gpui_core::init);
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let host = cx.new(|cx| Host {
            side: PopoverSide::Bottom,
            align: PopoverAlign::Start,
            touch: false,
            disabled: false,
            picked: Rc::default(),
            draws: Rc::default(),
            before: cx.focus_handle(),
            trigger: cx.focus_handle(),
        });
        *m.borrow_mut() = Some(host.clone());
        Root::new(host, window, cx)
    });
    vcx.simulate_resize(size(px(800.), px(600.)));
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let host = made.borrow().clone().unwrap();
    Opened { host, vcx }
}

impl Opened<'_> {
    fn bounds(&mut self, selector: &'static str) -> Option<Bounds<Pixels>> {
        self.vcx.debug_bounds(selector)
    }

    fn menu(&mut self) -> Option<Bounds<Pixels>> {
        self.bounds("gpui-core-popover")
    }

    fn click(&mut self, selector: &'static str) {
        let at = self.bounds(selector).expect(selector).center();
        self.click_at(at);
    }

    fn click_at(&mut self, at: Point<Pixels>) {
        self.vcx.simulate_click(at, Modifiers::none());
        self.vcx.run_until_parked();
    }

    fn tap(&mut self, selector: &'static str) {
        let at = self.bounds(selector).expect(selector).center();
        for phase in [TouchPhase::Started, TouchPhase::Ended] {
            self.vcx.simulate_event(TouchEvent {
                id: TouchId(1),
                phase,
                position: at,
                predicted_position: None,
                force: None,
            });
            self.vcx.run_until_parked();
        }
    }

    fn keys(&mut self, keys: &'static str) {
        for k in keys.split(' ') {
            let keystroke = Keystroke::parse(k).unwrap();
            self.vcx.simulate_event(KeyDownEvent {
                keystroke: keystroke.clone(),
                is_held: false,
                prefer_character_input: false,
            });
            self.vcx.simulate_event(KeyUpEvent { keystroke });
            self.vcx.run_until_parked();
        }
    }

    fn picked(&mut self) -> Vec<String> {
        self.host
            .read_with(self.vcx, |h, _| h.picked.borrow().clone())
    }

    /// Tab from the Press before it to the account menu's button.
    fn tab_to_button(&mut self) {
        let before = self.host.read_with(self.vcx, |h, _| h.before.clone());
        self.vcx.update(|window, cx| window.focus(&before, cx));
        self.keys("tab");
    }

    fn focus_trigger(&mut self) {
        let t = self.host.read_with(self.vcx, |h, _| h.trigger.clone());
        self.vcx.update(|window, cx| window.focus(&t, cx));
        self.vcx.run_until_parked();
    }

    fn trigger_focused(&mut self) -> bool {
        let t = self.host.read_with(self.vcx, |h, _| h.trigger.clone());
        self.vcx.update(|window, _| t.is_focused(window))
    }

    fn set(&mut self, f: impl FnOnce(&mut Host)) {
        self.host.update(self.vcx, |h, cx| {
            f(h);
            cx.notify();
        });
        self.vcx.run_until_parked();
    }

    fn draws(&mut self) -> usize {
        self.host.read_with(self.vcx, |h, _| h.draws.get())
    }
}

#[gpui::test]
fn a_click_opens_it_under_the_trigger_and_a_second_closes_it(cx: &mut TestAppContext) {
    let mut o = open(cx);
    assert!(o.menu().is_none());
    o.click("button-box");
    let menu = o.menu().expect("open");
    let trigger = o.bounds("button-box").unwrap();
    assert!(menu.top() >= trigger.bottom(), "{menu:?} {trigger:?}");
    assert_eq!(menu.left(), trigger.left(), "lined up with its start");
    assert!(o.bounds("menu-item-profile").is_some());
    o.click("button-box");
    assert!(
        o.menu().is_none(),
        "a second press on the trigger closes it"
    );
}

#[gpui::test]
fn from_the_pointer_nothing_is_highlighted_until_the_keys_move(cx: &mut TestAppContext) {
    let mut o = open(cx);
    o.click("button-box");
    o.keys("enter");
    assert!(o.picked().is_empty(), "nothing highlighted to pick");
    assert!(o.menu().is_some());
    o.keys("down enter");
    assert_eq!(o.picked(), ["profile"], "Down highlights the first entry");
    assert!(o.menu().is_none(), "a pick closes it");
}

#[gpui::test]
fn a_press_on_an_entry_picks_it_and_closes_the_menu(cx: &mut TestAppContext) {
    let mut o = open(cx);
    o.click("button-box");
    o.click("menu-item-team");
    assert!(o.picked().is_empty(), "a disabled entry isn't picked");
    assert!(o.menu().is_some());
    o.click("menu-item-billing");
    assert_eq!(o.picked(), ["billing"]);
    assert!(o.menu().is_none());
}

#[gpui::test]
fn enter_space_and_down_open_it_with_the_first_entry_highlighted(cx: &mut TestAppContext) {
    let mut o = open(cx);
    o.tab_to_button();
    o.keys("enter");
    assert!(o.menu().is_some(), "Enter opens it");
    o.keys("enter");
    assert_eq!(o.picked(), ["profile"], "its first entry was highlighted");
    assert!(o.menu().is_none());

    // The keyboard is back on the trigger: Space opens it, and Down moves past the disabled
    // entry.
    o.keys("space");
    assert!(o.menu().is_some(), "Space opens it");
    o.keys("down down enter");
    assert_eq!(o.picked(), ["profile", "log-out"]);

    o.keys("down");
    assert!(o.menu().is_some(), "Down opens it");
    o.keys("enter");
    assert_eq!(o.picked(), ["profile", "log-out", "profile"]);
}

#[gpui::test]
fn escape_closes_it_and_gives_the_keyboard_back_to_the_trigger(cx: &mut TestAppContext) {
    let mut o = open(cx);
    o.click("placed-trigger");
    assert!(o.menu().is_some());
    assert!(!o.trigger_focused(), "the menu has the keyboard");
    o.keys("escape");
    assert!(o.menu().is_none());
    assert!(o.trigger_focused());

    // The same from the keyboard, and after a pick.
    o.keys("down");
    assert!(o.menu().is_some());
    o.keys("escape");
    assert!(o.menu().is_none());
    assert!(o.trigger_focused());
    o.keys("enter enter");
    assert_eq!(o.picked(), ["one"]);
    assert!(o.trigger_focused(), "a pick gives it back too");
}

#[gpui::test]
fn a_press_outside_closes_it(cx: &mut TestAppContext) {
    let mut o = open(cx);
    o.click("button-box");
    assert!(o.menu().is_some());
    o.click_at(gpui::point(px(5.), px(5.)));
    assert!(o.menu().is_none());
    assert!(o.picked().is_empty());
}

#[gpui::test]
fn it_lines_up_with_each_side_and_alignment(cx: &mut TestAppContext) {
    let mut o = open(cx);
    let near = |a: Pixels, b: Pixels| (a - b).abs() <= px(1.);
    let cases = [
        (PopoverSide::Bottom, PopoverAlign::Start),
        (PopoverSide::Bottom, PopoverAlign::Center),
        (PopoverSide::Bottom, PopoverAlign::End),
        (PopoverSide::Top, PopoverAlign::End),
        (PopoverSide::Right, PopoverAlign::Start),
        (PopoverSide::Left, PopoverAlign::Center),
    ];
    for (side, align) in cases {
        o.set(|h| {
            h.side = side;
            h.align = align;
        });
        o.focus_trigger();
        o.keys("down");
        let t = o.bounds("placed-trigger").unwrap();
        let m = o.menu().expect("open");
        let what = format!("{side:?} {align:?}: {m:?} from {t:?}");
        match side {
            PopoverSide::Bottom => assert!(m.top() > t.bottom(), "{what}"),
            PopoverSide::Top => assert!(m.bottom() < t.top(), "{what}"),
            PopoverSide::Right => assert!(m.left() > t.right(), "{what}"),
            PopoverSide::Left => assert!(m.right() < t.left(), "{what}"),
        }
        let (start, end, m_start, m_end) = match side {
            PopoverSide::Bottom | PopoverSide::Top => (t.left(), t.right(), m.left(), m.right()),
            PopoverSide::Left | PopoverSide::Right => (t.top(), t.bottom(), m.top(), m.bottom()),
        };
        match align {
            PopoverAlign::Start => assert!(near(m_start, start), "{what}"),
            PopoverAlign::Center => {
                assert!(near((m_start + m_end) / 2., (start + end) / 2.), "{what}")
            }
            PopoverAlign::End => assert!(near(m_end, end), "{what}"),
        }
        o.keys("escape");
    }
}

#[gpui::test]
fn a_tap_opens_it_with_rows_for_touch(cx: &mut TestAppContext) {
    let mut o = open(cx);
    o.click("button-box");
    let row = o.bounds("menu-item-profile").unwrap();
    assert!(row.size.height < px(44.), "{row:?}");
    o.keys("escape");
    o.tap("button-box");
    assert!(o.menu().is_some(), "a tap opens it");
    let row = o.bounds("menu-item-profile").unwrap();
    assert!(row.size.height >= px(44.), "{row:?}");
    o.tap("menu-item-billing");
    assert_eq!(o.picked(), ["billing"], "a tap picks");
    assert!(o.menu().is_none());

    o.set(|h| h.touch = true);
    o.click("button-box");
    let row = o.bounds("menu-item-profile").unwrap();
    assert!(row.size.height >= px(44.), "touch(true): {row:?}");
}

#[gpui::test]
fn a_disabled_menu_does_not_open(cx: &mut TestAppContext) {
    let mut o = open(cx);
    o.set(|h| h.disabled = true);
    o.click("button-box");
    assert!(o.menu().is_none());
    o.tab_to_button();
    assert!(o.trigger_focused(), "Tab passes over the disabled trigger");
}

#[gpui::test]
fn nothing_redraws_while_idle(cx: &mut TestAppContext) {
    let mut o = open(cx);
    let rest = |o: &mut Opened| {
        let before = o.draws();
        for _ in 0..30 {
            o.vcx.update(|window, cx| window.simulate_next_frame(cx));
            o.vcx.executor().advance_clock(Duration::from_millis(16));
            o.vcx.run_until_parked();
        }
        o.vcx.executor().advance_clock(Duration::from_secs(5));
        o.vcx.run_until_parked();
        assert_eq!(o.draws(), before);
    };
    rest(&mut o);
    o.click("button-box");
    assert!(o.menu().is_some());
    rest(&mut o);
}
