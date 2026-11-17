//! Tabs: a press selects a tab (not a disabled one) and only its panel is drawn; Tab stops once,
//! on the selected tab; the arrows wrap, pass over a disabled tab and select what they reach,
//! or with `manual` only move the focus until Enter; left to itself it remembers the selection;
//! a narrow box cuts the tabs short or scrolls them (keeping the focused one in view); touch tabs
//! are 44 points; and nothing redraws while idle.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, ParentElement as _, Render, SharedString,
    Styled as _, TestAppContext, VisualTestContext, Window, div, px,
};
use gpui_core::{Press, Root, Tab, Tabs, TabsOverflow};

#[derive(Clone, Copy, Default)]
struct Opts {
    /// The host owns the selection.
    controlled: bool,
    manual: bool,
    touch: bool,
    scroll: bool,
    line: bool,
    width: Option<f32>,
}

struct Host {
    tab: SharedString,
    opts: Opts,
    changes: Rc<RefCell<Vec<SharedString>>>,
    draws: Rc<Cell<usize>>,
    before: FocusHandle,
}

const TABS: [(&str, &str); 5] = [
    ("account", "Account"),
    ("password", "Password"),
    ("billing", "Billing"),
    ("team", "Team members and their roles"),
    ("danger", "Danger zone"),
];

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let changes = self.changes.clone();
        let o = self.opts;
        let mut tabs = Tabs::new("settings")
            .accessibility_label("Settings")
            .tabs(TABS.iter().map(|(id, label)| {
                Tab::new(*id, *label)
                    .disabled(*id == "billing")
                    .child(div().child(format!("The {label} panel")))
            }))
            .manual(o.manual)
            .touch(o.touch)
            .overflow(if o.scroll {
                TabsOverflow::Scroll
            } else {
                TabsOverflow::Shrink
            })
            .on_change(cx.listener(move |this, id: &SharedString, _, cx| {
                changes.borrow_mut().push(id.clone());
                this.tab = id.clone();
                cx.notify();
            }));
        if o.line {
            tabs = tabs.line();
        }
        if o.controlled {
            tabs = tabs.selected(self.tab.clone());
        } else {
            tabs = tabs.default_selected("password");
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
            .child(
                div()
                    .when_width(o.width)
                    .debug_selector(|| "box".into())
                    .child(tabs),
            )
            .child(Press::new("after").child("After"))
    }
}

trait WhenWidth {
    fn when_width(self, w: Option<f32>) -> Self;
}

impl WhenWidth for gpui::Div {
    fn when_width(self, w: Option<f32>) -> Self {
        match w {
            Some(w) => self.w(px(w)),
            None => self,
        }
    }
}

fn open(cx: &mut TestAppContext, opts: Opts) -> (Entity<Host>, &mut VisualTestContext) {
    cx.update(gpui_core::init);
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let h = cx.new(|cx| Host {
            tab: "password".into(),
            opts,
            changes: Rc::default(),
            draws: Rc::default(),
            before: cx.focus_handle(),
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

/// From the Press before the tabs, Tab: onto the tab list's one stop.
fn tab_in(h: &Entity<Host>, vcx: &mut VisualTestContext) {
    let before = h.read_with(vcx, |h, _| h.before.clone());
    vcx.update(|window, cx| window.focus(&before, cx));
    keys(vcx, "tab");
}

fn changes(h: &Entity<Host>, vcx: &mut VisualTestContext) -> Vec<String> {
    h.read_with(vcx, |h, _| {
        h.changes.borrow().iter().map(|s| s.to_string()).collect()
    })
}

/// A debug selector made at run time.
fn sel(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

/// The panels drawn now.
fn panels(vcx: &mut VisualTestContext) -> Vec<&'static str> {
    TABS.iter()
        .map(|(id, _)| *id)
        .filter(|id| vcx.debug_bounds(sel(format!("tab-panel-{id}"))).is_some())
        .collect()
}

#[gpui::test]
fn a_press_selects_a_tab_but_not_a_disabled_one(cx: &mut TestAppContext) {
    let (h, vcx) = open(
        cx,
        Opts {
            controlled: true,
            ..Opts::default()
        },
    );
    assert_eq!(panels(vcx), ["password"], "only the selected panel");
    click(vcx, "tab-team");
    assert_eq!(panels(vcx), ["team"]);
    click(vcx, "tab-billing");
    assert_eq!(panels(vcx), ["team"], "disabled");
    // A press on the selected tab tells the host nothing.
    click(vcx, "tab-team");
    assert_eq!(changes(&h, vcx), ["team"]);
}

#[gpui::test]
fn tab_stops_once_and_the_arrows_wrap_past_a_disabled_tab(cx: &mut TestAppContext) {
    let (h, vcx) = open(
        cx,
        Opts {
            controlled: true,
            ..Opts::default()
        },
    );
    tab_in(&h, vcx);
    keys(vcx, "right");
    assert_eq!(panels(vcx), ["team"], "Billing, disabled, passed over");
    keys(vcx, "right right");
    assert_eq!(panels(vcx), ["account"], "wrapped round from the last");
    keys(vcx, "left");
    assert_eq!(panels(vcx), ["danger"], "and back");
    keys(vcx, "home");
    assert_eq!(panels(vcx), ["account"]);
    keys(vcx, "end");
    assert_eq!(panels(vcx), ["danger"]);
    // Tab leaves the list: the next stop is the Press after it, so Enter there selects nothing.
    keys(vcx, "tab enter");
    assert_eq!(panels(vcx), ["danger"]);
    assert_eq!(
        changes(&h, vcx),
        ["team", "danger", "account", "danger", "account", "danger"]
    );
}

#[gpui::test]
fn manual_tabs_move_the_focus_and_select_on_enter(cx: &mut TestAppContext) {
    let (h, vcx) = open(
        cx,
        Opts {
            controlled: true,
            manual: true,
            ..Opts::default()
        },
    );
    tab_in(&h, vcx);
    keys(vcx, "right right");
    assert_eq!(panels(vcx), ["password"], "the arrows don't select");
    assert!(changes(&h, vcx).is_empty());
    keys(vcx, "enter");
    assert_eq!(panels(vcx), ["danger"], "Enter selects the focused tab");
    keys(vcx, "home space");
    assert_eq!(panels(vcx), ["account"]);
}

#[gpui::test]
fn left_to_itself_it_remembers_the_selection(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, Opts::default());
    assert_eq!(panels(vcx), ["password"], "the default");
    click(vcx, "tab-account");
    assert_eq!(panels(vcx), ["account"]);
    // The host redrawing doesn't lose it.
    h.update(vcx, |_, cx| cx.notify());
    vcx.run_until_parked();
    assert_eq!(panels(vcx), ["account"]);
    tab_in(&h, vcx);
    keys(vcx, "left");
    assert_eq!(panels(vcx), ["danger"]);
    assert_eq!(changes(&h, vcx), ["account", "danger"]);
}

#[gpui::test]
fn a_narrow_box_cuts_the_tabs_short(cx: &mut TestAppContext) {
    for line in [false, true] {
        let (_, vcx) = open(
            cx,
            Opts {
                width: Some(220.),
                line,
                ..Opts::default()
            },
        );
        let b = vcx.debug_bounds("box").unwrap();
        let l = vcx.debug_bounds("tab-list").unwrap();
        assert!(l.right() <= b.right() + px(0.5), "{l:?} in {b:?}");
        for (id, _) in TABS {
            let t = vcx.debug_bounds(sel(format!("tab-{id}"))).unwrap();
            assert!(t.right() <= l.right(), "{id} {t:?} in {l:?}");
        }
    }
}

#[gpui::test]
fn a_scrolling_list_stays_in_its_box_and_shows_the_focused_tab(cx: &mut TestAppContext) {
    let (h, vcx) = open(
        cx,
        Opts {
            width: Some(220.),
            scroll: true,
            ..Opts::default()
        },
    );
    let b = vcx.debug_bounds("box").unwrap();
    let l = vcx.debug_bounds("tab-list").unwrap();
    assert!(l.right() <= b.right() + px(0.5), "{l:?} in {b:?}");
    let last = vcx.debug_bounds("tab-danger").unwrap();
    assert!(last.left() >= l.right(), "past the end at first: {last:?}");
    tab_in(&h, vcx);
    keys(vcx, "end");
    let last = vcx.debug_bounds("tab-danger").unwrap();
    assert!(
        last.right() <= l.right() + px(0.5) && last.left() >= l.left(),
        "scrolled into view: {last:?} in {l:?}"
    );
}

#[gpui::test]
fn touch_tabs_are_44_points(cx: &mut TestAppContext) {
    let (_, vcx) = open(
        cx,
        Opts {
            touch: true,
            ..Opts::default()
        },
    );
    for (id, _) in TABS {
        let t = vcx.debug_bounds(sel(format!("tab-{id}"))).unwrap();
        assert!(
            t.size.height >= px(44.) && t.size.width >= px(44.),
            "{id} {t:?}"
        );
    }
}

#[gpui::test]
fn nothing_redraws_while_idle(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, Opts::default());
    tab_in(&h, vcx);
    keys(vcx, "right");
    let before = h.read_with(vcx, |h, _| h.draws.get());
    for _ in 0..30 {
        vcx.update(|window, cx| window.simulate_next_frame(cx));
        vcx.executor()
            .advance_clock(std::time::Duration::from_millis(16));
        vcx.run_until_parked();
    }
    assert_eq!(h.read_with(vcx, |h, _| h.draws.get()), before);
}
