//! Dragging with the mouse: a drag past the threshold begins, hovers and drops on the target
//! under the pointer, and the source hears how it ended; a press that doesn't move is a click;
//! nested targets are asked innermost first and can pass a drop out; a target takes only the
//! types it accepts and what it says it can; a target clipped out of view takes nothing;
//! Escape cancels; the monitor's offsets; collected state notifies only on a change; the
//! preview is drawn above Root's popovers and toasts; and nothing draws while idle.

mod common;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use common::*;
use gpui::{
    AnyView, AppContext as _, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    Point, Render, ScrollHandle, StatefulInteractiveElement as _, StyleRefinement, Styled as _,
    TestAppContext, Window, div, point, px,
};
use gpui_dnd::*;

#[derive(Clone, Debug, PartialEq)]
struct Card(u32);

struct Other;

fn fmt(p: Option<Point<gpui::Pixels>>) -> String {
    match p {
        Some(p) => format!("({}, {})", f32::from(p.x), f32::from(p.y)),
        None => "none".into(),
    }
}

/// A card that logs its drag's begin and end.
fn card(id: &'static str, n: u32, log: &Log) -> Dnd {
    let (begun, ended) = (log.clone(), log.clone());
    dnd(id)
        .debug_selector(move || id.into())
        .w(px(100.))
        .h(px(40.))
        .source(
            DragSource::item(Card(n))
                .on_begin(move |m, _, _| {
                    begun.borrow_mut().push(format!(
                        "begin {}",
                        m.item().unwrap().get::<Card>().unwrap().0
                    ))
                })
                .on_end(move |end, _, _| {
                    ended.borrow_mut().push(format!(
                        "end {} dropped={} cancelled={} target={} result={}",
                        end.item().0,
                        end.dropped,
                        end.cancelled,
                        end.target.map(|t| t.to_string()).unwrap_or("-".into()),
                        end.result
                            .and_then(|r| r.get::<&str>())
                            .copied()
                            .unwrap_or("-"),
                    ))
                }),
        )
}

/// A target that logs hovers and drops and does what `outcome` says.
fn target(id: &'static str, log: &Log, outcome: impl Fn() -> DropOutcome + 'static) -> Dnd {
    let (hovered, dropped) = (log.clone(), log.clone());
    dnd(id).debug_selector(move || id.into()).target(
        DropTarget::new()
            .accept::<Card>()
            .on_hover(move |_, m, _, _| {
                hovered.borrow_mut().push(format!(
                    "hover {id} shallow={} deep={}",
                    m.is_over(Over::Shallow),
                    m.is_over(Over::Deep)
                ))
            })
            .on_drop(move |item, m, _, _| {
                dropped.borrow_mut().push(format!(
                    "drop {id} card={} did_drop={} result={}",
                    item.get::<Card>().unwrap().0,
                    m.did_drop(),
                    m.drop_result()
                        .and_then(|r| r.get::<&str>())
                        .copied()
                        .unwrap_or("-")
                ));
                outcome()
            }),
    )
}

fn handled(result: &'static str) -> impl Fn() -> DropOutcome {
    move || DropOutcome::Handled(Some(DropResult::new(result)))
}

/// A card at the top left and a target beside it.
fn simple(cx: &mut TestAppContext) -> (Opened<'_>, Log) {
    let log = log();
    let l = log.clone();
    let o = open(cx, move |_, _| {
        div()
            .p(px(20.))
            .flex()
            .gap(px(40.))
            .child(card("a", 1, &l))
            .child(target("t", &l, handled("ok")).w(px(200.)).h(px(200.)))
            .into_any_element()
    });
    (o, log)
}

#[gpui::test]
fn a_drag_drops_on_the_target_and_tells_the_source(cx: &mut TestAppContext) {
    let (mut o, log) = simple(cx);
    let (a, t) = (o.center("a"), o.center("t"));
    o.drag(a, t);
    let e = entries(&log);
    assert_eq!(e.first().map(String::as_str), Some("begin 1"));
    assert!(
        e.contains(&"hover t shallow=true deep=true".into()),
        "{e:?}"
    );
    assert_eq!(
        with(&log, "drop"),
        ["drop t card=1 did_drop=false result=-"]
    );
    assert_eq!(
        e.last().map(String::as_str),
        Some("end 1 dropped=true cancelled=false target=t result=ok")
    );
    assert!(
        o.vcx.update(|_, cx| DragMonitor::of(cx).item().is_none()),
        "the drag is over"
    );
}

#[gpui::test]
fn a_drop_where_nothing_takes_it_cancels(cx: &mut TestAppContext) {
    let (mut o, log) = simple(cx);
    let a = o.center("a");
    o.drag(a, point(px(700.), px(500.)));
    assert!(with(&log, "drop").is_empty());
    // It ends once the preview is back.
    o.wait(250);
    assert_eq!(
        with(&log, "end"),
        ["end 1 dropped=false cancelled=true target=- result=-"]
    );
}

#[gpui::test]
fn a_press_that_doesnt_move_far_is_a_click_and_a_drag_isnt(cx: &mut TestAppContext) {
    let log = log();
    let l = log.clone();
    let mut o = open(cx, move |_, _| {
        let clicks = l.clone();
        div()
            .p(px(20.))
            .flex()
            .gap(px(40.))
            .child(
                card("a", 1, &l).on_click(move |_, _, _| clicks.borrow_mut().push("click".into())),
            )
            .child(target("t", &l, handled("ok")).w(px(200.)).h(px(200.)))
            .into_any_element()
    });
    let a = o.center("a");
    // Within the quarter rem: a click.
    o.down(a);
    o.drag_to(a + point(px(2.), px(1.)));
    o.up(a + point(px(2.), px(1.)));
    assert_eq!(entries(&log), ["click"]);
    // Out and back onto itself: a drag, cancelled, and no click.
    log.borrow_mut().clear();
    o.hold(a, point(px(500.), px(500.)));
    o.drag_to(a);
    o.up(a);
    assert!(with(&log, "click").is_empty(), "{:?}", entries(&log));
    assert_eq!(with(&log, "begin"), ["begin 1"]);
}

/// The outer target round the inner one, and what the inner one does with a drop.
fn nested(cx: &mut TestAppContext, inner: impl Fn() -> DropOutcome + 'static) -> (Opened<'_>, Log) {
    let log = log();
    let l = log.clone();
    let inner = Rc::new(inner);
    let o = open(cx, move |_, _| {
        let inner = inner.clone();
        div()
            .p(px(20.))
            .flex()
            .gap(px(40.))
            .child(card("a", 7, &l))
            .child(
                target("outer", &l, handled("outer"))
                    .w(px(300.))
                    .h(px(300.))
                    .p(px(50.))
                    .child(target("inner", &l, move || inner()).size(px(100.))),
            )
            .into_any_element()
    });
    (o, log)
}

#[gpui::test]
fn the_innermost_target_is_asked_first_and_shallow_is_only_it(cx: &mut TestAppContext) {
    let (mut o, log) = nested(cx, handled("inner"));
    let (a, inner) = (o.center("a"), o.center("inner"));
    o.drag(a, inner);
    let hovers = with(&log, "hover");
    assert_eq!(
        &hovers[hovers.len() - 2..],
        [
            "hover inner shallow=true deep=true",
            "hover outer shallow=false deep=true"
        ],
        "innermost first"
    );
    assert_eq!(
        with(&log, "drop"),
        ["drop inner card=7 did_drop=false result=-"]
    );
    assert_eq!(
        with(&log, "end"),
        ["end 7 dropped=true cancelled=false target=inner result=inner"]
    );
}

#[gpui::test]
fn a_target_that_passes_leaves_the_drop_to_the_one_outside(cx: &mut TestAppContext) {
    let (mut o, log) = nested(cx, || DropOutcome::Pass);
    let (a, inner) = (o.center("a"), o.center("inner"));
    o.drag(a, inner);
    assert_eq!(
        with(&log, "drop"),
        [
            "drop inner card=7 did_drop=false result=-",
            "drop outer card=7 did_drop=false result=-"
        ]
    );
    assert_eq!(
        with(&log, "end"),
        ["end 7 dropped=true cancelled=false target=outer result=outer"]
    );
}

#[gpui::test]
fn a_target_that_handles_and_continues_shows_the_outer_one_its_result(cx: &mut TestAppContext) {
    let (mut o, log) = nested(cx, || {
        DropOutcome::HandledContinue(Some(DropResult::new("inner")))
    });
    let (a, inner) = (o.center("a"), o.center("inner"));
    o.drag(a, inner);
    assert_eq!(
        with(&log, "drop"),
        [
            "drop inner card=7 did_drop=false result=-",
            "drop outer card=7 did_drop=true result=inner"
        ]
    );
    assert_eq!(
        with(&log, "end"),
        ["end 7 dropped=true cancelled=false target=outer result=outer"]
    );
}

#[gpui::test]
fn a_target_takes_only_the_types_it_accepts_and_what_it_can(cx: &mut TestAppContext) {
    let log = log();
    let l = log.clone();
    let mut o = open(cx, move |_, _| {
        let (h, d) = (l.clone(), l.clone());
        div()
            .p(px(20.))
            .flex()
            .gap(px(40.))
            .child(card("a", 1, &l))
            .child(
                dnd("others")
                    .debug_selector(|| "others".into())
                    .size(px(150.))
                    .target(
                        DropTarget::new()
                            .accept::<Other>()
                            .on_hover(move |_, _, _, _| h.borrow_mut().push("hover others".into()))
                            .on_drop(move |_, _, _, _| {
                                d.borrow_mut().push("drop others".into());
                                DropOutcome::Handled(None)
                            }),
                    ),
            )
            .child(
                dnd("full")
                    .debug_selector(|| "full".into())
                    .size(px(150.))
                    .target(
                        DropTarget::new()
                            .accept::<Card>()
                            .can_drop(|_, _, _| false)
                            .on_drop(|_, _, _, _| DropOutcome::Handled(None)),
                    ),
            )
            .into_any_element()
    });
    let (a, others) = (o.center("a"), o.center("others"));
    o.drag(a, others);
    o.wait(250);
    assert!(
        with(&log, "hover others").is_empty(),
        "it doesn't accept cards"
    );
    assert_eq!(
        with(&log, "end"),
        ["end 1 dropped=false cancelled=true target=- result=-"]
    );

    log.borrow_mut().clear();
    let full = o.center("full");
    o.hold(a, full);
    let can = o
        .vcx
        .update(|_, cx| DragMonitor::of(cx).can_drop_on(&"full".into()));
    let over = o
        .vcx
        .update(|_, cx| DragMonitor::of(cx).is_over(&"full".into(), Over::Shallow));
    assert!(over && !can, "over it, and it can't take it");
    o.up(full);
    assert!(with(&log, "drop").is_empty());
    o.wait(250);
    assert_eq!(
        with(&log, "end"),
        ["end 1 dropped=false cancelled=true target=- result=-"]
    );
}

#[gpui::test]
fn a_target_clipped_or_scrolled_out_of_view_takes_nothing(cx: &mut TestAppContext) {
    let log = log();
    let l = log.clone();
    let scroll = ScrollHandle::new();
    let s = scroll.clone();
    let mut o = open(cx, move |_, _| {
        div()
            .p(px(20.))
            .flex()
            .gap(px(40.))
            .child(card("a", 1, &l))
            .child(
                target("back", &l, handled("back"))
                    .w(px(200.))
                    .h(px(400.))
                    .pt(px(150.))
                    .child(
                        div()
                            .id("list")
                            .debug_selector(|| "list".into())
                            .h(px(100.))
                            .overflow_y_scroll()
                            .track_scroll(&s)
                            .child(target("one", &l, handled("one")).h(px(80.)))
                            .child(target("two", &l, handled("two")).h(px(80.)))
                            .child(target("three", &l, handled("three")).h(px(80.))),
                    ),
            )
            .into_any_element()
    });
    let a = o.center("a");
    let list = o.bounds("list");
    // "two" runs 80 to 160 down the list: its lower part is cut off below the list's bottom.
    let below = point(list.center().x, list.bottom() + px(30.));
    o.drag(a, below);
    assert_eq!(
        with(&log, "drop"),
        ["drop back card=1 did_drop=false result=-"]
    );

    // Scrolled down 80: "one" sits above the list, scrolled out of view.
    log.borrow_mut().clear();
    scroll.set_offset(point(px(0.), px(-80.)));
    o.draw();
    let above = point(list.center().x, list.top() - px(40.));
    o.drag(a, above);
    assert_eq!(
        with(&log, "drop"),
        ["drop back card=1 did_drop=false result=-"]
    );
    // And "two" is now at the list's top, where it takes a drop.
    log.borrow_mut().clear();
    o.drag(a, point(list.center().x, list.top() + px(20.)));
    assert_eq!(
        with(&log, "drop"),
        ["drop two card=1 did_drop=false result=-"]
    );
}

#[gpui::test]
fn escape_cancels_a_drag(cx: &mut TestAppContext) {
    let (mut o, log) = simple(cx);
    let (a, t) = (o.center("a"), o.center("t"));
    o.hold(a, t);
    o.keys("escape");
    o.wait(250);
    assert_eq!(
        with(&log, "end"),
        ["end 1 dropped=false cancelled=true target=- result=-"]
    );
    o.up(t);
    assert!(
        with(&log, "drop").is_empty(),
        "the release after it drops nothing"
    );
}

#[gpui::test]
fn the_monitor_gives_the_offsets(cx: &mut TestAppContext) {
    let log = log();
    let l = log.clone();
    let mut o = open(cx, move |_, _| {
        let seen = l.clone();
        div()
            .p(px(20.))
            .flex()
            .gap(px(40.))
            .child(card("a", 1, &l))
            .child(
                dnd("t")
                    .debug_selector(|| "t".into())
                    .size(px(200.))
                    .target(
                        DropTarget::new()
                            .accept::<Card>()
                            .on_drop(move |_, m, _, _| {
                                seen.borrow_mut().push(format!(
                            "initial={} client={} diff={} source={} initial_source={} in_target={}",
                            fmt(m.initial_client_offset()),
                            fmt(m.client_offset()),
                            fmt(m.difference_from_initial_offset()),
                            fmt(m.source_client_offset()),
                            fmt(m.initial_source_client_offset()),
                            fmt(m.offset_in_target()),
                        ));
                                DropOutcome::Handled(None)
                            }),
                    ),
            )
            .into_any_element()
    });
    let a = o.bounds("a");
    let t = o.bounds("t");
    assert_eq!((a.origin.x, a.origin.y), (px(20.), px(20.)));
    let from = a.origin + point(px(30.), px(10.));
    let to = t.origin + point(px(50.), px(60.));
    o.drag(from, to);
    let grab = point(px(30.), px(10.));
    assert_eq!(
        with(&log, "initial")[0],
        format!(
            "initial={} client={} diff={} source={} initial_source={} in_target={}",
            fmt(Some(from)),
            fmt(Some(to)),
            fmt(Some(to - from)),
            fmt(Some(to - grab)),
            fmt(Some(a.origin)),
            fmt(Some(point(px(50.), px(60.)))),
        )
    );
}

/// A view that shows whether a card is over the target, drawn from cache.
struct Probe {
    over: Collected<bool>,
    renders: Rc<Cell<usize>>,
}

impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        div().child(if self.over.get() { "over" } else { "not over" })
    }
}

#[gpui::test]
fn collected_state_notifies_its_view_only_when_it_changes(cx: &mut TestAppContext) {
    let log = log();
    let l = log.clone();
    let probe_renders = Rc::new(Cell::new(0));
    let pr = probe_renders.clone();
    let probe = Rc::new(RefCell::new(None));
    let p = probe.clone();
    let mut o = open(cx, move |_, cx| {
        let view = p
            .borrow_mut()
            .get_or_insert_with(|| {
                let renders = pr.clone();
                cx.new(|cx| Probe {
                    over: cx.collect_dnd(|m| m.is_over(&"t".into(), Over::Shallow)),
                    renders,
                })
            })
            .clone();
        div()
            .p(px(20.))
            .flex()
            .gap(px(40.))
            .child(card("a", 1, &l))
            .child(target("t", &l, handled("ok")).w(px(200.)).h(px(200.)))
            .child(
                div()
                    .w(px(100.))
                    .h(px(40.))
                    .child(AnyView::from(view).cached(StyleRefinement::default().size_full())),
            )
            .into_any_element()
    });
    let (a, t) = (o.center("a"), o.center("t"));
    let start = probe_renders.get();
    o.hold(a, point(px(30.), px(300.)));
    o.drag_to(point(px(40.), px(310.)));
    assert_eq!(probe_renders.get(), start, "nothing it collects changed");
    o.drag_to(t);
    assert_eq!(probe_renders.get(), start + 1, "now it's over");
    o.drag_to(t + point(px(5.), px(5.)));
    o.drag_to(t + point(px(10.), px(10.)));
    assert_eq!(probe_renders.get(), start + 1, "still over");
    o.up(t);
    assert_eq!(probe_renders.get(), start + 2, "and over no more");
}

#[gpui::test]
fn several_items_drag_together_and_the_preview_counts_them(cx: &mut TestAppContext) {
    let log = log();
    let l = log.clone();
    let mut o =
        open(cx, move |_, _| {
            let (ended, dropped) = (l.clone(), l.clone());
            div()
                .p(px(20.))
                .flex()
                .gap(px(40.))
                .child(
                    dnd("sel")
                        .debug_selector(|| "sel".into())
                        .w(px(100.))
                        .h(px(40.))
                        .source(
                            DragSource::many(|_| vec![Card(1), Card(2), Card(3)]).on_end(
                                move |end, _, _| {
                                    ended
                                        .borrow_mut()
                                        .push(format!("end {} items", end.items.len()))
                                },
                            ),
                        ),
                )
                .child(
                    dnd("t")
                        .debug_selector(|| "t".into())
                        .size(px(200.))
                        .target(DropTarget::new().accept::<Card>().on_drop(
                            move |item, _, _, _| {
                                let ids: Vec<_> =
                                    item.all::<Card>().unwrap().iter().map(|c| c.0).collect();
                                dropped
                                    .borrow_mut()
                                    .push(format!("drop {ids:?} count={}", item.count()));
                                DropOutcome::Handled(None)
                            },
                        )),
                )
                .into_any_element()
        });
    let (sel, t) = (o.center("sel"), o.center("t"));
    o.hold(sel, t);
    o.draw();
    assert!(
        o.shown("dnd-preview-count"),
        "the count shows on the preview"
    );
    o.up(t);
    assert_eq!(entries(&log), ["drop [1, 2, 3] count=3", "end 3 items"]);
}

#[gpui::test]
fn the_preview_follows_the_pointer_from_the_grab_point(cx: &mut TestAppContext) {
    let (mut o, _log) = simple(cx);
    let a = o.bounds("a");
    let from = a.origin + point(px(30.), px(10.));
    o.hold(from, point(px(400.), px(300.)));
    o.draw();
    let preview = o.bounds("dnd-preview");
    assert_eq!(preview.origin, point(px(370.), px(290.)));
    assert_eq!(preview.size, a.size);
    assert!(!o.shown("dnd-preview-count"), "one item, no count");
    o.up(point(px(400.), px(300.)));
    o.draw();
    assert!(o.shown("dnd-preview"), "going back, nothing took it");
    o.wait(250);
    o.draw();
    assert!(!o.shown("dnd-preview"), "gone once it's back");
}

#[gpui::test]
fn the_dragged_element_and_the_target_under_it_take_their_styles(cx: &mut TestAppContext) {
    let log = log();
    let l = log.clone();
    let mut o = open(cx, move |_, _| {
        div()
            .p(px(20.))
            .flex()
            .gap(px(40.))
            .child(card("a", 1, &l).dragging_style(|s| s.w(px(60.))))
            .child(
                target("t", &l, handled("ok"))
                    .w(px(200.))
                    .h(px(200.))
                    .can_drop_style(|s| s.h(px(220.)))
                    .over_style(|s| s.w(px(240.))),
            )
            .into_any_element()
    });
    let (a, t) = (o.center("a"), o.center("t"));
    o.hold(a, point(px(30.), px(400.)));
    o.draw();
    assert_eq!(o.bounds("a").size.width, px(60.), "dragging");
    assert_eq!(o.bounds("t").size.height, px(220.), "it can take the card");
    assert_eq!(o.bounds("t").size.width, px(200.), "not over it yet");
    o.drag_to(t);
    o.draw();
    assert_eq!(o.bounds("t").size.width, px(240.), "over it");
    o.up(t);
    o.draw();
    assert_eq!(o.bounds("a").size.width, px(100.));
    assert_eq!(o.bounds("t").size, gpui::size(px(200.), px(200.)));
}

#[gpui::test]
fn nothing_draws_while_idle_or_while_a_drag_holds_still(cx: &mut TestAppContext) {
    let (mut o, _log) = simple(cx);
    let (a, t) = (o.center("a"), o.center("t"));
    let idle = o.renders.get();
    o.vcx
        .executor()
        .advance_clock(std::time::Duration::from_secs(5));
    o.settle();
    assert_eq!(o.renders.get(), idle, "idle");
    assert_eq!(o.frames_wanted(), 0, "no frame asked for while idle");

    o.hold(a, t);
    let held = o.renders.get();
    o.vcx
        .executor()
        .advance_clock(std::time::Duration::from_secs(5));
    o.settle();
    assert_eq!(o.renders.get(), held, "a drag holding still");
    assert_eq!(o.frames_wanted(), 0, "nor while a drag holds still");

    o.up(t);
    let after = o.renders.get();
    o.vcx
        .executor()
        .advance_clock(std::time::Duration::from_secs(5));
    o.settle();
    assert_eq!(o.renders.get(), after, "idle after the drop");
    assert_eq!(o.frames_wanted(), 0, "nor after the drop");
}
