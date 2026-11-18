//! Drags a host starts itself, as a calendar does from the events it draws on one canvas: a
//! press handed over with `press_at` becomes a drag past the threshold and stays a click short
//! of it; `begin_at` starts one at once (a long press, a key); a finger the host claimed drags;
//! `cancel` ends it; the source picks the cursor; a target that catches the drag outside its
//! bounds hears it there and takes the drop; a zone the canvas registers itself auto-scrolls,
//! past its edges and only for the drags it takes; nothing runs when it's over.

mod common;

use std::cell::Cell;
use std::rc::Rc;

use common::*;
use gpui::{
    App, Bounds, CursorStyle, DispatchPhase, InteractiveElement as _, IntoElement, LongPressEvent,
    MouseButton, ParentElement as _, Pixels, Point, StatefulInteractiveElement as _, Styled as _,
    TestAppContext, TouchDragEvent, TouchEvent, TouchId, TouchPhase, Window, canvas, div, point,
    px, size,
};
use gpui_dnd::*;

#[derive(Clone, Debug)]
struct Ev(u32);

/// Where in the event the canvas's events are taken, and their size.
const GRAB: Point<Pixels> = Point {
    x: px(10.),
    y: px(5.),
};

fn press(backend: Backend) -> Press {
    Press {
        grab: GRAB,
        size: size(px(80.), px(30.)),
        label: Some("Standup".into()),
        ..Press::new(backend)
    }
}

fn source(log: &Log, n: u32) -> DragSource<Ev> {
    let (b, e) = (log.clone(), log.clone());
    DragSource::item(Ev(n))
        .on_begin(move |m, _, _| {
            b.borrow_mut()
                .push(format!("begin {:?}", m.backend().unwrap()))
        })
        .on_end(move |end, _, _| {
            e.borrow_mut().push(format!(
                "end {} dropped={} cancelled={}",
                end.item().0,
                end.dropped,
                end.cancelled
            ))
        })
        .cursor(|ev, _| match ev.0 {
            2 => CursorStyle::ResizeUpDown,
            _ => CursorStyle::ClosedHand,
        })
}

/// How the canvas hands its presses over.
#[derive(Clone, Copy, PartialEq)]
enum Start {
    /// `press_at` on a mouse down, and on a claimed touch.
    Press,
    /// `begin_at` on a mouse down, and on a long press.
    Begin,
}

/// A 200 by 200 canvas that starts drags of `Ev(event)` itself, and a target to its right.
fn calendar(
    log: Log,
    start: Start,
    event: Rc<Cell<u32>>,
    target: impl Fn(DropTarget) -> DropTarget + 'static,
) -> impl Fn(&mut Window, &mut gpui::Context<Host>) -> gpui::AnyElement {
    let target = Rc::new(target);
    move |_, _| {
        let (l, c, t, d, h) = (
            log.clone(),
            log.clone(),
            log.clone(),
            log.clone(),
            log.clone(),
        );
        let (e, et) = (event.clone(), event.clone());
        div()
            .p(px(20.))
            .flex()
            .gap(px(40.))
            .child(
                div()
                    .id("canvas")
                    .debug_selector(|| "canvas".into())
                    .size(px(200.))
                    .on_mouse_down(MouseButton::Left, move |ev, window, cx| {
                        let s = source(&l, e.get());
                        match start {
                            Start::Press => {
                                press_at(window, cx, "event", ev.position, s, press(Backend::Mouse))
                            }
                            Start::Begin => {
                                begin_at(
                                    window,
                                    cx,
                                    "event",
                                    ev.position,
                                    s,
                                    press(Backend::Mouse),
                                );
                            }
                        }
                    })
                    .on_click(move |_, _, _| c.borrow_mut().push("click".into()))
                    .child(
                        canvas(
                            |_, _, _| (),
                            move |bounds, _, window, _| {
                                claim_touches(bounds, start, t.clone(), et.clone(), window)
                            },
                        )
                        .size_full(),
                    ),
            )
            .child(
                dnd("t")
                    .debug_selector(|| "t".into())
                    .size(px(200.))
                    .target(target(
                        DropTarget::new()
                            .accept::<Ev>()
                            .on_hover(move |_, m, _, _| {
                                let at = m.client_offset().unwrap();
                                h.borrow_mut().push(format!(
                                    "hover {:.0},{:.0} over={} can_drop={}",
                                    f32::from(at.x),
                                    f32::from(at.y),
                                    m.is_over(Over::Deep),
                                    m.can_drop()
                                ))
                            })
                            .on_drop(move |item, _, _, _| {
                                d.borrow_mut()
                                    .push(format!("drop {}", item.get::<Ev>().unwrap().0));
                                DropOutcome::Handled(None)
                            }),
                    )),
            )
            .into_any_element()
    }
}

/// The canvas claims a finger on it, as calendar does for its selected event.
fn claim_touches(
    bounds: Bounds<Pixels>,
    start: Start,
    log: Log,
    event: Rc<Cell<u32>>,
    window: &mut Window,
) {
    let (l, e) = (log.clone(), event.clone());
    window.on_mouse_event(move |ev: &TouchDragEvent, phase, window, cx| {
        if phase != DispatchPhase::Bubble
            || ev.phase != TouchPhase::Started
            || start != Start::Press
            || !bounds.contains(&ev.start_position)
        {
            return;
        }
        window.prevent_default();
        cx.stop_propagation();
        let s = source(&l, e.get());
        press_at(
            window,
            cx,
            "event",
            ev.start_position,
            s,
            press(Backend::Touch),
        );
    });
    window.on_mouse_event(move |ev: &LongPressEvent, phase, window, cx| {
        if phase != DispatchPhase::Bubble
            || ev.phase != TouchPhase::Started
            || start != Start::Begin
            || !bounds.contains(&ev.start_position)
        {
            return;
        }
        let s = source(&log, event.get());
        if begin_at(
            window,
            cx,
            "event",
            ev.start_position,
            s,
            press(Backend::Touch),
        ) {
            window.prevent_default();
            cx.stop_propagation();
        }
    });
}

fn dragging(o: &mut Opened) -> bool {
    o.vcx.update(|_, cx| DragMonitor::of(cx).is_dragging())
}

fn touch(o: &mut Opened, phase: TouchPhase, at: Point<Pixels>) {
    o.vcx.simulate_event(TouchEvent {
        id: TouchId(1),
        phase,
        position: at,
        predicted_position: None,
        force: None,
    });
    o.settle();
}

fn slide(o: &mut Opened, from: Point<Pixels>, to: Point<Pixels>) {
    for i in 1..=8 {
        let t = i as f32 / 8.;
        touch(o, TouchPhase::Moved, from + (to - from) * t);
        o.wait(16);
    }
}

fn frames(o: &mut Opened, n: usize) {
    for _ in 0..n {
        o.wait(16);
        o.frame();
    }
}

#[gpui::test]
fn a_press_past_the_threshold_drags_and_one_short_of_it_clicks(cx: &mut TestAppContext) {
    let log = log();
    let event = Rc::new(Cell::new(1));
    let mut o = open(cx, calendar(log.clone(), Start::Press, event, |t| t));
    let from = o.center("canvas");

    // A click: the host hears it, no drag.
    o.down(from);
    o.up(from);
    assert_eq!(with(&log, "click"), ["click"]);
    assert!(with(&log, "begin").is_empty());

    // Short of the threshold (a quarter rem), still no drag.
    o.down(from);
    o.drag_to(from + point(px(2.), px(0.)));
    assert!(!dragging(&mut o));
    assert!(with(&log, "begin").is_empty());

    // Past it, a drag from "event", the grab point under the pointer.
    let t = o.center("t");
    o.drag_to(from + point(px(20.), px(0.)));
    assert_eq!(with(&log, "begin"), ["begin Mouse"]);
    o.drag_to(t);
    let (source, offset, size_) = o.vcx.update(|_, cx| {
        let m = DragMonitor::of(cx);
        (
            m.is_dragging_source(&"event".into()),
            m.source_client_offset().unwrap(),
            m.source_size().unwrap(),
        )
    });
    assert!(source);
    assert_eq!(offset, t - GRAB);
    assert_eq!(size_, size(px(80.), px(30.)));
    o.draw();
    assert_eq!(o.bounds("dnd-preview").origin, t - GRAB);
    o.up(t);
    assert_eq!(with(&log, "drop"), ["drop 1"]);
    assert_eq!(with(&log, "end"), ["end 1 dropped=true cancelled=false"]);
    assert_eq!(
        with(&log, "click"),
        ["click"],
        "the drag's release isn't a click"
    );

    // Nothing runs once it's over.
    let renders = o.renders.get();
    o.wait(1000);
    assert_eq!(o.frames_wanted(), 0);
    assert_eq!(o.renders.get(), renders);
}

#[gpui::test]
fn begin_at_starts_at_once_and_cancel_ends_it(cx: &mut TestAppContext) {
    let log = log();
    let event = Rc::new(Cell::new(1));
    let mut o = open(cx, calendar(log.clone(), Start::Begin, event, |t| t));
    let from = o.center("canvas");
    o.down(from);
    assert_eq!(with(&log, "begin"), ["begin Mouse"], "at once, unmoved");
    assert!(dragging(&mut o));
    o.vcx.update(gpui_dnd::cancel);
    o.wait(300);
    assert_eq!(with(&log, "end"), ["end 1 dropped=false cancelled=true"]);
    assert!(!dragging(&mut o));
    o.up(from);

    // A second begin while one is on is refused.
    o.down(from);
    let again = o.vcx.update(|window, cx| {
        begin_at(
            window,
            cx,
            "other",
            from,
            DragSource::item(Ev(9)),
            Press::default(),
        )
    });
    assert!(!again);
    o.up(from);
}

#[gpui::test]
fn the_drag_shows_its_sources_cursor(cx: &mut TestAppContext) {
    let log = log();
    let event = Rc::new(Cell::new(2));
    let mut o = open(
        cx,
        calendar(log.clone(), Start::Press, event.clone(), |t| t),
    );
    let from = o.center("canvas");
    assert_eq!(o.vcx.update(|_, cx| drag_cursor(cx)), None);
    o.hold(from, from + point(px(0.), px(30.)));
    assert_eq!(
        o.vcx.update(|_, cx| drag_cursor(cx)),
        Some(CursorStyle::ResizeUpDown)
    );
    o.up(from + point(px(0.), px(30.)));
    o.wait(300);

    // A source without a cursor of its own shows a closed hand.
    o.vcx.update(|window, cx| {
        assert!(begin_at(
            window,
            cx,
            "plain",
            from,
            DragSource::item(Ev(3)),
            Press::default()
        ));
        assert_eq!(drag_cursor(cx), Some(CursorStyle::ClosedHand));
        gpui_dnd::cancel(window, cx);
    });
}

#[gpui::test]
fn a_finger_the_host_claimed_drags_past_the_slop(cx: &mut TestAppContext) {
    let log = log();
    let event = Rc::new(Cell::new(1));
    let mut o = open(cx, calendar(log.clone(), Start::Press, event, |t| t));
    let from = o.center("canvas");
    let t = o.center("t");
    touch(&mut o, TouchPhase::Started, from);
    touch(&mut o, TouchPhase::Moved, from + point(px(4.), px(0.)));
    assert!(with(&log, "begin").is_empty(), "within the slop");
    slide(&mut o, from, t);
    assert_eq!(with(&log, "begin"), ["begin Touch"]);
    assert!(
        o.vcx
            .update(|_, cx| DragMonitor::of(cx).is_over(&"t".into(), Over::Shallow))
    );
    touch(&mut o, TouchPhase::Ended, t);
    assert_eq!(with(&log, "drop"), ["drop 1"]);
    assert_eq!(with(&log, "end"), ["end 1 dropped=true cancelled=false"]);
}

#[gpui::test]
fn a_long_press_the_host_takes_begins_at_once(cx: &mut TestAppContext) {
    let log = log();
    let event = Rc::new(Cell::new(1));
    let mut o = open(cx, calendar(log.clone(), Start::Begin, event, |t| t));
    let from = o.center("canvas");
    let t = o.center("t");
    touch(&mut o, TouchPhase::Started, from);
    o.wait(600);
    assert_eq!(with(&log, "begin"), ["begin Touch"]);
    slide(&mut o, from, t);
    touch(&mut o, TouchPhase::Ended, t);
    assert_eq!(with(&log, "drop"), ["drop 1"]);
}

#[gpui::test]
fn a_target_that_catches_outside_hears_the_pointer_and_takes_the_drop(cx: &mut TestAppContext) {
    let log = log();
    let event = Rc::new(Cell::new(1));
    let mut o = open(
        cx,
        calendar(log.clone(), Start::Press, event, |t| t.catch_outside(true)),
    );
    let from = o.center("canvas");
    let t = o.bounds("t");
    // Below the target, over nothing.
    let outside = point(t.center().x, t.bottom() + px(100.));
    o.hold(from, outside);
    let last = with(&log, "hover").pop().unwrap();
    assert_eq!(
        last,
        format!(
            "hover {:.0},{:.0} over=false can_drop=true",
            f32::from(outside.x),
            f32::from(outside.y)
        ),
        "it hears the pointer outside, and isn't over it"
    );
    let ids = o.vcx.update(|_, cx| DragMonitor::of(cx).target_ids());
    assert!(ids.is_empty(), "no target is under the pointer: {ids:?}");
    o.up(outside);
    assert_eq!(with(&log, "drop"), ["drop 1"]);
    assert_eq!(with(&log, "end"), ["end 1 dropped=true cancelled=false"]);
}

#[gpui::test]
fn without_catching_outside_a_release_there_is_no_drop(cx: &mut TestAppContext) {
    let log = log();
    let event = Rc::new(Cell::new(1));
    let mut o = open(cx, calendar(log.clone(), Start::Press, event, |t| t));
    let from = o.center("canvas");
    let t = o.bounds("t");
    let outside = point(t.center().x, t.bottom() + px(100.));
    o.hold(from, outside);
    o.up(outside);
    o.wait(300);
    assert!(with(&log, "drop").is_empty());
    assert_eq!(with(&log, "end"), ["end 1 dropped=false cancelled=true"]);
}

#[gpui::test]
fn a_target_under_the_pointer_wins_over_one_that_catches_outside(cx: &mut TestAppContext) {
    let log = log();
    let l = log.clone();
    let catcher = calendar(log.clone(), Start::Press, Rc::new(Cell::new(1)), |t| {
        t.catch_outside(true)
    });
    let mut o = open(cx, move |window, cx| {
        let l = l.clone();
        div()
            .size_full()
            .child(catcher(window, cx))
            .child(
                dnd("list")
                    .debug_selector(|| "list".into())
                    .size(px(100.))
                    .target(DropTarget::new().accept::<Ev>().on_drop(move |_, _, _, _| {
                        l.borrow_mut().push("drop on list".into());
                        DropOutcome::Handled(None)
                    })),
            )
            .into_any_element()
    });
    let from = o.center("canvas");
    let list = o.center("list");
    o.drag(from, list);
    assert_eq!(with(&log, "drop"), ["drop on list"]);
}

#[gpui::test]
fn of_two_that_catch_outside_the_one_that_takes_the_item_does(cx: &mut TestAppContext) {
    let log = log();
    let (a, b) = (log.clone(), log.clone());
    let s = log.clone();
    let mut o = open(cx, move |_, _| {
        let (a, b, s) = (a.clone(), b.clone(), s.clone());
        let mine = |n: u32| {
            move |item: &DragItem, _: &DragMonitor, _: &App| {
                item.get::<Ev>().is_some_and(|e| e.0 == n)
            }
        };
        div()
            .p(px(20.))
            .flex()
            .gap(px(20.))
            .child(
                dnd("src")
                    .debug_selector(|| "src".into())
                    .size(px(50.))
                    .source(source(&s, 1)),
            )
            .child(
                dnd("one").size(px(100.)).target(
                    DropTarget::new()
                        .accept::<Ev>()
                        .catch_outside(true)
                        .can_drop(mine(1))
                        .on_drop(move |_, _, _, _| {
                            a.borrow_mut().push("drop on one".into());
                            DropOutcome::Handled(None)
                        }),
                ),
            )
            .child(
                // Drawn last, but takes only event 2.
                dnd("two").size(px(100.)).target(
                    DropTarget::new()
                        .accept::<Ev>()
                        .catch_outside(true)
                        .can_drop(mine(2))
                        .on_drop(move |_, _, _, _| {
                            b.borrow_mut().push("drop on two".into());
                            DropOutcome::Handled(None)
                        }),
                ),
            )
            .into_any_element()
    });
    let from = o.center("src");
    o.drag(from, point(px(400.), px(500.)));
    assert_eq!(with(&log, "drop"), ["drop on one"]);
}

/// A scroller in "minutes": an offset in a cell, up to 1000 down.
#[derive(Clone, Default)]
struct Minutes(Rc<Cell<Point<Pixels>>>);

impl Scrollable for Minutes {
    fn offset(&self) -> Point<Pixels> {
        self.0.get()
    }
    fn max_offset(&self) -> Point<Pixels> {
        point(px(0.), px(1000.))
    }
    fn set_offset(&self, offset: Point<Pixels>, _: &mut Window, _: &mut App) {
        self.0.set(offset)
    }
}

#[gpui::test]
fn a_zone_the_canvas_registers_scrolls_past_its_edges_for_the_drags_it_takes(
    cx: &mut TestAppContext,
) {
    let log = log();
    let minutes = Minutes::default();
    let m = minutes.clone();
    let event = Rc::new(Cell::new(1));
    let base = calendar(log.clone(), Start::Press, event.clone(), |t| {
        t.catch_outside(true)
    });
    let mut o = open(cx, move |window, cx| {
        let m = m.clone();
        div()
            .size_full()
            .child(base(window, cx))
            .child(
                canvas(
                    |_, _, _| (),
                    move |_, _, window, cx| {
                        // The canvas's hours: a band of it, 100 to 200 down the window.
                        let band = Bounds::new(point(px(20.), px(100.)), size(px(200.), px(100.)));
                        auto_scroll_zone(
                            window,
                            cx,
                            band,
                            Rc::new(m.clone()),
                            AutoScroll::vertical()
                                .for_items(|item| item.get::<Ev>().is_some_and(|e| e.0 != 3))
                                .beyond_edges(true),
                        );
                    },
                )
                .size(px(1.)),
            )
            .into_any_element()
    });
    let from = point(px(100.), px(150.));
    // Near the band's bottom: it scrolls down, frame by frame.
    o.hold(from, point(px(100.), px(195.)));
    frames(&mut o, 10);
    let down = minutes.0.get().y;
    assert!(down < px(0.), "scrolled down: {down:?}");

    // Past its top, over what's above it: back up, at full speed.
    o.drag_to(point(px(100.), px(40.)));
    frames(&mut o, 10);
    assert!(minutes.0.get().y > down, "scrolling up past the edge");

    // In the middle it stops, and asks for no frames.
    o.drag_to(point(px(100.), px(150.)));
    frames(&mut o, 2);
    assert_eq!(o.frames_wanted(), 0);
    o.up(point(px(100.), px(150.)));
    o.wait(300);

    // A drag it doesn't take doesn't scroll it.
    event.set(3);
    let still = minutes.0.get();
    o.hold(from, point(px(100.), px(198.)));
    frames(&mut o, 10);
    assert_eq!(minutes.0.get(), still);
    assert_eq!(o.frames_wanted(), 0);
    o.up(point(px(100.), px(198.)));
}

#[gpui::test]
fn a_keyboard_drag_the_host_begins_steps_through_its_target(cx: &mut TestAppContext) {
    let log = log();
    let event = Rc::new(Cell::new(1));
    let steps = log.clone();
    let mut o = open(
        cx,
        calendar(log.clone(), Start::Press, event.clone(), move |t| {
            let steps = steps.clone();
            t.keyboard_step(move |_, m, dir, _, _| {
                steps
                    .borrow_mut()
                    .push(format!("step {:.0},{:.0}", dir.x, dir.y));
                Some(m.client_offset().unwrap() + point(px(0.), px(10.) * dir.y))
            })
            .describe(|_, m| {
                format!(
                    "Standup, moving to {:.0}",
                    f32::from(m.client_offset().unwrap().y)
                )
                .into()
            })
        }),
    );
    let t = o.center("t");
    let began = o.vcx.update(|window, cx| {
        begin_at(
            window,
            cx,
            "event",
            t,
            source(&log, 1),
            press(Backend::Keyboard),
        )
    });
    assert!(began);
    assert_eq!(with(&log, "begin"), ["begin Keyboard"]);
    assert_eq!(
        o.said().unwrap(),
        "Picked up Standup. Arrow keys move it, Space drops it, Escape cancels.",
        "said by the item's label"
    );
    o.keys("down");
    assert_eq!(with(&log, "step"), ["step 0,1"]);
    let y = f32::from(t.y) + 10.;
    assert_eq!(
        o.said().unwrap(),
        format!("Over Standup, moving to {y:.0}.")
    );
    o.keys("space");
    assert_eq!(with(&log, "drop"), ["drop 1"]);
    assert_eq!(with(&log, "end"), ["end 1 dropped=true cancelled=false"]);
}
