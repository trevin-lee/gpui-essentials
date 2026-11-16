//! PopupMenu: the keys move a highlight over what can be picked and Enter picks it, the pointer
//! moves it too, a press picks, a disabled entry can't be picked, Escape closes its popover and
//! gives the focus back, touch rows are 44 points, and nothing redraws while idle.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, ParentElement as _, Render, SharedString,
    Styled as _, TestAppContext, VisualTestContext, Window, div, prelude::FluentBuilder as _, px,
    size,
};
use gpui_core::{IconName, MenuSelect, Popover, PopupMenu, PopupMenuItem, Press, Root};

struct Panel {
    open: bool,
    touch: bool,
    trigger: FocusHandle,
    picked: Rc<RefCell<Vec<(SharedString, gpui::Modifiers)>>>,
    draws: Rc<Cell<usize>>,
    /// The entry it opens on, if the host says.
    start: Option<SharedString>,
    /// What `on_highlight` was told.
    highlights: Rc<RefCell<Vec<Option<String>>>>,
}

impl Render for Panel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let picked = self.picked.clone();
        let highlights = self.highlights.clone();
        let start = self.start.clone();
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
                        .debug_selector(|| "trigger".into())
                        .w(px(120.))
                        .h(px(30.))
                        .child("Menu")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.open = !this.open;
                            cx.notify();
                        })),
                )
                .p(px(4.))
                .child(
                    PopupMenu::new("menu")
                        .when_some(start, |m, s| m.highlight(s))
                        .on_highlight(move |id, _| {
                            highlights.borrow_mut().push(id.map(|s| s.to_string()))
                        })
                        .touch(self.touch)
                        .item(
                            PopupMenuItem::new("copy", "Copy")
                                .icon(IconName::Copy)
                                .shortcut("⌘C"),
                        )
                        .item(PopupMenuItem::new("cut", "Cut").disabled(true))
                        .separator()
                        .item(PopupMenuItem::new("wrap", "Wrap lines").checked(true))
                        .item(PopupMenuItem::new(
                            "long",
                            "A label long enough that it has to be cut short in a narrow window",
                        ))
                        .on_select(cx.listener(move |this, pick: &MenuSelect, _, cx| {
                            picked.borrow_mut().push((pick.id.clone(), pick.modifiers));
                            this.open = false;
                            cx.notify();
                        })),
                ),
        )
    }
}

struct Opened<'a> {
    panel: Entity<Panel>,
    picked: Rc<RefCell<Vec<(SharedString, gpui::Modifiers)>>>,
    draws: Rc<Cell<usize>>,
    vcx: &'a mut VisualTestContext,
}

fn open(cx: &mut TestAppContext, touch: bool, width: f32) -> Opened<'_> {
    cx.update(gpui_core::init);
    let picked: Rc<RefCell<Vec<(SharedString, gpui::Modifiers)>>> = Rc::default();
    let draws = Rc::new(Cell::new(0));
    let (p, d) = (picked.clone(), draws.clone());
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let panel = cx.new(|cx| Panel {
            open: false,
            touch,
            trigger: cx.focus_handle(),
            picked: p,
            draws: d,
            start: None,
            highlights: Rc::default(),
        });
        *m.borrow_mut() = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    vcx.simulate_resize(size(px(width), px(500.)));
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let panel = made.borrow().clone().unwrap();
    Opened {
        panel,
        picked,
        draws,
        vcx,
    }
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

fn picked(o: &Opened<'_>) -> Vec<&'static str> {
    o.picked
        .borrow()
        .iter()
        .map(|s| match s.0.as_ref() {
            "copy" => "copy",
            "cut" => "cut",
            "wrap" => "wrap",
            "long" => "long",
            _ => "?",
        })
        .collect()
}

#[gpui::test]
fn the_keys_skip_what_cant_be_picked_and_enter_picks(cx: &mut TestAppContext) {
    let o = open(cx, false, 600.);
    keys(o.vcx, "tab space");
    assert!(o.vcx.debug_bounds("menu-item-copy").is_some(), "open");
    // It opens on Copy; down passes over disabled Cut and the separator to Wrap.
    keys(o.vcx, "down enter");
    assert_eq!(picked(&o), ["wrap"]);
    assert!(
        o.vcx.debug_bounds("menu-item-copy").is_none(),
        "closed on a pick"
    );
}

#[gpui::test]
fn up_wraps_and_home_and_end_go_to_the_ends(cx: &mut TestAppContext) {
    let o = open(cx, false, 600.);
    keys(o.vcx, "tab space up enter");
    assert_eq!(picked(&o), ["long"]);
    keys(o.vcx, "space end home enter");
    assert_eq!(picked(&o), ["long", "copy"]);
}

#[gpui::test]
fn the_pointer_moves_the_highlight_and_a_press_picks(cx: &mut TestAppContext) {
    let o = open(cx, false, 600.);
    keys(o.vcx, "tab space");
    let wrap = o.vcx.debug_bounds("menu-item-wrap").unwrap();
    o.vcx
        .simulate_mouse_move(wrap.center(), None, Modifiers::none());
    keys(o.vcx, "enter");
    assert_eq!(picked(&o), ["wrap"]);
    keys(o.vcx, "space");
    let copy = o.vcx.debug_bounds("menu-item-copy").unwrap();
    o.vcx.simulate_click(copy.center(), Modifiers::none());
    o.vcx.run_until_parked();
    assert_eq!(picked(&o), ["wrap", "copy"]);
}

#[gpui::test]
fn a_disabled_entry_cant_be_picked(cx: &mut TestAppContext) {
    let o = open(cx, false, 600.);
    keys(o.vcx, "tab space");
    let cut = o.vcx.debug_bounds("menu-item-cut").unwrap();
    o.vcx
        .simulate_mouse_move(cut.center(), None, Modifiers::none());
    o.vcx.simulate_click(cut.center(), Modifiers::none());
    o.vcx.run_until_parked();
    keys(o.vcx, "enter");
    // Neither the press nor the hover reached Cut: Enter picks Copy, where it opened.
    assert_eq!(picked(&o), ["copy"]);
}

#[gpui::test]
fn escape_closes_it_and_the_focus_goes_back(cx: &mut TestAppContext) {
    let o = open(cx, false, 600.);
    keys(o.vcx, "tab space down escape");
    assert!(o.vcx.debug_bounds("menu-item-copy").is_none());
    assert!(o.picked.borrow().is_empty());
    let trigger = o.panel.read_with(o.vcx, |p, _| p.trigger.clone());
    assert!(o.vcx.update(|w, _| trigger.is_focused(w)));
}

#[gpui::test]
fn touch_rows_are_44_points(cx: &mut TestAppContext) {
    let o = open(cx, true, 600.);
    keys(o.vcx, "tab space");
    for id in ["menu-item-copy", "menu-item-wrap", "menu-item-long"] {
        let b = o.vcx.debug_bounds(id).unwrap();
        assert!(b.size.height >= px(44.), "{id} {b:?}");
    }
}

#[gpui::test]
fn in_a_narrow_window_a_long_label_stays_inside(cx: &mut TestAppContext) {
    let o = open(cx, false, 200.);
    keys(o.vcx, "tab space");
    let p = o.vcx.debug_bounds("gpui-core-popover").unwrap();
    assert!(p.right() <= px(200.), "{p:?}");
    let long = o.vcx.debug_bounds("menu-item-long").unwrap();
    assert!(long.right() <= p.right(), "{long:?} {p:?}");
}

#[gpui::test]
fn open_and_idle_nothing_redraws(cx: &mut TestAppContext) {
    let o = open(cx, false, 600.);
    keys(o.vcx, "tab space down");
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

#[gpui::test]
fn it_opens_on_its_first_entry_or_the_hosts(cx: &mut TestAppContext) {
    let o = open(cx, false, 600.);
    keys(o.vcx, "tab space enter");
    assert_eq!(picked(&o), ["copy"]);
    o.panel.update(o.vcx, |p, _| p.start = Some("wrap".into()));
    keys(o.vcx, "space enter");
    assert_eq!(picked(&o), ["copy", "wrap"]);
}

#[gpui::test]
fn the_host_is_told_where_the_highlight_moves(cx: &mut TestAppContext) {
    let o = open(cx, false, 600.);
    keys(o.vcx, "tab space down down");
    let seen = o
        .panel
        .read_with(o.vcx, |p, _| p.highlights.borrow().clone());
    assert_eq!(seen, [Some("wrap".to_string()), Some("long".to_string())]);
}

#[gpui::test]
fn a_pick_comes_with_the_modifiers_held(cx: &mut TestAppContext) {
    let o = open(cx, false, 600.);
    keys(o.vcx, "tab space shift-enter");
    keys(o.vcx, "space");
    let wrap = o.vcx.debug_bounds("menu-item-wrap").unwrap();
    o.vcx.simulate_click(wrap.center(), Modifiers::shift());
    o.vcx.run_until_parked();
    keys(o.vcx, "space enter");
    let got = o.picked.borrow().clone();
    assert_eq!(got.len(), 3, "{got:?}");
    assert!(
        got[0].1.shift && got[0].0.as_ref() == "copy",
        "Shift-Enter: {got:?}"
    );
    assert!(
        got[1].1.shift && got[1].0.as_ref() == "wrap",
        "a Shift-click: {got:?}"
    );
    assert!(!got[2].1.shift, "Enter alone: {got:?}");
}
