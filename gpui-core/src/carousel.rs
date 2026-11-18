//! A carousel: slides in a row, one or several in view, moved by previous and next buttons,
//! the Left and Right keys, optional dots, and a swipe on a touch screen or a trackpad that
//! snaps to a slide (shadcn's Carousel, which is Embla's).

use std::rc::Rc;
use std::time::Duration;
// GPUI's clock: std's Instant, but in a browser (which has none) web-time's.
#[cfg(not(target_family = "wasm"))]
use std::time::Instant;
#[cfg(target_family = "wasm")]
use web_time::Instant;

use gpui::{
    AnyElement, App, Bounds, Element, ElementId, EntityId, FocusHandle, GlobalElementId,
    InspectorElementId, InteractiveElement as _, IntoElement, KeyDownEvent, LayoutId,
    ParentElement, Pixels, RenderOnce, Role, ScrollEvent, SharedString,
    StatefulInteractiveElement as _, StyleRefinement, Styled, TouchPhase, Window, accesskit,
    canvas, div, ease_out_quint, prelude::FluentBuilder as _, px, relative, rems,
};

use crate::base::{
    ActiveTheme as _, Disableable as _, IconName, Press, Selectable as _, Sizable as _, focus_ring,
};
use crate::button::{Button, ButtonVariants as _};

/// How long a move to another slide takes.
pub(crate) const MOVE: Duration = Duration::from_millis(250);
/// How far a swipe must go, in slides, to move on by one when it's let go short of halfway.
const FLICK: f32 = 0.15;
/// How much of a drag past the first or last slide shows.
const RUBBER: f32 = 0.3;
/// How far a trackpad or finger moves before the carousel decides the gesture is sideways (its
/// own) or up and down (the page's), in pixels.
const DECIDE: f32 = 4.;

type OnSlide = Rc<dyn Fn(&usize, &mut Window, &mut App)>;

fn css(v: f32) -> gpui::Rems {
    rems(v / 15.)
}

/// Slides in a row: one in view (or [`slides_per_view`](Carousel::slides_per_view)), with
/// previous and next buttons outside its sides (disabled at the ends unless it
/// [`loops`](Carousel::looping)), and dots below with [`dots`](Carousel::dots). Left and Right
/// move it while the focus is in it; a sideways swipe on a touch screen or a trackpad drags it
/// and snaps to the nearest slide when let go (on by a slide past a short flick). A move slides
/// briefly into place, then nothing redraws.
///
/// The buttons sit 48 pixels outside its left and right edges, as shadcn's do: leave the room,
/// or turn them off with [`buttons(false)`](Carousel::buttons). It's a region to assistive
/// technology described as a carousel, its slides groups described as slides and named "1 of
/// 5" and so on.
///
/// ```no_run
/// # use gpui::*;
/// # use gpui_core::Carousel;
/// # fn slides() -> impl IntoElement {
/// Carousel::new("photos")
///     .w(px(320.))
///     .dots(true)
///     .on_slide(|index, _, _| println!("slide {index}"))
///     .children((1..=5).map(|i| div().h(px(200.)).child(format!("{i}"))))
/// # }
/// ```
#[derive(IntoElement)]
pub struct Carousel {
    id: ElementId,
    slides: Vec<AnyElement>,
    per_view: usize,
    looping: bool,
    dots: bool,
    buttons: bool,
    index: Option<usize>,
    on_slide: Option<OnSlide>,
    label: Option<SharedString>,
    spacing: gpui::Rems,
    touch: bool,
    style: StyleRefinement,
}

impl Carousel {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            slides: Vec::new(),
            per_view: 1,
            looping: false,
            dots: false,
            buttons: true,
            index: None,
            on_slide: None,
            label: None,
            spacing: css(16.),
            touch: false,
            style: StyleRefinement::default(),
        }
    }

    /// How many slides are in view at once (one unless given). It moves a slide at a time, and
    /// stops when the last is in view.
    pub fn slides_per_view(mut self, n: usize) -> Self {
        self.per_view = n.max(1);
        self
    }

    /// Whether Next after the last slide goes back to the first and Previous before the first
    /// to the last (off by default: the buttons are disabled at the ends). A swipe stops at the
    /// ends either way.
    pub fn looping(mut self, looping: bool) -> Self {
        self.looping = looping;
        self
    }

    /// Whether dots below it show which slide is in view and move to one when pressed (off
    /// by default).
    pub fn dots(mut self, dots: bool) -> Self {
        self.dots = dots;
        self
    }

    /// Whether it has previous and next buttons (on by default).
    pub fn buttons(mut self, buttons: bool) -> Self {
        self.buttons = buttons;
        self
    }

    /// Moves it to slide `index` when this changes from what the host last gave (the first
    /// time, it starts there). The person's moves in between are theirs, told in
    /// [`on_slide`](Self::on_slide).
    pub fn index(mut self, index: usize) -> Self {
        self.index = Some(index);
        self
    }

    /// Told the slide it moved to, by the buttons, the keys, a dot or a swipe.
    pub fn on_slide(mut self, f: impl Fn(&usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_slide = Some(Rc::new(f));
        self
    }

    /// What it's called to assistive technology.
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The space between slides (16 CSS pixels unless given).
    pub fn spacing(mut self, spacing: gpui::Rems) -> Self {
        self.spacing = spacing;
        self
    }

    /// Buttons and dots 44 points across, for a finger.
    pub fn touch(mut self, touch: bool) -> Self {
        self.touch = touch;
        self
    }
}

impl ParentElement for Carousel {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.slides.extend(elements)
    }
}

/// The carousel's own box: its width, mostly.
impl Styled for Carousel {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// A trackpad's or a finger's gesture over the slides.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Gesture {
    Idle,
    /// Begun, not yet far enough to say which way: how far it has gone.
    Undecided(Pixels, Pixels),
    /// Sideways: dragging the slides this far (right is positive).
    Dragging(Pixels),
    /// Up and down: the page's.
    Ignored,
}

/// What a carousel remembers between frames.
struct CarouselState {
    focus: FocusHandle,
    index: usize,
    /// The index the host gave last frame.
    given: Option<usize>,
    /// The move under way: where it started (in slides) and when.
    from: f32,
    started: Option<Instant>,
    gesture: Gesture,
    /// One slide's step, in pixels, as last laid out.
    step: Pixels,
}

impl CarouselState {
    /// Where the slides are now, in slides from the first, and whether that's changing.
    fn shown(&self, now: Instant, reduce_motion: bool, last: usize) -> (f32, bool) {
        let to = self.index as f32;
        if let Gesture::Dragging(x) = self.gesture {
            let step = f32::from(self.step).max(1.);
            return (rubber(to - f32::from(x) / step, last), false);
        }
        let Some(at) = self.started else {
            return (to, false);
        };
        let t = now.saturating_duration_since(at).as_secs_f32() / MOVE.as_secs_f32();
        if reduce_motion || t >= 1. {
            (to, false)
        } else {
            (self.from + (to - self.from) * ease_out_quint()(t), true)
        }
    }
}

/// `pos` with what's past the ends shown at [`RUBBER`] strength.
fn rubber(pos: f32, last: usize) -> f32 {
    let last = last as f32;
    if pos < 0. {
        pos * RUBBER
    } else if pos > last {
        last + (pos - last) * RUBBER
    } else {
        pos
    }
}

/// Where a drag of `x` pixels (right positive) let go at `index` lands, given a slide's step.
fn snap(index: usize, x: Pixels, step: Pixels, last: usize) -> usize {
    if step <= px(0.) {
        return index;
    }
    let moved = f32::from(x) / f32::from(step);
    let mut to = (index as f32 - moved).round();
    if to == index as f32 && moved.abs() > FLICK {
        to = index as f32 - moved.signum();
    }
    to.clamp(0., last as f32) as usize
}

/// Everything a move needs.
#[derive(Clone)]
struct Mover {
    state: gpui::Entity<CarouselState>,
    view: EntityId,
    last: usize,
    looping: bool,
    on_slide: Option<OnSlide>,
}

impl Mover {
    /// Moves to slide `to` (wrapped round with looping, else held at the ends), sliding from
    /// where it's shown now, and tells the host.
    fn go(&self, to: isize, window: &mut Window, cx: &mut App) {
        let snaps = self.last as isize + 1;
        let to = if self.looping {
            to.rem_euclid(snaps)
        } else {
            to.clamp(0, self.last as isize)
        } as usize;
        self.settle(to, window, cx);
    }

    /// Moves to `to` (already in range) from where it's shown now.
    fn settle(&self, to: usize, window: &mut Window, cx: &mut App) {
        let now = cx.background_executor().now();
        let reduce = cx.reduce_motion();
        let last = self.last;
        let changed = self.state.update(cx, |s, _| {
            let (from, _) = s.shown(now, reduce, last);
            let changed = to != s.index;
            if changed || s.gesture != Gesture::Idle || from != to as f32 {
                s.from = from;
                s.started = Some(now);
            }
            s.gesture = Gesture::Idle;
            s.index = to;
            changed
        });
        cx.notify(self.view);
        if changed && let Some(f) = &self.on_slide {
            f(&to, window, cx);
        }
    }

    fn by(&self, delta: isize, window: &mut Window, cx: &mut App) {
        let at = self.state.read(cx).index as isize;
        self.go(at + delta, window, cx);
    }
}

impl RenderOnce for Carousel {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let count = self.slides.len();
        let n = self.per_view;
        let last = count.saturating_sub(n);
        let state = window.use_keyed_state(self.id.clone(), cx, |_, cx| CarouselState {
            focus: cx.focus_handle(),
            index: 0,
            given: None,
            from: 0.,
            started: None,
            gesture: Gesture::Idle,
            step: px(0.),
        });
        let view = window.current_view();
        let now = cx.background_executor().now();
        let reduce = cx.reduce_motion();

        // The host's index: where it starts, and a move when it changes.
        if let Some(i) = self.index {
            let i = i.min(last);
            state.update(cx, |s, _| {
                match s.given {
                    None => s.index = i,
                    Some(g) if g != i && s.index != i => {
                        (s.from, _) = s.shown(now, reduce, last);
                        s.started = Some(now);
                        s.index = i;
                    }
                    _ => {}
                }
                s.given = Some(i);
            });
        }
        state.update(cx, |s, _| s.index = s.index.min(last));

        let (pos, moving) = state.read(cx).shown(now, reduce, last);
        if moving {
            window.request_animation_frame();
        }
        let (index, focus) = {
            let s = state.read(cx);
            (s.index, s.focus.clone())
        };
        let mover = Mover {
            state: state.clone(),
            view,
            last,
            looping: self.looping && last > 0,
            on_slide: self.on_slide.clone(),
        };

        let spacing = self.spacing.to_pixels(window.rem_size());
        let per = 1. / n as f32;
        let step = state.read(cx).step;
        // The track hangs a spacing left of the view, as shadcn's `-ml-4`; each slide is a
        // `1/n`th of it with the spacing on its left, so a step is (view + spacing) / n, as
        // measured when it was last laid out.
        let track = div()
            .debug_selector(|| "carousel-track".into())
            .relative()
            .flex()
            .flex_row()
            .ml(-spacing)
            .left(-step * pos)
            .children(self.slides.into_iter().enumerate().map(|(i, slide)| {
                RoleDescription::new(
                    div()
                        .id(("carousel-slide", i))
                        .debug_selector(move || format!("carousel-slide-{i}"))
                        .role(Role::Group)
                        .aria_label(format!("{} of {}", i + 1, count))
                        .flex_none()
                        .min_w_0()
                        .w(relative(per))
                        .pl(spacing)
                        .child(slide),
                    "slide",
                )
            }));

        let viewport = div()
            .id("carousel-viewport")
            .debug_selector(|| "carousel-viewport".into())
            .relative()
            .overflow_hidden()
            .flex()
            .flex_col()
            .on_scroll({
                let mover = mover.clone();
                move |e: &ScrollEvent, window, cx| gesture(&mover, e, window, cx)
            })
            .child(track)
            // Measures a slide's step, for a drag.
            .child(
                canvas(
                    {
                        let state = state.clone();
                        move |b: Bounds<Pixels>, _, cx| {
                            let step = (b.size.width + spacing) * per;
                            let was = state.update(cx, |s, _| std::mem::replace(&mut s.step, step));
                            // Laid out at a new width (or for the first time) away from the
                            // first slide: draw again with the slides where they belong.
                            if was != step && pos != 0. {
                                cx.defer(move |cx| cx.notify(view));
                            }
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            );

        let touch = self.touch;
        let side = if touch {
            theme.touch_target
        } else {
            css(32.).to_pixels(window.rem_size())
        };
        let button = |id: &'static str, icon: IconName, label: &'static str, delta: isize| {
            let mover = mover.clone();
            let disabled = !mover.looping && if delta < 0 { index == 0 } else { index >= last };
            let gap = css(16.).to_pixels(window.rem_size());
            div()
                .debug_selector(move || id.into())
                .absolute()
                .top(relative(0.5))
                .mt(-side / 2.)
                .map(|d| {
                    if delta < 0 {
                        d.left(-side - gap)
                    } else {
                        d.right(-side - gap)
                    }
                })
                .child(
                    Button::new(id)
                        .icon(icon)
                        .accessibility_label(label)
                        .outline()
                        .disabled(disabled)
                        .when(touch, |b| b.touch())
                        .size(side)
                        .rounded_full()
                        .on_click(move |_, window, cx| mover.by(delta, window, cx)),
                )
        };
        let buttons = self.buttons.then(|| {
            [
                button(
                    "carousel-previous",
                    IconName::ArrowLeft,
                    "Previous slide",
                    -1,
                ),
                button("carousel-next", IconName::ArrowRight, "Next slide", 1),
            ]
        });

        let dots = (self.dots && last > 0).then(|| {
            div()
                .flex()
                .justify_center()
                .gap(if touch {
                    px(0.)
                } else {
                    css(8.).to_pixels(window.rem_size())
                })
                .pt(if touch {
                    px(0.)
                } else {
                    css(12.).to_pixels(window.rem_size())
                })
                .children((0..=last).map(|i| {
                    let mover = mover.clone();
                    let on = i == index;
                    let dot = div().size(css(8.)).rounded_full().bg(if on {
                        theme.primary
                    } else {
                        theme.border
                    });
                    Press::new(("carousel-dot", i))
                        .debug_selector(move || format!("carousel-dot-{i}"))
                        .accessibility_label(format!("Slide {}", i + 1))
                        .selected(on)
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .map(|p| {
                            if touch {
                                p.size(theme.touch_target)
                            } else {
                                p.size(css(8.))
                            }
                        })
                        .child(dot)
                        .on_click(move |_, window, cx| mover.go(i as isize, window, cx))
                }))
        });

        let ring = focus_ring(cx);
        crate::base::a11y_audit("Carousel", Role::Region, self.label.as_ref());
        let mut outer = div()
            .id(self.id)
            .role(Role::Region)
            .when_some(self.label, |d, l| d.aria_label(l))
            .relative()
            .flex()
            .flex_col()
            .track_focus(&focus.tab_stop(true))
            .focus_visible(move |s| s.shadow(vec![crate::base::ring_inside(ring)]))
            .on_key_down({
                let mover = mover.clone();
                move |e: &KeyDownEvent, window, cx| {
                    if e.keystroke.modifiers.modified() {
                        return;
                    }
                    let delta = match e.keystroke.key.as_str() {
                        "left" => -1,
                        "right" => 1,
                        _ => return,
                    };
                    // Anywhere inside, as shadcn's: a field there takes its own arrows first.
                    if !mover.state.read(cx).focus.contains_focused(window, cx) {
                        return;
                    }
                    window.prevent_default();
                    cx.stop_propagation();
                    mover.by(delta, window, cx);
                }
            });
        gpui::Refineable::refine(outer.style(), &self.style);
        RoleDescription::new(
            outer
                .child(viewport)
                .children(buttons.into_iter().flatten())
                .children(dots),
            "carousel",
        )
    }
}

/// A trackpad's or a finger's scroll over the slides: sideways it drags them and, let go,
/// snaps; up and down it's left to the page.
fn gesture(mover: &Mover, e: &ScrollEvent, window: &mut Window, cx: &mut App) {
    // A mouse wheel's notches don't move it.
    if !e.delta.precise() {
        return;
    }
    let d = e.delta.pixel_delta(window.line_height());
    let state = &mover.state;
    let g = state.read(cx).gesture;
    match e.touch_phase {
        TouchPhase::Started => {
            state.update(cx, |s, _| s.gesture = Gesture::Undecided(d.x, d.y));
            decide(mover, window, cx);
        }
        TouchPhase::Moved => match g {
            Gesture::Undecided(x, y) => {
                state.update(cx, |s, _| s.gesture = Gesture::Undecided(x + d.x, y + d.y));
                decide(mover, window, cx);
                if matches!(state.read(cx).gesture, Gesture::Dragging(_)) {
                    cx.stop_propagation();
                }
            }
            Gesture::Dragging(x) => {
                state.update(cx, |s, _| s.gesture = Gesture::Dragging(x + d.x));
                cx.notify(mover.view);
                cx.stop_propagation();
            }
            // The momentum after a sideways swipe it has snapped already: not the page's.
            Gesture::Idle if d.x.abs() > d.y.abs() => cx.stop_propagation(),
            _ => {}
        },
        TouchPhase::Ended | TouchPhase::Cancelled => {
            let (index, step) = {
                let s = state.read(cx);
                (s.index, s.step)
            };
            match g {
                Gesture::Dragging(x) => {
                    // Let go, it snaps; taken by the system, it goes back.
                    let to = if e.touch_phase == TouchPhase::Ended {
                        let x = x + d.x;
                        state.update(cx, |s, _| s.gesture = Gesture::Dragging(x));
                        snap(index, x, step, mover.last)
                    } else {
                        index
                    };
                    mover.settle(to, window, cx);
                    cx.stop_propagation();
                }
                _ => state.update(cx, |s, _| s.gesture = Gesture::Idle),
            }
        }
    }
}

/// Settles an undecided gesture once it has gone far enough one way.
fn decide(mover: &Mover, _: &mut Window, cx: &mut App) {
    let Gesture::Undecided(x, y) = mover.state.read(cx).gesture else {
        return;
    };
    if x.abs() + y.abs() < px(DECIDE) {
        return;
    }
    let to = if x.abs() > y.abs() {
        Gesture::Dragging(x)
    } else {
        Gesture::Ignored
    };
    mover.state.update(cx, |s, _| s.gesture = to);
    cx.notify(mover.view);
}

/// An element described to assistive technology in words of its own (a carousel, a slide):
/// GPUI has no `aria_roledescription`, so this sets it on the element's node and draws it
/// unchanged.
pub(crate) struct RoleDescription<E> {
    inner: E,
    description: &'static str,
}

impl<E: Element> RoleDescription<E> {
    pub(crate) fn new(inner: E, description: &'static str) -> Self {
        Self { inner, description }
    }
}

impl<E: Element> IntoElement for RoleDescription<E> {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl<E: Element> Element for RoleDescription<E> {
    type RequestLayoutState = E::RequestLayoutState;
    type PrepaintState = E::PrepaintState;

    fn id(&self) -> Option<ElementId> {
        self.inner.id()
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        self.inner.source_location()
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.inner.request_layout(id, inspector, window, cx)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.inner
            .prepaint(id, inspector, bounds, layout, window, cx)
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.inner
            .paint(id, inspector, bounds, layout, prepaint, window, cx)
    }

    fn a11y_role(&self) -> Option<accesskit::Role> {
        self.inner.a11y_role()
    }

    fn write_a11y_info(&self, node: &mut accesskit::Node) {
        self.inner.write_a11y_info(node);
        node.set_role_description(self.description);
    }

    fn a11y_synthetic_children(
        &mut self,
        prepaint: &mut Self::PrepaintState,
        builder: &mut gpui::A11ySubtreeBuilder,
    ) {
        self.inner.a11y_synthetic_children(prepaint, builder);
    }
}

#[cfg(test)]
mod tests {
    use gpui::{InteractiveElement as _, Role, StatefulInteractiveElement as _, div};

    use super::RoleDescription;

    #[test]
    fn a_role_description_is_told_with_the_element_s_own() {
        let el = RoleDescription::new(
            div().id("c").role(Role::Region).aria_label("Photos"),
            "carousel",
        );
        let mut node = gpui::accesskit::Node::new(Role::Region);
        gpui::Element::write_a11y_info(&el, &mut node);
        assert_eq!(node.role_description(), Some("carousel"));
        assert_eq!(node.label(), Some("Photos"));
    }
}
