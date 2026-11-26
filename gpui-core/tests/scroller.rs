//! Scroller: a scroll goes to the box under the pointer that can take it, along one way only,
//! as in a browser. A box that scrolls sideways inside one that scrolls down (a wide table in a
//! chat) takes only sideways scrolls; the rest goes to the box around it.

use std::time::Duration;

use gpui::{
    Context, InteractiveElement as _, IntoElement, Modifiers, ParentElement as _, Pixels, Point,
    Render, ScrollDelta, ScrollEvent, ScrollHandle, ScrollSource, Styled as _, TestAppContext,
    TouchPhase, VisualTestContext, Window, div, point, px,
};
use gpui_core::{ScrollAxes, ScrollExt as _};

/// A chat 300 by 200 scrolling down along `outer`, and in it, under a 50 high heading, a
/// table 300 by 100 scrolling along `inner` whose content is 1000 by 300.
struct Chat {
    outer: ScrollHandle,
    inner: ScrollHandle,
    outer_axes: ScrollAxes,
    inner_axes: ScrollAxes,
}

impl Render for Chat {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let table = div()
            .id("table")
            .flex_none()
            .w(px(300.))
            .h(px(100.))
            .child(div().w(px(1000.)).h(px(300.)))
            .scroll_axes(self.inner_axes)
            .track_scroll(&self.inner);
        div().size_full().child(
            div()
                .id("chat")
                .flex()
                .flex_col()
                .w(px(300.))
                .h(px(200.))
                .child(div().flex_none().w(px(600.)).h(px(50.)))
                .child(table)
                .child(div().flex_none().h(px(1000.)))
                .scroll_axes(self.outer_axes)
                .track_scroll(&self.outer),
        )
    }
}

struct Opened<'a> {
    outer: ScrollHandle,
    inner: ScrollHandle,
    vcx: &'a mut VisualTestContext,
}

/// Over the table.
fn table_center() -> Point<Pixels> {
    point(px(150.), px(100.))
}

fn open(cx: &mut TestAppContext, outer_axes: ScrollAxes, inner_axes: ScrollAxes) -> Opened<'_> {
    let (outer, inner) = (ScrollHandle::new(), ScrollHandle::new());
    let (o, i) = (outer.clone(), inner.clone());
    let (_, vcx) = cx.add_window_view(move |_, _| Chat {
        outer: o,
        inner: i,
        outer_axes,
        inner_axes,
    });
    vcx.run_until_parked();
    Opened { outer, inner, vcx }
}

impl Opened<'_> {
    fn scroll(&mut self, delta: ScrollDelta, phase: TouchPhase, modifiers: Modifiers) {
        self.vcx.simulate_event(ScrollEvent {
            position: table_center(),
            delta,
            modifiers,
            touch_phase: phase,
            source: ScrollSource::Pointer,
        });
        self.vcx.run_until_parked();
    }

    /// One step of a trackpad's gesture, a frame after the last.
    fn swipe(&mut self, x: f32, y: f32, phase: TouchPhase) {
        self.vcx.executor().advance_clock(Duration::from_millis(16));
        self.scroll(
            ScrollDelta::Pixels(point(px(x), px(y))),
            phase,
            Modifiers::none(),
        );
    }

    /// A mouse wheel's notches.
    fn wheel(&mut self, x: f32, y: f32, modifiers: Modifiers) {
        self.vcx
            .executor()
            .advance_clock(Duration::from_millis(300));
        self.scroll(
            ScrollDelta::Lines(point(x, y)),
            TouchPhase::Moved,
            modifiers,
        );
    }

    /// How far each has scrolled, right and down.
    fn chat(&self) -> Point<Pixels> {
        -self.outer.offset()
    }

    fn table(&self) -> Point<Pixels> {
        -self.inner.offset()
    }
}

const ZERO: Point<Pixels> = Point {
    x: Pixels::ZERO,
    y: Pixels::ZERO,
};

#[gpui::test]
fn a_wheel_down_over_a_sideways_table_scrolls_the_chat(cx: &mut TestAppContext) {
    let mut o = open(cx, ScrollAxes::Vertical, ScrollAxes::Horizontal);
    assert_eq!(o.inner.max_offset().x, px(700.));
    o.wheel(0., -3., Modifiers::none());
    assert_eq!(o.table(), ZERO, "the table doesn't slide sideways");
    assert!(o.chat().y > px(0.), "the chat scrolls down: {:?}", o.chat());
    assert_eq!(o.chat().x, px(0.));
}

#[gpui::test]
fn a_swipe_down_with_sideways_noise_never_moves_the_table(cx: &mut TestAppContext) {
    let mut o = open(cx, ScrollAxes::Vertical, ScrollAxes::Horizontal);
    o.swipe(0., 0., TouchPhase::Started);
    o.swipe(2., -12., TouchPhase::Moved);
    o.swipe(-3., -10., TouchPhase::Moved);
    // A frame that went more sideways than down, midway: still down.
    o.swipe(-6., -2., TouchPhase::Moved);
    o.swipe(-1., -9., TouchPhase::Moved);
    o.swipe(0., 0., TouchPhase::Ended);
    // The glide after the fingers lift (no phase on macOS: Moved).
    o.swipe(-4., -6., TouchPhase::Moved);
    o.swipe(-3., -1., TouchPhase::Moved);
    assert_eq!(o.table(), ZERO, "the table never slides sideways");
    assert_eq!(
        o.chat(),
        point(px(0.), px(40.)),
        "every step down scrolls the chat"
    );
}

#[gpui::test]
fn a_finger_pan_down_over_the_table_scrolls_the_chat(cx: &mut TestAppContext) {
    let o = open(cx, ScrollAxes::Vertical, ScrollAxes::Horizontal);
    for (x, y, phase) in [
        (1., -10., TouchPhase::Started),
        (-5., -2., TouchPhase::Moved),
        (0., -8., TouchPhase::Ended),
    ] {
        o.vcx.executor().advance_clock(Duration::from_millis(16));
        o.vcx.simulate_event(ScrollEvent {
            position: table_center(),
            delta: ScrollDelta::Pixels(point(px(x), px(y))),
            modifiers: Modifiers::none(),
            touch_phase: phase,
            source: ScrollSource::Touch,
        });
        o.vcx.run_until_parked();
    }
    assert_eq!(o.table(), ZERO);
    assert_eq!(o.chat(), point(px(0.), px(20.)));
}

#[gpui::test]
fn a_sideways_swipe_scrolls_the_table_and_its_end_moves_nothing_else(cx: &mut TestAppContext) {
    let mut o = open(cx, ScrollAxes::Vertical, ScrollAxes::Horizontal);
    o.swipe(-40., 3., TouchPhase::Started);
    assert_eq!(o.table(), point(px(40.), px(0.)));
    // Noise down, larger than sideways, midway: still sideways.
    o.swipe(-2., -6., TouchPhase::Moved);
    assert_eq!(o.table(), point(px(42.), px(0.)));
    assert_eq!(
        o.chat(),
        ZERO,
        "the chat keeps still while the table scrolls"
    );
    // On past the table's end, then its glide.
    for _ in 0..10 {
        o.swipe(-100., -4., TouchPhase::Moved);
    }
    o.swipe(0., 0., TouchPhase::Ended);
    for _ in 0..5 {
        o.swipe(-30., -2., TouchPhase::Moved);
    }
    assert_eq!(o.table(), point(px(700.), px(0.)), "at its end");
    assert_eq!(o.chat(), ZERO, "the rest moves the chat neither way");
}

#[gpui::test]
fn past_the_tables_end_the_rest_goes_on_the_same_way(cx: &mut TestAppContext) {
    // A chat that scrolls both ways: the rest of a sideways swipe scrolls it sideways only.
    let mut o = open(cx, ScrollAxes::Both, ScrollAxes::Horizontal);
    assert_eq!(o.outer.max_offset().x, px(300.));
    o.swipe(-400., 0., TouchPhase::Started);
    o.swipe(-400., 0., TouchPhase::Moved);
    assert_eq!(o.table(), point(px(700.), px(0.)));
    o.swipe(-50., -10., TouchPhase::Moved);
    assert_eq!(o.chat(), point(px(50.), px(0.)), "sideways, never down");
    assert_eq!(o.table(), point(px(700.), px(0.)));
}

#[gpui::test]
fn shift_and_a_wheel_scroll_the_table_sideways(cx: &mut TestAppContext) {
    let mut o = open(cx, ScrollAxes::Vertical, ScrollAxes::Horizontal);
    o.wheel(0., -3., Modifiers::shift());
    assert!(o.table().x > px(0.), "{:?}", o.table());
    assert_eq!(o.table().y, px(0.));
    assert_eq!(o.chat(), ZERO);
    // A mouse that turns sideways itself (a tilt wheel, or macOS turning Shift's).
    let at = o.table().x;
    o.wheel(-2., 0., Modifiers::shift());
    assert!(o.table().x > at, "{:?}", o.table());
    assert_eq!(o.chat(), ZERO);
}

#[gpui::test]
fn a_box_that_scrolls_both_ways_takes_both(cx: &mut TestAppContext) {
    let mut o = open(cx, ScrollAxes::Vertical, ScrollAxes::Both);
    assert_eq!(o.inner.max_offset(), point(px(700.), px(200.)));
    o.swipe(-30., 2., TouchPhase::Started);
    o.swipe(-30., -5., TouchPhase::Moved);
    assert_eq!(o.table(), point(px(60.), px(0.)));
    o.swipe(0., 0., TouchPhase::Ended);
    o.swipe(1., -25., TouchPhase::Started);
    o.swipe(-4., -25., TouchPhase::Moved);
    assert_eq!(o.table(), point(px(60.), px(50.)));
    o.wheel(0., -1., Modifiers::none());
    assert!(o.table().y > px(50.), "{:?}", o.table());
    o.wheel(0., -1., Modifiers::shift());
    assert!(o.table().x > px(60.), "{:?}", o.table());
    assert_eq!(o.chat(), ZERO, "it took every scroll itself");
    // At its bottom, down goes on to the chat.
    for _ in 0..10 {
        o.wheel(0., -10., Modifiers::none());
    }
    assert_eq!(o.table().y, px(200.));
    assert!(o.chat().y > px(0.), "{:?}", o.chat());
}

#[gpui::test]
fn a_tables_own_handle_reads_and_sets_its_scroll(cx: &mut TestAppContext) {
    let mut o = open(cx, ScrollAxes::Vertical, ScrollAxes::Horizontal);
    o.inner.set_offset(point(px(-120.), px(0.)));
    o.vcx.update(|window, _| window.refresh());
    o.vcx.run_until_parked();
    assert_eq!(o.table(), point(px(120.), px(0.)));
    o.swipe(30., 0., TouchPhase::Started);
    assert_eq!(o.table(), point(px(90.), px(0.)));
}
