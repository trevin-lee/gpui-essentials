//! Shared sources: one keyed source built once for many elements, each giving only its key.
//! A drag from each element carries the item made from its own key, labelled by that element,
//! by mouse, touch and keyboard; and a press a child element stops never picks its source up.

mod common;

use common::*;
use gpui::{
    InteractiveElement as _, IntoElement, MouseButton, ParentElement as _, Styled as _,
    TestAppContext, TouchEvent, TouchId, TouchPhase, div, point, px,
};
use gpui_dnd::*;

#[derive(Clone, Debug, PartialEq)]
struct Card(usize);

/// Three cards from one shared source, a target taking cards, and a fourth card with a button
/// inside that stops its presses.
fn board(log: &Log) -> impl Fn() -> gpui::AnyElement + use<> {
    let log = log.clone();
    move || {
        let (b, d) = (log.clone(), log.clone());
        let cards = DragSource::keyed(|key: &usize, _| Card(*key))
            .on_begin(move |m, _, _| {
                let item = m.item().unwrap();
                b.borrow_mut().push(format!(
                    "begin {:?} {}",
                    item.get::<Card>().unwrap(),
                    item.label().map(|l| l.to_string()).unwrap_or_default()
                ))
            })
            .shared();
        div()
            .p(px(20.))
            .flex()
            .gap(px(20.))
            .children((0..3).map(|i| {
                dnd(("card", i))
                    .debug_selector(move || format!("card-{i}"))
                    .label(format!("Card {i}"))
                    .w(px(80.))
                    .h(px(40.))
                    .shared_source(&cards, i)
            }))
            .child(
                dnd(("card", 3usize))
                    .debug_selector(|| "card-3".into())
                    .w(px(80.))
                    .h(px(40.))
                    .shared_source(&cards, 3)
                    .child(
                        div()
                            .id("button")
                            .debug_selector(|| "button".into())
                            .size(px(20.))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()),
                    ),
            )
            .child(
                dnd("t")
                    .debug_selector(|| "t".into())
                    .size(px(150.))
                    .target(
                        DropTarget::new()
                            .accept::<Card>()
                            .on_drop(move |item, _, _, _| {
                                d.borrow_mut()
                                    .push(format!("drop {:?}", item.get::<Card>().unwrap()));
                                DropOutcome::Handled(None)
                            }),
                    ),
            )
            .into_any_element()
    }
}

#[gpui::test]
fn each_element_drags_the_item_made_from_its_key(cx: &mut TestAppContext) {
    let log = log();
    let b = board(&log);
    let mut o = open(cx, move |_, _| b());
    let t = o.center("t");
    for i in 0..3 {
        let from = o.center(&format!("card-{i}"));
        o.drag(from, t);
    }
    assert_eq!(
        entries(&log),
        [
            "begin Card(0) Card 0",
            "drop Card(0)",
            "begin Card(1) Card 1",
            "drop Card(1)",
            "begin Card(2) Card 2",
            "drop Card(2)",
        ]
    );
}

#[gpui::test]
fn a_long_press_picks_up_a_shared_source_too(cx: &mut TestAppContext) {
    let log = log();
    let b = board(&log);
    let mut o = open(cx, move |_, _| b());
    let (from, t) = (o.center("card-1"), o.center("t"));
    let touch = |o: &mut Opened, phase, at| {
        o.vcx.simulate_event(TouchEvent {
            id: TouchId(1),
            phase,
            position: at,
            predicted_position: None,
            force: None,
        });
        o.settle();
    };
    touch(&mut o, TouchPhase::Started, from);
    o.wait(600);
    for i in 1..=6 {
        touch(
            &mut o,
            TouchPhase::Moved,
            from + (t - from) * (i as f32 / 6.),
        );
        o.wait(16);
    }
    touch(&mut o, TouchPhase::Ended, t);
    assert_eq!(entries(&log), ["begin Card(1) Card 1", "drop Card(1)"]);
}

#[gpui::test]
fn a_press_a_child_stops_picks_nothing_up(cx: &mut TestAppContext) {
    let log = log();
    let b = board(&log);
    let mut o = open(cx, move |_, _| b());
    let t = o.center("t");
    let button = o.center("button");
    o.drag(button, t);
    assert!(entries(&log).is_empty(), "{:?}", entries(&log));
    // Pressed beside the button, the card is picked up.
    let card = o.bounds("card-3");
    o.drag(point(card.right() - px(10.), card.center().y), t);
    assert_eq!(entries(&log), ["begin Card(3) ", "drop Card(3)"]);
}

#[gpui::test]
fn the_keyboard_picks_up_a_shared_source(cx: &mut TestAppContext) {
    let log = log();
    let b = board(&log);
    let mut o = open_in_root(cx, move |_, _| b());
    o.keys("tab tab space");
    assert_eq!(entries(&log), ["begin Card(1) Card 1"]);
    o.keys("escape");
}
