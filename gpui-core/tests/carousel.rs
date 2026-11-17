//! Carousel: the buttons move it a slide at a time and are disabled at the ends unless it
//! loops; Left and Right move it with the focus inside; the dots go to a slide; a sideways
//! swipe snaps to a slide when let go; a move slides into place, then nothing redraws. (Its
//! role description is checked in the crate's own unit test.)

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AppContext as _, Bounds, Context, Entity, IntoElement, KeyDownEvent, KeyUpEvent, Keystroke,
    Modifiers, ParentElement as _, Pixels, Render, ScrollDelta, ScrollEvent, ScrollSource,
    Styled as _, TestAppContext, TouchPhase, VisualTestContext, Window, div, point, px,
};
use gpui_core::{Carousel, Root};

struct Panel {
    looping: bool,
    per_view: usize,
    index: Option<usize>,
    slid: Rc<RefCell<Vec<usize>>>,
    draws: Rc<Cell<usize>>,
}

impl Render for Panel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let slid = self.slid.clone();
        let mut c = Carousel::new("c")
            .w(px(300.))
            .looping(self.looping)
            .slides_per_view(self.per_view)
            .dots(true)
            .on_slide(move |i, _, _| slid.borrow_mut().push(*i))
            .children((1..=5).map(|i| div().h(px(100.)).child(format!("Slide {i}"))));
        if let Some(i) = self.index {
            c = c.index(i);
        }
        // Room for the buttons outside its sides.
        div().size_full().p(px(80.)).child(c)
    }
}

struct Opened<'a> {
    panel: Entity<Panel>,
    slid: Rc<RefCell<Vec<usize>>>,
    draws: Rc<Cell<usize>>,
    vcx: &'a mut VisualTestContext,
}

fn open_with(cx: &mut TestAppContext, f: impl FnOnce(&mut Panel) + 'static) -> Opened<'_> {
    cx.update(gpui_core::init);
    let slid: Rc<RefCell<Vec<usize>>> = Rc::default();
    let draws = Rc::new(Cell::new(0));
    let made = Rc::new(RefCell::new(None));
    let (s, d, m) = (slid.clone(), draws.clone(), made.clone());
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let mut p = Panel {
            looping: false,
            per_view: 1,
            index: None,
            slid: s,
            draws: d,
        };
        f(&mut p);
        let panel = cx.new(|_| p);
        *m.borrow_mut() = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    vcx.run_until_parked();
    let panel = made.borrow().clone().unwrap();
    Opened {
        panel,
        slid,
        draws,
        vcx,
    }
}

fn open(cx: &mut TestAppContext) -> Opened<'_> {
    open_with(cx, |_| {})
}

impl Opened<'_> {
    fn bounds(&mut self, selector: &'static str) -> Bounds<Pixels> {
        self.vcx.debug_bounds(selector).expect(selector)
    }

    fn click(&mut self, selector: &'static str) {
        let at = self.bounds(selector).center();
        self.vcx.simulate_click(at, Modifiers::none());
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

    /// A trackpad gesture over the slides: these steps, then let go.
    fn swipe(&mut self, steps: &[(f32, f32)]) {
        let at = self.bounds("carousel-viewport").center();
        let ev = |phase, (x, y): (f32, f32)| ScrollEvent {
            position: at,
            delta: ScrollDelta::Pixels(point(px(x), px(y))),
            modifiers: Modifiers::none(),
            touch_phase: phase,
            source: ScrollSource::Pointer,
        };
        self.vcx.simulate_event(ev(TouchPhase::Started, (0., 0.)));
        for s in steps {
            self.vcx.simulate_event(ev(TouchPhase::Moved, *s));
        }
        self.vcx.simulate_event(ev(TouchPhase::Ended, (0., 0.)));
        // The momentum after it: not a second swipe.
        for _ in 0..3 {
            self.vcx.simulate_event(ev(TouchPhase::Moved, steps[0]));
        }
        self.vcx.run_until_parked();
    }

    fn frame(&mut self) -> usize {
        self.vcx.executor().advance_clock(Duration::from_millis(16));
        let asked = self.vcx.update(|window, cx| window.simulate_next_frame(cx));
        self.vcx.run_until_parked();
        asked
    }

    fn settle(&mut self) {
        for _ in 0..30 {
            self.frame();
        }
    }

    /// The slide whose box starts at the view's left edge, less the spacing (16) it hangs by.
    fn in_view(&mut self) -> Option<usize> {
        let left = self.bounds("carousel-viewport").left() - px(16.);
        const SLIDES: [&str; 5] = [
            "carousel-slide-0",
            "carousel-slide-1",
            "carousel-slide-2",
            "carousel-slide-3",
            "carousel-slide-4",
        ];
        SLIDES
            .iter()
            .position(|s| (self.bounds(s).left() - left).abs() < px(0.5))
    }

    fn slid(&self) -> Vec<usize> {
        self.slid.borrow().clone()
    }
}

#[gpui::test]
fn the_buttons_move_it_and_stop_at_the_ends(cx: &mut TestAppContext) {
    let mut o = open(cx);
    assert_eq!(o.in_view(), Some(0));
    o.click("carousel-previous");
    assert!(o.slid().is_empty(), "previous at the first slide");
    o.click("carousel-next");
    o.click("carousel-next");
    o.settle();
    assert_eq!(o.in_view(), Some(2));
    o.click("carousel-previous");
    o.settle();
    assert_eq!(o.in_view(), Some(1));
    for _ in 0..5 {
        o.click("carousel-next");
    }
    o.settle();
    assert_eq!(o.in_view(), Some(4));
    assert_eq!(
        o.slid(),
        [1, 2, 1, 2, 3, 4],
        "next at the last slide did nothing"
    );
}

#[gpui::test]
fn looping_it_goes_round(cx: &mut TestAppContext) {
    let mut o = open_with(cx, |p| p.looping = true);
    o.click("carousel-previous");
    o.settle();
    assert_eq!(o.in_view(), Some(4));
    o.click("carousel-next");
    o.settle();
    assert_eq!(o.in_view(), Some(0));
    assert_eq!(o.slid(), [4, 0]);
}

#[gpui::test]
fn several_in_view_it_stops_when_the_last_shows(cx: &mut TestAppContext) {
    let mut o = open_with(cx, |p| p.per_view = 2);
    // Each slide is half of the view and its spacing: (300 + 16) / 2.
    let s = o.bounds("carousel-slide-0");
    assert!((s.size.width - px(158.)).abs() < px(0.5), "{s:?}");
    for _ in 0..6 {
        o.click("carousel-next");
    }
    o.settle();
    assert_eq!(o.in_view(), Some(3), "slides 4 and 5 in view");
    assert_eq!(o.slid(), [1, 2, 3]);
    // A dot for each place it stops.
    assert!(o.vcx.debug_bounds("carousel-dot-3").is_some());
    assert!(o.vcx.debug_bounds("carousel-dot-4").is_none());
}

#[gpui::test]
fn left_and_right_move_it_with_the_focus_inside(cx: &mut TestAppContext) {
    let mut o = open(cx);
    o.keys("right");
    assert!(o.slid().is_empty(), "no focus, no move");
    o.keys("tab right right left");
    assert_eq!(o.slid(), [1, 2, 1]);
    // Left at the first slide stays there (it doesn't loop).
    o.keys("left left");
    assert_eq!(o.slid(), [1, 2, 1, 0]);
    // From its own Next button too.
    o.click("carousel-next");
    o.keys("right");
    assert_eq!(o.slid(), [1, 2, 1, 0, 1, 2]);
    o.settle();
    assert_eq!(o.in_view(), Some(2));
}

#[gpui::test]
fn a_dot_goes_to_its_slide(cx: &mut TestAppContext) {
    let mut o = open(cx);
    o.click("carousel-dot-3");
    o.settle();
    assert_eq!(o.in_view(), Some(3));
    assert_eq!(o.slid(), [3]);
}

#[gpui::test]
fn a_swipe_snaps_to_a_slide(cx: &mut TestAppContext) {
    let mut o = open(cx);
    // Past halfway (a slide's step is 316): the next one.
    o.swipe(&[(-10., 0.), (-190., 0.)]);
    o.settle();
    assert_eq!(o.in_view(), Some(1));
    // A short drag springs back.
    o.swipe(&[(-10., 0.), (-30., 0.)]);
    o.settle();
    assert_eq!(o.in_view(), Some(1));
    // A flick of a fifth of a slide goes on by one, back as well as forward.
    o.swipe(&[(10., 0.), (55., 0.)]);
    o.settle();
    assert_eq!(o.in_view(), Some(0));
    // Two slides' worth, two slides.
    o.swipe(&[(-10., 0.), (-620., 0.)]);
    o.settle();
    assert_eq!(o.in_view(), Some(2));
    // Up and down is the page's, not a move.
    o.swipe(&[(0., -10.), (-5., -400.)]);
    o.settle();
    assert_eq!(o.in_view(), Some(2));
    assert_eq!(o.slid(), [1, 0, 2]);
}

#[gpui::test]
fn a_swipe_stops_at_the_ends(cx: &mut TestAppContext) {
    let mut o = open_with(cx, |p| p.looping = true);
    o.swipe(&[(10., 0.), (300., 0.)]);
    o.settle();
    assert_eq!(o.in_view(), Some(0), "a swipe doesn't go round");
    assert!(o.slid().is_empty());
}

#[gpui::test]
fn while_dragged_the_slides_follow(cx: &mut TestAppContext) {
    let mut o = open(cx);
    let at = o.bounds("carousel-viewport").center();
    let before = o.bounds("carousel-slide-1").left();
    for (phase, dx) in [(TouchPhase::Started, 0.), (TouchPhase::Moved, -100.)] {
        o.vcx.simulate_event(ScrollEvent {
            position: at,
            delta: ScrollDelta::Pixels(point(px(dx), px(0.))),
            modifiers: Modifiers::none(),
            touch_phase: phase,
            source: ScrollSource::Touch,
        });
    }
    o.vcx.run_until_parked();
    let now = o.bounds("carousel-slide-1").left();
    assert!(
        (before - now - px(100.)).abs() < px(0.5),
        "{before:?} {now:?}"
    );
}

#[gpui::test]
fn the_host_moves_it_by_its_index(cx: &mut TestAppContext) {
    let mut o = open_with(cx, |p| p.index = Some(2));
    assert_eq!(o.in_view(), Some(2), "it starts where it's told");
    o.panel.update(o.vcx, |p, cx| {
        p.index = Some(4);
        cx.notify();
    });
    o.vcx.run_until_parked();
    o.settle();
    assert_eq!(o.in_view(), Some(4));
    assert!(o.slid().is_empty(), "the host's moves aren't told back");
}

#[gpui::test]
fn a_move_slides_briefly_then_nothing_redraws(cx: &mut TestAppContext) {
    let mut o = open(cx);
    for _ in 0..10 {
        assert_eq!(o.frame(), 0, "asked for a frame before anything happened");
    }
    o.click("carousel-next");
    assert_ne!(o.in_view(), Some(1), "it jumped rather than slid");
    let mut asked = 0;
    for _ in 0..30 {
        asked += o.frame();
    }
    assert!(asked > 0, "it didn't slide");
    assert_eq!(o.in_view(), Some(1));
    let settled = o.draws.get();
    for _ in 0..60 {
        assert_eq!(o.frame(), 0, "asked for a frame after it settled");
    }
    assert_eq!(o.draws.get(), settled, "kept drawing after it settled");
}
