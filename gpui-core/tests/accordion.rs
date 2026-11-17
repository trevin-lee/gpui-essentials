//! Accordion: a press opens an item and, in single mode, closes the other; several stay open in
//! multiple mode; a host that keeps the open items decides; Tab stops on every header, Enter and
//! Space toggle, Up and Down go round the headers and Home and End to the ends (passing over a
//! disabled one); the chevron points down, or up when open; touch headers are 44 points; a long title
//! wraps inside a narrow box; and once it has changed nothing redraws.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, ParentElement as _, Render, SharedString,
    Styled as _, TestAppContext, VisualTestContext, Window, div, px,
};
use gpui_core::{Accordion, AccordionItem, Disableable as _, Press, Root, Theme};

struct Panel {
    multiple: bool,
    /// The open items when the host keeps them.
    host: Option<Vec<SharedString>>,
    obeys: bool,
    touch: bool,
    width: f32,
    title: &'static str,
    told: Rc<RefCell<Vec<Vec<SharedString>>>>,
    draws: Rc<Cell<usize>>,
    before: FocusHandle,
    after: FocusHandle,
}

impl Render for Panel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let told = self.told.clone();
        let obeys = self.obeys;
        let item = |id: &'static str, title: &'static str| {
            AccordionItem::new(id).title(title).child(
                div()
                    .debug_selector(move || format!("body-{id}"))
                    .child("Body"),
            )
        };
        let mut a = Accordion::new("faq")
            .multiple(self.multiple)
            .touch(self.touch)
            .item(item("a", self.title))
            .item(item("b", "B"))
            .item(item("c", "C").disabled(true))
            .item(item("d", "D"))
            .on_change(cx.listener(move |this, open: &Vec<SharedString>, _, cx| {
                told.borrow_mut().push(open.clone());
                if obeys {
                    this.host = Some(open.clone());
                    cx.notify();
                }
            }));
        if let Some(open) = &self.host {
            a = a.open(open.clone());
        }
        div()
            .p(px(20.))
            .child(
                Press::new("before")
                    .track_focus(&self.before)
                    .child("Before"),
            )
            .child(
                div()
                    .w(px(self.width))
                    .debug_selector(|| "box".into())
                    .child(a),
            )
            .child(Press::new("after").track_focus(&self.after).child("After"))
    }
}

struct Opened<'a> {
    panel: Entity<Panel>,
    told: Rc<RefCell<Vec<Vec<SharedString>>>>,
    draws: Rc<Cell<usize>>,
    vcx: &'a mut VisualTestContext,
}

fn open_with(cx: &mut TestAppContext, setup: impl FnOnce(&mut Panel)) -> Opened<'_> {
    cx.update(gpui_core::init);
    let told = Rc::new(RefCell::new(Vec::new()));
    let draws = Rc::new(Cell::new(0));
    let (t, d) = (told.clone(), draws.clone());
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let panel = cx.new(|cx| {
            let mut p = Panel {
                multiple: false,
                host: None,
                obeys: true,
                touch: false,
                width: 300.,
                title: "A",
                told: t,
                draws: d,
                before: cx.focus_handle(),
                after: cx.focus_handle(),
            };
            setup(&mut p);
            p
        });
        *m.borrow_mut() = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let panel = made.borrow().clone().unwrap();
    Opened {
        panel,
        told,
        draws,
        vcx,
    }
}

/// A selector made at run time, as `debug_bounds` wants one.
fn sel(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn press(vcx: &mut VisualTestContext, id: &str) {
    let at = vcx
        .debug_bounds(sel(format!("accordion-header-{id}")))
        .expect("a header")
        .center();
    vcx.simulate_click(at, Modifiers::none());
    vcx.run_until_parked();
}

/// The items whose content is drawn.
fn shown(vcx: &mut VisualTestContext) -> String {
    ["a", "b", "c", "d"]
        .into_iter()
        .filter(|id| vcx.debug_bounds(sel(format!("body-{id}"))).is_some())
        .collect()
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

#[gpui::test]
fn one_item_opens_at_a_time(cx: &mut TestAppContext) {
    let o = open_with(cx, |_| {});
    assert_eq!(shown(o.vcx), "");
    assert!(o.vcx.debug_bounds("accordion-chevron-a-down").is_some());
    press(o.vcx, "a");
    assert_eq!(shown(o.vcx), "a");
    assert!(
        o.vcx.debug_bounds("accordion-chevron-a-up").is_some(),
        "the chevron points up when open"
    );
    assert!(o.vcx.debug_bounds("accordion-chevron-b-down").is_some());
    press(o.vcx, "b");
    assert_eq!(shown(o.vcx), "b", "opening one closes the other");
    press(o.vcx, "b");
    assert_eq!(shown(o.vcx), "", "and the open one closes");
    press(o.vcx, "c");
    assert_eq!(shown(o.vcx), "", "a disabled one doesn't open");
}

#[gpui::test]
fn several_stay_open_in_multiple_mode(cx: &mut TestAppContext) {
    let o = open_with(cx, |p| p.multiple = true);
    press(o.vcx, "d");
    press(o.vcx, "a");
    assert_eq!(shown(o.vcx), "ad");
    press(o.vcx, "d");
    assert_eq!(shown(o.vcx), "a");
    let told: Vec<Vec<String>> = o
        .told
        .borrow()
        .iter()
        .map(|v| v.iter().map(|s| s.to_string()).collect())
        .collect();
    assert_eq!(told, [vec!["d"], vec!["a", "d"], vec!["a"]], "in order");
}

#[gpui::test]
fn a_host_that_keeps_the_open_items_decides(cx: &mut TestAppContext) {
    let o = open_with(cx, |p| {
        p.host = Some(vec!["b".into()]);
        p.obeys = false;
    });
    assert_eq!(shown(o.vcx), "b");
    press(o.vcx, "a");
    assert_eq!(o.told.borrow().len(), 1);
    assert_eq!(shown(o.vcx), "b", "the host didn't change it");
    o.panel.update(o.vcx, |p, cx| {
        p.host = Some(vec!["a".into(), "d".into()]);
        cx.notify();
    });
    o.vcx.run_until_parked();
    assert_eq!(shown(o.vcx), "ad");
}

#[gpui::test]
fn the_keys_move_between_headers_and_toggle_them(cx: &mut TestAppContext) {
    let o = open_with(cx, |_| {});
    let before = o.panel.read_with(o.vcx, |p, _| p.before.clone());
    o.vcx.update(|window, cx| window.focus(&before, cx));
    keys(o.vcx, "tab enter");
    assert_eq!(shown(o.vcx), "a", "Enter on the first header");
    keys(o.vcx, "down space");
    assert_eq!(shown(o.vcx), "b", "Space on the second");
    keys(o.vcx, "down enter");
    assert_eq!(shown(o.vcx), "d", "Down passed over the disabled one");
    keys(o.vcx, "down enter");
    assert_eq!(shown(o.vcx), "a", "and went round to the first");
    keys(o.vcx, "up enter");
    assert_eq!(shown(o.vcx), "d", "Up goes round to the last");
    keys(o.vcx, "home enter");
    assert_eq!(shown(o.vcx), "a");
    keys(o.vcx, "end enter");
    assert_eq!(shown(o.vcx), "d");
}

#[gpui::test]
fn tab_stops_on_every_header_but_a_disabled_one(cx: &mut TestAppContext) {
    let o = open_with(cx, |_| {});
    let before = o.panel.read_with(o.vcx, |p, _| p.before.clone());
    let after = o.panel.read_with(o.vcx, |p, _| p.after.clone());
    o.vcx.update(|window, cx| window.focus(&before, cx));
    keys(o.vcx, "tab tab tab");
    assert!(!o.vcx.update(|window, _| after.is_focused(window)), "on D");
    keys(o.vcx, "enter");
    assert_eq!(shown(o.vcx), "d");
    keys(o.vcx, "tab");
    assert!(o.vcx.update(|window, _| after.is_focused(window)));
}

#[gpui::test]
fn touch_headers_are_44_points(cx: &mut TestAppContext) {
    // At half zoom, where a header is otherwise shorter than a finger.
    cx.update(|cx| {
        let mut t = Theme::dark();
        t.font_size = px(7.5);
        Theme::set(t, cx);
    });
    let o = open_with(cx, |p| p.touch = true);
    let b = o.vcx.debug_bounds("accordion-header-b").unwrap();
    assert!(b.size.height >= px(44.), "{b:?}");
}

#[gpui::test]
fn a_narrow_box_keeps_the_headers_inside_it(cx: &mut TestAppContext) {
    let o = open_with(cx, |p| {
        p.width = 60.;
        p.title = "A title far too long for the box it's in, with nowhere to go";
    });
    press(o.vcx, "a");
    let b = o.vcx.debug_bounds("box").unwrap();
    for id in [
        "accordion-header-a",
        "accordion-content-a",
        "accordion-header-d",
    ] {
        let h = o.vcx.debug_bounds(id).unwrap();
        assert!(h.right() <= b.right() + px(0.5), "{id} {h:?} in {b:?}");
    }
}

/// Delivers one display frame and says how many things asked for it.
fn frame(vcx: &mut VisualTestContext) -> usize {
    vcx.executor().advance_clock(Duration::from_millis(16));
    let asked = vcx.update(|window, cx| window.simulate_next_frame(cx));
    vcx.run_until_parked();
    asked
}

#[gpui::test]
fn once_it_has_changed_nothing_redraws(cx: &mut TestAppContext) {
    let o = open_with(cx, |_| {});
    press(o.vcx, "a");
    frame(o.vcx);
    let settled = o.draws.get();
    for _ in 0..30 {
        assert_eq!(frame(o.vcx), 0, "asked for a frame while idle");
    }
    assert_eq!(o.draws.get(), settled);
}
