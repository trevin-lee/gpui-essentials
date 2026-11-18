//! What the tests share: a window holding a provider and the host's elements, a log of what the
//! callbacks heard, and the mouse and keyboard to drive them.

#![allow(dead_code)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{
    AnyElement, Bounds, Context, Entity, IntoElement, Modifiers, MouseButton, ParentElement as _,
    Pixels, Point, Render, Styled as _, TestAppContext, VisualTestContext, Window, div, point, px,
    size,
};
use gpui_dnd::DndProvider;

pub type Log = Rc<RefCell<Vec<String>>>;

pub type Build = Rc<dyn Fn(&mut Window, &mut Context<Host>) -> AnyElement>;

pub struct Host {
    pub build: Build,
    pub renders: Rc<Cell<usize>>,
}

impl Render for Host {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        DndProvider::new(div().size_full().child((self.build)(window, cx)))
    }
}

pub struct Opened<'a> {
    pub host: Entity<Host>,
    pub renders: Rc<Cell<usize>>,
    pub vcx: &'a mut VisualTestContext,
}

pub fn open(
    cx: &mut TestAppContext,
    build: impl Fn(&mut Window, &mut Context<Host>) -> AnyElement + 'static,
) -> Opened<'_> {
    cx.update(gpui_core::init);
    let renders = Rc::new(Cell::new(0));
    let r = renders.clone();
    let build: Build = Rc::new(build);
    let (host, vcx) = cx.add_window_view(move |_, _| Host { build, renders: r });
    vcx.simulate_resize(size(px(800.), px(600.)));
    vcx.update(|window, _| window.activate_window());
    let mut o = Opened { host, renders, vcx };
    o.settle();
    o
}

impl Opened<'_> {
    pub fn settle(&mut self) {
        self.vcx.run_until_parked();
    }

    /// Draws a frame now, as the next one would be.
    pub fn draw(&mut self) {
        self.vcx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        self.vcx.run_until_parked();
    }

    pub fn bounds(&mut self, selector: &str) -> Bounds<Pixels> {
        let selector: &'static str = selector.to_owned().leak();
        self.vcx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} is drawn"))
    }

    pub fn shown(&mut self, selector: &str) -> bool {
        let selector: &'static str = selector.to_owned().leak();
        self.vcx.debug_bounds(selector).is_some()
    }

    pub fn center(&mut self, selector: &str) -> Point<Pixels> {
        self.bounds(selector).center()
    }

    pub fn down(&mut self, at: Point<Pixels>) {
        self.vcx
            .simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
        self.settle();
    }

    pub fn drag_to(&mut self, at: Point<Pixels>) {
        self.vcx
            .simulate_mouse_move(at, MouseButton::Left, Modifiers::none());
        self.settle();
    }

    pub fn up(&mut self, at: Point<Pixels>) {
        self.vcx
            .simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
        self.settle();
    }

    /// Presses at `from`, moves past the threshold, then to `to`, without letting go.
    pub fn hold(&mut self, from: Point<Pixels>, to: Point<Pixels>) {
        self.down(from);
        self.drag_to(from + point(px(10.), px(0.)));
        self.drag_to(to);
    }

    /// A whole drag from `from` to `to`.
    pub fn drag(&mut self, from: Point<Pixels>, to: Point<Pixels>) {
        self.hold(from, to);
        self.up(to);
    }

    pub fn keys(&mut self, keys: &str) {
        self.vcx.simulate_keystrokes(keys);
        self.settle();
    }
}

pub fn log() -> Log {
    Rc::default()
}

pub fn entries(log: &Log) -> Vec<String> {
    log.borrow().clone()
}

/// The entries that start with `prefix`.
pub fn with(log: &Log, prefix: &str) -> Vec<String> {
    log.borrow()
        .iter()
        .filter(|e| e.starts_with(prefix))
        .cloned()
        .collect()
}
