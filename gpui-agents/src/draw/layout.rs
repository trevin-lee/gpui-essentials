//! Containers: Stack and Card, Tabs, Accordion, Steps, Carousel, Separator and Modal.

use std::time::Duration;

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_core::{Disableable as _, ScrollExt as _};

use super::content::Glyph;
use super::{Draw, pressable};
use crate::openui::{
    Align, Component, Direction, Flex, Justify, Modal, Section, Size, StepItem, Surface,
};
use crate::view::Swipe;

/// How far a finger pans a carousel's slide before letting go turns it, in window pixels.
const SWIPE_TURN: f32 = 40.;

/// Where a slide dragged `x` pixels is drawn: with the finger, or a third as far past the
/// first or last slide, where there's nothing to turn to.
fn shown(x: f32, index: usize, count: usize) -> f32 {
    if (x > 0. && index == 0) || (x < 0. && index + 1 >= count) {
        x / 3.
    } else {
        x
    }
}

/// One step of a finger's pan over a carousel of `count` slides: `d` is how far it moved
/// (window pixels; GPUI locks a pan to the way it started, so one of them is 0). A sideways pan
/// drags the slide; let go past [`SWIPE_TURN`], it turns to the next (panned left) or the
/// previous one, else it eases back. Returns whether the step was the carousel's, the drag or
/// the momentum after it, which then goes no further.
pub(crate) fn swipe(
    s: &mut crate::view::Slide,
    count: usize,
    phase: TouchPhase,
    d: Point<f32>,
) -> bool {
    let sideways = d.x != 0. && d.y == 0.;
    let moved = d.x != 0. || d.y != 0.;
    match phase {
        TouchPhase::Started => {
            s.swipe = if !moved {
                Swipe::Fresh
            } else if sideways {
                Swipe::Dragging(d.x)
            } else {
                Swipe::Ignoring
            };
        }
        TouchPhase::Moved => match s.swipe {
            Swipe::Fresh if sideways => s.swipe = Swipe::Dragging(d.x),
            Swipe::Fresh if moved => s.swipe = Swipe::Ignoring,
            Swipe::Dragging(x) => s.swipe = Swipe::Dragging(x + d.x),
            _ => {}
        },
        TouchPhase::Ended | TouchPhase::Cancelled => {
            let ended = phase == TouchPhase::Ended;
            match std::mem::take(&mut s.swipe) {
                Swipe::Dragging(x) => {
                    let x = if ended { x + d.x } else { x };
                    let index = s.index.min(count.saturating_sub(1));
                    let to = if !ended {
                        None
                    } else if x <= -SWIPE_TURN && index + 1 < count {
                        Some(index + 1)
                    } else if x >= SWIPE_TURN && index > 0 {
                        Some(index - 1)
                    } else {
                        None
                    };
                    s.from = index;
                    s.index = to.unwrap_or(index);
                    s.back = if to.is_some() {
                        0.
                    } else {
                        shown(x, index, count)
                    };
                    if to.is_some() || s.back != 0. {
                        s.moves += 1;
                    }
                    if ended {
                        s.swipe = Swipe::Coasting;
                    }
                    return true;
                }
                Swipe::Coasting => return true,
                _ => return false,
            }
        }
    }
    matches!(s.swipe, Swipe::Dragging(_) | Swipe::Coasting)
}

/// OpenUI's easing for its accordion (cubic-bezier(0.87, 0, 0.13, 1)), a steep ease-in-out.
pub(crate) fn steep(t: f32) -> f32 {
    if t < 0.5 {
        16. * t.powi(5)
    } else {
        1. - (-2. * t + 2.).powi(5) / 2.
    }
}

impl Draw<'_, '_> {
    /// Stack and Card: children in a column or a row, with OpenUI's gaps and alignment. In a
    /// row that doesn't wrap the children share the width; in one that wraps each takes what
    /// it needs, up to the whole width.
    pub fn flex(&mut self, f: &Flex, path: &str, card: bool) -> AnyElement {
        let t = self.t;
        let row = f.direction == Direction::Row;
        let mut el = div()
            .w_full()
            .min_w_0()
            .flex()
            .gap(self.px(self.gap(f.gap)));
        el = if row { el.flex_row() } else { el.flex_col() };
        if row && f.wrap {
            el = el.flex_wrap();
        }
        let align = f
            .align
            .unwrap_or(if row { Align::Start } else { Align::Stretch });
        el = match align {
            Align::Start => el.items_start(),
            Align::Center => el.items_center(),
            Align::End => el.items_end(),
            Align::Baseline => el.items_baseline(),
            Align::Stretch => el.items_stretch(),
        };
        el = match f.justify.unwrap_or(Justify::Start) {
            Justify::Start => el.justify_start(),
            Justify::Center => el.justify_center(),
            Justify::End => el.justify_end(),
            Justify::Between => el.justify_between(),
            Justify::Around => el.justify_around(),
            Justify::Evenly => el.justify_evenly(),
        };
        for (i, c) in f.children.iter().enumerate() {
            let child = self.component(c, &format!("{path}.c{i}"));
            // Each child in a column of its own, at its own width (most take the whole).
            let cell = div().min_w_0().flex().flex_col().items_start().child(child);
            let cell = if row && f.wrap {
                cell.flex_auto().max_w_full()
            } else if row {
                cell.flex_1()
            } else if align == Align::Stretch {
                cell.w_full()
            } else {
                cell.max_w_full()
            };
            el = el.child(cell);
        }
        if !card {
            return el.into_any_element();
        }
        // A card is a plain column with a transparent edge; sunk is recessed and padded; clear
        // is padded at the sides.
        let el = el.border_1().border_color(transparent_black());
        match f.surface {
            Surface::Card => el,
            Surface::Sunk => el
                .p(self.px(t.space.card))
                .rounded(self.px(t.radii.card))
                .bg(t.colors.sunk)
                .border_color(t.colors.line),
            Surface::Clear => el.px(self.px(t.space.card)).rounded(self.px(t.radii.card)),
        }
        .into_any_element()
    }

    /// A strip of triggers in a recessed track, the chosen one raised, its content below.
    pub fn tabs(&mut self, sections: &[Section], path: &str) -> AnyElement {
        let t = self.t;
        if sections.is_empty() {
            return div().into_any_element();
        }
        let chosen = self
            .v
            .ui
            .tabs
            .get(path)
            .copied()
            .unwrap_or(0)
            .min(sections.len() - 1);
        let mut strip = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_row()
            .gap(self.px(2.))
            .p(self.px(4.))
            .rounded(self.px(t.radii.lg))
            .bg(t.colors.sunk);
        for (i, s) in sections.iter().enumerate() {
            let on = i == chosen;
            let key = path.to_string();
            let label = self.line(
                s.title.clone(),
                t.text.md,
                if on {
                    t.colors.text
                } else {
                    t.colors.text_muted
                },
                if on { t.text.medium } else { self.regular() },
            );
            let choose = self.cx.listener(move |this, _, _, cx| {
                this.ui.tabs.insert(key.clone(), i);
                cx.notify();
            });
            let trigger = pressable(self.id(&format!("{path}.tab{i}")), self.t)
                .role(Role::Tab)
                .accessibility_label(s.title.clone())
                .focus_on_press(false)
                .flex_1()
                .min_w_0()
                .h(self.px(30.))
                .px(self.px(8.))
                .rounded(self.px(t.radii.sm))
                .border_1()
                .border_color(if on {
                    t.colors.line
                } else {
                    transparent_black()
                })
                .when(on, |b| b.bg(t.colors.raised))
                .when(!on, |b| b.hover(|s| s.bg(t.colors.sunk)))
                .debug_selector({
                    let sel = self.selector(&format!("tab:{}", s.title));
                    move || sel
                })
                .child(label);
            strip = strip.child(self.on_press(trigger, choose));
        }
        let content = self
            .column(
                &sections[chosen].content,
                path,
                &format!("s{chosen}."),
                t.space.l,
            )
            .p(self.px(t.space.l));
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(self.px(t.space.s))
            .child(strip)
            .child(content)
            .into_any_element()
    }

    /// Sections that open and close, each with a chevron that turns as it does; opening fades
    /// the content in. One is open at a time, as react-ui's (`type="single" collapsible`):
    /// opening one closes the other, and pressing the open one closes it. Until someone
    /// presses one, the newest section is open, as react-ui opens each that arrives.
    pub fn accordion(&mut self, sections: &[Section], path: &str) -> AnyElement {
        let t = self.t;
        let open = self.v.ui.accordion.get(path);
        let newest = sections.len().checked_sub(1);
        let mut list = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .rounded(self.px(14.))
            .border_1()
            .border_color(t.colors.line)
            .px(self.px(t.space.m + 2.));
        let n = sections.len();
        for (i, s) in sections.iter().enumerate() {
            let is_open = match open {
                Some(o) => o.contains(&i),
                None => newest == Some(i),
            };
            let item_key = format!("{path}.s{i}");
            let moves = self.v.ui.toggles.get(&item_key).copied().unwrap_or(0);
            let key = path.to_string();
            let (from, to) = if is_open {
                (0., std::f32::consts::PI)
            } else {
                (std::f32::consts::PI, 0.)
            };
            let chevron = self.glyph(Glyph::ChevronDown, 16., t.colors.text);
            let chevron = if moves == 0 {
                chevron.turned(to).into_any_element()
            } else {
                chevron
                    .with_animation(
                        self.id(&format!("{item_key}.turn{moves}")),
                        Animation::new(Duration::from_millis(300)).with_easing(steep),
                        move |g, d| g.turned(from + (to - from) * d),
                    )
                    .into_any_element()
            };
            let trigger = pressable(self.id(&format!("{item_key}.trigger")), self.t)
                .accessibility_label(s.title.clone())
                .focus_on_press(false)
                .w_full()
                .min_w_0()
                .justify_between()
                .gap(self.px(t.space.s))
                .py(self.px(t.space.l))
                .when(i == 0, |b| b.pt(self.px(t.space.m + 2.)))
                .debug_selector({
                    let sel = self.selector(&format!("section:{}", s.title));
                    move || sel
                })
                .child(
                    self.text(s.title.clone(), t.text.md, t.colors.text, self.regular())
                        .flex_1(),
                )
                .child(chevron)
                .on_click(self.cx.listener(move |this, _, _, cx| {
                    let set = this
                        .ui
                        .accordion
                        .entry(key.clone())
                        .or_insert_with(|| newest.into_iter().collect());
                    let was_open = set.contains(&i);
                    // Whatever was open closes (its chevron turns back too).
                    let closing: Vec<usize> = set.drain().filter(|&j| j != i).collect();
                    if !was_open {
                        set.insert(i);
                    }
                    for j in closing.into_iter().chain([i]) {
                        *this.ui.toggles.entry(format!("{key}.s{j}")).or_default() += 1;
                    }
                    cx.notify();
                }));
            let mut item = div().w_full().min_w_0().flex().flex_col().child(trigger);
            if i + 1 < n {
                item = item.border_b_1().border_color(t.colors.line);
            }
            if is_open {
                let sel = self.selector(&format!("open:{}", s.title));
                let content = self
                    .column(&s.content, path, &format!("s{i}."), t.space.l)
                    .pb(self.px(t.space.m))
                    .debug_selector(move || sel);
                let content = if moves == 0 {
                    content.into_any_element()
                } else {
                    content
                        .with_animation(
                            self.id(&format!("{item_key}.reveal{moves}")),
                            Animation::new(Duration::from_millis(220))
                                .with_easing(ease_out_quint()),
                            |c, d| c.opacity(d).mt(px(-6. * (1. - d))),
                        )
                        .into_any_element()
                };
                item = item.child(content);
            }
            list = list.child(item);
        }
        list.into_any_element()
    }

    /// Numbered circles joined by a line, each with its title and details. The details are
    /// Markdown, as react-ui draws them (its `MarkDownRenderer`).
    pub fn steps(&mut self, steps: &[StepItem], path: &str) -> AnyElement {
        let t = self.t;
        let mut col = div().w_full().min_w_0().flex().flex_col();
        let n = steps.len();
        for (i, s) in steps.iter().enumerate() {
            let last = i + 1 == n;
            let number = div()
                .flex_none()
                .size(self.px(24.))
                .rounded_full()
                .bg(t.colors.raised)
                .border_1()
                .border_color(t.colors.line)
                .flex()
                .items_center()
                .justify_center()
                .child(self.line(
                    (i + 1).to_string(),
                    t.text.sm,
                    t.colors.text_muted,
                    self.regular(),
                ));
            let rail = div()
                .flex_none()
                .flex()
                .flex_col()
                .items_center()
                .child(div().py(self.px(4.)).child(number))
                .when(!last, |d| {
                    d.child(div().w(px(1.)).flex_1().bg(t.colors.raised))
                });
            let body = div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(self.px(2.))
                .pt(self.px(4.))
                .when(!last, |d| d.pb(self.px(t.space.l)))
                .child(self.text(
                    s.title.clone(),
                    t.text.md,
                    t.colors.text_muted,
                    self.regular(),
                ))
                .child(
                    self.markdown(
                        &s.details,
                        t.text.md,
                        self.regular(),
                        &format!("{path}.s{i}.d"),
                    )
                    .w_full()
                    .text_color(t.colors.text),
                );
            col = col.child(
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_row()
                    .gap(self.px(t.space.m))
                    .child(rail)
                    .child(body),
            );
        }
        col.into_any_element()
    }

    /// Slides one at a time with arrows and a count, sliding across when it changes.
    pub fn carousel(
        &mut self,
        slides: &[Vec<Component>],
        surface: Surface,
        path: &str,
    ) -> AnyElement {
        let t = self.t;
        if slides.is_empty() {
            return div().into_any_element();
        }
        let slide = self.v.ui.carousel.get(path).copied().unwrap_or_default();
        let index = slide.index.min(slides.len() - 1);
        let sel = self.selector(&format!("{path}:slide{index}"));
        let body = self
            .column(&slides[index], path, &format!("s{index}."), t.space.m)
            .debug_selector(move || sel);
        let card = div()
            .w_full()
            .min_w_0()
            .p(self.px(t.space.m))
            .rounded(self.px(14.))
            .border_1()
            .border_color(t.colors.line)
            .when(surface == Surface::Sunk, |d| d.bg(t.colors.highlight))
            .child(body);
        let count = slides.len();
        let moving = match slide.swipe {
            // A finger has it: it follows, held back past the first or last slide.
            Swipe::Dragging(x) => card
                .relative()
                .left(px(shown(x, index, count)))
                .into_any_element(),
            _ => self.slide_in(card, path, slide, index),
        };
        let key = path.to_string();
        let swiped = self.cx.listener(move |this, e: &ScrollEvent, _, cx| {
            if e.source != ScrollSource::Touch {
                return;
            }
            let d = e.delta.pixel_delta(px(1.));
            let s = this.ui.carousel.entry(key.clone()).or_default();
            let before = s.swipe;
            if swipe(
                s,
                count,
                e.touch_phase,
                point(f32::from(d.x), f32::from(d.y)),
            ) {
                cx.stop_propagation();
            }
            if s.swipe != before {
                cx.notify();
            }
        });
        let clip = self
            .sideways(
                div()
                    .id(self.id(&format!("{path}.clip")))
                    .w_full()
                    .min_w_0()
                    .overflow_hidden(),
                &format!("{path}.clip"),
            )
            .on_scroll(swiped)
            .child(moving);
        let controls = self.pager(path, index, slides.len(), false);
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(self.px(t.space.s))
            .child(clip)
            .when(slides.len() > 1, |d| d.child(controls))
            .into_any_element()
    }

    /// A slide moving in from the side it came from, eased to rest.
    pub fn slide_in(
        &self,
        el: Div,
        path: &str,
        slide: crate::view::Slide,
        index: usize,
    ) -> AnyElement {
        if slide.moves == 0 {
            return el.into_any_element();
        }
        if slide.back != 0. {
            // A swipe that didn't turn it: back from where the finger let go.
            let back = px(slide.back);
            return el
                .with_animation(
                    self.id(&format!("{path}.slide{}", slide.moves)),
                    Animation::new(Duration::from_millis(220)).with_easing(ease_out_quint()),
                    move |el, d| el.relative().left(back * (1. - d)),
                )
                .into_any_element();
        }
        let dir = if index >= slide.from { 1. } else { -1. };
        let shift = self.pixels(48.);
        el.with_animation(
            self.id(&format!("{path}.slide{}", slide.moves)),
            Animation::new(Duration::from_millis(260)).with_easing(ease_out_quint()),
            move |el, d| {
                el.relative()
                    .left(shift * dir * (1. - d))
                    .opacity(0.35 + 0.65 * d)
            },
        )
        .into_any_element()
    }

    /// Previous and next, with "2 / 5" between: a carousel's, centred under its slide, or a
    /// table's pages, at its right as react-ui has them (named "Previous page" and "Next page").
    pub fn pager(&mut self, path: &str, index: usize, count: usize, pages: bool) -> AnyElement {
        let t = self.t;
        let arrow = |d: &mut Self, glyph: Glyph, to: usize, enabled: bool, name: &str| {
            let label = match (pages, name) {
                (true, "previous") => "Previous page",
                (true, _) => "Next page",
                (false, _) => name,
            };
            let key = path.to_string();
            let turn = d.cx.listener(move |this, _, _, cx| {
                let s = this.ui.carousel.entry(key.clone()).or_default();
                s.from = s.index;
                s.index = to;
                s.back = 0.;
                s.moves += 1;
                cx.notify();
            });
            let press = pressable(d.id(&format!("{path}.{name}")), d.t)
                .accessibility_label(label.to_string())
                .disabled(!enabled)
                .focus_on_press(false)
                .flex_none()
                .size(d.px(28.))
                .rounded(d.px(t.radii.sm))
                .border_1()
                .border_color(t.colors.line_strong)
                .when(enabled, |b| b.hover(|s| s.bg(t.colors.highlight)))
                .debug_selector({
                    let sel = d.selector(&format!("{path}:{name}"));
                    move || sel
                })
                .child(d.glyph(
                    glyph,
                    14.,
                    if enabled {
                        t.colors.text
                    } else {
                        t.colors.text_faint
                    },
                ));
            d.on_press(press, turn)
        };
        let prev = arrow(
            self,
            Glyph::ChevronLeft,
            index.saturating_sub(1),
            index > 0,
            "previous",
        );
        let next = arrow(
            self,
            Glyph::ChevronRight,
            (index + 1).min(count - 1),
            index + 1 < count,
            "next",
        );
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_row()
            .items_center()
            .map(|d| {
                if pages {
                    d.justify_end().gap(self.px(8.))
                } else {
                    d.justify_center().gap(self.px(t.space.m))
                }
            })
            .child(prev)
            .child(
                self.line(
                    format!("{} / {count}", index + 1),
                    t.text.sm,
                    t.colors.text_muted,
                    self.regular(),
                )
                .flex_none(),
            )
            .child(next)
            .into_any_element()
    }

    pub fn separator(&self, vertical: bool) -> AnyElement {
        let c = self.t.colors.line;
        if vertical {
            div().w(px(1.)).h_full().min_h(self.px(16.)).bg(c)
        } else {
            div().h(px(1.)).w_full().bg(c)
        }
        .into_any_element()
    }

    /// A dialog over the window while its `open` is true; the close button, a press outside
    /// and Escape close it.
    pub fn modal(&mut self, m: &Modal, path: &str) -> AnyElement {
        let t = self.t;
        if !self.v.modal_open(path, &m.open) {
            return div().into_any_element();
        }
        let width = match m.size {
            Size::Sm | Size::Small | Size::S | Size::Xs | Size::ExtraSmall => 400.,
            Size::Lg | Size::Large | Size::L | Size::Xl => 720.,
            _ => 560.,
        };
        let focus = self
            .window
            .use_keyed_state(self.id(&format!("{path}.focus")), self.cx, |_, cx| {
                cx.focus_handle()
            })
            .read(self.cx)
            .clone();
        if !focus.contains_focused(self.window, self.cx) {
            focus.focus(self.window, self.cx);
        }
        let viewport = self.window.viewport_size();
        let (key, open) = (path.to_string(), m.open.clone());
        let close_on_button = self
            .cx
            .listener(move |this, _: &ClickEvent, _, cx| this.close_modal(&key, &open, cx));
        let (key, open) = (path.to_string(), m.open.clone());
        let close_outside = self
            .cx
            .listener(move |this, _: &ClickEvent, _, cx| this.close_modal(&key, &open, cx));
        let close_button = pressable(self.id(&format!("{path}.close")), self.t)
            .accessibility_label("Close")
            .flex_none()
            .size(self.px(28.))
            .rounded(self.px(t.radii.sm))
            .hover(|s| s.bg(t.colors.highlight))
            .debug_selector({
                let sel = self.selector(&format!("close:{}", m.title));
                move || sel
            })
            .child(self.glyph(Glyph::Close, 14., t.colors.text_muted));
        let close_button = self.on_press(close_button, close_on_button);
        let (esc_key, esc_open) = (path.to_string(), m.open.clone());
        let panel = div()
            .id(self.id(&format!("{path}.panel")))
            .track_focus(&focus)
            .on_key_down(self.cx.listener(move |this, e: &KeyDownEvent, _, cx| {
                if e.keystroke.key == "escape" {
                    this.close_modal(&esc_key, &esc_open, cx);
                }
            }))
            .occlude()
            .on_click(|_, _, cx| cx.stop_propagation())
            .w(self.px(width))
            .max_w(viewport.width * 0.92)
            .max_h(viewport.height * 0.9)
            .min_w_0()
            .scroll_y()
            .flex()
            .flex_col()
            .gap(self.px(t.space.m))
            .p(self.px(t.space.xl))
            .rounded(self.px(t.radii.card))
            .bg(t.colors.popover)
            .border_1()
            .border_color(t.colors.line)
            .shadow_lg()
            .text_color(t.colors.text)
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_row()
                    .items_start()
                    .gap(self.px(t.space.s))
                    .child(
                        self.text(
                            m.title.clone(),
                            t.text.heading,
                            t.colors.text,
                            t.text.semibold,
                        )
                        .flex_1(),
                    )
                    .child(close_button),
            )
            .child(self.column(&m.children, path, "c", t.space.m));
        let backdrop = div()
            .id(self.id(&format!("{path}.backdrop")))
            .size_full()
            .w(viewport.width)
            .h(viewport.height)
            .bg(t.colors.overlay)
            .flex()
            .items_center()
            .justify_center()
            .font_family(t.fonts.body.clone())
            .text_size(self.px(t.text.md))
            .line_height(self.px(t.text.md * t.text.body_leading))
            .on_click(close_outside)
            .child(panel);
        deferred(
            anchored()
                .position_mode(AnchoredPositionMode::Window)
                .position(point(px(0.), px(0.)))
                .child(backdrop),
        )
        .with_priority(2)
        .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::Slide;
    // The built-in #[test], not GPUI's, which gpui::* brings in.
    use std::prelude::v1::test;

    fn step(s: &mut Slide, phase: TouchPhase, x: f32, y: f32) -> bool {
        swipe(s, 3, phase, point(x, y))
    }

    #[test]
    fn a_swipe_keeps_its_momentum_from_the_chat_and_then_lets_go() {
        let mut s = Slide::default();
        assert!(
            !step(&mut s, TouchPhase::Started, 0., 0.),
            "a finger down is no one's yet"
        );
        assert!(step(&mut s, TouchPhase::Moved, -30., 0.));
        assert!(step(&mut s, TouchPhase::Moved, -30., 0.));
        assert_eq!(s.swipe, Swipe::Dragging(-60.));
        assert!(step(&mut s, TouchPhase::Ended, 0., 0.));
        assert_eq!((s.index, s.from, s.moves), (1, 0, 1));
        // GPUI's momentum after the release: still the carousel's, and it turns nothing more.
        assert!(step(&mut s, TouchPhase::Moved, -40., 0.));
        assert!(step(&mut s, TouchPhase::Ended, -5., 0.));
        assert_eq!((s.index, s.swipe), (1, Swipe::Idle));
        // An up-and-down pan is never the carousel's.
        assert!(!step(&mut s, TouchPhase::Started, 0., -12.));
        assert!(!step(&mut s, TouchPhase::Moved, 0., -40.));
        assert!(!step(&mut s, TouchPhase::Ended, 0., 0.));
        assert_eq!(s.index, 1);
    }

    #[test]
    fn a_cancelled_or_short_swipe_eases_back_from_where_it_was() {
        let mut s = Slide::default();
        step(&mut s, TouchPhase::Started, -12., 0.);
        step(&mut s, TouchPhase::Moved, -50., 0.);
        assert!(step(&mut s, TouchPhase::Cancelled, 0., 0.));
        assert_eq!((s.index, s.back, s.swipe), (0, -62., Swipe::Idle));
        step(&mut s, TouchPhase::Started, -12., 0.);
        step(&mut s, TouchPhase::Ended, -8., 0.);
        assert_eq!((s.index, s.back), (0, -20.));
        // Past the first slide it's held back to a third, and eases back from there.
        step(&mut s, TouchPhase::Started, 30., 0.);
        step(&mut s, TouchPhase::Ended, 30., 0.);
        assert_eq!((s.index, s.back), (0, 20.));
    }
}
