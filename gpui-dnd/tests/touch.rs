//! Dragging by touch: a long press picks a source up and the drag follows the finger to the
//! drop; a touch that moves at once isn't a drag and still scrolls; an at-once source picks up
//! as soon as the finger moves; a cancelled touch cancels; the provider can turn touch off; and
//! nothing runs while the finger holds still.

mod common;

use common::*;
use gpui::{
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Point, ScrollHandle,
    StatefulInteractiveElement as _, Styled as _, TestAppContext, TouchEvent, TouchId, TouchPhase,
    div, point, px,
};
use gpui_dnd::*;

#[derive(Clone)]
struct Card(u32);

/// Past GPUI's long-press delay (half a second).
const LONG: u64 = 600;

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

/// Slides the finger from `from` to `to` in steps, as a finger does.
fn slide(o: &mut Opened, from: Point<Pixels>, to: Point<Pixels>) {
    for i in 1..=8 {
        let t = i as f32 / 8.;
        touch(o, TouchPhase::Moved, from + (to - from) * t);
        o.wait(16);
    }
}

fn board(log: Log, activation: TouchActivation) -> impl Fn() -> gpui::AnyElement {
    move || {
        let (b, e, d) = (log.clone(), log.clone(), log.clone());
        div()
            .p(px(20.))
            .flex()
            .gap(px(40.))
            .child(
                dnd("a")
                    .debug_selector(|| "a".into())
                    .w(px(100.))
                    .h(px(40.))
                    .source(
                        DragSource::item(Card(1))
                            .touch_activation(activation)
                            .on_begin(move |m, _, _| {
                                b.borrow_mut()
                                    .push(format!("begin {:?}", m.backend().unwrap()))
                            })
                            .on_end(move |end, _, _| {
                                e.borrow_mut().push(format!(
                                    "end dropped={} cancelled={}",
                                    end.dropped, end.cancelled
                                ))
                            }),
                    ),
            )
            .child(
                dnd("t")
                    .debug_selector(|| "t".into())
                    .size(px(200.))
                    .target(
                        DropTarget::new()
                            .accept::<Card>()
                            .on_drop(move |item, _, _, _| {
                                d.borrow_mut()
                                    .push(format!("drop {}", item.get::<Card>().unwrap().0));
                                DropOutcome::Handled(None)
                            }),
                    ),
            )
            .into_any_element()
    }
}

#[gpui::test]
fn a_long_press_picks_up_and_the_drag_follows_the_finger(cx: &mut TestAppContext) {
    let log = log();
    let b = board(log.clone(), TouchActivation::LongPress);
    let mut o = open(cx, move |_, _| b());
    let a = o.bounds("a");
    let from = a.origin + point(px(30.), px(10.));
    let t = o.center("t");
    touch(&mut o, TouchPhase::Started, from);
    assert!(with(&log, "begin").is_empty(), "not before the long press");
    o.wait(LONG);
    assert_eq!(with(&log, "begin"), ["begin Touch"]);
    slide(&mut o, from, t);
    o.draw();
    assert_eq!(
        o.bounds("dnd-preview").origin,
        t - point(px(30.), px(10.)),
        "the preview follows the finger, not GPUI's mouse position"
    );
    assert!(
        o.vcx
            .update(|_, cx| DragMonitor::of(cx).is_over(&"t".into(), Over::Shallow))
    );
    touch(&mut o, TouchPhase::Ended, t);
    assert_eq!(with(&log, "drop"), ["drop 1"]);
    assert_eq!(with(&log, "end"), ["end dropped=true cancelled=false"]);
}

#[gpui::test]
fn a_touch_that_moves_at_once_isnt_a_drag_and_still_scrolls(cx: &mut TestAppContext) {
    let log = log();
    let scroll = ScrollHandle::new();
    let s = scroll.clone();
    let l = log.clone();
    let mut o = open(cx, move |_, _| {
        let b = l.clone();
        div()
            .id("list")
            .debug_selector(|| "list".into())
            .h(px(200.))
            .w(px(200.))
            .overflow_y_scroll()
            .track_scroll(&s)
            .children((0..20usize).map(|i| {
                let b = b.clone();
                dnd(("row", i))
                    .debug_selector(move || format!("row-{i}"))
                    .h(px(40.))
                    .source(
                        DragSource::item(Card(i as u32))
                            .on_begin(move |_, _, _| b.borrow_mut().push("begin".into())),
                    )
            }))
            .into_any_element()
    });
    let from = o.center("row-3");
    touch(&mut o, TouchPhase::Started, from);
    slide(&mut o, from, from - point(px(0.), px(120.)));
    touch(&mut o, TouchPhase::Ended, from - point(px(0.), px(120.)));
    o.wait(LONG);
    assert!(with(&log, "begin").is_empty(), "no drag");
    assert!(scroll.offset().y < px(0.), "the list scrolled");
}

#[gpui::test]
fn an_at_once_source_picks_up_as_soon_as_the_finger_moves(cx: &mut TestAppContext) {
    let log = log();
    let b = board(log.clone(), TouchActivation::Immediate);
    let mut o = open(cx, move |_, _| b());
    let (a, t) = (o.center("a"), o.center("t"));
    touch(&mut o, TouchPhase::Started, a);
    touch(&mut o, TouchPhase::Moved, a + point(px(3.), px(0.)));
    assert!(with(&log, "begin").is_empty(), "within the slop");
    slide(&mut o, a, t);
    assert_eq!(with(&log, "begin"), ["begin Touch"]);
    assert!(
        o.vcx
            .update(|_, cx| DragMonitor::of(cx).is_over(&"t".into(), Over::Shallow)),
        "over the target under the finger, though GPUI's mouse position stays where it started"
    );
    touch(&mut o, TouchPhase::Ended, t);
    assert_eq!(with(&log, "drop"), ["drop 1"]);

    // A touch that lifts without moving isn't a drag.
    log.borrow_mut().clear();
    touch(&mut o, TouchPhase::Started, a);
    touch(&mut o, TouchPhase::Ended, a);
    o.wait(LONG);
    assert!(entries(&log).is_empty());
}

#[gpui::test]
fn a_cancelled_touch_cancels_the_drag(cx: &mut TestAppContext) {
    let log = log();
    let b = board(log.clone(), TouchActivation::LongPress);
    let mut o = open(cx, move |_, _| b());
    let (a, t) = (o.center("a"), o.center("t"));
    touch(&mut o, TouchPhase::Started, a);
    o.wait(LONG);
    slide(&mut o, a, t);
    touch(&mut o, TouchPhase::Cancelled, t);
    o.wait(250);
    assert!(with(&log, "drop").is_empty());
    assert_eq!(with(&log, "end"), ["end dropped=false cancelled=true"]);
}

#[gpui::test]
fn the_provider_can_turn_touch_off(cx: &mut TestAppContext) {
    let log = log();
    let b = board(log.clone(), TouchActivation::LongPress);
    let mut o = open_with(cx, |p| p.touch(false), move |_, _| b());
    let a = o.center("a");
    touch(&mut o, TouchPhase::Started, a);
    o.wait(LONG);
    touch(&mut o, TouchPhase::Ended, a);
    assert!(with(&log, "begin").is_empty());
}

#[gpui::test]
fn nothing_runs_while_the_finger_holds_still(cx: &mut TestAppContext) {
    let log = log();
    let b = board(log.clone(), TouchActivation::LongPress);
    let mut o = open(cx, move |_, _| b());
    let (a, t) = (o.center("a"), o.center("t"));
    touch(&mut o, TouchPhase::Started, a);
    o.wait(LONG);
    slide(&mut o, a, t);
    o.frame();
    let held = o.renders.get();
    o.wait(5000);
    assert_eq!(o.renders.get(), held);
    assert_eq!(o.frames_wanted(), 0);
    touch(&mut o, TouchPhase::Ended, t);
    o.frame();
    assert_eq!(o.frames_wanted(), 0, "nor after the drop");
}
