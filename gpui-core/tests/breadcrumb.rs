//! Breadcrumb: a press or Enter on a link navigates, the current page is no link and no Tab
//! stop; in a narrow box (or past `max_items`) the middle folds into an ellipsis whose menu
//! navigates to the folded items, and nothing spills out; touch links are 44 points; and nothing
//! redraws while idle, the width it measures included.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, ParentElement as _, Render, SharedString,
    Styled as _, TestAppContext, VisualTestContext, Window, div, px,
};
use gpui_core::{Breadcrumb, BreadcrumbItem, Press, Root};

#[derive(Clone, Copy, Default)]
struct Opts {
    width: f32,
    max: Option<usize>,
    touch: bool,
}

struct Host {
    opts: Opts,
    went: Rc<RefCell<Vec<SharedString>>>,
    draws: Rc<Cell<usize>>,
    before: FocusHandle,
    after: FocusHandle,
}

const ITEMS: [(&str, &str); 5] = [
    ("home", "Home"),
    ("projects", "Projects"),
    ("agents", "Agents"),
    ("settings", "Settings"),
    ("models", "Models and their keys"),
];

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let went = self.went.clone();
        let mut b = Breadcrumb::new("path")
            .items(ITEMS.iter().map(|(id, l)| BreadcrumbItem::new(*id, *l)))
            .touch(self.opts.touch)
            .on_navigate(move |id, _, _| went.borrow_mut().push(id.clone()));
        if let Some(m) = self.opts.max {
            b = b.max_items(m);
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
                    .w(px(self.opts.width))
                    .debug_selector(|| "box".into())
                    .child(b),
            )
            .child(Press::new("after").track_focus(&self.after).child("After"))
    }
}

fn open(cx: &mut TestAppContext, opts: Opts) -> (Entity<Host>, &mut VisualTestContext) {
    cx.update(gpui_core::init);
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let h = cx.new(|cx| Host {
            opts,
            went: Rc::default(),
            draws: Rc::default(),
            before: cx.focus_handle(),
            after: cx.focus_handle(),
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

fn sel(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn went(h: &Entity<Host>, vcx: &mut VisualTestContext) -> Vec<String> {
    h.read_with(vcx, |h, _| {
        h.went.borrow().iter().map(|s| s.to_string()).collect()
    })
}

/// The items drawn in the row.
fn shown(vcx: &mut VisualTestContext) -> Vec<&'static str> {
    ITEMS
        .iter()
        .map(|(id, _)| *id)
        .filter(|id| vcx.debug_bounds(sel(format!("crumb-{id}"))).is_some())
        .collect()
}

#[gpui::test]
fn a_press_on_a_link_navigates_but_not_on_the_page(cx: &mut TestAppContext) {
    let (h, vcx) = open(
        cx,
        Opts {
            width: 900.,
            ..Opts::default()
        },
    );
    assert_eq!(
        shown(vcx),
        ["home", "projects", "agents", "settings", "models"]
    );
    assert!(vcx.debug_bounds("crumb-more").is_none(), "room for all");
    click(vcx, "crumb-agents");
    click(vcx, "crumb-models");
    assert_eq!(went(&h, vcx), ["agents"]);
}

#[gpui::test]
fn tab_reaches_the_links_not_the_page(cx: &mut TestAppContext) {
    let (h, vcx) = open(
        cx,
        Opts {
            width: 900.,
            ..Opts::default()
        },
    );
    let before = h.read_with(vcx, |h, _| h.before.clone());
    vcx.update(|window, cx| window.focus(&before, cx));
    keys(vcx, "tab enter tab tab tab space tab");
    assert_eq!(went(&h, vcx), ["home", "settings"]);
    // The page is passed over: the next stop is after the breadcrumb.
    let after = h.read_with(vcx, |h, _| h.after.clone());
    assert!(vcx.update(|window, _| after.is_focused(window)));
}

#[gpui::test]
fn a_narrow_box_folds_the_middle_into_a_menu(cx: &mut TestAppContext) {
    let (h, vcx) = open(
        cx,
        Opts {
            width: 260.,
            ..Opts::default()
        },
    );
    let s = shown(vcx);
    assert_eq!(s.first(), Some(&"home"));
    assert_eq!(s.last(), Some(&"models"));
    assert!(s.len() < ITEMS.len(), "{s:?}");
    let b = vcx.debug_bounds("box").unwrap();
    for id in s {
        let c = vcx.debug_bounds(sel(format!("crumb-{id}"))).unwrap();
        assert!(c.right() <= b.right() + px(0.5), "{id} {c:?} in {b:?}");
    }
    let more = vcx.debug_bounds("crumb-more").unwrap();
    assert!(more.right() <= b.right(), "{more:?}");
    click(vcx, "crumb-more");
    // The folded items are in its menu; picking one navigates and closes it.
    assert!(vcx.debug_bounds("menu-item-projects").is_some());
    assert!(vcx.debug_bounds("menu-item-home").is_none());
    assert!(vcx.debug_bounds("menu-item-models").is_none());
    click(vcx, "menu-item-projects");
    assert_eq!(went(&h, vcx), ["projects"]);
    assert!(vcx.debug_bounds("menu-item-projects").is_none(), "closed");
}

#[gpui::test]
fn the_keyboard_opens_the_ellipsis_menu(cx: &mut TestAppContext) {
    let (h, vcx) = open(
        cx,
        Opts {
            width: 900.,
            max: Some(3),
            ..Opts::default()
        },
    );
    assert_eq!(shown(vcx), ["home", "settings", "models"], "past the most");
    let before = h.read_with(vcx, |h, _| h.before.clone());
    vcx.update(|window, cx| window.focus(&before, cx));
    keys(vcx, "tab tab enter down enter");
    assert_eq!(went(&h, vcx), ["agents"]);
}

#[gpui::test]
fn touch_links_are_44_points(cx: &mut TestAppContext) {
    let (_, vcx) = open(
        cx,
        Opts {
            width: 900.,
            max: Some(3),
            touch: true,
        },
    );
    for what in ["crumb-home", "crumb-settings", "crumb-more"] {
        let b = vcx.debug_bounds(what).unwrap();
        assert!(b.size.height >= px(44.), "{what} {b:?}");
    }
    let more = vcx.debug_bounds("crumb-more").unwrap();
    assert!(more.size.width >= px(44.), "{more:?}");
}

#[gpui::test]
fn nothing_redraws_while_idle(cx: &mut TestAppContext) {
    let (h, vcx) = open(
        cx,
        Opts {
            width: 260.,
            ..Opts::default()
        },
    );
    let before = h.read_with(vcx, |h, _| h.draws.get());
    for _ in 0..30 {
        vcx.update(|window, cx| window.simulate_next_frame(cx));
        vcx.executor()
            .advance_clock(std::time::Duration::from_millis(16));
        vcx.run_until_parked();
    }
    assert_eq!(h.read_with(vcx, |h, _| h.draws.get()), before);
}
