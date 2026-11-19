//! Charts, painted: bars (grouped, stacked, horizontal), lines, areas, radar, pies and donuts
//! (whole and half), radial rings, a single stacked bar, and scatter plots, in the theme's
//! series colours with a legend. Axis labels are painted inside the chart's own box, cut short
//! to fit, so they never leave it.

use gpui::prelude::FluentBuilder as _;
use gpui::*;

use super::Draw;
use crate::openui::{Bars, Chart, ChartKind, Curve, Scatter, Slices, SlicesKind};

/// Rounded steps for an axis from lo to hi, about `n` of them, covering both.
pub(crate) fn ticks(lo: f64, hi: f64, n: usize) -> Vec<f64> {
    let (lo, hi) = if (hi - lo).abs() < f64::EPSILON {
        (lo.min(0.), lo.max(0.) + 1.)
    } else {
        (lo, hi)
    };
    let raw = (hi - lo) / n.max(1) as f64;
    let exp = raw.log10().floor();
    let base = 10f64.powf(exp);
    let f = raw / base;
    let step = base
        * if f < 1.5 {
            1.
        } else if f < 3. {
            2.
        } else if f < 7. {
            5.
        } else {
            10.
        };
    if !step.is_finite() || step <= 0. {
        return vec![lo, hi];
    }
    let start = (lo / step).floor() * step;
    let end = (hi / step).ceil() * step;
    let mut out = Vec::new();
    let mut v = start;
    while v <= end + step * 0.5 && out.len() < 50 {
        out.push(if v.abs() < step * 1e-9 { 0. } else { v });
        v += step;
    }
    out
}

/// A number short enough for an axis: 1200 is 1.2K, 3500000 is 3.5M.
pub(crate) fn short(v: f64) -> String {
    let a = v.abs();
    let (n, suffix) = if a >= 1e12 {
        (v / 1e12, "T")
    } else if a >= 1e9 {
        (v / 1e9, "B")
    } else if a >= 1e6 {
        (v / 1e6, "M")
    } else if a >= 1e4 {
        (v / 1e3, "K")
    } else {
        (v, "")
    };
    let s = format!("{:.2}", n);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    format!("{}{suffix}", if s == "-0" { "0" } else { s })
}

/// Text painted in a chart, cut short with an ellipsis to `max` wide.
#[derive(Clone)]
struct Painter {
    family: SharedString,
    size: Pixels,
    color: Hsla,
}

#[derive(Clone, Copy, PartialEq)]
enum Anchor {
    Start,
    Middle,
    End,
}

impl Painter {
    fn shape(&self, text: &str, window: &mut Window) -> ShapedLine {
        let run = TextRun {
            len: text.len(),
            font: font(self.family.clone()),
            color: self.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        window.text_system().shape_line(
            SharedString::from(text.to_string()),
            self.size,
            &[run],
            None,
        )
    }

    /// Paints `text` at x (by `anchor`) and y (its top), at most `max` wide, kept within
    /// `left..right`.
    #[allow(clippy::too_many_arguments)]
    fn paint(
        &self,
        text: &str,
        x: Pixels,
        y: Pixels,
        anchor: Anchor,
        max: Pixels,
        left: Pixels,
        right: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) {
        let text: String = text.chars().filter(|c| !c.is_control()).take(200).collect();
        if text.is_empty() || max <= px(4.) {
            return;
        }
        let mut line = self.shape(&text, window);
        if line.width() > max {
            let chars: Vec<char> = text.chars().collect();
            let (mut lo, mut hi) = (0usize, chars.len());
            while lo < hi {
                let mid = (lo + hi).div_ceil(2);
                let s: String = chars[..mid].iter().collect::<String>() + "…";
                if self.shape(&s, window).width() <= max {
                    lo = mid;
                } else {
                    hi = mid - 1;
                }
            }
            if lo == 0 {
                return;
            }
            line = self.shape(&(chars[..lo].iter().collect::<String>() + "…"), window);
        }
        let w = line.width();
        let mut x0 = match anchor {
            Anchor::Start => x,
            Anchor::Middle => x - w / 2.,
            Anchor::End => x - w,
        };
        x0 = x0.max(left).min(right - w);
        let _ = line.paint(
            point(x0, y),
            self.size * 1.25,
            TextAlign::Left,
            None,
            window,
            cx,
        );
    }

    fn width(&self, text: &str, window: &mut Window) -> Pixels {
        self.shape(text, window).width()
    }

    /// `text` as the most of it that fits `max` up to a space, and the rest (None when it fits
    /// whole, or its first word doesn't).
    fn split_two(&self, text: &str, max: Pixels, window: &mut Window) -> (String, Option<String>) {
        if self.width(text, window) <= max {
            return (text.to_string(), None);
        }
        let mut best = None;
        for (i, _) in text.match_indices(' ') {
            if self.width(&text[..i], window) <= max {
                best = Some(i);
            } else {
                break;
            }
        }
        match best {
            Some(i) => (
                text[..i].to_string(),
                Some(text[i + 1..].trim_start().to_string()),
            ),
            None => (text.to_string(), None),
        }
    }
}

/// A category name longer than this many characters gets a row tall enough for two lines.
const LONG_NAME: usize = 20;

/// A rectangle filled, with rounded corners where asked.
fn bar(
    window: &mut Window,
    b: Bounds<Pixels>,
    color: Hsla,
    radius: Pixels,
    corners: (bool, bool, bool, bool),
) {
    let r = radius.min(b.size.width / 2.).min(b.size.height / 2.);
    let (tl, tr, br, bl) = corners;
    let pick = |on: bool| if on { r } else { px(0.) };
    window.paint_quad(fill(b, color).corner_radii(Corners {
        top_left: pick(tl),
        top_right: pick(tr),
        bottom_right: pick(br),
        bottom_left: pick(bl),
    }));
}

fn stroke_line(window: &mut Window, pts: &[Point<Pixels>], width: Pixels, color: Hsla) {
    if pts.len() < 2 {
        return;
    }
    let mut p = PathBuilder::stroke(width);
    p.move_to(pts[0]);
    for q in &pts[1..] {
        p.line_to(*q);
    }
    if let Ok(path) = p.build() {
        window.paint_path(path, color);
    }
}

fn fill_polygon(window: &mut Window, pts: &[Point<Pixels>], color: Hsla) {
    if pts.len() < 3 {
        return;
    }
    let mut p = PathBuilder::fill();
    p.move_to(pts[0]);
    for q in &pts[1..] {
        p.line_to(*q);
    }
    p.close();
    if let Ok(path) = p.build() {
        window.paint_path(path, color);
    }
}

/// The points of a curve through `pts`: straight, smooth (Catmull-Rom) or stepped.
fn curve_points(pts: &[Point<Pixels>], curve: Curve) -> Vec<Point<Pixels>> {
    match curve {
        Curve::Linear => pts.to_vec(),
        // d3's curveStep, as the web's charts draw "step": the change halfway between points.
        Curve::Step => {
            let mut out = Vec::new();
            for (i, p) in pts.iter().enumerate() {
                if i > 0 {
                    let q = pts[i - 1];
                    let mid = (q.x + p.x) / 2.;
                    out.push(point(mid, q.y));
                    out.push(point(mid, p.y));
                }
                out.push(*p);
            }
            out
        }
        Curve::Natural => {
            if pts.len() < 3 {
                return pts.to_vec();
            }
            let mut out = Vec::new();
            for i in 0..pts.len() - 1 {
                let p0 = pts[i.saturating_sub(1)];
                let (p1, p2) = (pts[i], pts[i + 1]);
                let p3 = pts[(i + 2).min(pts.len() - 1)];
                for s in 0..12 {
                    let t = s as f32 / 12.;
                    let (t2, t3) = (t * t, t * t * t);
                    let f = |a: f32, b: f32, c: f32, d: f32| {
                        0.5 * (2. * b
                            + (-a + c) * t
                            + (2. * a - 5. * b + 4. * c - d) * t2
                            + (-a + 3. * b - 3. * c + d) * t3)
                    };
                    out.push(point(
                        px(f(p0.x.into(), p1.x.into(), p2.x.into(), p3.x.into())),
                        px(f(p0.y.into(), p1.y.into(), p2.y.into(), p3.y.into())),
                    ));
                }
            }
            out.push(pts[pts.len() - 1]);
            out
        }
    }
}

impl Draw<'_, '_> {
    pub fn chart(&mut self, c: &Chart, path: &str) -> AnyElement {
        let limits = self.v.limits;
        let too_many = match c {
            Chart::Bars(b) => {
                b.labels.len() > limits.chart_points || b.series.len() > limits.chart_series
            }
            Chart::Slices(s) => s.slices.len() > limits.chart_points,
            Chart::Scatter(s) => {
                s.sets.len() > limits.chart_series
                    || s.sets.iter().any(|s| s.points.len() > limits.chart_points)
            }
        };
        if too_many {
            return self.note("This chart has too many points to show.", None);
        }
        let empty = match c {
            Chart::Bars(b) => b
                .series
                .iter()
                .all(|s| s.values.iter().all(Option::is_none)),
            Chart::Slices(s) => s.total() <= 0.,
            Chart::Scatter(s) => s.bounds().is_none(),
        };
        if empty {
            return self.note("This chart has no data to show.", None);
        }
        let _ = path;
        match c {
            Chart::Bars(b) if b.kind == ChartKind::Radar => self.radar(b),
            Chart::Bars(b) if b.kind == ChartKind::HorizontalBar => self.horizontal_bars(b, path),
            Chart::Bars(b) => self.cartesian(b),
            Chart::Slices(s) => self.slices(s),
            Chart::Scatter(s) => self.scatter(s),
        }
    }

    fn painter(&self, size: f32, color: Hsla) -> Painter {
        Painter {
            family: self.t.fonts.body.clone(),
            size: self.pixels(size),
            color,
        }
    }

    /// Swatches with names, wrapping.
    fn legend(&self, items: &[(String, Hsla)]) -> Div {
        let t = self.t;
        let mut row = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_row()
            .flex_wrap()
            .justify_center()
            .gap_x(self.px(14.))
            .gap_y(self.px(4.));
        for (name, color) in items {
            row = row.child(
                div()
                    .max_w_full()
                    .min_w_0()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(self.px(6.))
                    .child(
                        div()
                            .flex_none()
                            .size(self.px(10.))
                            .rounded(self.px(2.))
                            .bg(*color),
                    )
                    .child(self.line(name.clone(), t.text.xs, t.colors.text_muted, self.regular())),
            );
        }
        row
    }

    /// The chart's box: a caption for each axis label, the painting, the legend.
    fn chart_box(
        &self,
        painting: impl IntoElement,
        height: f32,
        legend: Option<Div>,
        x_label: Option<&str>,
        y_label: Option<&str>,
    ) -> AnyElement {
        let t = self.t;
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(self.px(8.))
            .when_some(y_label, |d, s| {
                d.child(self.line(
                    s.to_string(),
                    t.text.xs,
                    t.colors.text_muted,
                    self.regular(),
                ))
            })
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .h(self.px(height))
                    .overflow_hidden()
                    .child(painting),
            )
            .when_some(x_label, |d, s| {
                d.child(
                    div()
                        .w_full()
                        .min_w_0()
                        .flex()
                        .justify_center()
                        .child(self.line(
                            s.to_string(),
                            t.text.xs,
                            t.colors.text_muted,
                            self.regular(),
                        )),
                )
            })
            .children(legend)
            .into_any_element()
    }

    /// Bars, lines and areas over categories.
    fn cartesian(&mut self, b: &Bars) -> AnyElement {
        let t = self.t;
        let colors = t.series_colors(b.series.len());
        let height = b.height.map_or(240., |h| (h - 27.).clamp(100., 600.));
        let b2 = b.clone();
        let (grid, zero) = (t.colors.line, t.colors.line_strong);
        let label = self.painter(t.text.xs, t.colors.text_muted);
        let cols = colors.clone();
        let radius = self.pixels(4.);
        let painting = canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| {
                let b = &b2;
                let (lo, hi) = b.range();
                let ticks = ticks(lo, hi, 4);
                let (lo, hi) = (*ticks.first().unwrap_or(&lo), *ticks.last().unwrap_or(&hi));
                let line_h = label.size * 1.25;
                let gutter = ticks
                    .iter()
                    .map(|v| label.width(&short(*v), window))
                    .fold(px(0.), |a, w| a.max(w))
                    + px(8.);
                let gutter = gutter.min(bounds.size.width * 0.3);
                let left = bounds.origin.x + gutter;
                let right = bounds.origin.x + bounds.size.width - px(2.);
                let top = bounds.origin.y + px(6.);
                let bottom = bounds.origin.y + bounds.size.height - line_h - px(6.);
                if right <= left || bottom <= top {
                    return;
                }
                let span = (hi - lo).max(f64::EPSILON);
                let y_of = |v: f64| bottom - (bottom - top) * (((v - lo) / span) as f32);
                for v in &ticks {
                    let y = y_of(*v);
                    window.paint_quad(fill(
                        Bounds::new(point(left, y), size(right - left, px(1.))),
                        if *v == 0. { zero } else { grid },
                    ));
                    label.paint(
                        &short(*v),
                        left - px(6.),
                        y - line_h / 2.,
                        Anchor::End,
                        gutter - px(6.),
                        bounds.origin.x,
                        left - px(4.),
                        window,
                        cx,
                    );
                }
                let n = b.labels.len().max(1);
                let band = (right - left) / n as f32;
                // Category labels: as many as fit, evenly spaced.
                let widest = b
                    .labels
                    .iter()
                    .map(|l| label.width(l, window))
                    .fold(px(0.), |a, w| a.max(w))
                    .min(px(120.));
                let every = ((widest + px(8.)) / band).ceil().max(1.) as usize;
                for (i, l) in b.labels.iter().enumerate() {
                    if i % every == 0 {
                        let x = left + band * (i as f32 + 0.5);
                        label.paint(
                            l,
                            x,
                            bottom + px(4.),
                            Anchor::Middle,
                            band * every as f32 - px(4.),
                            bounds.origin.x,
                            right,
                            window,
                            cx,
                        );
                    }
                }
                let zero_y = y_of(0f64.clamp(lo, hi));
                match b.kind {
                    ChartKind::Bar => {
                        let group = band * 0.72;
                        let k = if b.stacked { 1 } else { b.series.len().max(1) };
                        let w = (group / k as f32).min(px(48.));
                        let gap = if k > 1 { px(2.) } else { px(0.) };
                        for i in 0..b.labels.len() {
                            let center = left + band * (i as f32 + 0.5);
                            let (mut pos, mut neg) = (0f64, 0f64);
                            for (si, s) in b.series.iter().enumerate() {
                                let Some(v) = s.values.get(i).copied().flatten() else {
                                    continue;
                                };
                                let (a, z) = if b.stacked {
                                    let base = if v >= 0. { pos } else { neg };
                                    if v >= 0. {
                                        pos += v
                                    } else {
                                        neg += v
                                    }
                                    (base, base + v)
                                } else {
                                    (0., v)
                                };
                                let x = if b.stacked {
                                    center - w / 2.
                                } else {
                                    center - (w * k as f32) / 2. + w * si as f32
                                };
                                let (y1, y2) = (y_of(a), y_of(z));
                                let rect = Bounds::new(
                                    point(x + gap / 2., y1.min(y2)),
                                    size((w - gap).max(px(1.)), (y1 - y2).abs().max(px(1.))),
                                );
                                let up = v >= 0.;
                                bar(window, rect, cols[si], radius, (up, up, !up, !up));
                            }
                        }
                    }
                    ChartKind::Line | ChartKind::Area => {
                        for (si, s) in b.series.iter().enumerate().rev() {
                            // A gap where a value is missing.
                            let mut runs: Vec<Vec<Point<Pixels>>> = vec![Vec::new()];
                            for (i, v) in s.values.iter().enumerate() {
                                match v {
                                    Some(v) => runs
                                        .last_mut()
                                        .unwrap()
                                        .push(point(left + band * (i as f32 + 0.5), y_of(*v))),
                                    None => runs.push(Vec::new()),
                                }
                            }
                            for run in runs.iter().filter(|r| !r.is_empty()) {
                                let pts = curve_points(run, b.curve);
                                if b.kind == ChartKind::Area {
                                    let mut poly = pts.clone();
                                    poly.push(point(pts[pts.len() - 1].x, zero_y));
                                    poly.push(point(pts[0].x, zero_y));
                                    fill_polygon(
                                        window,
                                        &poly,
                                        Hsla {
                                            a: 0.28,
                                            ..cols[si]
                                        },
                                    );
                                }
                                stroke_line(window, &pts, px(2.), cols[si]);
                                if run.len() == 1 || b.labels.len() <= 24 {
                                    for p in run {
                                        let r = px(3.);
                                        window.paint_quad(
                                            fill(
                                                Bounds::new(
                                                    point(p.x - r, p.y - r),
                                                    size(r * 2., r * 2.),
                                                ),
                                                cols[si],
                                            )
                                            .corner_radii(r),
                                        );
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            },
        )
        .size_full();
        let legend = (b.series.len() > 1 || b.series.first().is_some_and(|s| !s.name.is_empty()))
            .then(|| {
                self.legend(
                    &b.series
                        .iter()
                        .zip(&colors)
                        .map(|(s, c)| (s.name.clone(), *c))
                        .collect::<Vec<_>>(),
                )
            });
        self.chart_box(
            painting,
            height,
            legend,
            b.x_label.as_deref(),
            b.y_label.as_deref(),
        )
    }

    /// Bars along rows, a category's name beside each.
    fn horizontal_bars(&mut self, b: &Bars, path: &str) -> AnyElement {
        let t = self.t;
        let colors = t.series_colors(b.series.len());
        let k = if b.stacked { 1 } else { b.series.len().max(1) };
        // A long category name takes two lines beside its bars (a row tall enough for them).
        let long = b.labels.iter().any(|l| l.chars().count() > LONG_NAME);
        let row = (12. * k as f32 + 12.).max(if long { 44. } else { 28. });
        let height = row * b.labels.len() as f32 + 24.;
        let b2 = b.clone();
        let (grid, zero) = (t.colors.line, t.colors.line_strong);
        let label = self.painter(t.text.xs, t.colors.text_muted);
        let name = self.painter(t.text.sm, t.colors.text);
        let cols = colors.clone();
        let (row_px, radius) = (self.pixels(row), self.pixels(4.));
        let painting = canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| {
                let b = &b2;
                let (lo, hi) = b.range();
                let ticks = ticks(lo, hi, 4);
                let (lo, hi) = (*ticks.first().unwrap_or(&lo), *ticks.last().unwrap_or(&hi));
                // The names' column: as wide as the widest, up to 45% of the chart (on two lines
                // where a row has room for them, then cut short).
                let names_w = b
                    .labels
                    .iter()
                    .map(|l| name.width(l, window))
                    .fold(px(0.), |a, w| a.max(w))
                    .min(bounds.size.width * 0.45)
                    + px(10.);
                let left = bounds.origin.x + names_w;
                let right = bounds.origin.x + bounds.size.width - px(14.);
                let top = bounds.origin.y;
                let bottom = top + row_px * b.labels.len() as f32;
                if right <= left {
                    return;
                }
                let span = (hi - lo).max(f64::EPSILON);
                let x_of = |v: f64| left + (right - left) * (((v - lo) / span) as f32);
                let line_h = label.size * 1.25;
                for v in &ticks {
                    let x = x_of(*v);
                    window.paint_quad(fill(
                        Bounds::new(point(x, top), size(px(1.), bottom - top)),
                        if *v == 0. { zero } else { grid },
                    ));
                    label.paint(
                        &short(*v),
                        x,
                        bottom + px(4.),
                        Anchor::Middle,
                        px(60.),
                        left - px(20.),
                        bounds.origin.x + bounds.size.width,
                        window,
                        cx,
                    );
                }
                let _ = line_h;
                let bar_h = px(12.);
                for (i, l) in b.labels.iter().enumerate() {
                    let y0 = top + row_px * i as f32;
                    let room = names_w - px(10.);
                    let name_h = name.size * 1.25;
                    let lines = match name.split_two(l, room, window) {
                        (first, Some(rest)) if row_px >= name_h * 2. => vec![first, rest],
                        _ => vec![l.clone()],
                    };
                    let mut y = y0 + row_px / 2. - name_h * lines.len() as f32 / 2.;
                    for line in &lines {
                        name.paint(
                            line,
                            bounds.origin.x,
                            y,
                            Anchor::Start,
                            room,
                            bounds.origin.x,
                            left - px(6.),
                            window,
                            cx,
                        );
                        y += name_h;
                    }
                    let block = bar_h * k as f32;
                    let start = y0 + (row_px - block) / 2.;
                    let (mut pos, mut neg) = (0f64, 0f64);
                    for (si, s) in b.series.iter().enumerate() {
                        let Some(v) = s.values.get(i).copied().flatten() else {
                            continue;
                        };
                        let (a, z) = if b.stacked {
                            let base = if v >= 0. { pos } else { neg };
                            if v >= 0. {
                                pos += v
                            } else {
                                neg += v
                            }
                            (base, base + v)
                        } else {
                            (0., v)
                        };
                        let y = if b.stacked {
                            start
                        } else {
                            start + bar_h * si as f32
                        };
                        let (x1, x2) = (x_of(a), x_of(z));
                        let rect = Bounds::new(
                            point(x1.min(x2), y + px(1.)),
                            size((x1 - x2).abs().max(px(1.)), bar_h - px(2.)),
                        );
                        let up = v >= 0.;
                        bar(window, rect, cols[si], radius, (!up, up, up, !up));
                    }
                }
            },
        )
        .size_full();
        let legend = (b.series.len() > 1).then(|| {
            self.legend(
                &b.series
                    .iter()
                    .zip(&colors)
                    .map(|(s, c)| (s.name.clone(), *c))
                    .collect::<Vec<_>>(),
            )
        });
        // The chart's words for assistive technology: each category's whole name and its
        // values, as a name cut short on screen is still read whole.
        let words = b
            .labels
            .iter()
            .enumerate()
            .map(|(i, l)| {
                let values: Vec<String> = b
                    .series
                    .iter()
                    .filter_map(|s| {
                        let v = s.values.get(i).copied().flatten()?;
                        Some(if b.series.len() > 1 {
                            format!("{} {}", s.name, short(v))
                        } else {
                            short(v)
                        })
                    })
                    .collect();
                format!("{l}: {}", values.join(", "))
            })
            .collect::<Vec<_>>()
            .join("; ");
        let painting = div()
            .id(self.id(&format!("{path}.hbar")))
            .role(Role::Figure)
            .aria_label(format!("Horizontal bar chart. {words}"))
            .size_full()
            .child(painting);
        self.chart_box(
            painting,
            height,
            legend,
            b.x_label.as_deref(),
            b.y_label.as_deref(),
        )
    }

    /// Values around a circle, one spoke per category.
    fn radar(&mut self, b: &Bars) -> AnyElement {
        let t = self.t;
        let colors = t.series_colors(b.series.len());
        let height = 280.;
        let b2 = b.clone();
        let (grid, label) = (t.colors.line, self.painter(t.text.xs, t.colors.text_muted));
        let cols = colors.clone();
        let painting = canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| {
                let b = &b2;
                let n = b.labels.len();
                if n < 3 {
                    return;
                }
                let line_h = label.size * 1.25;
                let c = bounds.center();
                // Small enough that the labels at the sides fit beside it whole.
                let sides: Vec<(f32, Pixels)> = b
                    .labels
                    .iter()
                    .enumerate()
                    .map(|(i, name)| (spoke(i, n).cos(), label.width(name, window)))
                    .collect();
                let r = radar_radius(bounds.size, line_h, &sides);
                let hi = b
                    .series
                    .iter()
                    .flat_map(|s| s.values.iter().flatten())
                    .fold(0f64, |a, v| a.max(*v))
                    .max(f64::EPSILON);
                let at = |i: usize, f: f32| {
                    let a = spoke(i, n);
                    point(c.x + r * f * a.cos(), c.y + r * f * a.sin())
                };
                for level in 1..=4 {
                    let f = level as f32 / 4.;
                    let mut ring: Vec<Point<Pixels>> = (0..n).map(|i| at(i, f)).collect();
                    ring.push(ring[0]);
                    stroke_line(window, &ring, px(1.), grid);
                }
                for (i, name) in b.labels.iter().enumerate() {
                    stroke_line(window, &[c, at(i, 1.)], px(1.), grid);
                    let p = at(i, 1.12);
                    let anchor = if (p.x - c.x).abs() < px(4.) {
                        Anchor::Middle
                    } else if p.x > c.x {
                        Anchor::Start
                    } else {
                        Anchor::End
                    };
                    let room = if anchor == Anchor::Middle {
                        bounds.size.width * 0.5
                    } else if p.x > c.x {
                        bounds.origin.x + bounds.size.width - p.x
                    } else {
                        p.x - bounds.origin.x
                    };
                    label.paint(
                        name,
                        p.x,
                        p.y - line_h / 2.,
                        anchor,
                        room,
                        bounds.origin.x,
                        bounds.origin.x + bounds.size.width,
                        window,
                        cx,
                    );
                }
                for (si, s) in b.series.iter().enumerate() {
                    let pts: Vec<Point<Pixels>> = (0..n)
                        .map(|i| {
                            at(
                                i,
                                (s.values.get(i).copied().flatten().unwrap_or(0.).max(0.) / hi)
                                    as f32,
                            )
                        })
                        .collect();
                    fill_polygon(
                        window,
                        &pts,
                        Hsla {
                            a: 0.22,
                            ..cols[si]
                        },
                    );
                    let mut ring = pts.clone();
                    ring.push(pts[0]);
                    stroke_line(window, &ring, px(2.), cols[si]);
                }
            },
        )
        .size_full();
        let legend = Some(
            self.legend(
                &b.series
                    .iter()
                    .zip(&colors)
                    .map(|(s, c)| (s.name.clone(), *c))
                    .collect::<Vec<_>>(),
            ),
        );
        self.chart_box(painting, height, legend, None, None)
    }

    /// Parts of a whole: a pie or donut (whole or half), rings, or one stacked bar, with a
    /// legend of each part's value and share.
    fn slices(&mut self, s: &Slices) -> AnyElement {
        let t = self.t;
        let colors = t.series_colors(s.slices.len());
        let shares = s.shares();
        let s2 = s.clone();
        let cols = colors.clone();
        let track = t.colors.sunk;
        // Slices are parted by a thin line of the ground they're drawn on.
        let parting = t.colors.background;
        let height = match s.kind {
            SlicesKind::StackedBar => 24.,
            _ if s.semicircle => 130.,
            _ => 220.,
        };
        let radius = self.pixels(6.);
        let painting = canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let s = &s2;
                let shares = s.shares();
                match s.kind {
                    SlicesKind::StackedBar => {
                        let mut x = bounds.origin.x;
                        let last = shares.iter().rposition(|f| *f > 0.).unwrap_or(0);
                        let first = shares.iter().position(|f| *f > 0.).unwrap_or(0);
                        for (i, f) in shares.iter().enumerate() {
                            if *f <= 0. {
                                continue;
                            }
                            let w = bounds.size.width * *f as f32;
                            bar(
                                window,
                                Bounds::new(point(x, bounds.origin.y), size(w, bounds.size.height)),
                                cols[i],
                                radius,
                                (i == first, i == last, i == last, i == first),
                            );
                            x += w;
                        }
                    }
                    SlicesKind::Radial => {
                        let n = s.slices.len().max(1);
                        let c = bounds.center();
                        let outer = bounds.size.width.min(bounds.size.height) / 2. - px(2.);
                        let ring = (outer / n as f32).min(px(18.));
                        let max = s
                            .slices
                            .iter()
                            .map(|x| x.value)
                            .fold(0f64, f64::max)
                            .max(f64::EPSILON);
                        for (i, sl) in s.slices.iter().enumerate() {
                            let r = outer - ring * i as f32 - ring / 2.;
                            let full: Vec<Point<Pixels>> =
                                (0..=96).map(|k| arc(c, r, k as f32 / 96.)).collect();
                            stroke_line(window, &full, ring * 0.7, track);
                            let f = (sl.value / max) as f32;
                            let steps = (96. * f).ceil().max(1.) as usize;
                            let part: Vec<Point<Pixels>> = (0..=steps)
                                .map(|k| arc(c, r, f * k as f32 / steps as f32))
                                .collect();
                            stroke_line(window, &part, ring * 0.7, cols[i]);
                        }
                    }
                    SlicesKind::Pie | SlicesKind::Donut => {
                        let half = s.semicircle;
                        let c = if half {
                            point(
                                bounds.center().x,
                                bounds.origin.y + bounds.size.height - px(4.),
                            )
                        } else {
                            bounds.center()
                        };
                        let r = if half {
                            (bounds.size.height - px(8.)).min(bounds.size.width / 2. - px(2.))
                        } else {
                            bounds.size.width.min(bounds.size.height) / 2. - px(2.)
                        };
                        let inner = if s.kind == SlicesKind::Donut {
                            r * 0.6
                        } else {
                            px(0.)
                        };
                        let (start, sweep) = if half { (0.75f32, 0.5f32) } else { (0., 1.) };
                        let mut at = 0f32;
                        for (i, f) in shares.iter().enumerate() {
                            let f = *f as f32;
                            if f <= 0. {
                                continue;
                            }
                            let (a, z) = (start + at * sweep, start + (at + f) * sweep);
                            at += f;
                            let steps = ((z - a) * 120.).ceil().max(2.) as usize;
                            let mut poly: Vec<Point<Pixels>> = (0..=steps)
                                .map(|k| arc(c, r, a + (z - a) * k as f32 / steps as f32))
                                .collect();
                            if inner > px(0.) {
                                poly.extend(
                                    (0..=steps).rev().map(|k| {
                                        arc(c, inner, a + (z - a) * k as f32 / steps as f32)
                                    }),
                                );
                            } else {
                                poly.push(c);
                            }
                            fill_polygon(window, &poly, cols[i]);
                            if shares.iter().filter(|f| **f > 0.).count() > 1 {
                                let mut edge = poly.clone();
                                edge.push(poly[0]);
                                stroke_line(window, &edge, px(1.5), parting);
                            }
                        }
                    }
                }
            },
        )
        .size_full();
        // The legend: each part with its value and share.
        let mut list = div().w_full().min_w_0().flex().flex_col();
        let total = s.total();
        for (i, sl) in s.slices.iter().enumerate() {
            let pct = if total > 0. {
                format!("{:.0}%", shares[i] * 100.)
            } else {
                "0%".into()
            };
            list = list.child(
                div()
                    .w_full()
                    .min_w_0()
                    .h(self.px(30.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(self.px(8.))
                    .when(i + 1 < s.slices.len(), |d| {
                        d.border_b_1().border_color(t.colors.line)
                    })
                    .child(
                        div()
                            .flex_none()
                            .size(self.px(10.))
                            .rounded(self.px(2.))
                            .bg(colors[i]),
                    )
                    .child(
                        self.line(sl.label.clone(), t.text.sm, t.colors.text, self.regular())
                            .flex_1(),
                    )
                    .child(
                        self.line(
                            crate::openui::num_text(sl.value),
                            t.text.sm,
                            t.colors.text,
                            t.text.medium,
                        )
                        .flex_none()
                        .max_w(relative(0.4)),
                    )
                    .child(
                        self.line(pct, t.text.sm, t.colors.text_muted, self.regular())
                            .flex_none()
                            .w(self.px(44.))
                            .text_right(),
                    ),
            );
        }
        let width = if s.kind == SlicesKind::StackedBar {
            None
        } else {
            Some(if s.semicircle { 260. } else { 220. })
        };
        let picture = div().w_full().min_w_0().flex().justify_center().child(
            div()
                .h(self.px(height))
                .min_w_0()
                .overflow_hidden()
                .map(|d| match width {
                    Some(w) => d.w(self.px(w)).max_w_full(),
                    None => d.w_full(),
                })
                .child(painting),
        );
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(self.px(12.))
            .child(picture)
            .child(list)
            .into_any_element()
    }

    /// Points by x and y, sized by z when there is one.
    fn scatter(&mut self, s: &Scatter) -> AnyElement {
        let t = self.t;
        let colors = t.series_colors(s.sets.len());
        let s2 = s.clone();
        let (grid, label) = (t.colors.line, self.painter(t.text.xs, t.colors.text_muted));
        let cols = colors.clone();
        let painting = canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| {
                let s = &s2;
                let Some(((x0, x1), (y0, y1))) = s.bounds() else {
                    return;
                };
                let (xt, yt) = (ticks(x0.min(0.), x1, 4), ticks(y0.min(0.), y1, 4));
                let (xlo, xhi) = (xt[0], xt[xt.len() - 1]);
                let (ylo, yhi) = (yt[0], yt[yt.len() - 1]);
                let line_h = label.size * 1.25;
                let gutter = (yt
                    .iter()
                    .map(|v| label.width(&short(*v), window))
                    .fold(px(0.), |a, w| a.max(w))
                    + px(8.))
                .min(bounds.size.width * 0.3);
                let left = bounds.origin.x + gutter;
                let right = bounds.origin.x + bounds.size.width - px(8.);
                let top = bounds.origin.y + px(8.);
                let bottom = bounds.origin.y + bounds.size.height - line_h - px(6.);
                if right <= left || bottom <= top {
                    return;
                }
                let x_of = |v: f64| {
                    left + (right - left) * (((v - xlo) / (xhi - xlo).max(f64::EPSILON)) as f32)
                };
                let y_of = |v: f64| {
                    bottom - (bottom - top) * (((v - ylo) / (yhi - ylo).max(f64::EPSILON)) as f32)
                };
                for v in &yt {
                    let y = y_of(*v);
                    window.paint_quad(fill(
                        Bounds::new(point(left, y), size(right - left, px(1.))),
                        grid,
                    ));
                    label.paint(
                        &short(*v),
                        left - px(6.),
                        y - line_h / 2.,
                        Anchor::End,
                        gutter - px(6.),
                        bounds.origin.x,
                        left - px(4.),
                        window,
                        cx,
                    );
                }
                let room = (right - left) / xt.len() as f32;
                for v in &xt {
                    let x = x_of(*v);
                    window.paint_quad(fill(
                        Bounds::new(point(x, top), size(px(1.), bottom - top)),
                        grid,
                    ));
                    label.paint(
                        &short(*v),
                        x,
                        bottom + px(4.),
                        Anchor::Middle,
                        room,
                        left - gutter,
                        right + px(8.),
                        window,
                        cx,
                    );
                }
                let zmax = s
                    .sets
                    .iter()
                    .flat_map(|s| s.points.iter().filter_map(|p| p.z))
                    .fold(0f64, f64::max);
                for (si, set) in s.sets.iter().enumerate() {
                    for p in &set.points {
                        let r = match p.z {
                            Some(z) if zmax > 0. => px(3. + 7. * (z.max(0.) / zmax) as f32),
                            _ => px(4.),
                        };
                        let (x, y) = (x_of(p.x), y_of(p.y));
                        window.paint_quad(
                            fill(
                                Bounds::new(point(x - r, y - r), size(r * 2., r * 2.)),
                                Hsla {
                                    a: 0.85,
                                    ..cols[si]
                                },
                            )
                            .corner_radii(r),
                        );
                    }
                }
            },
        )
        .size_full();
        let legend = Some(
            self.legend(
                &s.sets
                    .iter()
                    .zip(&colors)
                    .map(|(s, c)| (s.name.clone(), *c))
                    .collect::<Vec<_>>(),
            ),
        );
        self.chart_box(
            painting,
            260.,
            legend,
            s.x_label.as_deref(),
            s.y_label.as_deref(),
        )
    }
}

/// The angle of a radar's `i`th spoke of `n`, from the top, clockwise.
fn spoke(i: usize, n: usize) -> f32 {
    std::f32::consts::TAU * i as f32 / n as f32 - std::f32::consts::FRAC_PI_2
}

/// How far out a radar's outer ring is in a picture of `size`: as far as the picture's height
/// takes (with a line for the labels above and below), but no further than lets each label at
/// the side (by its spoke's cosine and its width) sit whole beside the ring, its anchor 1.12 of
/// the radius out. At least a fifth of the picture, where labels that long are cut short.
fn radar_radius(size: gpui::Size<Pixels>, line_h: Pixels, labels: &[(f32, Pixels)]) -> Pixels {
    let gap = px(4.);
    let mut r = size.width.min(size.height) / 2. - line_h * 2.;
    for &(cos, w) in labels {
        let cos = cos.abs();
        // Top and bottom labels are centred on their spoke.
        if cos < 0.05 {
            continue;
        }
        r = r.min((size.width / 2. - gap - w) / (1.12 * cos));
    }
    r.max(size.width.min(size.height) * 0.2).max(px(10.))
}

/// A point on a circle, `turn` of the way round from the top, clockwise.
fn arc(c: Point<Pixels>, r: Pixels, turn: f32) -> Point<Pixels> {
    let a = turn * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
    point(c.x + r * a.cos(), c.y + r * a.sin())
}

#[cfg(test)]
mod tests {
    /// A long category name goes on two lines, broken at a space, where one won't hold it.
    /// (GPUI's test window is the Mac's dev-dependency here, so the macOS job runs it.)
    #[cfg(target_os = "macos")]
    #[gpui::test]
    fn a_long_name_is_split_at_a_space_into_two_lines(cx: &mut gpui::TestAppContext) {
        use gpui::px;
        let window = cx.add_window(|_, _| gpui::Empty);
        let p = super::Painter {
            family: "Helvetica".into(),
            size: px(14.),
            color: gpui::black(),
        };
        window
            .update(cx, |_, window, _| {
                let name = "Okafor (robotics and autonomous systems)";
                let whole = p.width(name, window);
                // Room for about two thirds of it: the first line ends at a space and fits.
                let (first, rest) = p.split_two(name, whole * 0.66, window);
                let rest = rest.expect("two lines");
                assert!(p.width(&first, window) <= whole * 0.66, "{first}");
                assert_eq!(format!("{first} {rest}"), name);
                // A name that fits stays one line; a single long word too (cut short instead).
                assert_eq!(p.split_two("Patel", whole, window), ("Patel".into(), None));
                assert_eq!(
                    p.split_two("Pneumonoultramicroscopic", px(20.), window).1,
                    None
                );
            })
            .unwrap();
    }

    /// A radar at a phone's width (393 points) leaves its side labels room to be whole: the
    /// ring shrinks for "Mentorship" rather than cutting it to "Mentor…".
    #[test]
    fn a_radars_side_labels_fit_beside_its_ring() {
        use gpui::{px, size};
        let picture = size(px(330.), px(280.));
        let line_h = px(15.);
        // Five spokes, the long label at the right (spoke 1 of 5) and one at the left.
        let n = 5;
        let widths = [px(40.), px(84.), px(50.), px(50.), px(70.)];
        let labels: Vec<(f32, gpui::Pixels)> = widths
            .iter()
            .enumerate()
            .map(|(i, w)| (super::spoke(i, n).cos(), *w))
            .collect();
        let r = super::radar_radius(picture, line_h, &labels);
        for (i, w) in widths.iter().enumerate() {
            let x = (r * 1.12 * super::spoke(i, n).cos()).abs();
            if super::spoke(i, n).cos().abs() >= 0.05 {
                assert!(
                    x + *w <= picture.width / 2.,
                    "label {i} ({w:?}) runs past the side: ring {r:?}"
                );
            }
        }
        // With short labels the ring is as big as the height lets it be.
        let short: Vec<(f32, gpui::Pixels)> = labels.iter().map(|(c, _)| (*c, px(10.))).collect();
        assert_eq!(
            super::radar_radius(picture, line_h, &short),
            picture.height / 2. - line_h * 2.
        );
    }

    use super::*;
    // The built-in #[test], not GPUI's, which gpui::* brings in.
    use std::prelude::v1::test;

    #[test]
    fn axes_get_round_steps_that_cover_the_data() {
        assert_eq!(ticks(0., 180., 4), vec![0., 50., 100., 150., 200.]);
        assert_eq!(ticks(-10., 150., 4), vec![-50., 0., 50., 100., 150.]);
        assert_eq!(ticks(5., 5., 4).first(), Some(&0.));
        assert!(ticks(0., 1e300, 4).len() <= 50);
        assert!(!ticks(0., f64::MAX, 4).is_empty());
    }

    #[test]
    fn axis_numbers_are_short() {
        assert_eq!(short(1200.), "1200");
        assert_eq!(short(48200.), "48.2K");
        assert_eq!(short(3_500_000.), "3.5M");
        assert_eq!(short(-0.0), "0");
        assert_eq!(short(0.25), "0.25");
    }
}
