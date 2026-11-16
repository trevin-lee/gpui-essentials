//! Popover: where it's placed against its trigger and the window's edges, how it closes, where
//! the focus goes, and that it draws nothing while closed and nothing again while idle.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    AppContext as _, Bounds, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, ParentElement as _, Pixels, Point, Render,
    ScrollHandle, StatefulInteractiveElement as _, Styled as _, TestAppContext, VisualTestContext,
    Window, div, point, prelude::FluentBuilder as _, px, size,
};
use gpui_core::{Popover, Press, Root};

#[derive(Clone, Copy, PartialEq)]
enum Place {
    TopLeft,
    Bottom,
    Right,
}

struct Panel {
    open: bool,
    place: Place,
    rows: usize,
    /// The panel's width; none to size it by its content.
    width: Option<Pixels>,
    /// One long row instead of the short ones.
    long: bool,
    take_focus: bool,
    /// Stay open when the trigger scrolls away.
    stay: bool,
    return_to: Option<FocusHandle>,
    /// Hang from this point instead of a trigger.
    point: Option<Point<Pixels>>,
    trigger: FocusHandle,
    other: FocusHandle,
    scroll: ScrollHandle,
    dismissed: Rc<Cell<usize>>,
    draws: Rc<Cell<usize>>,
}

impl Render for Panel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let dismissed = self.dismissed.clone();
        let mut popover = Popover::new("pop")
            .open(self.open)
            .take_focus(self.take_focus)
            .close_when_scrolled_away(!self.stay)
            .on_dismiss(cx.listener(move |this, _, _, cx| {
                dismissed.set(dismissed.get() + 1);
                this.open = false;
                cx.notify();
            }))
            .children((0..self.rows).map(|i| div().h(px(20.)).child(format!("Row {i}"))))
            .when_some(self.width, |p, w| p.w(w))
            .when(self.long, |p| {
                p.child("A long line of words that can't fit in a narrow window at all, wrapping")
            });
        if let Some(to) = &self.return_to {
            popover = popover.return_focus(to);
        }
        let trigger = Press::new("trigger")
            .track_focus(&self.trigger)
            .debug_selector(|| "trigger".into())
            .w(px(120.))
            .h(px(30.))
            .child("Open")
            .on_click(cx.listener(|this, _, _, cx| {
                this.open = !this.open;
                cx.notify();
            }));
        let popover = match self.point {
            Some(p) => popover.anchor(p),
            None => popover.trigger(trigger),
        };
        let other = Press::new("other")
            .track_focus(&self.other)
            .debug_selector(|| "other".into())
            .ml(px(300.))
            .size(px(30.))
            .child("X");
        let page = div()
            .id("page")
            .track_scroll(&self.scroll)
            .overflow_y_scroll()
            .size_full()
            .flex()
            .flex_col()
            .when_place(self.place)
            .child(popover)
            .child(other)
            .child(div().flex_none().h(px(2000.)));
        div().size_full().child(page)
    }
}

trait WhenPlace {
    fn when_place(self, place: Place) -> Self;
}

impl WhenPlace for gpui::Stateful<gpui::Div> {
    fn when_place(self, place: Place) -> Self {
        match place {
            Place::TopLeft => self.pt(px(20.)).pl(px(20.)),
            // Near the bottom of a 300px window.
            Place::Bottom => self.pt(px(250.)).pl(px(20.)),
            // Flush with the right of a 400px window.
            Place::Right => self.pt(px(20.)).items_end(),
        }
    }
}

struct Opened<'a> {
    panel: Entity<Panel>,
    dismissed: Rc<Cell<usize>>,
    draws: Rc<Cell<usize>>,
    vcx: &'a mut VisualTestContext,
}

fn open_with(
    cx: &mut TestAppContext,
    place: Place,
    configure: impl FnOnce(&mut Panel) + 'static,
) -> Opened<'_> {
    cx.update(gpui_core::init);
    let dismissed = Rc::new(Cell::new(0));
    let draws = Rc::new(Cell::new(0));
    let (d, n) = (dismissed.clone(), draws.clone());
    let made = Rc::new(std::cell::RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let panel = cx.new(|cx| {
            let mut p = Panel {
                open: false,
                place,
                rows: 3,
                width: Some(px(200.)),
                long: false,
                take_focus: true,
                stay: false,
                return_to: None,
                point: None,
                trigger: cx.focus_handle(),
                other: cx.focus_handle(),
                scroll: ScrollHandle::new(),
                dismissed: d,
                draws: n,
            };
            configure(&mut p);
            p
        });
        *m.borrow_mut() = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    vcx.simulate_resize(size(px(400.), px(300.)));
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let panel = made.borrow().clone().unwrap();
    Opened {
        panel,
        dismissed,
        draws,
        vcx,
    }
}

fn open(cx: &mut TestAppContext, place: Place) -> Opened<'_> {
    open_with(cx, place, |_| {})
}

fn bounds(vcx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    vcx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} drawn"))
}

fn popover(vcx: &mut VisualTestContext) -> Option<Bounds<Pixels>> {
    vcx.debug_bounds("gpui-core-popover")
}

fn click(vcx: &mut VisualTestContext, at: Point<Pixels>) {
    vcx.simulate_click(at, Modifiers::none());
    vcx.run_until_parked();
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
    }
    vcx.run_until_parked();
}

fn set_open(o: &mut Opened<'_>, open: bool) {
    o.panel.update(o.vcx, |p, cx| {
        p.open = open;
        cx.notify();
    });
    o.vcx.run_until_parked();
}

fn focused(o: &mut Opened<'_>, which: fn(&Panel) -> &FocusHandle) -> bool {
    let handle = o.panel.read_with(o.vcx, |p, _| which(p).clone());
    o.vcx.update(|window, _| handle.is_focused(window))
}

#[gpui::test]
fn it_hangs_below_its_trigger_at_least_as_wide(cx: &mut TestAppContext) {
    let o = open(cx, Place::TopLeft);
    let t = bounds(o.vcx, "trigger");
    click(o.vcx, t.center());
    let p = popover(o.vcx).expect("open");
    assert!(
        p.top() >= t.bottom() && p.top() <= t.bottom() + px(8.),
        "{p:?} {t:?}"
    );
    assert_eq!(p.left(), t.left());
    assert!(p.size.width >= t.size.width);
}

#[gpui::test]
fn near_the_bottom_it_goes_above(cx: &mut TestAppContext) {
    let o = open(cx, Place::Bottom);
    let t = bounds(o.vcx, "trigger");
    click(o.vcx, t.center());
    let p = popover(o.vcx).expect("open");
    assert!(p.bottom() <= t.top(), "{p:?} {t:?}");
}

#[gpui::test]
fn at_the_right_edge_it_shifts_in(cx: &mut TestAppContext) {
    let o = open(cx, Place::Right);
    let t = bounds(o.vcx, "trigger");
    click(o.vcx, t.center());
    let p = popover(o.vcx).expect("open");
    assert!(p.right() <= px(400.) - px(8.) + px(0.5), "{p:?}");
    assert!(p.left() >= px(0.));
}

#[gpui::test]
fn tall_content_scrolls_inside_the_room_there_is(cx: &mut TestAppContext) {
    let o = open_with(cx, Place::TopLeft, |p| p.rows = 100);
    let t = bounds(o.vcx, "trigger");
    click(o.vcx, t.center());
    let p = popover(o.vcx).expect("open");
    assert!(p.bottom() <= px(300.), "{p:?}");
    assert!(p.top() >= t.bottom(), "{p:?}");
}

#[gpui::test]
fn a_click_outside_dismisses_it(cx: &mut TestAppContext) {
    let o = open(cx, Place::TopLeft);
    let t = bounds(o.vcx, "trigger");
    click(o.vcx, t.center());
    assert!(popover(o.vcx).is_some());
    click(o.vcx, point(px(390.), px(290.)));
    assert_eq!(o.dismissed.get(), 1);
    assert!(popover(o.vcx).is_none());
}

#[gpui::test]
fn a_click_inside_doesnt(cx: &mut TestAppContext) {
    let o = open(cx, Place::TopLeft);
    let t = bounds(o.vcx, "trigger");
    click(o.vcx, t.center());
    let p = popover(o.vcx).unwrap();
    click(o.vcx, p.center());
    assert_eq!(o.dismissed.get(), 0);
    assert!(popover(o.vcx).is_some());
}

#[gpui::test]
fn a_second_press_on_the_trigger_toggles_it_closed(cx: &mut TestAppContext) {
    let o = open(cx, Place::TopLeft);
    let t = bounds(o.vcx, "trigger");
    click(o.vcx, t.center());
    click(o.vcx, t.center());
    assert_eq!(
        o.dismissed.get(),
        0,
        "the press is the host's toggle, not a click outside"
    );
    assert!(!o.panel.read_with(o.vcx, |p, _| p.open));
    assert!(popover(o.vcx).is_none());
}

#[gpui::test]
fn it_takes_the_keyboard_and_escape_gives_it_back(cx: &mut TestAppContext) {
    let mut o = open(cx, Place::TopLeft);
    key(o.vcx, "tab");
    assert!(focused(&mut o, |p| &p.trigger));
    key(o.vcx, "space");
    assert!(popover(o.vcx).is_some());
    assert!(
        !focused(&mut o, |p| &p.trigger),
        "the popover has the keyboard"
    );
    key(o.vcx, "escape");
    assert_eq!(o.dismissed.get(), 1);
    assert!(popover(o.vcx).is_none());
    assert!(
        focused(&mut o, |p| &p.trigger),
        "the focus is back on the trigger"
    );
}

#[gpui::test]
fn closed_by_the_host_the_focus_goes_back(cx: &mut TestAppContext) {
    let mut o = open(cx, Place::TopLeft);
    key(o.vcx, "tab space");
    assert!(!focused(&mut o, |p| &p.trigger));
    set_open(&mut o, false);
    assert!(focused(&mut o, |p| &p.trigger));
}

#[gpui::test]
fn the_host_can_name_where_the_focus_goes(cx: &mut TestAppContext) {
    let mut o = open(cx, Place::TopLeft);
    let other = o.panel.read_with(o.vcx, |p, _| p.other.clone());
    o.panel.update(o.vcx, |p, _| p.return_to = Some(other));
    key(o.vcx, "tab space escape");
    assert!(focused(&mut o, |p| &p.other));
}

#[gpui::test]
fn a_click_that_focuses_something_else_keeps_it_there(cx: &mut TestAppContext) {
    let mut o = open(cx, Place::TopLeft);
    key(o.vcx, "tab space");
    let other = bounds(o.vcx, "other");
    // `other` sits under the trigger; the popover covers it, so press its uncovered corner.
    let p = popover(o.vcx).unwrap();
    let at = if other.right() > p.right() {
        point(other.right() - px(2.), other.center().y)
    } else {
        point(other.center().x, other.bottom() - px(2.))
    };
    assert!(!p.contains(&at), "{p:?} {other:?}");
    click(o.vcx, at);
    assert_eq!(o.dismissed.get(), 1);
    assert!(focused(&mut o, |p| &p.other));
}

#[gpui::test]
fn without_take_focus_the_focus_stays_and_escape_is_the_hosts(cx: &mut TestAppContext) {
    let mut o = open_with(cx, Place::TopLeft, |p| p.take_focus = false);
    key(o.vcx, "tab space");
    assert!(popover(o.vcx).is_some());
    assert!(focused(&mut o, |p| &p.trigger));
    key(o.vcx, "escape");
    assert_eq!(o.dismissed.get(), 0);
}

#[gpui::test]
fn its_trigger_scrolling_out_of_view_dismisses_it(cx: &mut TestAppContext) {
    let mut o = open(cx, Place::TopLeft);
    let t = bounds(o.vcx, "trigger");
    click(o.vcx, t.center());
    assert!(popover(o.vcx).is_some());
    o.panel.update(o.vcx, |p, cx| {
        p.scroll.set_offset(point(px(0.), px(-500.)));
        cx.notify();
    });
    o.vcx.run_until_parked();
    assert_eq!(o.dismissed.get(), 1);
    assert!(popover(o.vcx).is_none());
    let _ = &mut o;
}

#[gpui::test]
fn it_hangs_from_a_point(cx: &mut TestAppContext) {
    let mut o = open_with(cx, Place::TopLeft, |p| {
        p.point = Some(point(px(150.), px(100.)))
    });
    set_open(&mut o, true);
    let p = popover(o.vcx).expect("open");
    assert_eq!(p.left(), px(150.));
    assert!(p.top() >= px(100.) && p.top() <= px(108.), "{p:?}");
}

#[gpui::test]
fn closed_it_draws_nothing_and_open_it_doesnt_redraw(cx: &mut TestAppContext) {
    let mut o = open(cx, Place::TopLeft);
    assert!(popover(o.vcx).is_none());
    set_open(&mut o, true);
    assert!(popover(o.vcx).is_some());
    let before = o.draws.get();
    for _ in 0..30 {
        o.vcx.update(|window, cx| window.simulate_next_frame(cx));
        o.vcx
            .executor()
            .advance_clock(std::time::Duration::from_millis(16));
        o.vcx.run_until_parked();
    }
    assert_eq!(o.draws.get(), before, "nothing changed, so nothing redrew");
}

#[gpui::test]
fn short_content_is_as_wide_as_the_trigger(cx: &mut TestAppContext) {
    let o = open_with(cx, Place::TopLeft, |p| {
        p.width = None;
        p.rows = 1;
    });
    let t = bounds(o.vcx, "trigger");
    click(o.vcx, t.center());
    let p = popover(o.vcx).expect("open");
    assert!(p.size.width >= t.size.width, "{p:?} {t:?}");
}

#[gpui::test]
fn in_a_narrow_window_it_stays_inside(cx: &mut TestAppContext) {
    let o = open_with(cx, Place::TopLeft, |p| {
        p.width = None;
        p.long = true;
    });
    o.vcx.simulate_resize(size(px(160.), px(300.)));
    o.vcx.run_until_parked();
    let t = bounds(o.vcx, "trigger");
    click(o.vcx, t.center());
    let p = popover(o.vcx).expect("open");
    assert!(p.left() >= px(0.) && p.right() <= px(160.), "{p:?}");
    assert!(p.bottom() <= px(300.), "{p:?}");
}

#[gpui::test]
fn told_to_stay_it_stays_at_the_edge_when_its_trigger_scrolls_away(cx: &mut TestAppContext) {
    let o = open_with(cx, Place::TopLeft, |p| {
        p.stay = true;
        p.rows = 40;
    });
    let t = bounds(o.vcx, "trigger");
    click(o.vcx, t.center());
    o.panel.update(o.vcx, |p, cx| {
        p.scroll.set_offset(point(px(0.), px(-500.)));
        cx.notify();
    });
    o.vcx.run_until_parked();
    assert_eq!(o.dismissed.get(), 0);
    let p = popover(o.vcx).expect("still open");
    assert!(p.top() >= px(0.) && p.bottom() <= px(300.), "{p:?}");
    // Back in view, it hangs from the trigger again.
    o.panel.update(o.vcx, |p, cx| {
        p.scroll.set_offset(point(px(0.), px(0.)));
        cx.notify();
    });
    o.vcx.run_until_parked();
    let p = popover(o.vcx).unwrap();
    assert!(
        p.top() >= t.bottom() && p.top() <= t.bottom() + px(8.),
        "{p:?} {t:?}"
    );
}
