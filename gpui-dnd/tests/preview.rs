//! Previews: after Escape or a drop where nothing takes it, the preview goes back to the
//! source and the drag ends when it lands, asking for frames only on its way; a drop on a
//! target, reduced motion, or no preview ends it at once; a press lands it; and a custom drag
//! layer draws the preview where the monitor says, on the way back too.

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::*;
use gpui::{
    InteractiveElement as _, IntoElement, ParentElement as _, Point, Styled as _, TestAppContext,
    div, point, px,
};
use gpui_dnd::*;

#[derive(Clone)]
struct Card;

/// A card that logs its end, and a target that takes cards.
fn board(log: Log, preview: impl Fn() -> Preview<Card> + 'static) -> impl Fn() -> gpui::AnyElement {
    move || {
        let ended = log.clone();
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
                        DragSource::item(Card)
                            .preview(preview())
                            .on_end(move |end, _, _| {
                                ended.borrow_mut().push(format!(
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
                            .on_drop(|_, _, _, _| DropOutcome::Handled(None)),
                    ),
            )
            .into_any_element()
    }
}

fn returning(o: &mut Opened) -> bool {
    o.vcx.update(|_, cx| DragMonitor::of(cx).is_returning())
}

fn dragging(o: &mut Opened) -> bool {
    o.vcx.update(|_, cx| DragMonitor::of(cx).is_dragging())
}

#[gpui::test]
fn escape_sends_the_preview_back_and_the_drag_ends_when_it_lands(cx: &mut TestAppContext) {
    let log = log();
    let b = board(log.clone(), Preview::default);
    let mut o = open(cx, move |_, _| b());
    let a = o.bounds("a");
    let from = a.origin + point(px(30.), px(10.));
    let held = point(px(430.), px(310.));
    o.hold(from, held);
    o.keys("escape");
    assert!(returning(&mut o) && dragging(&mut o));
    assert!(with(&log, "end").is_empty(), "not until it lands");
    o.draw();
    assert_eq!(o.bounds("dnd-preview").origin, point(px(400.), px(300.)));
    assert!(
        !o.vcx
            .update(|_, cx| DragMonitor::of(cx).is_over(&"t".into(), Over::Deep)),
        "no target is under a preview going back"
    );

    // Half way in time it's most of the way back (ease-out), still short of the source.
    o.wait(100);
    o.frame();
    let mid = o.bounds("dnd-preview").origin;
    assert!(mid.x < px(400.) && mid.x > a.origin.x, "{mid:?}");
    assert!(
        mid.x < px(400.) - (px(400.) - a.origin.x) / 2.,
        "past half way: {mid:?}"
    );

    o.wait(150);
    o.frame();
    assert_eq!(with(&log, "end"), ["end dropped=false cancelled=true"]);
    assert!(!dragging(&mut o));
    assert!(!o.shown("dnd-preview"));
    // The last frame it asked for comes, and then nothing more is asked for.
    o.frame();
    assert_eq!(o.frames_wanted(), 0, "idle after it lands");
}

#[gpui::test]
fn a_drop_where_nothing_takes_it_goes_back_too(cx: &mut TestAppContext) {
    let log = log();
    let b = board(log.clone(), Preview::default);
    let mut o = open(cx, move |_, _| b());
    let a = o.center("a");
    o.drag(a, point(px(600.), px(500.)));
    assert!(returning(&mut o));
    o.wait(250);
    assert_eq!(with(&log, "end"), ["end dropped=false cancelled=true"]);
}

#[gpui::test]
fn a_drop_on_a_target_ends_at_once(cx: &mut TestAppContext) {
    let log = log();
    let b = board(log.clone(), Preview::default);
    let mut o = open(cx, move |_, _| b());
    let (a, t) = (o.center("a"), o.center("t"));
    o.drag(a, t);
    assert_eq!(with(&log, "end"), ["end dropped=true cancelled=false"]);
    assert!(!dragging(&mut o));
}

#[gpui::test]
fn with_reduced_motion_or_no_preview_a_cancel_ends_at_once(cx: &mut TestAppContext) {
    let log = log();
    let b = board(log.clone(), Preview::default);
    let mut o = open(cx, move |_, _| b());
    o.vcx.update(|_, cx| cx.set_reduce_motion(true));
    let a = o.center("a");
    o.hold(a, point(px(600.), px(500.)));
    o.keys("escape");
    assert_eq!(with(&log, "end"), ["end dropped=false cancelled=true"]);
    assert!(!dragging(&mut o));
    o.vcx.update(|_, cx| cx.set_reduce_motion(false));

    log.borrow_mut().clear();
    let b = board(log.clone(), || Preview::None);
    o.host
        .update(o.vcx, |h, _| h.build = Rc::new(move |_, _| b()));
    o.draw();
    o.hold(a, point(px(600.), px(500.)));
    o.keys("escape");
    assert_eq!(with(&log, "end"), ["end dropped=false cancelled=true"]);
}

#[gpui::test]
fn a_press_while_it_goes_back_lands_it(cx: &mut TestAppContext) {
    let log = log();
    let b = board(log.clone(), Preview::default);
    let mut o = open(cx, move |_, _| b());
    let a = o.center("a");
    o.hold(a, point(px(600.), px(500.)));
    o.keys("escape");
    assert!(returning(&mut o));
    o.down(point(px(700.), px(50.)));
    assert_eq!(with(&log, "end"), ["end dropped=false cancelled=true"]);
    o.up(point(px(700.), px(50.)));
    // A second Escape while it goes back lands it too.
    log.borrow_mut().clear();
    o.hold(a, point(px(600.), px(500.)));
    o.keys("escape");
    o.keys("escape");
    assert_eq!(with(&log, "end"), ["end dropped=false cancelled=true"]);
}

#[gpui::test]
fn moves_while_it_goes_back_are_ignored(cx: &mut TestAppContext) {
    let log = log();
    let b = board(log.clone(), Preview::default);
    let mut o = open(cx, move |_, _| b());
    let (a, t) = (o.center("a"), o.center("t"));
    o.hold(a, point(px(600.), px(500.)));
    o.keys("escape");
    o.drag_to(t);
    assert!(
        o.vcx
            .update(|_, cx| DragMonitor::of(cx).target_ids().is_empty()),
        "the pointer's moves don't hover targets"
    );
    o.up(t);
    assert!(returning(&mut o), "nor does a release drop it");
}

#[gpui::test]
fn a_custom_layer_draws_the_preview_where_the_monitor_says(cx: &mut TestAppContext) {
    let log = log();
    let seen: Rc<RefCell<Vec<Point<gpui::Pixels>>>> = Rc::default();
    let s = seen.clone();
    let b = board(log.clone(), || Preview::Layer);
    let mut o = open_with(
        cx,
        move |p| {
            let s = s.clone();
            p.layer(move |m, _, _| {
                s.borrow_mut().push(m.preview_origin());
                Some(
                    div()
                        .debug_selector(|| "my-preview".into())
                        .absolute()
                        .left(m.preview_origin().x)
                        .top(m.preview_origin().y)
                        .size(px(30.))
                        .into_any_element(),
                )
            })
        },
        move |_, _| b(),
    );
    let a = o.bounds("a");
    o.hold(
        a.origin + point(px(30.), px(10.)),
        point(px(430.), px(310.)),
    );
    o.draw();
    assert!(!o.shown("dnd-preview"), "no default preview");
    assert_eq!(o.bounds("my-preview").origin, point(px(400.), px(300.)));
    o.keys("escape");
    o.wait(100);
    o.frame();
    let mid = o.bounds("my-preview").origin;
    assert!(mid.x < px(400.) && mid.x > a.origin.x, "{mid:?}");
    o.wait(150);
    o.frame();
    assert!(!o.shown("my-preview"));
    assert!(!seen.borrow().is_empty());
}

#[gpui::test]
fn the_default_preview_shows_the_sources_label(cx: &mut TestAppContext) {
    let mut o = open(cx, |_, _| {
        div()
            .p(px(20.))
            .flex()
            .gap(px(40.))
            .child(
                dnd("labelled")
                    .debug_selector(|| "labelled".into())
                    .label("report.pdf")
                    .w(px(120.))
                    .h(px(36.))
                    .source(DragSource::item(Card)),
            )
            .child(
                dnd("bare")
                    .debug_selector(|| "bare".into())
                    .w(px(120.))
                    .h(px(36.))
                    .source(DragSource::item(Card)),
            )
            .into_any_element()
    });
    let a = o.center("labelled");
    o.hold(a, point(px(400.), px(300.)));
    o.draw();
    let label = o.bounds("dnd-preview-label");
    assert!(o.bounds("dnd-preview").contains(&label.center()));
    o.keys("escape");
    o.wait(250);
    let b = o.center("bare");
    o.hold(b, point(px(400.), px(300.)));
    o.draw();
    assert!(o.shown("dnd-preview") && !o.shown("dnd-preview-label"));
}

#[gpui::test]
fn without_a_layer_a_layer_preview_draws_nothing(cx: &mut TestAppContext) {
    let log = log();
    let b = board(log.clone(), || Preview::Layer);
    let mut o = open(cx, move |_, _| b());
    let (a, t) = (o.center("a"), o.center("t"));
    o.hold(a, t);
    o.draw();
    assert!(!o.shown("dnd-preview") && !o.shown("dnd-layer"));
    o.up(t);
    assert_eq!(with(&log, "end"), ["end dropped=true cancelled=false"]);
}
