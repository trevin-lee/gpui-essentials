//! Sidebar: a press on an item selects it, Up and Down move between the items (passing over a
//! disabled one and a folded group), a collapsible group folds and unfolds, the trigger and ⌘B
//! collapse it to a rail of icons and back, a narrow window opens it over the content instead
//! (closed by Escape, the scrim, or a pick), long labels are cut short, touch items are 44
//! points, and nothing redraws while idle.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, ParentElement as _, Render, SharedString,
    Styled as _, TestAppContext, VisualTestContext, Window, div, px, size,
};
use gpui_core::{
    IconName, Press, Root, Sidebar, SidebarGroup, SidebarItem, SidebarState, SidebarTrigger,
};

struct Host {
    state: Entity<SidebarState>,
    page: SharedString,
    touch: bool,
    picked: Rc<RefCell<Vec<String>>>,
    actions: Rc<RefCell<Vec<String>>>,
    draws: Rc<Cell<usize>>,
    page_focus: FocusHandle,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let picked = self.picked.clone();
        let actions = self.actions.clone();
        Sidebar::new("nav", &self.state)
            .touch(self.touch)
            .header(div().child("Acme"))
            .group(
                SidebarGroup::new("platform")
                    .label("Platform")
                    .item(
                        SidebarItem::new("inbox", "Inbox")
                            .icon(IconName::Inbox)
                            .badge("4"),
                    )
                    .item(
                        SidebarItem::new("agents", "Agents and every one of their long names")
                            .icon(IconName::Bot)
                            .action(IconName::Plus, "New agent", move |id, _, _| {
                                actions.borrow_mut().push(id.to_string())
                            }),
                    )
                    .item(
                        SidebarItem::new("billing", "Billing")
                            .icon(IconName::CreditCard)
                            .disabled(true),
                    )
                    .item(SidebarItem::new("settings", "Settings").icon(IconName::Settings)),
            )
            .group(
                SidebarGroup::new("projects")
                    .label("Projects")
                    .collapsible(true)
                    .item(SidebarItem::new("design", "Design").icon(IconName::Frame))
                    .item(SidebarItem::new("travel", "Travel").icon(IconName::Map)),
            )
            .footer(div().child("trevin"))
            .active(self.page.clone())
            .on_select(cx.listener(move |this, id: &SharedString, _, cx| {
                picked.borrow_mut().push(id.to_string());
                this.page = id.clone();
                cx.notify();
            }))
            .content(
                div()
                    .debug_selector(|| "content".into())
                    .size_full()
                    .child(SidebarTrigger::new("toggle", &self.state))
                    .child(
                        Press::new("page")
                            .track_focus(&self.page_focus)
                            .child("Page"),
                    ),
            )
    }
}

fn open(
    cx: &mut TestAppContext,
    width: f32,
    touch: bool,
) -> (Entity<Host>, &mut VisualTestContext) {
    cx.update(gpui_core::init);
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let h = cx.new(|cx| Host {
            state: cx.new(|_| SidebarState::new()),
            page: "inbox".into(),
            touch,
            picked: Rc::default(),
            actions: Rc::default(),
            draws: Rc::default(),
            page_focus: cx.focus_handle(),
        });
        *m.borrow_mut() = Some(h.clone());
        Root::new(h, window, cx)
    });
    vcx.simulate_resize(size(px(width), px(600.)));
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

fn picked(h: &Entity<Host>, vcx: &mut VisualTestContext) -> Vec<String> {
    h.read_with(vcx, |h, _| h.picked.borrow().clone())
}

fn focus_item(h: &Entity<Host>, vcx: &mut VisualTestContext) {
    // A press on the page, then Shift-Tab back from it would wander; pressing an item focuses it.
    let _ = h;
    click(vcx, "sidebar-item-inbox");
}

const TOGGLE: &str = if cfg!(target_os = "macos") {
    "cmd-b"
} else {
    "ctrl-b"
};

#[gpui::test]
fn a_press_selects_an_item_and_its_action_is_its_own(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, 1000., false);
    click(vcx, "sidebar-item-settings");
    click(vcx, "sidebar-item-billing");
    assert_eq!(picked(&h, vcx), ["settings"], "Billing is disabled");
    click(vcx, "sidebar-action-agents");
    assert_eq!(picked(&h, vcx), ["settings"]);
    assert_eq!(
        h.read_with(vcx, |h, _| h.actions.borrow().clone()),
        ["agents"]
    );
}

#[gpui::test]
fn up_and_down_move_between_the_items(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, 1000., false);
    focus_item(&h, vcx);
    keys(vcx, "down down enter");
    assert_eq!(
        picked(&h, vcx),
        ["inbox", "settings"],
        "Billing passed over"
    );
    keys(vcx, "end enter home enter up enter");
    assert_eq!(
        picked(&h, vcx),
        ["inbox", "settings", "travel", "inbox", "inbox"]
    );
}

#[gpui::test]
fn a_collapsible_group_folds_away(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, 1000., false);
    assert!(vcx.debug_bounds("sidebar-item-design").is_some());
    click(vcx, "sidebar-group-label-projects");
    assert!(vcx.debug_bounds("sidebar-item-design").is_none());
    // The keys pass over what's folded.
    focus_item(&h, vcx);
    keys(vcx, "end enter");
    assert_eq!(picked(&h, vcx), ["inbox", "settings"]);
    click(vcx, "sidebar-group-label-projects");
    assert!(vcx.debug_bounds("sidebar-item-design").is_some());
}

#[gpui::test]
fn the_trigger_and_the_shortcut_collapse_it_to_a_rail(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, 1000., false);
    let wide = vcx.debug_bounds("sidebar").unwrap().size.width;
    click(vcx, "sidebar-trigger");
    let rail = vcx.debug_bounds("sidebar").unwrap();
    assert!(rail.size.width < wide / 3., "{rail:?}");
    // Icons only: no labels, badges, actions, header or group labels.
    assert!(vcx.debug_bounds("sidebar-label-inbox").is_none());
    assert!(vcx.debug_bounds("sidebar-action-agents").is_none());
    assert!(vcx.debug_bounds("sidebar-group-label-platform").is_none());
    let item = vcx.debug_bounds("sidebar-item-inbox").unwrap();
    assert!(item.right() <= rail.right(), "{item:?} in {rail:?}");
    // The rail's items still go, from the keyboard too, and the shortcut expands it again.
    focus_item(&h, vcx);
    keys(vcx, "down enter");
    assert_eq!(picked(&h, vcx), ["inbox", "agents"]);
    keys(vcx, TOGGLE);
    assert!(vcx.debug_bounds("sidebar-label-inbox").is_some());
    keys(vcx, TOGGLE);
    assert!(vcx.debug_bounds("sidebar-label-inbox").is_none());
    assert!(h.read_with(vcx, |h, cx| h.state.read(cx).is_collapsed()));
}

#[gpui::test]
fn a_narrow_window_opens_it_over_the_content(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, 500., false);
    assert!(
        vcx.debug_bounds("sidebar").is_none(),
        "not beside the content"
    );
    let content = vcx.debug_bounds("content").unwrap();
    assert!(content.left() <= px(1.), "{content:?}");
    click(vcx, "sidebar-trigger");
    let s = vcx.debug_bounds("sidebar").expect("open over it");
    assert!(s.right() <= px(500.), "{s:?}");
    // The keyboard is in it: Down and Enter pick the next item, which closes it.
    keys(vcx, "down enter");
    assert_eq!(picked(&h, vcx), ["agents"]);
    assert!(vcx.debug_bounds("sidebar").is_none());
    click(vcx, "sidebar-trigger");
    keys(vcx, "escape");
    assert!(vcx.debug_bounds("sidebar").is_none(), "Escape closes it");
    click(vcx, "sidebar-trigger");
    let scrim = vcx.debug_bounds("sidebar-scrim").unwrap();
    vcx.simulate_click(
        gpui::point(scrim.right() - px(20.), scrim.center().y),
        Modifiers::none(),
    );
    vcx.run_until_parked();
    assert!(vcx.debug_bounds("sidebar").is_none(), "so does the scrim");
    // Widened, it's beside the content again.
    vcx.simulate_resize(size(px(1000.), px(600.)));
    vcx.run_until_parked();
    assert!(vcx.debug_bounds("sidebar").is_some());
}

#[gpui::test]
fn long_labels_are_cut_short(cx: &mut TestAppContext) {
    let (_, vcx) = open(cx, 1000., false);
    let s = vcx.debug_bounds("sidebar").unwrap();
    let l = vcx.debug_bounds("sidebar-label-agents").unwrap();
    assert!(l.right() <= s.right(), "{l:?} in {s:?}");
    let a = vcx.debug_bounds("sidebar-action-agents").unwrap();
    assert!(
        a.right() <= s.right() && a.left() >= l.right() - px(1.),
        "{a:?} {l:?}"
    );
}

#[gpui::test]
fn touch_items_are_44_points(cx: &mut TestAppContext) {
    let (_, vcx) = open(cx, 1000., true);
    for what in ["sidebar-item-inbox", "sidebar-group-label-projects"] {
        let b = vcx.debug_bounds(what).unwrap();
        assert!(b.size.height >= px(44.), "{what} {b:?}");
    }
    click(vcx, "sidebar-trigger");
    let b = vcx.debug_bounds("sidebar-item-inbox").unwrap();
    assert!(b.size.height >= px(44.) && b.size.width >= px(44.), "{b:?}");
}

#[gpui::test]
fn nothing_redraws_while_idle(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, 500., false);
    click(vcx, "sidebar-trigger");
    let before = h.read_with(vcx, |h, _| h.draws.get());
    for _ in 0..30 {
        vcx.update(|window, cx| window.simulate_next_frame(cx));
        vcx.executor()
            .advance_clock(std::time::Duration::from_millis(16));
        vcx.run_until_parked();
    }
    assert_eq!(h.read_with(vcx, |h, _| h.draws.get()), before);
}
