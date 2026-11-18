//! Providers side by side, with no provider round the window: views that each wrap themselves
//! in one (two tables on a page). One of them is active: a drag has one preview, each move is
//! followed once, each drop is heard once, and a drag goes from one view's source to the other's
//! target and back. The same with a provider nested inside one of them, with a view drawn from
//! cache that isn't drawn again, and with the active view going away mid-drag.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    AnyElement, AnyView, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    Modifiers, MouseButton, ParentElement as _, Pixels, Point, Render, StyleRefinement,
    Styled as _, TestAppContext, VisualTestContext, Window, div, point, px, size,
};
use gpui_dnd::*;

#[derive(Clone)]
struct Card;

#[derive(Default)]
struct Counts {
    previews: Cell<usize>,
    hovers: Cell<usize>,
    drops: Cell<usize>,
    ends: Cell<usize>,
}

/// One self-wrapped view: a source and a target in its own provider, maybe a second provider
/// inside round the target.
struct Side {
    name: &'static str,
    nested: bool,
    counts: Rc<Counts>,
}

impl Render for Side {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let name = self.name;
        let c = self.counts.clone();
        let (p, e, h, d) = (c.clone(), c.clone(), c.clone(), c);
        let source = dnd(format!("src-{name}"))
            .debug_selector(move || format!("src-{name}"))
            .w(px(80.))
            .h(px(40.))
            .source(
                DragSource::item(Card)
                    .preview(Preview::build(move |_, _, _| {
                        p.previews.set(p.previews.get() + 1);
                        div()
                            .debug_selector(|| "the-preview".into())
                            .w(px(80.))
                            .h(px(40.))
                            .into_any_element()
                    }))
                    .on_end(move |_, _, _| e.ends.set(e.ends.get() + 1)),
            );
        let target = dnd(format!("tgt-{name}"))
            .debug_selector(move || format!("tgt-{name}"))
            .size(px(150.))
            .target(
                DropTarget::new()
                    .accept::<Card>()
                    .on_hover(move |_, _, _, _| h.hovers.set(h.hovers.get() + 1))
                    .on_drop(move |_, _, _, _| {
                        d.drops.set(d.drops.get() + 1);
                        DropOutcome::Handled(None)
                    }),
            );
        let target: AnyElement = if self.nested {
            DndProvider::new(target).into_any_element()
        } else {
            target.into_any_element()
        };
        DndProvider::new(
            div()
                .w(px(300.))
                .h(px(400.))
                .p(px(20.))
                .flex()
                .flex_col()
                .gap(px(40.))
                .child(source)
                .child(target),
        )
    }
}

/// The window: the two views side by side and nothing round them; `a` drawn from cache when
/// `cached`, and left out when `hide_a`.
struct Page {
    a: Entity<Side>,
    b: Entity<Side>,
    cached: bool,
    hide_a: bool,
}

impl Render for Page {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let a: AnyElement = if self.cached {
            AnyView::from(self.a.clone())
                .cached(StyleRefinement::default().w(px(300.)).h(px(400.)))
                .into_any_element()
        } else {
            self.a.clone().into_any_element()
        };
        div()
            .flex()
            .gap(px(20.))
            .children((!self.hide_a).then_some(a))
            .child(self.b.clone())
    }
}

struct Open<'a> {
    page: Entity<Page>,
    a: Rc<Counts>,
    b: Rc<Counts>,
    vcx: &'a mut VisualTestContext,
}

fn open(cx: &mut TestAppContext, nested_a: bool, cached: bool) -> Open<'_> {
    cx.update(gpui_core::init);
    let (ca, cb) = (Rc::new(Counts::default()), Rc::new(Counts::default()));
    let (a2, b2) = (ca.clone(), cb.clone());
    let (page, vcx) = cx.add_window_view(move |_, cx| {
        let a = cx.new(|_| Side {
            name: "a",
            nested: nested_a,
            counts: a2,
        });
        let b = cx.new(|_| Side {
            name: "b",
            nested: false,
            counts: b2,
        });
        Page {
            a,
            b,
            cached,
            hide_a: false,
        }
    });
    vcx.simulate_resize(size(px(800.), px(600.)));
    vcx.run_until_parked();
    let mut o = Open {
        page,
        a: ca,
        b: cb,
        vcx,
    };
    o.draw();
    o
}

impl Open<'_> {
    fn draw(&mut self) {
        self.vcx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        self.vcx.run_until_parked();
    }

    fn center(&mut self, sel: &str) -> Point<Pixels> {
        let sel: &'static str = sel.to_owned().leak();
        self.vcx.debug_bounds(sel).unwrap().center()
    }

    fn previews(&self) -> usize {
        self.a.previews.get() + self.b.previews.get()
    }

    fn hovers(&self) -> usize {
        self.a.hovers.get() + self.b.hovers.get()
    }

    fn mv(&mut self, at: Point<Pixels>) {
        self.vcx
            .simulate_mouse_move(at, MouseButton::Left, Modifiers::none());
        self.vcx.run_until_parked();
    }

    fn start(&mut self, from: Point<Pixels>) {
        self.vcx
            .simulate_mouse_down(from, MouseButton::Left, Modifiers::none());
        self.vcx.run_until_parked();
        self.mv(from + point(px(10.), px(0.)));
    }

    fn up(&mut self, at: Point<Pixels>) {
        self.vcx
            .simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
        self.vcx.run_until_parked();
    }

    /// Moves to `at` and checks the move was followed once and the preview drawn once.
    fn step(&mut self, at: Point<Pixels>) {
        let hovers = self.hovers();
        self.mv(at);
        assert!(self.hovers() <= hovers + 1, "a move is followed once");
        let previews = self.previews();
        self.draw();
        assert_eq!(self.previews(), previews + 1, "one preview a frame");
        let p = self
            .vcx
            .debug_bounds("the-preview")
            .expect("the preview is drawn");
        assert!(
            p.contains(&at) || p.origin.x <= at.x,
            "it follows the pointer: {p:?} {at:?}"
        );
    }
}

fn across(o: &mut Open, from: &str, to: &str) {
    let (s, t) = (o.center(from), o.center(to));
    o.start(s);
    let hovers = o.hovers();
    for i in 1..=4 {
        o.step(s + (t - s) * (i as f32 / 4.));
    }
    assert!(o.hovers() > hovers, "the target under it heard the hover");
    o.up(t);
}

#[gpui::test]
fn siblings_give_one_preview_and_one_drop(cx: &mut TestAppContext) {
    let mut o = open(cx, false, false);
    across(&mut o, "src-a", "tgt-b");
    assert_eq!(
        (o.a.drops.get(), o.b.drops.get()),
        (0, 1),
        "dropped once, on b"
    );
    assert_eq!(o.a.ends.get(), 1, "the source heard it once");
    // And back from b to a.
    across(&mut o, "src-b", "tgt-a");
    assert_eq!((o.a.drops.get(), o.b.drops.get()), (1, 1));
    assert_eq!(o.b.ends.get(), 1);
    o.draw();
    assert!(
        o.vcx.debug_bounds("the-preview").is_none(),
        "no preview left"
    );
}

#[gpui::test]
fn a_provider_nested_in_a_sibling_stays_out_of_the_way(cx: &mut TestAppContext) {
    let mut o = open(cx, true, false);
    across(&mut o, "src-b", "tgt-a");
    assert_eq!((o.a.drops.get(), o.b.drops.get()), (1, 0));
    across(&mut o, "src-a", "tgt-b");
    assert_eq!((o.a.drops.get(), o.b.drops.get()), (1, 1));
    assert_eq!(o.a.ends.get() + o.b.ends.get(), 2);
}

#[gpui::test]
fn a_sibling_drawn_from_cache_and_not_again(cx: &mut TestAppContext) {
    // a is drawn from cache: it isn't drawn again unless it's told to. A drag in b, and one
    // across, still have one preview that follows the pointer, and each drop is heard once.
    let mut o = open(cx, false, true);
    across(&mut o, "src-b", "tgt-b");
    assert_eq!(o.b.drops.get(), 1);
    across(&mut o, "src-b", "tgt-a");
    assert_eq!(o.a.drops.get(), 1);
    across(&mut o, "src-a", "tgt-b");
    assert_eq!(o.b.drops.get(), 2);
}

#[gpui::test]
fn the_active_view_going_away_mid_drag_hands_over(cx: &mut TestAppContext) {
    let mut o = open(cx, false, false);
    let (s, t) = (o.center("src-b"), o.center("tgt-b"));
    o.start(s);
    o.step(s + point(px(20.), px(20.)));
    // a (the first drawn, the active provider) leaves the window.
    o.page.update(o.vcx, |p, cx| {
        p.hide_a = true;
        cx.notify();
    });
    o.draw();
    o.draw();
    let t = {
        let _ = t;
        o.center("tgt-b")
    };
    o.step(t);
    o.up(t);
    assert_eq!(
        o.b.drops.get(),
        1,
        "b's provider took over and the drop landed once"
    );
}
