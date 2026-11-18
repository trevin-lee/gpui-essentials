//! Dragging with the keyboard: Tab reaches a source, Space picks it up, the arrows move to the
//! nearest target in their direction that can take it, Space drops and Escape cancels; each
//! step is said in the live region; the drag holds the keyboard; a source can opt out; and
//! nothing runs while it holds still.

mod common;

use common::*;
use gpui::{
    InteractiveElement as _, IntoElement, ParentElement as _, Styled as _, TestAppContext, div, px,
};
use gpui_dnd::*;

#[derive(Clone, Debug)]
struct Card(u32);

struct Other;

/// A card, then three targets in a row to its right: First and Third take cards, the middle one
/// only takes `Other`s, and a fourth below First takes cards but says it can't now.
fn board(log: &Log, keyboard: bool) -> impl Fn() -> gpui::AnyElement + use<> {
    let log = log.clone();
    move || {
        let target = |id: &'static str, label: &'static str| {
            let (h, d) = (log.clone(), log.clone());
            dnd(id)
                .debug_selector(move || id.into())
                .label(label)
                .size(px(100.))
                .target(
                    DropTarget::new()
                        .accept::<Card>()
                        .on_hover(move |_, m, _, _| {
                            h.borrow_mut().push(format!(
                                "hover {id} at {:?}",
                                m.offset_in_target()
                                    .map(|p| (f32::from(p.x), f32::from(p.y)))
                            ))
                        })
                        .on_drop(move |item, _, _, _| {
                            d.borrow_mut()
                                .push(format!("drop {} on {id}", item.get::<Card>().unwrap().0));
                            DropOutcome::Handled(None)
                        }),
                )
        };
        let (b, e) = (log.clone(), log.clone());
        div()
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(40.))
            .child(
                div()
                    .flex()
                    .gap(px(40.))
                    .child(
                        dnd("a")
                            .debug_selector(|| "a".into())
                            .label("Write the brief")
                            .w(px(100.))
                            .h(px(40.))
                            .source(
                                DragSource::item(Card(1))
                                    .keyboard(keyboard)
                                    .on_begin(move |m, _, _| {
                                        b.borrow_mut()
                                            .push(format!("begin {:?}", m.backend().unwrap()))
                                    })
                                    .on_end(move |end, _, _| {
                                        e.borrow_mut().push(format!(
                                            "end dropped={} target={}",
                                            end.dropped,
                                            end.target.map(|t| t.to_string()).unwrap_or("-".into())
                                        ))
                                    }),
                            ),
                    )
                    .child(target("first", "First"))
                    .child(
                        dnd("middle")
                            .debug_selector(|| "middle".into())
                            .size(px(100.))
                            .target(DropTarget::new().accept::<Other>()),
                    )
                    .child(target("third", "Third"))
                    // A second Tab stop, so a Tab that got through would move the focus.
                    .child(
                        dnd("b")
                            .w(px(40.))
                            .h(px(40.))
                            .source(DragSource::item(Card(2)).keyboard(keyboard)),
                    ),
            )
            .child(
                dnd("full")
                    .debug_selector(|| "full".into())
                    .size(px(100.))
                    .ml(px(140.))
                    .target(DropTarget::new().accept::<Card>().can_drop(|_, _, _| false)),
            )
            .into_any_element()
    }
}

fn over(o: &mut Opened) -> Vec<String> {
    o.vcx
        .update(|_, cx| DragMonitor::of(cx).target_ids())
        .iter()
        .map(|t| t.to_string())
        .collect()
}

#[gpui::test]
fn tab_space_arrows_and_space_drop_with_each_step_said(cx: &mut TestAppContext) {
    let log = log();
    let b = board(&log, true);
    let mut o = open_in_root(cx, move |_, _| b());
    o.keys("tab");
    o.keys("space");
    assert_eq!(with(&log, "begin"), ["begin Keyboard"]);
    assert_eq!(
        o.said().as_deref(),
        Some("Picked up Write the brief. Arrow keys move it, Space drops it, Escape cancels.")
    );
    o.keys("right");
    assert_eq!(over(&mut o), ["first"]);
    assert_eq!(o.said().as_deref(), Some("Over First."));
    assert_eq!(
        with(&log, "hover").last().map(String::as_str),
        Some("hover first at Some((50.0, 50.0))"),
        "the keyboard's pointer is at the target's centre"
    );
    // The middle one doesn't take cards: the next step right skips it.
    o.keys("right");
    assert_eq!(over(&mut o), ["third"]);
    assert_eq!(o.said().as_deref(), Some("Over Third."));
    o.keys("left");
    assert_eq!(over(&mut o), ["first"]);
    o.draw();
    assert!(o.shown("dnd-preview"), "the preview sits on the target");
    o.keys("space");
    assert_eq!(with(&log, "drop"), ["drop 1 on first"]);
    assert_eq!(with(&log, "end"), ["end dropped=true target=first"]);
    assert_eq!(
        o.said().as_deref(),
        Some("Dropped Write the brief on First.")
    );
}

#[gpui::test]
fn enter_works_as_space(cx: &mut TestAppContext) {
    let log = log();
    let b = board(&log, true);
    let mut o = open_in_root(cx, move |_, _| b());
    o.keys("tab enter right enter");
    assert_eq!(with(&log, "drop"), ["drop 1 on first"]);
}

#[gpui::test]
fn the_arrows_skip_targets_that_cant_take_it(cx: &mut TestAppContext) {
    let log = log();
    let b = board(&log, true);
    let mut o = open_in_root(cx, move |_, _| b());
    o.keys("tab space right");
    assert_eq!(over(&mut o), ["first"]);
    // Below First is "full", which can't take it now: nothing below, so it stays.
    o.keys("down");
    assert_eq!(over(&mut o), ["first"]);
    assert!(with(&log, "hover full").is_empty());
}

#[gpui::test]
fn escape_cancels_and_the_focus_stays_on_the_source(cx: &mut TestAppContext) {
    let log = log();
    let b = board(&log, true);
    let mut o = open_in_root(cx, move |_, _| b());
    o.keys("tab space right escape");
    o.wait(250);
    assert_eq!(with(&log, "end"), ["end dropped=false target=-"]);
    assert_eq!(
        o.said().as_deref(),
        Some("Cancelled. Write the brief is back where it was.")
    );
    // Still focused: Space picks it up again.
    o.keys("space");
    assert_eq!(with(&log, "begin"), ["begin Keyboard", "begin Keyboard"]);
}

#[gpui::test]
fn a_keyboard_drag_holds_the_keyboard(cx: &mut TestAppContext) {
    let log = log();
    let b = board(&log, true);
    let mut o = open_in_root(cx, move |_, _| b());
    o.keys("tab space");
    // Tab doesn't move the focus away mid-drag, and the drop still lands.
    o.keys("tab right space");
    assert_eq!(with(&log, "drop"), ["drop 1 on first"]);
    o.keys("space");
    assert_eq!(
        with(&log, "begin").len(),
        2,
        "the focus is still on the source"
    );
}

#[gpui::test]
fn a_source_can_leave_the_keyboard_out(cx: &mut TestAppContext) {
    let log = log();
    let b = board(&log, false);
    let mut o = open_in_root(cx, move |_, _| b());
    o.keys("tab space");
    assert!(with(&log, "begin").is_empty());
    assert!(o.said().is_none());
}

#[gpui::test]
fn the_arrows_do_nothing_in_a_mouse_drag(cx: &mut TestAppContext) {
    let log = log();
    let b = board(&log, true);
    let mut o = open_in_root(cx, move |_, _| b());
    let a = o.center("a");
    o.hold(a, gpui::point(px(400.), px(500.)));
    o.keys("right");
    assert!(over(&mut o).is_empty());
    assert!(o.said().is_none(), "mouse drags aren't announced");
}

#[gpui::test]
fn a_press_ends_a_keyboard_drag(cx: &mut TestAppContext) {
    let log = log();
    let b = board(&log, true);
    let mut o = open_in_root(cx, move |_, _| b());
    o.keys("tab space right");
    o.down(gpui::point(px(700.), px(550.)));
    assert_eq!(with(&log, "end"), ["end dropped=false target=-"]);
    o.up(gpui::point(px(700.), px(550.)));
}

#[gpui::test]
fn nothing_runs_while_a_keyboard_drag_holds_still(cx: &mut TestAppContext) {
    let log = log();
    let b = board(&log, true);
    let mut o = open_in_root(cx, move |_, _| b());
    o.keys("tab space right");
    o.frame();
    let held = o.renders.get();
    o.wait(5000);
    assert_eq!(o.renders.get(), held);
    assert_eq!(o.frames_wanted(), 0);
    o.keys("space");
    o.frame();
    assert_eq!(o.frames_wanted(), 0, "nor after the drop");
}

#[gpui::test]
fn the_announcements_can_be_replaced(cx: &mut TestAppContext) {
    let log = log();
    let b = board(&log, true);
    let mut o = open_with(
        cx,
        |p| {
            p.announcements(Announcements {
                picked_up: std::rc::Rc::new(|item| format!("Lifted {}", item_name(item)).into()),
                ..Announcements::default()
            })
        },
        move |_, _| b(),
    );
    // No Root here: focus the source by pressing it is off, so tab through GPUI's own focus.
    o.vcx.update(|window, cx| window.focus_next(cx));
    o.keys("space");
    assert_eq!(o.said().as_deref(), Some("Lifted Write the brief"));
}
