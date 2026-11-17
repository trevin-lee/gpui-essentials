//! shadcn's pie chart, and its donut: each value a slice, sized by its share of the total.
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::PieChart;
//! # struct Browsers;
//! # impl Render for Browsers {
//! fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
//!     PieChart::new("browsers")
//!         .slice("chrome", "Chrome", 275.)
//!         .slice("safari", "Safari", 200.)
//!         .slice("firefox", "Firefox", 187.)
//!         .donut(true)
//!         .center_label("662", "Visitors")
//! }
//! # }
//! ```

use std::f32::consts::TAU;
use std::rc::Rc;

use gpui::{
    App, Bounds, ElementId, FontWeight, Hsla, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, MouseMoveEvent, ParentElement, Pixels, Point, RenderOnce, ShapedLine,
    SharedString, StatefulInteractiveElement as _, StyleRefinement, Styled, TextAlign,
    TruncateFrom, Window, canvas, deferred, div, point, prelude::FluentBuilder as _, px, rems,
};

use super::{
    Options, Paint, PathBuilder, chart_state, css, empty_state, frame, keyboard, legend,
    tooltip_card, tooltip_row, trim,
};
use crate::theme::ActiveTheme as _;

/// A slice as drawn: which value it shows, its share of the total, and the angles it spans, in
/// radians counterclockwise from three o'clock (where Recharts starts a pie). Zero, negative
/// and missing values have no slice.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PieSlice {
    /// The value's place among those the host gave.
    pub index: usize,
    pub value: f64,
    /// Its share of the total, 0 to 1.
    pub fraction: f64,
    pub start: f32,
    pub end: f32,
}

impl PieSlice {
    /// The angle halfway round it.
    pub fn middle(&self) -> f32 {
        (self.start + self.end) / 2.
    }
}

/// The slices for `values`, in order round the circle from three o'clock, counterclockwise,
/// each as wide as its share of the total. Only values above zero get one.
pub fn slice_angles(values: &[f64]) -> Vec<PieSlice> {
    let shown = |v: f64| v.is_finite() && v > 0.;
    let total: f64 = values.iter().copied().filter(|v| shown(*v)).sum();
    if !(total.is_finite() && total > 0.) {
        return Vec::new();
    }
    let mut at = 0f64;
    values
        .iter()
        .enumerate()
        .filter(|(_, v)| shown(**v))
        .map(|(index, &value)| {
            let fraction = value / total;
            let start = at;
            at += fraction;
            PieSlice {
                index,
                value,
                fraction,
                start: (start * TAU as f64) as f32,
                end: (at * TAU as f64) as f32,
            }
        })
        .collect()
}

/// Which of `slices` (its place in that list) holds `p`, in a ring around `centre` from
/// `inner` (0 for a pie) to `outer`. None in the hole, outside the circle, or without slices.
pub fn slice_at(
    slices: &[PieSlice],
    centre: Point<Pixels>,
    inner: Pixels,
    outer: Pixels,
    p: Point<Pixels>,
) -> Option<usize> {
    let (dx, dy) = (f32::from(p.x - centre.x), f32::from(centre.y - p.y));
    let r = (dx * dx + dy * dy).sqrt();
    if r > f32::from(outer) || r < f32::from(inner) {
        return None;
    }
    let angle = dy.atan2(dx).rem_euclid(TAU);
    slices
        .iter()
        .position(|s| angle >= s.start && angle < s.end)
        // The last slice ends at a full turn; rounding can leave a sliver past its end.
        .or_else(|| {
            let last = slices.len().checked_sub(1)?;
            (angle >= slices[last].start).then_some(last)
        })
}

/// The point at `angle` (counterclockwise from three o'clock) and distance `r` from `centre`.
fn polar(centre: Point<Pixels>, r: Pixels, angle: f32) -> Point<Pixels> {
    point(centre.x + r * angle.cos(), centre.y - r * angle.sin())
}

/// A slice's outline: its outer arc, then its inner arc back (or the centre, for a pie).
fn sector(
    centre: Point<Pixels>,
    inner: Pixels,
    outer: Pixels,
    start: f32,
    end: f32,
) -> Option<gpui::Path<Pixels>> {
    // A point every two degrees or so: a smooth arc at any size a chart is drawn.
    let steps = ((end - start) / TAU * 180.).ceil().max(2.) as usize;
    let at =
        |r: Pixels, k: usize| polar(centre, r, start + (end - start) * k as f32 / steps as f32);
    let mut b = PathBuilder::fill();
    b.move_to(at(outer, 0));
    for k in 1..=steps {
        b.line_to(at(outer, k));
    }
    if inner > px(0.) {
        for k in (0..=steps).rev() {
            b.line_to(at(inner, k));
        }
    } else {
        b.line_to(centre);
    }
    b.close();
    b.build().ok()
}

/// A pie's centre and its inner and outer radii in `bounds`.
fn radii(donut: bool, bounds: Bounds<Pixels>) -> (Point<Pixels>, Pixels, Pixels) {
    let outer = bounds.size.width.min(bounds.size.height) / 2. * 0.8;
    let inner = if donut { outer * 0.6 } else { px(0.) };
    (bounds.center(), inner, outer)
}

/// `fraction` as a percentage with at most one decimal ("24.4%", "50%").
fn percent(fraction: f64) -> String {
    format!("{}%", trim(&format!("{:.1}", fraction * 100.)))
}

struct Slice {
    key: SharedString,
    label: SharedString,
    value: f64,
    color: Option<Hsla>,
}

/// shadcn's pie chart: each value a slice, from three o'clock counterclockwise as Recharts
/// draws it, coloured with the theme's `chart_1` to `chart_5` in turn unless given colours, or
/// a [`donut`](Self::donut) with an optional [`center_label`](Self::center_label) in its hole.
/// Zero and negative values are left out. The pointer, a tap, or Left and Right show a slice's
/// label, value and share in a tooltip, and push the slice out a little. Full width and 250
/// pixels tall unless styled otherwise; the pie fills 80% of the shorter side.
#[derive(IntoElement)]
pub struct PieChart {
    id: ElementId,
    slices: Vec<Slice>,
    donut: bool,
    center: Option<(SharedString, SharedString)>,
    options: Options,
    style: StyleRefinement,
}

impl PieChart {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            slices: Vec::new(),
            donut: false,
            center: None,
            options: Options::default(),
            style: StyleRefinement::default(),
        }
    }

    /// A slice: `key` names it for [`slice_color`](Self::slice_color), `label` is what the
    /// legend and the tooltip call it. Only a value above zero is drawn.
    pub fn slice(
        mut self,
        key: impl Into<SharedString>,
        label: impl Into<SharedString>,
        value: impl Into<f64>,
    ) -> Self {
        self.slices.push(Slice {
            key: key.into(),
            label: label.into(),
            value: value.into(),
            color: None,
        });
        self
    }

    /// The colour of the slice `key` (otherwise the theme's chart colour for its place).
    pub fn slice_color(mut self, key: impl Into<SharedString>, color: impl Into<Hsla>) -> Self {
        let key = key.into();
        let color = color.into();
        for s in self.slices.iter_mut().filter(|s| s.key == key) {
            s.color = Some(color);
        }
        self
    }

    /// A ring rather than a disc: the hole is 60% of the radius, as shadcn's donut.
    pub fn donut(mut self, donut: bool) -> Self {
        self.donut = donut;
        self
    }

    /// What a donut shows in its hole: `value` large and bold (a total), `caption` small under
    /// it ("Visitors"). Each is cut short with an ellipsis to fit. A pie has no hole for it.
    pub fn center_label(
        mut self,
        value: impl Into<SharedString>,
        caption: impl Into<SharedString>,
    ) -> Self {
        self.center = Some((value.into(), caption.into()));
        self
    }

    /// How it writes a value in the tooltip and to assistive technology (in full, `1,234`,
    /// unless told otherwise).
    pub fn format_value(mut self, format: impl Fn(f64) -> SharedString + 'static) -> Self {
        self.options.format = Some(Rc::new(format));
        self
    }

    /// Whether it shows a legend under the pie: each slice's colour and label.
    pub fn legend(mut self, show: bool) -> Self {
        self.options.legend = show;
        self
    }

    /// Whether the pointer, a tap or the keyboard shows a tooltip for a slice (it does unless
    /// told not to; the slice is still pushed out without one).
    pub fn tooltip(mut self, show: bool) -> Self {
        self.options.tooltip = show;
        self
    }

    /// What it shows, to assistive technology (otherwise a summary: every slice's label, value
    /// and share, and the centre label).
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.options.label = Some(label.into());
        self
    }

    /// What it says when there's nothing to draw ("No data" unless told otherwise).
    pub fn empty_text(mut self, text: impl Into<SharedString>) -> Self {
        self.options.empty_text = text.into();
        self
    }

    /// The slices as drawn: only values above zero, in order.
    pub fn angles(&self) -> Vec<PieSlice> {
        let values: Vec<f64> = self.slices.iter().map(|s| s.value).collect();
        slice_angles(&values)
    }

    /// The pie's centre and its inner (0 for a pie) and outer radii in `bounds`: Recharts'
    /// 80% of the shorter side's half, the hole 60% of that.
    pub fn radii(&self, bounds: Bounds<Pixels>) -> (Point<Pixels>, Pixels, Pixels) {
        radii(self.donut, bounds)
    }

    /// Which slice (its place in [`angles`](Self::angles)) holds `p` in a pie drawn in `bounds`.
    pub fn slice_at(&self, bounds: Bounds<Pixels>, p: Point<Pixels>) -> Option<usize> {
        let (centre, inner, outer) = self.radii(bounds);
        slice_at(&self.angles(), centre, inner, outer, p)
    }

    /// What it tells assistive technology it shows, unless given a label.
    pub fn accessibility_summary(&self) -> String {
        let kind = if self.donut {
            "Donut chart"
        } else {
            "Pie chart"
        };
        let slices = self.angles();
        if slices.is_empty() {
            return format!("{kind}, no data");
        }
        let parts: Vec<String> = slices.iter().map(|s| self.describe(s)).collect();
        let mut out = format!("{kind}: {}", parts.join("; "));
        if let Some((value, caption)) = &self.center
            && self.donut
        {
            out.push_str(&format!(". {value} {caption}"));
        }
        out
    }

    /// A slice in words: "Chrome 275, 24.4%".
    fn describe(&self, s: &PieSlice) -> String {
        format!(
            "{} {}, {}",
            self.slices[s.index].label,
            self.options.value_text(s.value),
            percent(s.fraction)
        )
    }
}

impl Styled for PieChart {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for PieChart {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::init_soon(cx);
        let theme = cx.theme().clone();
        let state = chart_state(&self.id, window, cx);
        let slices = Rc::new(self.angles());
        let n = slices.len();
        let (focus, active) = {
            let s = state.read(cx);
            (s.focus.clone(), s.active.filter(|k| *k < n))
        };
        let name = self.id.to_string();
        let label = self
            .options
            .label
            .clone()
            .unwrap_or_else(|| self.accessibility_summary().into());
        let root = frame(&self.id, label, &theme, &self.style);
        if n == 0 {
            return empty_state(root, &name, self.options.empty_text.clone(), &theme);
        }
        let root = keyboard(root, &state, &focus, n, cx).when_some(active, |d, k| {
            let s = &slices[k];
            d.aria_value(format!(
                "{}: {}, {}",
                self.slices[s.index].label,
                self.options.value_text(s.value),
                percent(s.fraction)
            ))
        });
        // Each slice's colour, by its place among the values the host gave.
        let colors: Rc<Vec<Hsla>> = Rc::new(
            self.slices
                .iter()
                .enumerate()
                .map(|(i, s)| s.color.unwrap_or_else(|| theme.chart(i)))
                .collect(),
        );
        let donut = self.donut;

        // The pie, painted: the slices (the tooltip's pushed out), a line of the page's colour
        // between them, and the centre label in a donut's hole.
        let pie_canvas = {
            let state = state.clone();
            let slices = slices.clone();
            let colors = colors.clone();
            let center = self.center.clone().filter(|_| donut);
            let background = theme.background;
            let foreground = theme.foreground;
            let muted = theme.muted_foreground;
            canvas(
                move |bounds, window, cx| {
                    state.update(cx, |s, _| s.plot = bounds);
                    let rem = window.rem_size();
                    let (centre, inner, outer) = radii(donut, bounds);
                    let mut paints = Vec::new();
                    for (k, s) in slices.iter().enumerate() {
                        let out = if active == Some(k) {
                            outer + css(6.).to_pixels(rem)
                        } else {
                            outer
                        };
                        if let Some(path) = sector(centre, inner, out, s.start, s.end) {
                            paints.push(Paint::Path(path, colors[s.index].into()));
                        }
                    }
                    if slices.len() > 1 {
                        let mut b = PathBuilder::stroke(css(2.).to_pixels(rem));
                        for s in slices.iter() {
                            b.move_to(polar(centre, inner, s.start));
                            b.line_to(polar(centre, outer, s.start));
                        }
                        if let Ok(path) = b.build() {
                            paints.push(Paint::Path(path, background.into()));
                        }
                    }
                    let lines = center
                        .as_ref()
                        .map(|(value, caption)| {
                            centre_lines(value, caption, centre, inner, foreground, muted, window)
                        })
                        .unwrap_or_default();
                    (paints, lines)
                },
                |_, (paints, lines), window, cx| {
                    for p in paints {
                        match p {
                            Paint::Quad(q) => window.paint_quad(q),
                            Paint::Path(path, background) => window.paint_path(path, background),
                        }
                    }
                    for (origin, line_height, line) in lines {
                        let _ = line.paint(origin, line_height, TextAlign::Left, None, window, cx);
                    }
                },
            )
            .absolute()
            .size_full()
        };

        let tooltip = (self.options.tooltip && active.is_some()).then(|| {
            let k = active.unwrap_or(0);
            let s = slices[k];
            // In the plot's top corner on the slice's side, clear of the pie and its
            // centre label.
            let right = s.middle().cos() >= 0.;
            let name = name.clone();
            let card = tooltip_card(&theme)
                .debug_selector(move || format!("{name}-tooltip-{k}"))
                .top_0()
                .map(|d| if right { d.right_0() } else { d.left_0() })
                .child(tooltip_row(
                    colors[s.index],
                    self.slices[s.index].label.clone(),
                    self.options.value_text(s.value),
                    Some(percent(s.fraction).into()),
                    &theme,
                ));
            deferred(card).with_priority(1)
        });

        let hit = {
            let slices = slices.clone();
            move |bounds: Bounds<Pixels>, p: Point<Pixels>| {
                let (centre, inner, outer) = radii(donut, bounds);
                slice_at(&slices, centre, inner, outer, p)
            }
        };
        let hit = Rc::new(hit);
        let plot = div()
            .id(ElementId::NamedChild(
                std::sync::Arc::new(self.id.clone()),
                "plot".into(),
            ))
            .debug_selector({
                let name = name.clone();
                move || format!("{name}-plot")
            })
            .relative()
            .flex_1()
            .min_h_0()
            .on_mouse_move({
                let (state, hit) = (state.clone(), hit.clone());
                move |e: &MouseMoveEvent, _, cx| {
                    state.update(cx, |s, cx| {
                        let k = hit(s.plot, e.position);
                        s.set(k, cx)
                    });
                }
            })
            .on_hover({
                let state = state.clone();
                move |hovered: &bool, _, cx| {
                    if !*hovered {
                        state.update(cx, |s, cx| s.set(None, cx));
                    }
                }
            })
            // A tap (or a click) shows the slice's tooltip, as there's no hover on a touch
            // screen; anywhere else hides it.
            .on_mouse_down(MouseButton::Left, {
                let (state, hit) = (state.clone(), hit.clone());
                move |e: &MouseDownEvent, _, cx| {
                    state.update(cx, |s, cx| {
                        let k = hit(s.plot, e.position);
                        s.set(k, cx)
                    });
                }
            })
            .on_mouse_down_out({
                let state = state.clone();
                move |_, _, cx| state.update(cx, |s, cx| s.set(None, cx))
            })
            .child(pie_canvas)
            .children(tooltip);

        let legend = self.options.legend.then(|| {
            legend(
                slices
                    .iter()
                    .map(|s| (colors[s.index], self.slices[s.index].label.clone())),
                &theme,
            )
        });
        root.child(plot).children(legend).into_any_element()
    }
}

/// A donut's centre label as lines to paint: `value` large and bold over `caption`, both
/// centred in the hole and cut short to fit it.
fn centre_lines(
    value: &SharedString,
    caption: &SharedString,
    centre: Point<Pixels>,
    inner: Pixels,
    foreground: Hsla,
    muted: Hsla,
    window: &mut Window,
) -> Vec<(Point<Pixels>, Pixels, ShapedLine)> {
    let rem = window.rem_size();
    let room = inner * 1.6;
    if room <= px(0.) {
        return Vec::new();
    }
    let base = window.text_style();
    let text_system = window.text_system().clone();
    let shape = |text: &SharedString, size: Pixels, weight: FontWeight, color: Hsla| {
        let mut style = base.clone();
        style.font_weight = weight;
        style.color = color;
        let run = [style.to_run(text.len())];
        let mut wrapper = text_system.line_wrapper(style.font(), size);
        let (text, runs) = wrapper.truncate_line(text.clone(), room, "…", &run, TruncateFrom::End);
        text_system.shape_line(text, size, &runs, None)
    };
    let big = rems(2.).to_pixels(rem);
    let small = base.font_size.to_pixels(rem);
    let (big_h, small_h) = (big * 1.2, small * 1.5);
    let mut out = Vec::new();
    let mut y = centre.y - (big_h + if caption.is_empty() { px(0.) } else { small_h }) / 2.;
    if !value.is_empty() {
        let line = shape(value, big, FontWeight::BOLD, foreground);
        out.push((point(centre.x - line.width / 2., y), big_h, line));
        y += big_h;
    }
    if !caption.is_empty() {
        let line = shape(caption, small, FontWeight::NORMAL, muted);
        out.push((point(centre.x - line.width / 2., y), small_h, line));
    }
    out
}
