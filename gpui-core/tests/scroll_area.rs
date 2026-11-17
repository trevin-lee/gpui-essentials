//! ScrollArea: its thumbs are sized by the content and placed by the scroll; a drag on a thumb
//! scrolls, a press on a track pages; the keys scroll it when it has the focus; the host scrolls
//! it through its handle; the bars show on hover and while scrolling (not for a finger), then
//! fade, and after that nothing redraws.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AppContext as _, Bounds, Context, Entity, InteractiveElement as _, IntoElement, KeyDownEvent,
    KeyUpEvent, Keystroke, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ParentElement as _, Pixels, Point, Render, ScrollDelta, ScrollEvent, ScrollSource, Styled as _,
    TestAppContext, TouchPhase, VisualTestContext, Window, div, point, px,
};
use gpui_core::{Root, ScrollArea, ScrollAreaHandle, ScrollAxes};

struct Panel {
    handle: ScrollAreaHandle,
    axes: ScrollAxes,
    draws: Rc<Cell<usize>>,
}

impl Render for Panel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        // 200 by 100, over ten rows of 20: twice as tall as it is (and, sideways, ten
        // columns of 40: twice as wide).
        let area = ScrollArea::new("area")
            .axes(self.axes)
            .track_scroll(&self.handle)
            .w(px(200.))
            .h(px(100.));
        let area = match self.axes {
            ScrollAxes::Vertical => {
                area.children((0..10).map(|i| div().h(px(20.)).child(format!("Row {i}"))))
            }
            ScrollAxes::Horizontal => area.children((0..10).map(|i| {
                div()
                    .flex_none()
                    .w(px(40.))
                    .h(px(20.))
                    .child(format!("{i}"))
            })),
            ScrollAxes::Both => area.children((0..10).map(|i| {
                div()
                    .flex_none()
                    .w(px(400.))
                    .h(px(20.))
                    .child(format!("{i}"))
            })),
        };
        div().size_full().p(px(20.)).child(
            div()
                .debug_selector(|| "area".into())
                .w(px(200.))
                .h(px(100.))
                .child(area),
        )
    }
}

struct Opened<'a> {
    panel: Entity<Panel>,
    handle: ScrollAreaHandle,
    draws: Rc<Cell<usize>>,
    vcx: &'a mut VisualTestContext,
}

fn open(cx: &mut TestAppContext, axes: ScrollAxes) -> Opened<'_> {
    cx.update(gpui_core::init);
    let handle = ScrollAreaHandle::new();
    let draws = Rc::new(Cell::new(0));
    let made = Rc::new(std::cell::RefCell::new(None));
    let (h, d, m) = (handle.clone(), draws.clone(), made.clone());
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let panel = cx.new(|_| Panel {
            handle: h,
            axes,
            draws: d,
        });
        *m.borrow_mut() = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    vcx.run_until_parked();
    let panel = made.borrow().clone().unwrap();
    Opened {
        panel,
        handle,
        draws,
        vcx,
    }
}

impl Opened<'_> {
    fn area(&mut self) -> Bounds<Pixels> {
        self.vcx.debug_bounds("area").expect("drawn")
    }

    fn get(&mut self, selector: &'static str) -> Option<Bounds<Pixels>> {
        self.vcx.debug_bounds(selector)
    }

    fn position(&self) -> Point<Pixels> {
        self.handle.position()
    }

    fn hover(&mut self) {
        let at = self.area().center();
        self.vcx.simulate_mouse_move(at, None, Modifiers::none());
        self.vcx.run_until_parked();
    }

    fn leave(&mut self) {
        self.vcx
            .simulate_mouse_move(point(px(5.), px(5.)), None, Modifiers::none());
        self.vcx.run_until_parked();
    }

    fn press(&mut self, at: Point<Pixels>) {
        self.vcx.simulate_event(MouseDownEvent {
            button: MouseButton::Left,
            position: at,
            modifiers: Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        self.vcx.simulate_event(MouseUpEvent {
            button: MouseButton::Left,
            position: at,
            modifiers: Modifiers::none(),
            click_count: 1,
        });
        self.vcx.run_until_parked();
    }

    fn keys(&mut self, keys: &'static str) {
        for k in keys.split(' ') {
            let keystroke = Keystroke::parse(k).unwrap();
            self.vcx.simulate_event(KeyDownEvent {
                keystroke: keystroke.clone(),
                is_held: false,
                prefer_character_input: false,
            });
            self.vcx.simulate_event(KeyUpEvent { keystroke });
        }
        self.vcx.run_until_parked();
    }

    fn wheel(&mut self, dy: f32, source: ScrollSource) {
        let at = self.area().center();
        self.vcx.simulate_event(ScrollEvent {
            position: at,
            delta: ScrollDelta::Pixels(point(px(0.), px(dy))),
            modifiers: Modifiers::none(),
            touch_phase: TouchPhase::Moved,
            source,
        });
        self.vcx.run_until_parked();
    }

    /// Scrolls through the handle, as a host would, and draws.
    fn scroll_to(&mut self, y: f32) {
        self.handle.scroll_to(point(px(0.), px(y)));
        self.redraw();
    }

    fn redraw(&mut self) {
        self.panel.update(self.vcx, |_, cx| cx.notify());
        self.vcx.run_until_parked();
    }

    /// Delivers one display frame and says how many things had asked for it.
    fn frame(&mut self) -> usize {
        self.vcx.executor().advance_clock(Duration::from_millis(16));
        let asked = self.vcx.update(|window, cx| window.simulate_next_frame(cx));
        self.vcx.run_until_parked();
        asked
    }
}

fn near(a: Pixels, b: f32) -> bool {
    (a - px(b)).abs() < px(0.01)
}

#[gpui::test]
fn the_thumb_is_sized_by_the_content_and_placed_by_the_scroll(cx: &mut TestAppContext) {
    let mut o = open(cx, ScrollAxes::Vertical);
    assert!(o.get("scroll-area-bar-y").is_none(), "no bar until hovered");
    o.hover();
    let area = o.area();
    let bar = o.get("scroll-area-bar-y").expect("a bar on hover");
    assert!(near(bar.size.width, 10.), "{bar:?}");
    assert_eq!(bar.right(), area.right());
    // The track is the bar less a pixel at each end; the view is half the content, so the
    // thumb is half the track.
    let t = o.get("scroll-area-thumb-y").unwrap();
    assert!(near(t.size.height, 49.), "{t:?}");
    assert!(near(t.top() - area.top(), 1.), "{t:?}");
    assert!(near(t.size.width, 7.), "{t:?}");
    assert!(o.get("scroll-area-bar-x").is_none(), "no sideways overflow");

    // Halfway down, it's halfway along what's left of the track.
    o.scroll_to(50.);
    let t = o.get("scroll-area-thumb-y").unwrap();
    assert!(near(t.top() - area.top(), 1. + 24.5), "{t:?}");
    o.scroll_to(1000.);
    assert_eq!(
        o.position(),
        point(px(0.), px(100.)),
        "held inside the content"
    );
    let t = o.get("scroll-area-thumb-y").unwrap();
    assert!(near(t.bottom() - area.top(), 99.), "{t:?}");
}

#[gpui::test]
fn sideways_the_thumb_runs_across(cx: &mut TestAppContext) {
    let mut o = open(cx, ScrollAxes::Horizontal);
    o.hover();
    let area = o.area();
    let t = o.get("scroll-area-thumb-x").expect("a sideways bar");
    assert!(near(t.size.width, 99.), "{t:?}");
    assert!(near(t.left() - area.left(), 1.), "{t:?}");
    assert!(o.get("scroll-area-bar-y").is_none());
}

#[gpui::test]
fn both_ways_the_bars_share_the_corner(cx: &mut TestAppContext) {
    let mut o = open(cx, ScrollAxes::Both);
    o.hover();
    // Both overflow: each track stops short of the corner by the other bar's 10.
    let y = o.get("scroll-area-bar-y").expect("a bar down");
    let x = o.get("scroll-area-bar-x").expect("a bar across");
    assert!(near(y.size.height, 90.), "{y:?}");
    assert!(near(x.size.width, 190.), "{x:?}");
}

#[gpui::test]
fn dragging_the_thumb_scrolls(cx: &mut TestAppContext) {
    let mut o = open(cx, ScrollAxes::Vertical);
    o.hover();
    let t = o.get("scroll-area-thumb-y").unwrap();
    let grab = t.center();
    o.vcx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: grab,
        modifiers: Modifiers::none(),
        click_count: 1,
        first_mouse: false,
    });
    // 24.5 of the track's 49 to spare: half way.
    for dy in [10., 24.5] {
        o.vcx.simulate_event(MouseMoveEvent {
            position: grab + point(px(0.), px(dy)),
            pressed_button: Some(MouseButton::Left),
            modifiers: Modifiers::none(),
        });
    }
    o.vcx.run_until_parked();
    assert!(near(o.position().y, 50.), "{:?}", o.position());
    // Past the end, and outside the area: it follows to the end.
    o.vcx.simulate_event(MouseMoveEvent {
        position: grab + point(px(300.), px(500.)),
        pressed_button: Some(MouseButton::Left),
        modifiers: Modifiers::none(),
    });
    o.vcx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: grab + point(px(300.), px(500.)),
        modifiers: Modifiers::none(),
        click_count: 1,
    });
    o.vcx.run_until_parked();
    assert!(near(o.position().y, 100.), "{:?}", o.position());
    // Let go, a move no longer scrolls.
    o.vcx.simulate_event(MouseMoveEvent {
        position: grab,
        pressed_button: None,
        modifiers: Modifiers::none(),
    });
    o.vcx.run_until_parked();
    assert!(near(o.position().y, 100.), "{:?}", o.position());
}

#[gpui::test]
fn a_press_on_the_track_pages(cx: &mut TestAppContext) {
    let mut o = open(cx, ScrollAxes::Vertical);
    o.hover();
    let bar = o.get("scroll-area-bar-y").unwrap();
    let below = point(bar.center().x, bar.bottom() - px(3.));
    let above = point(bar.center().x, bar.top() + px(3.));
    // A page is seven eighths of the view.
    o.press(below);
    assert!(near(o.position().y, 87.5), "{:?}", o.position());
    o.press(below);
    assert!(near(o.position().y, 100.), "held at the end");
    o.press(above);
    assert!(near(o.position().y, 12.5), "{:?}", o.position());
    // A press on the thumb itself doesn't page.
    let t = o.get("scroll-area-thumb-y").unwrap();
    o.press(t.center());
    assert!(near(o.position().y, 12.5), "{:?}", o.position());
}

#[gpui::test]
fn with_the_focus_the_keys_scroll_it(cx: &mut TestAppContext) {
    let mut o = open(cx, ScrollAxes::Vertical);
    o.keys("down");
    assert_eq!(o.position().y, px(0.), "no focus, no scroll");
    o.keys("tab down");
    assert!(near(o.position().y, 40.), "{:?}", o.position());
    o.keys("up");
    assert!(near(o.position().y, 0.), "{:?}", o.position());
    o.keys("pagedown");
    assert!(near(o.position().y, 87.5), "{:?}", o.position());
    o.keys("home");
    assert!(near(o.position().y, 0.), "{:?}", o.position());
    o.keys("end");
    assert!(near(o.position().y, 100.), "{:?}", o.position());
    o.keys("shift-space");
    assert!(near(o.position().y, 12.5), "{:?}", o.position());
    o.keys("space");
    assert!(near(o.position().y, 100.), "{:?}", o.position());
    o.keys("pageup");
    assert!(near(o.position().y, 12.5), "{:?}", o.position());
    // A key scroll shows the bars, as a wheel's does.
    assert!(o.get("scroll-area-bar-y").is_some());
}

#[gpui::test]
fn sideways_the_arrows_and_home_and_end_go_across(cx: &mut TestAppContext) {
    let mut o = open(cx, ScrollAxes::Horizontal);
    o.keys("tab right right");
    assert!(near(o.position().x, 80.), "{:?}", o.position());
    o.keys("down");
    assert_eq!(o.position().y, px(0.));
    o.keys("end");
    assert!(near(o.position().x, 200.), "{:?}", o.position());
    o.keys("home");
    assert!(near(o.position().x, 0.), "{:?}", o.position());
}

#[gpui::test]
fn the_host_scrolls_it_through_the_handle(cx: &mut TestAppContext) {
    let mut o = open(cx, ScrollAxes::Vertical);
    assert_eq!(o.handle.max_position(), point(px(0.), px(100.)));
    // The least that brings the last row into view.
    o.handle.scroll_to_child(9);
    o.redraw();
    assert!(near(o.position().y, 100.), "{:?}", o.position());
    o.handle.scroll_to_child_top(3);
    o.redraw();
    assert!(near(o.position().y, 60.), "{:?}", o.position());
    o.handle.scroll_by(point(px(0.), px(-15.)));
    o.redraw();
    assert!(near(o.position().y, 45.), "{:?}", o.position());
    o.handle.scroll_to_top();
    o.redraw();
    assert_eq!(o.position().y, px(0.));
    o.handle.scroll_to_bottom();
    o.redraw();
    assert!(near(o.position().y, 100.), "{:?}", o.position());
    // A scroll made while placing (a child brought into view) shows the bars too.
    o.handle.scroll_to_child_top(0);
    o.redraw();
    assert!(o.get("scroll-area-bar-y").is_some());
}

#[gpui::test]
fn the_bars_show_while_scrolling_then_fade_and_nothing_redraws(cx: &mut TestAppContext) {
    let mut o = open(cx, ScrollAxes::Vertical);
    // Opened and idle: nothing asks for frames.
    for _ in 0..10 {
        assert_eq!(o.frame(), 0, "asked for a frame before anything happened");
    }
    o.wheel(-30., ScrollSource::Pointer);
    assert!(near(o.position().y, 30.), "{:?}", o.position());
    assert!(
        o.get("scroll-area-bar-y").is_some(),
        "a scroll shows the bars"
    );
    // The wheel was over it; away from it, they stay a moment, then fade over a few frames.
    o.leave();
    let mut asked = 0;
    for _ in 0..60 {
        asked += o.frame();
    }
    assert!(asked > 0, "the bars didn't fade");
    assert!(o.get("scroll-area-bar-y").is_none(), "the bars stayed");
    let settled = o.draws.get();
    for _ in 0..60 {
        assert_eq!(o.frame(), 0, "asked for a frame after the bars faded");
    }
    assert_eq!(o.draws.get(), settled, "kept drawing after the bars faded");

    // Hovered, they stay; the pointer leaving starts the fade.
    o.hover();
    for _ in 0..60 {
        o.frame();
    }
    assert!(o.get("scroll-area-bar-y").is_some(), "faded while hovered");
    o.leave();
    for _ in 0..60 {
        o.frame();
    }
    assert!(
        o.get("scroll-area-bar-y").is_none(),
        "stayed after the pointer left"
    );
    let settled = o.draws.get();
    for _ in 0..30 {
        assert_eq!(o.frame(), 0);
    }
    assert_eq!(o.draws.get(), settled);
}

#[gpui::test]
fn a_finger_scrolls_it_without_bars(cx: &mut TestAppContext) {
    let mut o = open(cx, ScrollAxes::Vertical);
    o.wheel(-30., ScrollSource::Touch);
    assert!(near(o.position().y, 30.), "{:?}", o.position());
    assert!(o.get("scroll-area-bar-y").is_none(), "bars for a finger");
    for _ in 0..30 {
        assert_eq!(o.frame(), 0);
    }
}
