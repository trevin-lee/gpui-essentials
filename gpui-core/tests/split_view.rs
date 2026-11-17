//! SplitView: panes sit at their widths with the last taking the rest; a drag on a handle (its
//! hit area wider than its line, 44 points for touch) resizes within the pane's minimum and
//! maximum and leaves the last pane its minimum; a focused handle moves with the arrows, Shift,
//! Home and End; a double press resets; the host is told the widths and passing them back keeps
//! them; a collapsed pane gives its room to the others; below the narrow width one pane shows,
//! chosen by the host, which is told; content never draws outside its pane; and nothing
//! redraws while idle.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AppContext as _, Bounds, Context, Entity, InteractiveElement as _, IntoElement, KeyDownEvent,
    KeyUpEvent, Keystroke, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ParentElement as _, Pixels, Point, Render, Styled as _, TestAppContext, VisualTestContext,
    Window, div, point, prelude::FluentBuilder as _, px, rgb,
};
use gpui_core::{Pane, Root, SplitView};

struct Host {
    width: f32,
    collapsed: [bool; 3],
    /// What it passes back as the widths; on_resize sets it when `keep`.
    widths: Option<Vec<Pixels>>,
    keep: bool,
    told: Rc<RefCell<Vec<Vec<Pixels>>>>,
    narrow_below: Option<f32>,
    shows: usize,
    /// What on_narrow_change said, every time, if it listens.
    narrow: Vec<bool>,
    listens: bool,
    touch: bool,
    /// A child far wider than its pane, in the first pane.
    wide: bool,
    draws: Rc<Cell<usize>>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let told = self.told.clone();
        let mut split = SplitView::new("split")
            .pane(
                Pane::new(div().size_full().child("Sidebar").when(self.wide, |d| {
                    d.child(div().w(px(2000.)).h(px(20.)).bg(rgb(0xff0000)))
                }))
                .width(px(200.))
                .min(px(120.))
                .max(px(300.))
                .collapsed(self.collapsed[0])
                .accessibility_label("Sidebar"),
            )
            .pane(
                Pane::new(div().size_full().child("List"))
                    .width(px(250.))
                    .min(px(150.))
                    .max(px(400.))
                    .collapsed(self.collapsed[1]),
            )
            .pane(
                Pane::new(div().size_full().child("Detail"))
                    .min(px(200.))
                    .collapsed(self.collapsed[2]),
            )
            .touch(self.touch)
            .narrow_shows(self.shows)
            .on_resize(cx.listener(move |this, widths: &[Pixels], _, cx| {
                told.borrow_mut().push(widths.to_vec());
                if this.keep {
                    this.widths = Some(widths.to_vec());
                    cx.notify();
                }
            }));
        if self.listens {
            split = split.on_narrow_change(cx.listener(|this, narrow: &bool, _, cx| {
                this.narrow.push(*narrow);
                cx.notify();
            }));
        }
        if let Some(w) = &self.widths {
            split = split.widths(w.clone());
        }
        if let Some(below) = self.narrow_below {
            split = split.narrow_below(px(below));
        }
        div().size_full().child(
            div()
                .debug_selector(|| "box".into())
                .w(px(self.width))
                .h(px(300.))
                .child(split),
        )
    }
}

struct Opened<'a> {
    host: Entity<Host>,
    told: Rc<RefCell<Vec<Vec<Pixels>>>>,
    draws: Rc<Cell<usize>>,
    vcx: &'a mut VisualTestContext,
}

fn open_with(cx: &mut TestAppContext, set: impl FnOnce(&mut Host) + 'static) -> Opened<'_> {
    cx.update(gpui_core::init);
    let told: Rc<RefCell<Vec<Vec<Pixels>>>> = Rc::default();
    let draws: Rc<Cell<usize>> = Rc::default();
    let made = Rc::new(RefCell::new(None));
    let (m, t, d) = (made.clone(), told.clone(), draws.clone());
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let host = cx.new(|_| {
            let mut h = Host {
                width: 900.,
                collapsed: [false; 3],
                widths: None,
                keep: false,
                told: t,
                narrow_below: None,
                shows: 0,
                narrow: Vec::new(),
                listens: true,
                touch: false,
                wide: false,
                draws: d,
            };
            set(&mut h);
            h
        });
        *m.borrow_mut() = Some(host.clone());
        Root::new(host, window, cx)
    });
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let host = made.borrow().clone().unwrap();
    Opened {
        host,
        told,
        draws,
        vcx,
    }
}

fn open(cx: &mut TestAppContext) -> Opened<'_> {
    open_with(cx, |_| {})
}

fn pane(vcx: &mut VisualTestContext, i: usize) -> Bounds<Pixels> {
    vcx.debug_bounds(["split-pane-0", "split-pane-1", "split-pane-2"][i])
        .unwrap_or_default()
}

/// Each pane's width, rounded.
fn widths(vcx: &mut VisualTestContext) -> [f32; 3] {
    [0, 1, 2].map(|i| f32::from(pane(vcx, i).size.width).round())
}

fn hit(vcx: &mut VisualTestContext, i: usize) -> Point<Pixels> {
    vcx.debug_bounds(["split-hit-0", "split-hit-1"][i])
        .expect("a handle")
        .center()
}

fn down(vcx: &mut VisualTestContext, at: Point<Pixels>, clicks: usize) {
    vcx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: Modifiers::none(),
        click_count: clicks,
        first_mouse: false,
    });
    vcx.run_until_parked();
}

fn drag_to(vcx: &mut VisualTestContext, at: Point<Pixels>) {
    vcx.simulate_event(MouseMoveEvent {
        position: at,
        pressed_button: Some(MouseButton::Left),
        modifiers: Modifiers::none(),
    });
    vcx.run_until_parked();
}

fn up(vcx: &mut VisualTestContext, at: Point<Pixels>, clicks: usize) {
    vcx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: Modifiers::none(),
        click_count: clicks,
    });
    vcx.run_until_parked();
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

/// Delivers display frames with nothing happening, and says how many views drew.
fn idle(o: &mut Opened) -> usize {
    let before = o.draws.get();
    for _ in 0..30 {
        o.vcx.update(|window, cx| window.simulate_next_frame(cx));
        o.vcx.executor().advance_clock(Duration::from_millis(16));
        o.vcx.run_until_parked();
    }
    o.draws.get() - before
}

#[gpui::test]
fn panes_sit_at_their_widths_and_the_last_takes_the_rest(cx: &mut TestAppContext) {
    let o = open(cx);
    // Two one-pixel lines between them.
    assert_eq!(widths(o.vcx), [200., 250., 448.]);
    let (a, b, c) = (pane(o.vcx, 0), pane(o.vcx, 1), pane(o.vcx, 2));
    assert_eq!(b.left(), a.right() + px(1.));
    assert_eq!(c.left(), b.right() + px(1.));
    assert_eq!(c.right(), o.vcx.debug_bounds("box").unwrap().right());
}

#[gpui::test]
fn a_drag_resizes_within_the_min_and_max(cx: &mut TestAppContext) {
    let o = open(cx);
    // The hit area is wider than the line: a press 3 pixels off it still takes.
    let at = hit(o.vcx, 0) + point(px(3.), px(0.));
    down(o.vcx, at, 1);
    let before = o.draws.get();
    drag_to(o.vcx, at + point(px(50.), px(0.)));
    assert_eq!(widths(o.vcx), [250., 250., 398.]);
    assert!(o.draws.get() > before, "a move didn't redraw");
    let before = o.draws.get();
    drag_to(o.vcx, at + point(px(60.), px(0.)));
    assert!(o.draws.get() > before, "every move redraws");
    drag_to(o.vcx, at + point(px(500.), px(0.)));
    assert_eq!(widths(o.vcx)[0], 300., "past the maximum is the maximum");
    drag_to(o.vcx, at - point(px(1000.), px(0.)));
    assert_eq!(widths(o.vcx)[0], 120., "past the minimum is the minimum");
    assert!(o.told.borrow().is_empty(), "told before the drag ended");
    up(o.vcx, at, 1);
    assert_eq!(*o.told.borrow(), [vec![px(120.), px(250.)]]);
    // Let go, moving doesn't drag.
    o.vcx.simulate_event(MouseMoveEvent {
        position: at,
        pressed_button: None,
        modifiers: Modifiers::none(),
    });
    o.vcx.run_until_parked();
    assert_eq!(widths(o.vcx)[0], 120.);
}

#[gpui::test]
fn a_drag_leaves_the_last_pane_its_minimum(cx: &mut TestAppContext) {
    let o = open_with(cx, |h| h.width = 700.);
    let at = hit(o.vcx, 1);
    down(o.vcx, at, 1);
    drag_to(o.vcx, at + point(px(300.), px(0.)));
    up(o.vcx, at, 1);
    // 700 less the lines, the sidebar's 200 and the detail's minimum of 200.
    assert_eq!(widths(o.vcx), [200., 298., 200.]);
}

#[gpui::test]
fn the_keyboard_moves_a_focused_handle(cx: &mut TestAppContext) {
    let o = open(cx);
    keys(o.vcx, "tab right");
    assert_eq!(widths(o.vcx)[0], 210.);
    keys(o.vcx, "shift-right");
    assert_eq!(widths(o.vcx)[0], 260.);
    keys(o.vcx, "home");
    assert_eq!(widths(o.vcx)[0], 120.);
    keys(o.vcx, "left");
    assert_eq!(widths(o.vcx)[0], 120., "no further than the minimum");
    keys(o.vcx, "end");
    assert_eq!(widths(o.vcx)[0], 300.);
    // The next handle: the list.
    keys(o.vcx, "tab shift-left");
    assert_eq!(widths(o.vcx), [300., 200., 398.]);
    let told = o.told.borrow();
    assert_eq!(
        told.len(),
        5,
        "told each real move, not the one at the minimum"
    );
    assert_eq!(told.last().unwrap(), &vec![px(300.), px(200.)]);
}

#[gpui::test]
fn a_double_press_resets_the_width(cx: &mut TestAppContext) {
    let o = open(cx);
    keys(o.vcx, "tab end");
    assert_eq!(widths(o.vcx)[0], 300.);
    let at = hit(o.vcx, 0);
    down(o.vcx, at, 1);
    up(o.vcx, at, 1);
    down(o.vcx, at, 2);
    up(o.vcx, at, 2);
    assert_eq!(widths(o.vcx)[0], 200.);
    assert_eq!(o.told.borrow().last().unwrap(), &vec![px(200.), px(250.)]);
}

#[gpui::test]
fn widths_told_and_passed_back_are_kept(cx: &mut TestAppContext) {
    let o = open_with(cx, |h| h.keep = true);
    let at = hit(o.vcx, 0);
    down(o.vcx, at, 1);
    drag_to(o.vcx, at + point(px(40.), px(0.)));
    up(o.vcx, at, 1);
    let kept = o.host.read_with(o.vcx, |h, _| h.widths.clone());
    assert_eq!(kept, Some(vec![px(240.), px(250.)]));
    assert_eq!(widths(o.vcx), [240., 250., 408.]);
    // The host changes them: they're laid out.
    o.host.update(o.vcx, |h, cx| {
        h.widths = Some(vec![px(180.), px(300.)]);
        cx.notify();
    });
    o.vcx.run_until_parked();
    assert_eq!(widths(o.vcx), [180., 300., 418.]);
    // Passing the same ones again doesn't undo a drag.
    o.host.update(o.vcx, |h, _| h.keep = false);
    let at = hit(o.vcx, 0);
    down(o.vcx, at, 1);
    drag_to(o.vcx, at + point(px(20.), px(0.)));
    up(o.vcx, at, 1);
    assert_eq!(widths(o.vcx)[0], 200.);
}

#[gpui::test]
fn kept_widths_lay_out_a_new_view(cx: &mut TestAppContext) {
    let o = open_with(cx, |h| h.widths = Some(vec![px(180.), px(300.)]));
    assert_eq!(widths(o.vcx), [180., 300., 418.]);
}

#[gpui::test]
fn a_collapsed_pane_gives_its_room_to_the_others(cx: &mut TestAppContext) {
    let o = open(cx);
    o.host.update(o.vcx, |h, cx| {
        h.collapsed[0] = true;
        cx.notify();
    });
    o.vcx.run_until_parked();
    assert_eq!(widths(o.vcx), [0., 250., 649.]);
    assert_eq!(pane(o.vcx, 1).left(), px(0.));
    assert!(
        o.vcx.debug_bounds("split-hit-0").is_none(),
        "no handle for it"
    );
    // The last collapsed: the list takes the rest.
    o.host.update(o.vcx, |h, cx| {
        h.collapsed = [false, false, true];
        cx.notify();
    });
    o.vcx.run_until_parked();
    assert_eq!(widths(o.vcx), [200., 699., 0.]);
}

#[gpui::test]
fn narrow_it_shows_the_chosen_pane_and_says_so(cx: &mut TestAppContext) {
    let mut o = open_with(cx, |h| {
        h.width = 500.;
        h.narrow_below = Some(600.);
        h.shows = 2;
    });
    assert_eq!(widths(o.vcx), [0., 0., 500.]);
    assert!(o.vcx.debug_bounds("split-hit-0").is_none());
    assert_eq!(o.host.read_with(o.vcx, |h, _| h.narrow.clone()), [true]);
    o.host.update(o.vcx, |h, cx| {
        h.shows = 1;
        cx.notify();
    });
    o.vcx.run_until_parked();
    assert_eq!(widths(o.vcx), [0., 500., 0.]);
    assert_eq!(idle(&mut o), 0, "narrow and idle, it drew again");
}

#[gpui::test]
fn narrow_it_shows_one_pane_with_no_one_listening(cx: &mut TestAppContext) {
    // Nothing redraws the host when it turns out narrow: the split view redraws itself.
    let o = open_with(cx, |h| {
        h.narrow_below = Some(600.);
        h.listens = false;
    });
    assert_eq!(widths(o.vcx), [200., 250., 448.]);
    // Drawn once at 500 by the guess it was wide, then again by itself, narrow.
    o.host.update(o.vcx, |h, cx| {
        h.width = 500.;
        cx.notify();
    });
    o.vcx.run_until_parked();
    assert_eq!(widths(o.vcx), [500., 0., 0.]);
}

#[gpui::test]
fn widening_past_the_line_shows_every_pane_and_says_so(cx: &mut TestAppContext) {
    let o = open_with(cx, |h| {
        h.width = 500.;
        h.narrow_below = Some(600.);
    });
    o.host.update(o.vcx, |h, cx| {
        h.width = 900.;
        cx.notify();
    });
    o.vcx.run_until_parked();
    assert_eq!(widths(o.vcx), [200., 250., 448.]);
    assert_eq!(
        o.host.read_with(o.vcx, |h, _| h.narrow.clone()),
        [true, false]
    );
}

#[gpui::test]
fn touch_handles_take_presses_44_points_wide(cx: &mut TestAppContext) {
    let o = open_with(cx, |h| h.touch = true);
    let b = o.vcx.debug_bounds("split-hit-0").unwrap();
    assert!(b.size.width >= px(44.), "{b:?}");
    // A press 20 points off the line drags it.
    let at = b.center() + point(px(20.), px(0.));
    down(o.vcx, at, 1);
    drag_to(o.vcx, at + point(px(30.), px(0.)));
    up(o.vcx, at, 1);
    assert_eq!(widths(o.vcx)[0], 230.);
}

#[gpui::test]
fn content_never_draws_outside_its_pane(cx: &mut TestAppContext) {
    let o = open_with(cx, |h| h.wide = true);
    // The 2000-pixel child doesn't widen its pane or push the others.
    assert_eq!(widths(o.vcx), [200., 250., 448.]);
    let first = pane(o.vcx, 0);
    let clipped = o.vcx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
        let scale = window.scale_factor();
        let wide: Vec<_> = window
            .rendered_scene()
            .quads
            .iter()
            // Only the 2000-pixel child: the window and its root are narrower.
            .filter(|q| (q.bounds.size.width.as_f32() - 2000. * scale).abs() < 1.)
            .map(|q| {
                let m = q.content_mask.bounds;
                (m.origin.x.as_f32() + m.size.width.as_f32()) / scale
            })
            .collect();
        wide
    });
    assert!(!clipped.is_empty(), "the wide child wasn't drawn");
    for right in clipped {
        assert!(
            right <= f32::from(first.right()) + 0.5,
            "drawn to {right}, past the pane's edge at {:?}",
            first.right()
        );
    }
}

#[gpui::test]
fn nothing_redraws_while_idle(cx: &mut TestAppContext) {
    let mut o = open(cx);
    assert_eq!(idle(&mut o), 0, "drew again with nothing happening");
    // Nor after a drag.
    let at = hit(o.vcx, 0);
    down(o.vcx, at, 1);
    drag_to(o.vcx, at + point(px(30.), px(0.)));
    up(o.vcx, at, 1);
    assert_eq!(idle(&mut o), 0, "drew again after the drag");
}
