//! A scroll area: content that scrolls inside a box, with thin scrollbars laid over its edges
//! (shadcn's ScrollArea). The bars show while it scrolls or the pointer is over it, then fade;
//! a thumb can be dragged and a press on a bar's track pages. Wheels, trackpads and touch scroll
//! it as any GPUI scrolling box, and with the focus the arrows, Page Up and Page Down, Space,
//! Home and End do.

use std::time::Duration;
// GPUI's clock: std's Instant, but in a browser (which has none) web-time's.
#[cfg(not(target_family = "wasm"))]
use std::time::Instant;
#[cfg(target_family = "wasm")]
use web_time::Instant;

use gpui::{
    AnyElement, App, Bounds, ElementId, EntityId, FocusHandle, Hsla, InteractiveElement as _,
    IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ParentElement, Pixels, Point, RenderOnce, Role, ScrollHandle, ScrollSource,
    StatefulInteractiveElement as _, StyleRefinement, Styled, Task, Window, canvas, div, point,
    prelude::FluentBuilder as _, px, rems,
};

use crate::base::{ActiveTheme as _, focus_ring};

/// How long the bars stay after the last scroll, or after the pointer leaves (Radix's
/// `scrollHideDelay`).
pub(crate) const HOLD: Duration = Duration::from_millis(600);
/// How long they take to fade out.
pub(crate) const FADE: Duration = Duration::from_millis(150);

/// A bar's thickness, its padding round the thumb, and the shortest a thumb gets, in CSS pixels.
const BAR: f32 = 10.;
const PAD: f32 = 1.;
const MIN_THUMB: f32 = 18.;
/// How far an arrow key scrolls, in CSS pixels, and how much of the view a page is (a
/// browser's).
const LINE: f32 = 40.;
const PAGE: f32 = 0.875;

/// Which ways a [`ScrollArea`] scrolls.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScrollAxes {
    #[default]
    Vertical,
    Horizontal,
    Both,
}

impl ScrollAxes {
    fn vertical(self) -> bool {
        self != Self::Horizontal
    }

    fn horizontal(self) -> bool {
        self != Self::Vertical
    }
}

/// A [`ScrollArea`]'s scroll, for the host to read and set. Give it with
/// [`ScrollArea::track_scroll`]. Setting it draws nothing by itself: notify the view after.
/// Positions are how far it's scrolled from the top left, so they grow as it scrolls down and
/// right.
#[derive(Clone, Debug, Default)]
pub struct ScrollAreaHandle(ScrollHandle);

impl ScrollAreaHandle {
    pub fn new() -> Self {
        Self::default()
    }

    /// How far it's scrolled each way.
    pub fn position(&self) -> Point<Pixels> {
        -self.0.offset()
    }

    /// How far it can scroll each way (zero where its content fits), as of its last draw.
    pub fn max_position(&self) -> Point<Pixels> {
        self.0.max_offset()
    }

    /// The box its content is seen through, in window coordinates, as of its last draw.
    pub fn viewport(&self) -> Bounds<Pixels> {
        self.0.bounds()
    }

    /// Scrolls to `position` (held inside the content when it's drawn).
    pub fn scroll_to(&self, position: Point<Pixels>) {
        self.0.set_offset(-position.max(&Point::default()));
    }

    /// Scrolls by `delta`, right and down for positive values.
    pub fn scroll_by(&self, delta: Point<Pixels>) {
        self.scroll_to(self.position() + delta);
    }

    pub fn scroll_to_top(&self) {
        self.scroll_to(point(self.position().x, px(0.)));
    }

    pub fn scroll_to_bottom(&self) {
        self.0.scroll_to_bottom();
    }

    /// Scrolls the least that brings the area's `index`th child (of those given to it) into
    /// view.
    pub fn scroll_to_child(&self, index: usize) {
        self.0.scroll_to_item(index);
    }

    /// Scrolls so the area's `index`th child is at the top.
    pub fn scroll_to_child_top(&self, index: usize) {
        self.0.scroll_to_top_of_item(index);
    }

    /// GPUI's handle underneath: for a [`gpui::ScrollAnchor`] on an element deep inside the
    /// content (`ScrollAnchor::for_handle(handle.scroll_handle().clone())`).
    pub fn scroll_handle(&self) -> &ScrollHandle {
        &self.0
    }
}

/// Content that scrolls inside a box, with shadcn's thin scrollbars over its edges. Size it like
/// a `div` (it is one: give it a height, or a most height) and give it children; padding goes on
/// a child, as on the web. Vertical unless told otherwise.
///
/// The bars draw only where the content overflows, while it scrolls or the pointer is over it,
/// then fade (not on a touch screen, where it scrolls with GPUI's momentum and shows no bars).
/// Drag a thumb to scroll, or press a track to page. With the focus (Tab), the arrows scroll a
/// line, Page Up, Page Down and Space a page, Home and End to the ends. Once the bars have faded
/// nothing redraws.
///
/// ```no_run
/// # use gpui::*;
/// # use gpui_core::ScrollArea;
/// # fn tags() -> impl IntoElement {
/// ScrollArea::new("tags")
///     .h(px(288.))
///     .w(px(192.))
///     .rounded_md()
///     .border_1()
///     .child(div().p_4().children((1..=50).map(|i| div().child(format!("v1.2.0-beta.{i}")))))
/// # }
/// ```
#[derive(IntoElement)]
pub struct ScrollArea {
    id: ElementId,
    axes: ScrollAxes,
    handle: Option<ScrollAreaHandle>,
    label: Option<gpui::SharedString>,
    tab_stop: bool,
    thumb: Option<Hsla>,
    ring: Option<Hsla>,
    style: StyleRefinement,
    children: Vec<AnyElement>,
}

impl ScrollArea {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            axes: ScrollAxes::Vertical,
            handle: None,
            label: None,
            tab_stop: true,
            thumb: None,
            ring: None,
            style: StyleRefinement::default(),
            children: Vec::new(),
        }
    }

    /// Which ways it scrolls. Sideways, its children are laid in a row, each as wide as its
    /// content; both ways, they're stacked, each as wide as its content.
    pub fn axes(mut self, axes: ScrollAxes) -> Self {
        self.axes = axes;
        self
    }

    /// Scrolls sideways only.
    pub fn horizontal(self) -> Self {
        self.axes(ScrollAxes::Horizontal)
    }

    /// Scrolls both ways.
    pub fn both(self) -> Self {
        self.axes(ScrollAxes::Both)
    }

    /// Reads and sets its scroll through `handle`.
    pub fn track_scroll(mut self, handle: &ScrollAreaHandle) -> Self {
        self.handle = Some(handle.clone());
        self
    }

    /// What it's called to assistive technology.
    pub fn accessibility_label(mut self, label: impl Into<gpui::SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Whether Tab stops on it to scroll it from the keyboard (it does unless told not to: a
    /// list whose rows take the focus themselves says no).
    pub fn tab_stop(mut self, tab_stop: bool) -> Self {
        self.tab_stop = tab_stop;
        self
    }

    /// The thumbs' colour (the theme's border unless given).
    pub fn thumb_color(mut self, color: impl Into<Hsla>) -> Self {
        self.thumb = Some(color.into());
        self
    }

    /// The keyboard focus ring's colour, drawn as given (the theme's ring at half strength
    /// unless given).
    pub fn focus_ring_color(mut self, color: impl Into<Hsla>) -> Self {
        self.ring = Some(color.into());
        self
    }
}

impl ParentElement for ScrollArea {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

/// The area's own box: its size, border, corners and so on.
impl Styled for ScrollArea {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Axis {
    X,
    Y,
}

fn along(p: Point<Pixels>, axis: Axis) -> Pixels {
    match axis {
        Axis::X => p.x,
        Axis::Y => p.y,
    }
}

fn with(p: Point<Pixels>, axis: Axis, v: Pixels) -> Point<Pixels> {
    match axis {
        Axis::X => point(v, p.y),
        Axis::Y => point(p.x, v),
    }
}

/// The scroll as it was drawn: the viewport, how far it can scroll and how far it has.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Geometry {
    viewport: Bounds<Pixels>,
    max: Point<Pixels>,
    position: Point<Pixels>,
}

impl Geometry {
    fn read(handle: &ScrollHandle) -> Self {
        let max = handle.max_offset();
        let position = (-handle.offset()).max(&Point::default()).min(&max);
        Self {
            viewport: handle.bounds(),
            max,
            position,
        }
    }

    fn length(&self, axis: Axis) -> Pixels {
        match axis {
            Axis::X => self.viewport.size.width,
            Axis::Y => self.viewport.size.height,
        }
    }

    fn overflows(&self, axis: Axis) -> bool {
        along(self.max, axis) > px(0.5)
    }
}

/// Where a thumb runs: its track's start (from the viewport's edge) and length along the axis,
/// and the thumb's start on the track and length.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Thumb {
    track_start: Pixels,
    track: Pixels,
    start: Pixels,
    length: Pixels,
}

/// The thumb for `g` along `axis`, given the bar's thickness and padding in pixels and whether
/// the other bar takes the corner.
fn thumb(g: &Geometry, axis: Axis, bar: Pixels, pad: Pixels, min: Pixels, corner: bool) -> Thumb {
    let view = g.length(axis);
    let track = (view - if corner { bar } else { px(0.) } - pad * 2.).max(px(0.));
    let content = view + along(g.max, axis);
    let ratio = if content > px(0.) { view / content } else { 1. };
    let length = (track * ratio).max(min).min(track);
    let max = along(g.max, axis);
    let start = if max > px(0.) {
        (track - length) * (along(g.position, axis) / max).clamp(0., 1.)
    } else {
        px(0.)
    };
    Thumb {
        track_start: pad,
        track,
        start,
        length,
    }
}

/// A thumb being dragged: along which axis, and where on the thumb it was grabbed.
#[derive(Clone, Copy, Debug)]
struct Drag {
    axis: Axis,
    grab: Pixels,
}

/// What an area remembers between frames.
struct AreaState {
    /// Its own scroll, when the host gave none.
    handle: ScrollAreaHandle,
    focus: FocusHandle,
    /// The scroll as last drawn.
    drawn: Geometry,
    /// Whether it has been drawn yet (its first position isn't a scroll).
    placed: bool,
    /// When it last scrolled, or the pointer left it or let go of a thumb: the bars stay for
    /// [`HOLD`] after, then fade.
    shown_at: Option<Instant>,
    hovered: bool,
    /// The last scroll came from a finger: no bars.
    touch: bool,
    drag: Option<Drag>,
    /// Draws again when the bars are to start fading.
    hold: Option<Task<()>>,
}

impl AreaState {
    /// How opaque the bars are now, and whether that's changing.
    fn opacity(&self, now: Instant, reduce_motion: bool) -> (f32, bool) {
        if self.drag.is_some() {
            return (1., false);
        }
        if self.touch {
            return (0., false);
        }
        if self.hovered {
            return (1., false);
        }
        let Some(at) = self.shown_at else {
            return (0., false);
        };
        let t = now.saturating_duration_since(at);
        if t < HOLD {
            (1., false)
        } else if reduce_motion || t >= HOLD + FADE {
            (0., false)
        } else {
            (1. - (t - HOLD).as_secs_f32() / FADE.as_secs_f32(), true)
        }
    }

    /// Shows the bars from now, and draws `view` again when they're to fade.
    fn show(&mut self, view: EntityId, now: Instant, window: &Window, cx: &App) {
        self.shown_at = Some(now);
        self.hold = Some(window.spawn(cx, async move |cx| {
            cx.background_executor().timer(HOLD).await;
            cx.update(|_, cx| cx.notify(view)).ok();
        }));
    }
}

/// The page or line keys' scroll from `g`, or none for a key it doesn't take.
fn key_scroll(
    key: &str,
    shift: bool,
    g: &Geometry,
    axes: ScrollAxes,
    line: Pixels,
) -> Option<Point<Pixels>> {
    let y = axes.vertical() && g.overflows(Axis::Y);
    let x = axes.horizontal() && g.overflows(Axis::X);
    // Page Up, Page Down, Space, Home and End go up and down, or sideways when it only scrolls
    // that way.
    let main = if y || !x { Axis::Y } else { Axis::X };
    let page = g.length(main) * PAGE;
    let p = g.position;
    let to = match key {
        "up" if y => with(p, Axis::Y, p.y - line),
        "down" if y => with(p, Axis::Y, p.y + line),
        "left" if x => with(p, Axis::X, p.x - line),
        "right" if x => with(p, Axis::X, p.x + line),
        "pageup" => with(p, main, along(p, main) - page),
        "pagedown" => with(p, main, along(p, main) + page),
        "space" if shift => with(p, main, along(p, main) - page),
        "space" => with(p, main, along(p, main) + page),
        "home" => with(p, main, px(0.)),
        "end" => with(p, main, along(g.max, main)),
        _ => return None,
    };
    (x || y).then(|| to.max(&Point::default()).min(&g.max))
}

impl RenderOnce for ScrollArea {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let state = window.use_keyed_state(self.id.clone(), cx, |_, cx| AreaState {
            handle: ScrollAreaHandle::new(),
            focus: cx.focus_handle(),
            drawn: Geometry::default(),
            placed: false,
            shown_at: None,
            hovered: false,
            touch: false,
            drag: None,
            hold: None,
        });
        let handle = self
            .handle
            .clone()
            .unwrap_or_else(|| state.read(cx).handle.clone());
        let view = window.current_view();
        let now = cx.background_executor().now();
        let rem = window.rem_size();
        let css = |v: f32| rems(v / 15.).to_pixels(rem);
        let (bar, pad, min) = (css(BAR), css(PAD), css(MIN_THUMB));
        let axes = self.axes;

        // The scroll now: a wheel, a key or a thumb moved it since the last draw.
        let g = Geometry::read(&handle.0);
        state.update(cx, |s, cx| {
            if s.placed && g.position != s.drawn.position && !s.touch {
                s.show(view, now, window, cx);
            }
            s.drawn = g;
        });
        let (opacity, fading) = state.read(cx).opacity(now, cx.reduce_motion());
        if fading {
            window.request_animation_frame();
        }
        let focus = state.read(cx).focus.clone();

        let show_y = axes.vertical() && g.overflows(Axis::Y) && opacity > 0.;
        let show_x = axes.horizontal() && g.overflows(Axis::X) && opacity > 0.;
        let thumb_color = self.thumb.unwrap_or(theme.border);

        let ring = match self.ring {
            Some(color) => gpui::BoxShadow {
                color,
                ..focus_ring(cx)
            },
            None => focus_ring(cx),
        };

        let viewport = div()
            .id("scroll-area-viewport")
            .flex_auto()
            .min_h_0()
            .min_w_0()
            .when(axes.vertical(), |d| d.overflow_y_scroll())
            .when(axes.horizontal(), |d| d.overflow_x_scroll())
            .map(|d| match axes {
                ScrollAxes::Vertical => d.flex().flex_col(),
                ScrollAxes::Horizontal => d.flex().flex_row().items_start(),
                ScrollAxes::Both => d.flex().flex_col().items_start(),
            })
            .track_scroll(&handle.0)
            .on_scroll({
                let state = state.clone();
                move |e, _, cx| {
                    let touch = e.source == ScrollSource::Touch;
                    state.update(cx, |s, _| {
                        s.touch = touch;
                        if touch {
                            s.shown_at = None;
                            s.hold = None;
                        }
                    });
                }
            })
            .children(self.children);

        // A bar along `axis`: its thumb, a press on the track paging toward the pointer.
        let make_bar = |axis: Axis, corner: bool| {
            let t = thumb(&g, axis, bar, pad, min, corner);
            let state = state.clone();
            let handle = handle.clone();
            let thumb_el = div()
                .absolute()
                .rounded_full()
                .bg(thumb_color)
                .map(|d| match axis {
                    Axis::Y => d
                        .debug_selector(|| "scroll-area-thumb-y".into())
                        .top(t.track_start + t.start)
                        .h(t.length)
                        // The inner edge's transparent border, then the padding.
                        .left(pad * 2.)
                        .right(pad),
                    Axis::X => d
                        .debug_selector(|| "scroll-area-thumb-x".into())
                        .left(t.track_start + t.start)
                        .w(t.length)
                        .top(pad * 2.)
                        .bottom(pad),
                });
            div()
                .id(match axis {
                    Axis::Y => "scroll-area-bar-y",
                    Axis::X => "scroll-area-bar-x",
                })
                .absolute()
                .map(|d| match axis {
                    Axis::Y => d
                        .debug_selector(|| "scroll-area-bar-y".into())
                        .top_0()
                        .right_0()
                        .w(bar)
                        .h(t.track + pad * 2.),
                    Axis::X => d
                        .debug_selector(|| "scroll-area-bar-x".into())
                        .left_0()
                        .bottom_0()
                        .h(bar)
                        .w(t.track + pad * 2.),
                })
                .opacity(opacity)
                .on_mouse_down(MouseButton::Left, move |e: &MouseDownEvent, window, cx| {
                    // Not a press on the content: no focus moves, no text is selected.
                    window.prevent_default();
                    cx.stop_propagation();
                    let g = state.read(cx).drawn;
                    let edge = along(g.viewport.origin, axis) + t.track_start;
                    let at = along(e.position, axis) - edge;
                    if at >= t.start && at <= t.start + t.length {
                        state.update(cx, |s, _| {
                            s.drag = Some(Drag {
                                axis,
                                grab: at - t.start,
                            })
                        });
                    } else {
                        let page = g.length(axis) * PAGE;
                        let by = if at < t.start { -page } else { page };
                        handle.scroll_to(with(
                            g.position,
                            axis,
                            (along(g.position, axis) + by).clamp(px(0.), along(g.max, axis)),
                        ));
                    }
                    cx.notify(view);
                })
                .child(thumb_el)
        };
        let bar_y = show_y.then(|| make_bar(Axis::Y, show_x));
        let bar_x = show_x.then(|| make_bar(Axis::X, show_y));

        // After the viewport is placed: if it scrolled or changed size while placing (a
        // child scrolled into view, content that grew), draw again with the bars in step; and
        // while a thumb is dragged, follow the pointer anywhere until it's let go.
        let follow = canvas(
            {
                let state = state.clone();
                let handle = handle.clone();
                move |_, window, cx| {
                    let now = cx.background_executor().now();
                    let fresh = Geometry::read(&handle.0);
                    let redraw = state.update(cx, |s, cx| {
                        let was = s.drawn;
                        let placed = s.placed;
                        s.placed = true;
                        s.drawn = fresh;
                        if !placed || fresh == was {
                            return false;
                        }
                        if fresh.position != was.position && !s.touch {
                            s.show(view, now, window, cx);
                            return true;
                        }
                        // Only the bars use the sizes, and only while they show.
                        let showing = s.opacity(now, false).0 > 0.;
                        showing
                            && (fresh.viewport.size != was.viewport.size || fresh.max != was.max)
                    });
                    if redraw {
                        cx.defer(move |cx| cx.notify(view));
                    }
                }
            },
            {
                let state = state.clone();
                let handle = handle.clone();
                move |_, _, window, cx| {
                    let Some(drag) = state.read(cx).drag else {
                        return;
                    };
                    let s = state.clone();
                    let h = handle.clone();
                    window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| {
                        if phase != gpui::DispatchPhase::Bubble
                            || e.pressed_button != Some(MouseButton::Left)
                        {
                            return;
                        }
                        let g = s.read(cx).drawn;
                        let corner =
                            drag.axis == Axis::Y && axes.horizontal() && g.overflows(Axis::X)
                                || drag.axis == Axis::X && axes.vertical() && g.overflows(Axis::Y);
                        let t = thumb(&g, drag.axis, bar, pad, min, corner);
                        let room = t.track - t.length;
                        let max = along(g.max, drag.axis);
                        if room <= px(0.) || max <= px(0.) {
                            return;
                        }
                        let start = along(e.position, drag.axis)
                            - along(g.viewport.origin, drag.axis)
                            - t.track_start
                            - drag.grab;
                        let to = max * (start / room).clamp(0., 1.);
                        if to != along(g.position, drag.axis) {
                            h.scroll_to(with(g.position, drag.axis, to));
                            cx.notify(view);
                        }
                    });
                    let s = state.clone();
                    window.on_mouse_event(move |_: &MouseUpEvent, phase, window, cx| {
                        if phase != gpui::DispatchPhase::Bubble {
                            return;
                        }
                        let now = cx.background_executor().now();
                        s.update(cx, |s, cx| {
                            if s.drag.take().is_some() {
                                s.show(view, now, window, cx);
                            }
                        });
                        cx.notify(view);
                    });
                }
            },
        )
        .absolute()
        .size_0();

        let mut outer = div()
            .id(self.id)
            .role(Role::ScrollView)
            .when_some(self.label, |d, l| d.aria_label(l))
            .relative()
            .flex()
            .flex_col()
            .overflow_hidden()
            .track_focus(&focus.clone().tab_stop(self.tab_stop))
            .focus_visible(move |s| s.shadow(vec![ring]))
            // Hover shows the bars; there's none on a touch screen (a tap would leave it on).
            .when(!cfg!(target_os = "ios"), |d| {
                let state = state.clone();
                d.on_hover(move |hovered, window, cx| {
                    let now = cx.background_executor().now();
                    let hovered = *hovered;
                    state.update(cx, |s, cx| {
                        if s.hovered == hovered {
                            return;
                        }
                        s.hovered = hovered;
                        // (A finger's scroll moves GPUI's pointer too: only a pointer's own
                        // scroll ends the touch.)
                        if !hovered {
                            s.show(view, now, window, cx);
                        }
                    });
                    cx.notify(view);
                })
            })
            .on_key_down({
                let state = state.clone();
                let handle = handle.clone();
                move |e: &KeyDownEvent, window, cx| {
                    let m = e.keystroke.modifiers;
                    if m.control || m.alt || m.platform || m.function {
                        return;
                    }
                    // Only its own focus: a field or a list inside keeps its keys.
                    if !state.read(cx).focus.is_focused(window) {
                        return;
                    }
                    let g = Geometry::read(&handle.0);
                    let line = rems(LINE / 15.).to_pixels(window.rem_size());
                    if let Some(to) = key_scroll(&e.keystroke.key, m.shift, &g, axes, line) {
                        window.prevent_default();
                        cx.stop_propagation();
                        if to != g.position {
                            handle.scroll_to(to);
                            cx.notify(view);
                        }
                    }
                }
            });
        gpui::Refineable::refine(outer.style(), &self.style);
        outer
            .child(viewport)
            .children(bar_y)
            .children(bar_x)
            .child(follow)
    }
}
