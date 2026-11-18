//! Auto-scroll: a drag held near a scroll container's edge scrolls it on each frame, faster
//! nearer the edge, hovering what scrolls under the pointer; it stops asking for frames when
//! the pointer leaves the edge, the content's end is reached, or the drag ends; the innermost
//! container that can scroll goes first; keyboard drags don't scroll.

mod common;

use common::*;
use gpui::{
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Point, ScrollHandle,
    StatefulInteractiveElement as _, Styled as _, TestAppContext, div, point, px,
};
use gpui_dnd::*;

#[derive(Clone)]
struct Card;

fn rows(prefix: &'static str, n: usize) -> impl Iterator<Item = Dnd> {
    (0..n).map(move |i| {
        dnd(gpui::ElementId::NamedInteger(prefix.into(), i as u64))
            .h(px(40.))
            .target(DropTarget::new().accept::<Card>())
    })
}

/// A card at the left and a 200 by 200 list of 20 rows, 40 tall, to its right.
fn list(cx: &mut TestAppContext) -> (Opened<'_>, ScrollHandle) {
    let scroll = ScrollHandle::new();
    let s = scroll.clone();
    let o = open(cx, move |_, _| {
        div()
            .p(px(20.))
            .flex()
            .gap(px(40.))
            .child(
                dnd("a")
                    .debug_selector(|| "a".into())
                    .w(px(100.))
                    .h(px(40.))
                    .source(DragSource::item(Card)),
            )
            .child(
                div()
                    .id("list")
                    .debug_selector(|| "list".into())
                    .w(px(200.))
                    .h(px(200.))
                    .overflow_y_scroll()
                    .track_scroll(&s)
                    .children(rows("row", 20))
                    .auto_scroll(&s, AutoScroll::vertical()),
            )
            .into_any_element()
    });
    (o, scroll)
}

/// Lets `n` frames of 16 ms go by.
fn frames(o: &mut Opened, n: usize) {
    for _ in 0..n {
        o.wait(16);
        o.frame();
    }
}

fn near_bottom(o: &mut Opened, by: Pixels) -> Point<Pixels> {
    let list = o.bounds("list");
    point(list.center().x, list.bottom() - by)
}

#[gpui::test]
fn a_drag_held_near_the_bottom_scrolls_on_each_frame(cx: &mut TestAppContext) {
    let (mut o, scroll) = list(cx);
    let a = o.center("a");
    let at = near_bottom(&mut o, px(10.));
    o.hold(a, at);
    let over_before = o.vcx.update(|_, cx| DragMonitor::of(cx).target_ids());
    frames(&mut o, 10);
    let y = scroll.offset().y;
    assert!(y < px(0.), "it scrolled down: {y:?}");
    frames(&mut o, 10);
    assert!(
        scroll.offset().y < y,
        "and goes on while the pointer holds still"
    );
    let over_after = o.vcx.update(|_, cx| DragMonitor::of(cx).target_ids());
    assert_ne!(
        over_before, over_after,
        "the rows that scrolled under it are hovered"
    );

    // Away from the edge it stops, and asks for no more frames.
    let list = o.bounds("list");
    o.drag_to(list.center());
    frames(&mut o, 2);
    let stopped = scroll.offset().y;
    frames(&mut o, 5);
    assert_eq!(scroll.offset().y, stopped);
    assert_eq!(o.frames_wanted(), 0);
}

#[gpui::test]
fn it_stops_at_the_end_and_scrolls_back_near_the_top(cx: &mut TestAppContext) {
    let (mut o, scroll) = list(cx);
    let a = o.center("a");
    let at = near_bottom(&mut o, px(2.));
    o.hold(a, at);
    frames(&mut o, 200);
    assert_eq!(scroll.offset().y, -scroll.max_offset().y, "at the end");
    o.frame();
    assert_eq!(o.frames_wanted(), 0, "nothing more to scroll: no frames");

    let list = o.bounds("list");
    o.drag_to(point(list.center().x, list.top() + px(5.)));
    frames(&mut o, 10);
    assert!(
        scroll.offset().y > -scroll.max_offset().y,
        "scrolling back up"
    );
}

#[gpui::test]
fn it_scrolls_faster_nearer_the_edge(cx: &mut TestAppContext) {
    let (mut o, scroll) = list(cx);
    let a = o.center("a");
    let far = near_bottom(&mut o, px(30.));
    o.hold(a, far);
    frames(&mut o, 5);
    let slow = -scroll.offset().y;
    o.up(far);
    o.wait(300);
    scroll.set_offset(point(px(0.), px(0.)));
    o.draw();
    let near = near_bottom(&mut o, px(3.));
    o.hold(a, near);
    frames(&mut o, 5);
    let fast = -scroll.offset().y;
    assert!(fast > slow * 1.5, "near: {fast:?}, farther: {slow:?}");
}

#[gpui::test]
fn the_drag_ending_stops_it(cx: &mut TestAppContext) {
    let (mut o, scroll) = list(cx);
    let a = o.center("a");
    let at = near_bottom(&mut o, px(10.));
    o.hold(a, at);
    frames(&mut o, 3);
    o.up(at);
    let y = scroll.offset().y;
    // No row takes it, so the preview goes back first.
    o.wait(250);
    frames(&mut o, 5);
    assert_eq!(scroll.offset().y, y);
    assert_eq!(o.frames_wanted(), 0);
}

#[gpui::test]
fn the_innermost_container_that_can_scroll_goes_first(cx: &mut TestAppContext) {
    let outer = ScrollHandle::new();
    let inner = ScrollHandle::new();
    let (so, si) = (outer.clone(), inner.clone());
    let mut o = open(cx, move |_, _| {
        div()
            .p(px(20.))
            .flex()
            .gap(px(40.))
            .child(
                dnd("a")
                    .debug_selector(|| "a".into())
                    .w(px(100.))
                    .h(px(40.))
                    .source(DragSource::item(Card)),
            )
            .child(
                div()
                    .id("outer")
                    .debug_selector(|| "outer".into())
                    .w(px(220.))
                    .h(px(300.))
                    .overflow_y_scroll()
                    .track_scroll(&so)
                    .child(
                        div()
                            .id("inner")
                            .debug_selector(|| "inner".into())
                            .w(px(200.))
                            .h(px(300.))
                            .overflow_y_scroll()
                            .track_scroll(&si)
                            .children(rows("in", 10))
                            .auto_scroll(&si, AutoScroll::vertical()),
                    )
                    .child(div().h(px(200.)))
                    .auto_scroll(&so, AutoScroll::vertical()),
            )
            .into_any_element()
    });
    let a = o.center("a");
    let outer_bounds = o.bounds("outer");
    let at = point(
        outer_bounds.left() + px(100.),
        outer_bounds.bottom() - px(5.),
    );
    o.hold(a, at);
    frames(&mut o, 3);
    assert!(inner.offset().y < px(0.), "the inner list scrolls");
    assert_eq!(outer.offset().y, px(0.), "the outer one waits");
    frames(&mut o, 300);
    assert_eq!(inner.offset().y, -inner.max_offset().y);
    assert!(outer.offset().y < px(0.), "then the outer one scrolls");
}

#[gpui::test]
fn keyboard_drags_dont_auto_scroll(cx: &mut TestAppContext) {
    let (mut o, scroll) = list(cx);
    o.vcx.update(|window, cx| window.focus_next(cx));
    o.keys("space");
    assert!(o.vcx.update(|_, cx| DragMonitor::of(cx).is_dragging()));
    // Wherever the keyboard's pointer is, nothing scrolls by itself.
    for _ in 0..6 {
        o.keys("down");
    }
    frames(&mut o, 10);
    let y = scroll.offset().y;
    frames(&mut o, 10);
    assert_eq!(scroll.offset().y, y);
    assert_eq!(o.frames_wanted(), 0);
}
