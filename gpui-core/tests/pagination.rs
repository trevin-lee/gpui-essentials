//! Pagination: a press on a page, Previous or Next asks for that page, the ends disable
//! Previous and Next, Tab and Enter reach and press the links, ellipses stand for the pages left
//! out, a narrow box drops the words and then shows the compact form without spilling, touch
//! links are 44 points, and nothing redraws while idle, the width it measures included.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{
    AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _, IntoElement,
    KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, ParentElement as _, Render, Styled as _,
    TestAppContext, VisualTestContext, Window, div, px,
};
use gpui_core::{Pagination, Press, Root};

#[derive(Clone, Copy)]
struct Opts {
    width: f32,
    total: usize,
    touch: bool,
}

const WIDE: Opts = Opts {
    width: 700.,
    total: 10,
    touch: false,
};

struct Host {
    page: usize,
    opts: Opts,
    asked: Rc<RefCell<Vec<usize>>>,
    draws: Rc<Cell<usize>>,
    before: FocusHandle,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let asked = self.asked.clone();
        div()
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(
                Press::new("before")
                    .track_focus(&self.before)
                    .child("Before"),
            )
            .child(
                div()
                    .w(px(self.opts.width))
                    .debug_selector(|| "box".into())
                    .child(
                        Pagination::new("pages")
                            .total(self.opts.total)
                            .page(self.page)
                            .touch(self.opts.touch)
                            .on_page(cx.listener(move |this, page: &usize, _, cx| {
                                asked.borrow_mut().push(*page);
                                this.page = *page;
                                cx.notify();
                            })),
                    ),
            )
    }
}

fn open(cx: &mut TestAppContext, opts: Opts) -> (Entity<Host>, &mut VisualTestContext) {
    cx.update(gpui_core::init);
    let made = Rc::new(RefCell::new(None));
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let h = cx.new(|cx| Host {
            page: 1,
            opts,
            asked: Rc::default(),
            draws: Rc::default(),
            before: cx.focus_handle(),
        });
        *m.borrow_mut() = Some(h.clone());
        Root::new(h, window, cx)
    });
    vcx.update(|window, _| window.activate_window());
    vcx.run_until_parked();
    let h = made.borrow().clone().unwrap();
    (h, vcx)
}

fn keys(vcx: &mut VisualTestContext, keys: &'static str) {
    for k in keys.split(' ') {
        let keystroke = Keystroke::parse(k).unwrap();
        vcx.simulate_event(KeyDownEvent {
            keystroke: keystroke.clone(),
            is_held: false,
            prefer_character_input: false,
        });
        vcx.simulate_event(KeyUpEvent { keystroke });
        vcx.run_until_parked();
    }
}

fn click(vcx: &mut VisualTestContext, what: &'static str) {
    let at = vcx.debug_bounds(what).expect(what).center();
    vcx.simulate_click(at, Modifiers::none());
    vcx.run_until_parked();
}

fn asked(h: &Entity<Host>, vcx: &mut VisualTestContext) -> Vec<usize> {
    h.read_with(vcx, |h, _| h.asked.borrow().clone())
}

fn shown(vcx: &mut VisualTestContext, total: usize) -> Vec<usize> {
    (1..=total)
        .filter(|n| {
            vcx.debug_bounds(Box::leak(format!("page-{n}").into_boxed_str()))
                .is_some()
        })
        .collect()
}

#[gpui::test]
fn presses_ask_for_pages_and_the_ends_stop_previous_and_next(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, WIDE);
    assert_eq!(shown(vcx, 10), [1, 2, 3, 4, 5, 10]);
    assert!(vcx.debug_bounds("page-gap-0").is_some());
    click(vcx, "page-prev");
    assert!(asked(&h, vcx).is_empty(), "no page before the first");
    click(vcx, "page-next");
    click(vcx, "page-5");
    assert_eq!(asked(&h, vcx), [2, 5]);
    assert_eq!(shown(vcx, 10), [1, 4, 5, 6, 10], "round the current page");
    click(vcx, "page-5");
    assert_eq!(
        asked(&h, vcx),
        [2, 5],
        "the current page isn't asked for again"
    );
    click(vcx, "page-10");
    click(vcx, "page-next");
    click(vcx, "page-prev");
    assert_eq!(asked(&h, vcx), [2, 5, 10, 9]);
}

#[gpui::test]
fn tab_and_enter_reach_and_press_the_links(cx: &mut TestAppContext) {
    let (h, vcx) = open(cx, WIDE);
    let before = h.read_with(vcx, |h, _| h.before.clone());
    vcx.update(|window, cx| window.focus(&before, cx));
    // Previous is disabled on the first page, so the first stop is page 1, then 2.
    keys(vcx, "tab tab enter");
    assert_eq!(asked(&h, vcx), [2]);
    keys(vcx, "tab space");
    assert_eq!(asked(&h, vcx), [2, 3]);
}

#[gpui::test]
fn a_narrow_box_drops_the_words_then_goes_compact(cx: &mut TestAppContext) {
    let full = {
        let (_, vcx) = open(cx, WIDE);
        vcx.debug_bounds("page-prev").unwrap().size.width
    };
    for width in [400., 140.] {
        let (_, vcx) = open(cx, Opts { width, ..WIDE });
        let b = vcx.debug_bounds("box").unwrap();
        let prev = vcx.debug_bounds("page-prev").unwrap();
        let next = vcx.debug_bounds("page-next").unwrap();
        assert!(prev.size.width < full, "the words dropped at {width}");
        assert!(
            prev.left() >= b.left() - px(0.5) && next.right() <= b.right() + px(0.5),
            "{prev:?} {next:?} in {b:?}"
        );
        let compact = vcx.debug_bounds("page-compact");
        if width < 200. {
            assert!(compact.is_some(), "compact at {width}");
            assert!(shown(vcx, 10).is_empty());
        } else {
            assert!(compact.is_none(), "pages still shown at {width}");
            let last = vcx.debug_bounds("page-10").unwrap();
            assert!(last.right() <= b.right(), "{last:?} in {b:?}");
        }
    }
}

#[gpui::test]
fn touch_links_are_44_points(cx: &mut TestAppContext) {
    let (_, vcx) = open(
        cx,
        Opts {
            touch: true,
            ..WIDE
        },
    );
    for what in ["page-prev", "page-1", "page-2", "page-next"] {
        let b = vcx.debug_bounds(what).unwrap();
        assert!(
            b.size.height >= px(44.) && b.size.width >= px(44.),
            "{what} {b:?}"
        );
    }
}

#[gpui::test]
fn nothing_redraws_while_idle(cx: &mut TestAppContext) {
    let (h, vcx) = open(
        cx,
        Opts {
            width: 200.,
            ..WIDE
        },
    );
    let before = h.read_with(vcx, |h, _| h.draws.get());
    for _ in 0..30 {
        vcx.update(|window, cx| window.simulate_next_frame(cx));
        vcx.executor()
            .advance_clock(std::time::Duration::from_millis(16));
        vcx.run_until_parked();
    }
    assert_eq!(h.read_with(vcx, |h, _| h.draws.get()), before);
}
