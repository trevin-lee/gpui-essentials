//! A slider: a value between a minimum and a maximum, in steps. Drag the thumb or press the
//! track; with the focus, the arrow keys step it, Page Up and Page Down take ten steps, Home and
//! End go to the ends.

use gpui::{
    App, Bounds, Context, DispatchPhase, ElementId, Entity, EventEmitter, FocusHandle, Hsla,
    InteractiveElement as _, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, ParentElement as _, Pixels, Rems, RenderOnce, Role,
    StatefulInteractiveElement as _, StyleRefinement, Styled, Window, canvas, div,
    prelude::FluentBuilder as _, relative, rems,
};

use crate::press::focus_ring;
use crate::size::Disableable;
use crate::theme::ActiveTheme as _;

/// What a slider tells its host.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SliderEvent {
    /// The value changed (while dragging, or by a key).
    Change(f32),
    /// A drag ended, or a key stepped it: the value to keep.
    Release(f32),
}

/// A slider's value and range. Make it an entity (`cx.new(|_| SliderState::new()…)`), draw it
/// with [`Slider`], and subscribe to its [`SliderEvent`]s.
pub struct SliderState {
    min: f32,
    max: f32,
    step: f32,
    value: f32,
    dragging: bool,
    track: Bounds<Pixels>,
    focus: Option<FocusHandle>,
}

impl Default for SliderState {
    fn default() -> Self {
        Self::new()
    }
}

impl SliderState {
    /// 0 to 100 in steps of 1, at 0.
    pub fn new() -> Self {
        Self {
            min: 0.,
            max: 100.,
            step: 1.,
            value: 0.,
            dragging: false,
            track: Bounds::default(),
            focus: None,
        }
    }

    pub fn min(mut self, min: f32) -> Self {
        self.min = min;
        self.value = self.fit(self.value);
        self
    }

    pub fn max(mut self, max: f32) -> Self {
        self.max = max;
        self.value = self.fit(self.value);
        self
    }

    /// The step (anything not finite or not above zero is taken as no step at all).
    pub fn step(mut self, step: f32) -> Self {
        self.step = step;
        self.value = self.fit(self.value);
        self
    }

    /// Its value to begin with.
    pub fn default_value(mut self, value: f32) -> Self {
        self.value = self.fit(value);
        self
    }

    pub fn value(&self) -> f32 {
        self.value
    }

    /// Sets the value (fitted to the range and step) without telling anyone.
    pub fn set_value(&mut self, value: f32, cx: &mut Context<Self>) {
        let v = self.fit(value);
        if v != self.value {
            self.value = v;
            cx.notify();
        }
    }

    fn range(&self) -> (f32, f32) {
        let (a, b) = (self.min, self.max);
        if !a.is_finite() || !b.is_finite() {
            return (0., 0.);
        }
        if a <= b { (a, b) } else { (b, a) }
    }

    /// `v` inside the range, on a step counted from the minimum.
    fn fit(&self, v: f32) -> f32 {
        let (lo, hi) = self.range();
        if !v.is_finite() {
            return lo;
        }
        let v = v.clamp(lo, hi);
        if !(self.step.is_finite() && self.step > 0.) {
            return v;
        }
        let n = ((v - lo) / self.step).round();
        // Rounded to the step's own precision, so 0.1 steps read 0.3, not 0.30000001.
        let snapped = lo + n * self.step;
        let digits = (-self.step.log10().floor()).clamp(0., 6.) as i32;
        let scale = 10f32.powi(digits);
        ((snapped * scale).round() / scale).clamp(lo, hi)
    }

    /// Where the value sits along the track, 0 to 1.
    fn fraction(&self) -> f32 {
        let (lo, hi) = self.range();
        if hi > lo {
            (self.value - lo) / (hi - lo)
        } else {
            0.
        }
    }

    fn change(&mut self, v: f32, cx: &mut Context<Self>) -> bool {
        let v = self.fit(v);
        if v == self.value {
            return false;
        }
        self.value = v;
        cx.emit(SliderEvent::Change(v));
        cx.notify();
        true
    }

    fn at_x(&mut self, x: Pixels, cx: &mut Context<Self>) {
        let width = self.track.size.width;
        if width <= Pixels::ZERO {
            return;
        }
        let f = ((x - self.track.origin.x) / width).clamp(0., 1.);
        let (lo, hi) = self.range();
        self.change(lo + f * (hi - lo), cx);
    }

    fn key(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        let (lo, hi) = self.range();
        let step = if self.step.is_finite() && self.step > 0. {
            self.step
        } else {
            (hi - lo) / 100.
        };
        let to = match key {
            "left" | "down" => self.value - step,
            "right" | "up" => self.value + step,
            "pageup" => self.value + step * 10.,
            "pagedown" => self.value - step * 10.,
            "home" => lo,
            "end" => hi,
            _ => return false,
        };
        if self.change(to, cx) {
            cx.emit(SliderEvent::Release(self.value));
        }
        true
    }
}

impl EventEmitter<SliderEvent> for SliderState {}

fn css(v: f32) -> Rems {
    rems(v / 15.)
}

/// Draws a [`SliderState`]: a track in the muted colour filled with the primary colour up to
/// the thumb. Give it a width; it's as tall as its thumb (or 44 points on a touch screen, with
/// [`touch`](Slider::touch)).
#[derive(IntoElement)]
pub struct Slider {
    state: Entity<SliderState>,
    disabled: bool,
    touch: bool,
    track: Option<Hsla>,
    fill: Option<Hsla>,
    thumb: Option<Hsla>,
    style: StyleRefinement,
}

impl Slider {
    pub fn new(state: &Entity<SliderState>) -> Self {
        Self {
            state: state.clone(),
            disabled: false,
            touch: false,
            track: None,
            fill: None,
            thumb: None,
            style: StyleRefinement::default(),
        }
    }

    /// The empty track's colour (otherwise the theme's input colour in the dark, its muted
    /// colour in the light), for a host with a palette of its own.
    pub fn track_color(mut self, color: impl Into<Hsla>) -> Self {
        self.track = Some(color.into());
        self
    }

    /// The filled track's colour, and the thumb's ring (otherwise the theme's primary).
    pub fn fill_color(mut self, color: impl Into<Hsla>) -> Self {
        self.fill = Some(color.into());
        self
    }

    /// The thumb's colour (otherwise the theme's background).
    pub fn thumb_color(mut self, color: impl Into<Hsla>) -> Self {
        self.thumb = Some(color.into());
        self
    }

    /// At least 44 points tall, for a finger.
    pub fn touch(mut self) -> Self {
        self.touch = true;
        self
    }
}

impl Disableable for Slider {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Styled for Slider {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Slider {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let state = self.state.clone();
        let focus = state.update(cx, |s, cx| {
            s.focus.get_or_insert_with(|| cx.focus_handle()).clone()
        });
        let (value, fraction, (lo, hi), step) = {
            let s = state.read(cx);
            (s.value, s.fraction(), s.range(), s.step)
        };
        let disabled = self.disabled;
        // The empty track: the input colour in the dark (muted is all but the page there), the
        // muted colour in the light.
        let track = self.track.unwrap_or(if theme.mode.is_dark() {
            theme.input
        } else {
            theme.muted
        });
        let fill_color = self.fill.unwrap_or(theme.primary);
        let thumb_color = self.thumb.unwrap_or(theme.background);
        let ring = focus_ring(cx);
        let thumb = css(16.);

        let mut el = div()
            .id(ElementId::Integer(state.entity_id().as_u64()))
            .role(Role::Slider)
            .aria_numeric_value(value as f64)
            .aria_min_numeric_value(lo as f64)
            .aria_max_numeric_value(hi as f64)
            .when(step.is_finite() && step > 0., |d| {
                d.aria_numeric_value_step(step as f64)
            })
            .relative()
            .flex()
            .items_center()
            .min_w_0()
            .h(thumb)
            .when(self.touch, |d| d.min_h(theme.touch_target))
            .when(disabled, |d| d.opacity(0.5));
        gpui::Refineable::refine(el.style(), &self.style);

        if !disabled {
            el = el
                .track_focus(&focus.tab_stop(true))
                // The ring goes round the whole slider, when the keyboard put the focus there.
                .rounded_full()
                .focus_visible(move |s| s.shadow(vec![ring]))
                .cursor_pointer()
                .on_key_down({
                    let state = state.clone();
                    move |e: &KeyDownEvent, window, cx| {
                        if e.keystroke.modifiers.modified() {
                            return;
                        }
                        let key = e.keystroke.key.clone();
                        if state.update(cx, |s, cx| s.key(&key, cx)) {
                            window.prevent_default();
                            cx.stop_propagation();
                        }
                    }
                })
                .on_mouse_down(MouseButton::Left, {
                    let state = state.clone();
                    move |e: &MouseDownEvent, window, cx| {
                        window.focus(&state.read(cx).focus.clone().expect("drawn"), cx);
                        state.update(cx, |s, cx| {
                            s.dragging = true;
                            s.at_x(e.position.x, cx);
                        });
                        cx.stop_propagation();
                    }
                });
        }

        // The track, and the thumb on it.
        el.child(
            div()
                .relative()
                .flex_1()
                .h(css(6.))
                .rounded_full()
                .bg(track)
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .w(relative(fraction))
                        .rounded_full()
                        .bg(fill_color),
                )
                .child(
                    div()
                        .absolute()
                        .top(css(-5.))
                        .left(relative(fraction))
                        .ml(css(-8.))
                        .size(thumb)
                        .rounded_full()
                        .border_1()
                        .border_color(fill_color)
                        .bg(thumb_color),
                )
                // Measures the track, and follows a drag past its edges until the button is let go.
                .child(
                    canvas(
                        {
                            let state = state.clone();
                            move |bounds, _, cx| {
                                state.update(cx, |s, _| s.track = bounds);
                            }
                        },
                        {
                            let state = state.clone();
                            move |_, _, window, cx| {
                                if !state.read(cx).dragging {
                                    return;
                                }
                                let s = state.clone();
                                window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| {
                                    if phase == DispatchPhase::Bubble
                                        && e.pressed_button == Some(MouseButton::Left)
                                    {
                                        s.update(cx, |s, cx| s.at_x(e.position.x, cx));
                                    }
                                });
                                let s = state.clone();
                                window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                                    if phase == DispatchPhase::Bubble {
                                        s.update(cx, |s, cx| {
                                            if s.dragging {
                                                s.dragging = false;
                                                cx.emit(SliderEvent::Release(s.value));
                                                cx.notify();
                                            }
                                        });
                                    }
                                });
                            }
                        },
                    )
                    .absolute()
                    .size_full(),
                ),
        )
    }
}
