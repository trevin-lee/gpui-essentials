//! Charts: shadcn's Chart (its Recharts wrappers) drawn natively: [`BarChart`], [`LineChart`]
//! and [`AreaChart`]. The host passes plain data:
//! the categories along the x axis and a series of values for each, with a label and,
//! optionally, a colour (otherwise the theme's `chart_1` to `chart_5` in turn).
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::BarChart;
//! # struct Sales;
//! # impl Render for Sales {
//! fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
//!     BarChart::new("visitors")
//!         .categories(["Jan", "Feb", "Mar", "Apr", "May", "Jun"])
//!         .series("desktop", "Desktop", [186., 305., 237., 73., 209., 214.])
//!         .series("mobile", "Mobile", [80., 200., 120., 190., 130., 140.])
//!         .legend(true)
//! }
//! # }
//! ```
//!
//! Every chart draws a quiet horizontal grid at nice tick values, the y axis's labels short
//! (`1.2k`, `3.4M`, or the host's own with `format_value`), the x axis's cut with an ellipsis (or every other one left out) when they
//! don't fit, an optional legend, and a tooltip with every series' value at the x under the
//! pointer (or a tap). It takes the focus: Left and Right move the tooltip between x values,
//! Home and End go to the ends, Escape hides it. Nothing animates, and nothing redraws until
//! the pointer reaches another x or a key moves it.
//!
//! The geometry is plain functions of the data and the plot's bounds ([`BarChart::bars`],
//! [`LineChart::lines`], [`AreaChart::areas`], [`curve_segments`], [`nice_ticks`]…), so a host or
//! a test can check it without a window.

use std::rc::Rc;

use gpui::{
    App, Background, Bounds, Context, Corners, ElementId, FocusHandle, FontWeight, Hsla,
    InteractiveElement, IntoElement, KeyBinding, MouseButton, MouseDownEvent, MouseMoveEvent,
    PaintQuad, ParentElement, Path, PathBuilder, Pixels, Point, Rems, RenderOnce, Role, ShapedLine,
    SharedString, StatefulInteractiveElement as _, StyleRefinement, Styled, Subscription,
    TextAlign, TruncateFrom, Window, actions, canvas, deferred, div, fill, linear_color_stop,
    linear_gradient, point, prelude::FluentBuilder as _, px, quad, relative, rems, size,
    transparent_black,
};

use crate::press::focus_ring;
use crate::theme::{ActiveTheme as _, Theme};

actions!(
    gpui_core_chart,
    [
        /// Moves the tooltip to the x value before (from none: the last).
        PreviousPoint,
        /// Moves the tooltip to the x value after (from none: the first).
        NextPoint,
        FirstPoint,
        LastPoint,
        /// Hides the tooltip.
        HideTooltip
    ]
);

const CONTEXT: &str = "GpuiCoreChart";

pub(crate) fn init(cx: &mut App) {
    let c = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("left", PreviousPoint, c),
        KeyBinding::new("right", NextPoint, c),
        KeyBinding::new("home", FirstPoint, c),
        KeyBinding::new("end", LastPoint, c),
        KeyBinding::new("escape", HideTooltip, c),
    ]);
}

fn css(v: f32) -> Rems {
    rems(v / 15.)
}

/// How many grid lines a chart aims for (Recharts' `tickCount`).
const TICKS: usize = 5;
/// Recharts' `barCategoryGap`: the space each side of a category's bars, of its band.
const CATEGORY_GAP: f32 = 0.1;
/// Recharts' `barGap`: between the bars of one category, in pixels.
const BAR_GAP: f32 = 4.;
/// The space kept between two x labels, in pixels.
const LABEL_GAP: f32 = 8.;
/// The space a line or an area chart keeps each side of its first and last points, in pixels
/// (shadcn's `margin={{ left: 12, right: 12 }}`).
const POINT_PAD: f32 = 12.;
/// A line's width, in pixels (Recharts' `strokeWidth={2}`).
const LINE_WIDTH: f32 = 2.;

// ---------------------------------------------------------------------------------------------
// Numbers

/// Round tick values from `min` to `max`: about `count` of them, a step of 1, 2, 2.5 or 5
/// times a power of ten, the first at or below `min` and the last at or above `max`. Empty if
/// either end isn't a number; `[0, 1]`'s ticks when both are 0.
pub fn nice_ticks(min: f64, max: f64, count: usize) -> Vec<f64> {
    if !min.is_finite() || !max.is_finite() {
        return Vec::new();
    }
    let (mut lo, mut hi) = if min <= max { (min, max) } else { (max, min) };
    if lo == hi {
        if lo == 0. {
            hi = 1.;
        } else {
            // One value: a span from it to zero.
            lo = lo.min(0.);
            hi = hi.max(0.);
        }
    }
    let intervals = count.max(2) - 1;
    let raw = (hi - lo) / intervals as f64;
    if !raw.is_finite() || raw <= 0. {
        return vec![lo, hi];
    }
    let magnitude = 10f64.powf(raw.log10().floor());
    for m in [1., 2., 2.5, 5., 10., 20., 25., 50., 100.] {
        let step = m * magnitude;
        let first = (lo / step).floor();
        let last = (hi / step).ceil();
        // One more interval than asked keeps the ends close to the data.
        if last - first <= (intervals + 1) as f64 {
            // Rounded to the step's precision, so 0.2 steps give 0.6, not 0.6000000000000001.
            let decimals = (-step.log10().floor() + 1.).clamp(0., 15.) as i32;
            let p = 10f64.powi(decimals);
            return (first as i64..=last as i64)
                .map(|k| (k as f64 * step * p).round() / p + 0.)
                .collect();
        }
    }
    vec![lo, hi]
}

/// `v` short, for an axis: at most three significant digits and a `k`, `M`, `B` or `T`
/// (`1.2k`, `3.4M`, `250k`, `0.25`). A dash for something that isn't a number.
pub fn compact_number(v: f64) -> String {
    if !v.is_finite() {
        return "–".into();
    }
    const UNITS: [(f64, &str); 5] = [(1., ""), (1e3, "k"), (1e6, "M"), (1e9, "B"), (1e12, "T")];
    let a = v.abs();
    let mut u = UNITS.iter().rposition(|(d, _)| a >= *d).unwrap_or(0);
    loop {
        let (div, suffix) = UNITS[u];
        let scaled = v / div;
        let s = scaled.abs();
        let decimals = if s >= 100. {
            0
        } else if s >= 10. {
            1
        } else if s >= 1. {
            2
        } else {
            3
        };
        let p = 10f64.powi(decimals);
        let rounded = (scaled * p).round() / p;
        // 999.95k rounds to 1000k: that's 1M.
        if rounded.abs() >= 1000. && u + 1 < UNITS.len() {
            u += 1;
            continue;
        }
        return format!(
            "{}{suffix}",
            trim(&format!("{:.*}", decimals as usize, rounded + 0.))
        );
    }
}

/// `v` in full for a tooltip: thousands grouped with commas, at most two decimals
/// (`1,234,567.89`). A dash for something that isn't a number.
pub fn format_value(v: f64) -> String {
    if !v.is_finite() {
        return "–".into();
    }
    let s = trim(&format!("{:.2}", v + 0.));
    let (sign, s) = s.strip_prefix('-').map_or(("", s.as_str()), |r| ("-", r));
    let (int, frac) = s.split_once('.').map_or((s, None), |(i, f)| (i, Some(f)));
    let mut grouped = String::new();
    for (i, ch) in int.chars().enumerate() {
        if i > 0 && (int.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    match frac {
        Some(f) => format!("{sign}{grouped}.{f}"),
        None => format!("{sign}{grouped}"),
    }
}

/// A decimal without its trailing zeros (and its point, if nothing's left after it); `-0` is 0.
fn trim(s: &str) -> String {
    let s = if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.')
    } else {
        s
    };
    if s == "-0" { "0".into() } else { s.into() }
}

/// Every how many x labels one is drawn: 1 when each fits in its band, more when they'd
/// crowd. A label takes its own width, but no more than `min_slot` (past that it's cut short
/// with an ellipsis rather than leaving more out), and a little space after it.
pub fn label_stride(band: Pixels, widest: Pixels, min_slot: Pixels) -> usize {
    let band = f32::from(band);
    if band.is_nan() || band <= 0. {
        return 1;
    }
    let need = f32::from(widest).min(f32::from(min_slot)) + LABEL_GAP;
    ((need / band).ceil() as usize).max(1)
}

// ---------------------------------------------------------------------------------------------
// Scales and layout

/// The value axis: its ends and its tick values. It always spans zero, so bars grow from a
/// zero line and negative values fall below it.
#[derive(Clone, Debug, PartialEq)]
pub struct ValueScale {
    pub min: f64,
    pub max: f64,
    pub ticks: Vec<f64>,
}

impl ValueScale {
    /// A scale for values from `lo` to `hi` (and zero), on nice ticks.
    pub fn new(lo: f64, hi: f64) -> Self {
        let (lo, hi) = (
            if lo.is_finite() { lo.min(0.) } else { 0. },
            if hi.is_finite() { hi.max(0.) } else { 0. },
        );
        let ticks = nice_ticks(lo, hi, TICKS);
        let min = ticks.first().copied().unwrap_or(0.);
        let max = ticks.last().copied().unwrap_or(1.);
        Self { min, max, ticks }
    }

    /// Where `v` is from the plot's top, 0 to 1.
    pub fn fraction(&self, v: f64) -> f32 {
        if self.max > self.min {
            ((self.max - v) / (self.max - self.min)) as f32
        } else {
            1.
        }
    }

    /// The y of `v` in `plot`.
    pub fn y(&self, v: f64, plot: Bounds<Pixels>) -> Pixels {
        plot.origin.y + plot.size.height * self.fraction(v)
    }
}

/// One bar as drawn: which series and category it shows, where, and which ends are rounded
/// (the end away from the zero line; in a stack, only the outermost bar's).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarRect {
    pub series: usize,
    pub category: usize,
    pub value: f64,
    pub bounds: Bounds<Pixels>,
    pub round_top: bool,
    pub round_bottom: bool,
}

#[derive(Clone)]
struct Series {
    key: SharedString,
    label: SharedString,
    values: Vec<f64>,
    color: Option<Hsla>,
}

/// What every chart shows: the categories along x and the series' values at each.
#[derive(Clone, Default)]
struct Data {
    categories: Vec<SharedString>,
    series: Vec<Series>,
}

impl Data {
    /// How many x values: the categories, or the longest series if it's longer.
    fn len(&self) -> usize {
        self.series
            .iter()
            .map(|s| s.values.len())
            .max()
            .unwrap_or(0)
            .max(self.categories.len())
    }

    /// Series `s`'s value at `i`, if it has a number there.
    fn value(&self, s: usize, i: usize) -> Option<f64> {
        self.series[s]
            .values
            .get(i)
            .copied()
            .filter(|v| v.is_finite())
    }

    /// Nothing to draw: no x values, or not one number.
    fn is_empty(&self) -> bool {
        let n = self.len();
        (0..self.series.len()).all(|s| (0..n).all(|i| self.value(s, i).is_none()))
    }

    /// The label of x value `i` (its number from 1 when the host gave none).
    fn category(&self, i: usize) -> SharedString {
        match self.categories.get(i) {
            Some(c) if !c.is_empty() => c.clone(),
            _ => format!("{}", i + 1).into(),
        }
    }

    /// The smallest and largest values, each series on its own.
    fn extent(&self) -> (f64, f64) {
        let n = self.len();
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for s in 0..self.series.len() {
            for i in 0..n {
                if let Some(v) = self.value(s, i) {
                    lo = lo.min(v);
                    hi = hi.max(v);
                }
            }
        }
        (lo, hi)
    }

    /// The lowest and highest a stack reaches: negatives stacked down from zero, positives up.
    fn stacked_extent(&self) -> (f64, f64) {
        let mut lo = 0f64;
        let mut hi = 0f64;
        for i in 0..self.len() {
            let (mut neg, mut pos) = (0., 0.);
            for s in 0..self.series.len() {
                match self.value(s, i) {
                    Some(v) if v < 0. => neg += v,
                    Some(v) => pos += v,
                    None => {}
                }
            }
            lo = lo.min(neg);
            hi = hi.max(pos);
        }
        (lo, hi)
    }

    /// Series `s`'s colour: its own, or the theme's chart colours in turn.
    fn color(&self, s: usize, theme: &Theme) -> Hsla {
        self.series[s].color.unwrap_or_else(|| theme.chart(s))
    }

    /// The top of series `s`'s area at `i` in a stack: its value and those of the series
    /// before it (a gap counts as zero there).
    fn cumulative(&self, s: usize, i: usize) -> f64 {
        (0..=s).map(|s| self.value(s, i).unwrap_or(0.)).sum()
    }

    /// The lowest and highest a stack of areas reaches.
    fn cumulative_extent(&self) -> (f64, f64) {
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for i in 0..self.len() {
            for s in 0..self.series.len() {
                let v = self.cumulative(s, i);
                lo = lo.min(v);
                hi = hi.max(v);
            }
        }
        (lo, hi)
    }

    /// What x value `i` shows, to assistive technology: "Feb: Desktop 305, Mobile 200".
    fn describe(&self, i: usize, options: &Options) -> String {
        let values: Vec<String> = (0..self.series.len())
            .filter_map(|s| {
                self.value(s, i)
                    .map(|v| format!("{} {}", self.series[s].label, options.value_text(v)))
            })
            .collect();
        format!("{}: {}", self.category(i), values.join(", "))
    }

    /// The chart in a sentence, to assistive technology.
    fn summary(&self, kind: &str, options: &Options) -> String {
        if self.is_empty() {
            return format!("{kind}, no data");
        }
        let names: Vec<&str> = self.series.iter().map(|s| s.label.as_ref()).collect();
        let names = match names.as_slice() {
            [] => String::new(),
            [one] => one.to_string(),
            [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
        };
        let n = self.len();
        let span = if n == 1 {
            format!("1 point, {}", self.category(0))
        } else {
            format!(
                "{n} points, {} to {}",
                self.category(0),
                self.category(n - 1)
            )
        };
        let (lo, hi) = self.extent();
        format!(
            "{kind} of {names}: {span}; values from {} to {}",
            options.value_text(lo),
            options.value_text(hi)
        )
    }
}

/// How x values sit along the plot.
#[derive(Clone, Copy, Debug, PartialEq)]
enum XScale {
    /// Each in an equal band, at its centre (a bar chart: Recharts' band scale).
    Band,
    /// Each at a point, the first and last [`POINT_PAD`] in from the ends (a line or an area:
    /// Recharts' point scale with shadcn's margins). A single point is in the middle.
    Point,
}

impl XScale {
    /// Where the points run from and to: the plot, less the padding a point scale keeps.
    fn span(self, plot: Bounds<Pixels>) -> (f32, f32) {
        let (left, w) = (f32::from(plot.origin.x), f32::from(plot.size.width));
        let pad = if self == XScale::Point && w > POINT_PAD * 4. {
            POINT_PAD
        } else {
            0.
        };
        (left + pad, w - pad * 2.)
    }

    /// The distance between neighbouring x values.
    fn spacing(self, plot: Bounds<Pixels>, n: usize) -> Pixels {
        let (_, w) = self.span(plot);
        px(match self {
            XScale::Band => w / n.max(1) as f32,
            XScale::Point if n > 1 => w / (n - 1) as f32,
            XScale::Point => w,
        })
    }

    /// The x of value `i` of `n`: its band's centre, or its point.
    fn x(self, plot: Bounds<Pixels>, n: usize, i: usize) -> Pixels {
        let (left, w) = self.span(plot);
        let step = f32::from(self.spacing(plot, n));
        px(match self {
            XScale::Band => left + step * (i as f32 + 0.5),
            XScale::Point if n > 1 => left + step * i as f32,
            XScale::Point => left + w / 2.,
        })
    }

    /// The x value nearest `x` (the nearest end's when it's outside the plot).
    fn nearest(self, plot: Bounds<Pixels>, n: usize, x: Pixels) -> Option<usize> {
        if n == 0 || plot.size.width <= px(0.) {
            return None;
        }
        let (left, _) = self.span(plot);
        let step = f32::from(self.spacing(plot, n));
        let f = (f32::from(x) - left) / step;
        let i = match self {
            XScale::Band => f.floor(),
            XScale::Point if n > 1 => f.round(),
            XScale::Point => 0.,
        };
        Some(if i.is_nan() || i < 0. {
            0
        } else {
            (i as usize).min(n - 1)
        })
    }
}

/// The band (a bar chart's slot) of x value `i` of `n` in `plot`.
fn band(plot: Bounds<Pixels>, n: usize, i: usize) -> Bounds<Pixels> {
    let w = plot.size.width / n.max(1) as f32;
    Bounds::new(
        point(plot.origin.x + w * i as f32, plot.origin.y),
        size(w, plot.size.height),
    )
}

/// A bar chart's bars in `plot`, by Recharts' rules: each category a band, 10% of it empty
/// each side; grouped bars side by side with 4 pixels between, or one stack per band.
fn bar_layout(
    data: &Data,
    scale: &ValueScale,
    stacked: bool,
    plot: Bounds<Pixels>,
) -> Vec<BarRect> {
    let n = data.len();
    let m = data.series.len();
    if n == 0 || m == 0 || data.is_empty() {
        return Vec::new();
    }
    let band_w = f32::from(plot.size.width) / n as f32;
    let offset = band_w * CATEGORY_GAP;
    let group = band_w - offset * 2.;
    let slots = if stacked { 1 } else { m };
    let mut gap = if slots > 1 { BAR_GAP } else { 0. };
    if (group - gap * (slots - 1) as f32) / (slots as f32) < 1. {
        gap = 0.;
    }
    let bar_w = ((group - gap * (slots - 1) as f32) / slots as f32).max(0.);
    let left = f32::from(plot.origin.x);
    let y = |v: f64| f32::from(scale.y(v, plot));
    let rect = |x: f32, from: f64, to: f64| {
        let (a, b) = (y(from), y(to));
        let top = a.min(b);
        Bounds::new(point(px(x), px(top)), size(px(bar_w), px((a - b).abs())))
    };
    let mut bars = Vec::new();
    for i in 0..n {
        let x0 = left + band_w * i as f32 + offset;
        if stacked {
            // The outermost bar each way is the one that's rounded.
            let last_pos = (0..m).rposition(|s| data.value(s, i).is_some_and(|v| v > 0.));
            let last_neg = (0..m).rposition(|s| data.value(s, i).is_some_and(|v| v < 0.));
            let (mut pos, mut neg) = (0., 0.);
            for s in 0..m {
                let Some(v) = data.value(s, i).filter(|v| *v != 0.) else {
                    continue;
                };
                let (from, to) = if v > 0. {
                    pos += v;
                    (pos - v, pos)
                } else {
                    neg += v;
                    (neg - v, neg)
                };
                bars.push(BarRect {
                    series: s,
                    category: i,
                    value: v,
                    bounds: rect(x0, from, to),
                    round_top: v > 0. && last_pos == Some(s),
                    round_bottom: v < 0. && last_neg == Some(s),
                });
            }
        } else {
            for s in 0..m {
                let Some(v) = data.value(s, i).filter(|v| *v != 0.) else {
                    continue;
                };
                bars.push(BarRect {
                    series: s,
                    category: i,
                    value: v,
                    bounds: rect(x0 + (bar_w + gap) * s as f32, 0., v),
                    round_top: v > 0.,
                    round_bottom: v < 0.,
                });
            }
        }
    }
    bars
}

/// How a line runs between its points.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Curve {
    /// Straight from point to point (Recharts' `type="linear"`).
    Linear,
    /// Smooth, and never past a point's value between two points (Recharts' and d3's
    /// `monotone`): a rise stays a rise, without the bumps a plain spline adds.
    #[default]
    Monotone,
}

/// One piece of a line: a cubic Bézier from `from` to `to`. A straight piece has its control
/// points on the line, a third and two thirds of the way along.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CurveSegment {
    pub from: Point<Pixels>,
    pub c1: Point<Pixels>,
    pub c2: Point<Pixels>,
    pub to: Point<Pixels>,
}

impl CurveSegment {
    /// The same piece, the other way round.
    fn reversed(self) -> Self {
        Self {
            from: self.to,
            c1: self.c2,
            c2: self.c1,
            to: self.from,
        }
    }
}

/// The pieces of a line through `points` (in increasing x), straight or monotone. Two points
/// are joined straight either way; fewer make no line.
pub fn curve_segments(points: &[Point<Pixels>], curve: Curve) -> Vec<CurveSegment> {
    let n = points.len();
    if n < 2 {
        return Vec::new();
    }
    let x: Vec<f32> = points.iter().map(|p| f32::from(p.x)).collect();
    let y: Vec<f32> = points.iter().map(|p| f32::from(p.y)).collect();
    let at = |x: f32, y: f32| point(px(x), px(y));
    if curve == Curve::Linear || n == 2 {
        return points
            .windows(2)
            .map(|w| {
                let (a, b) = (w[0], w[1]);
                CurveSegment {
                    from: a,
                    c1: a + (b - a) / 3.,
                    c2: a + (b - a) * (2. / 3.),
                    to: b,
                }
            })
            .collect();
    }
    // d3's curveMonotoneX (Steffen's method): a tangent at each point no steeper than its
    // neighbouring slopes allow, and flat at a peak or a trough.
    let h: Vec<f32> = (0..n - 1).map(|i| x[i + 1] - x[i]).collect();
    let slope: Vec<f32> = (0..n - 1)
        .map(|i| {
            if h[i] > 0. {
                (y[i + 1] - y[i]) / h[i]
            } else {
                0.
            }
        })
        .collect();
    let mut t = vec![0f32; n];
    for i in 1..n - 1 {
        let (s0, s1, h0, h1) = (slope[i - 1], slope[i], h[i - 1], h[i]);
        if s0 * s1 > 0. && h0 + h1 > 0. {
            let p = (s0 * h1 + s1 * h0) / (h0 + h1);
            t[i] = 2. * s0.signum() * s0.abs().min(s1.abs()).min(0.5 * p.abs());
        }
    }
    t[0] = (3. * slope[0] - t[1]) / 2.;
    t[n - 1] = (3. * slope[n - 2] - t[n - 2]) / 2.;
    (0..n - 1)
        .map(|i| {
            let dx = h[i] / 3.;
            CurveSegment {
                from: points[i],
                c1: at(x[i] + dx, y[i] + dx * t[i]),
                c2: at(x[i + 1] - dx, y[i + 1] - dx * t[i + 1]),
                to: points[i + 1],
            }
        })
        .collect()
}

/// One unbroken run of a series as drawn: its line's points and, for an area, the points of
/// its bottom edge (the zero line, or the top of the area under it in a stack) at the same x.
#[derive(Clone, Debug, PartialEq)]
pub struct SeriesRun {
    pub series: usize,
    /// Which x value the run starts at.
    pub start: usize,
    pub points: Vec<Point<Pixels>>,
    /// Empty for a line.
    pub base: Vec<Point<Pixels>>,
}

/// Each series' runs of values with no gap between, as points in `plot`.
fn line_runs(data: &Data, scale: &ValueScale, plot: Bounds<Pixels>) -> Vec<SeriesRun> {
    let n = data.len();
    let mut runs = Vec::new();
    if data.is_empty() {
        return runs;
    }
    for s in 0..data.series.len() {
        let mut run: Option<SeriesRun> = None;
        for i in 0..n {
            match data.value(s, i) {
                Some(v) => {
                    let p = point(XScale::Point.x(plot, n, i), scale.y(v, plot));
                    run.get_or_insert_with(|| SeriesRun {
                        series: s,
                        start: i,
                        points: Vec::new(),
                        base: Vec::new(),
                    })
                    .points
                    .push(p);
                }
                None => runs.extend(run.take()),
            }
        }
        runs.extend(run);
    }
    runs
}

/// Each series' areas in `plot`: from its line down to the zero line, or, stacked, from the
/// top of the stack under it (gaps count as zero in a stack, so each series is one run).
fn area_runs(
    data: &Data,
    scale: &ValueScale,
    stacked: bool,
    plot: Bounds<Pixels>,
) -> Vec<SeriesRun> {
    let n = data.len();
    if !stacked {
        let zero = scale.y(0., plot);
        let mut runs = line_runs(data, scale, plot);
        for r in &mut runs {
            r.base = r.points.iter().map(|p| point(p.x, zero)).collect();
        }
        return runs;
    }
    if data.is_empty() {
        return Vec::new();
    }
    (0..data.series.len())
        .filter(|s| (0..n).any(|i| data.value(*s, i).is_some()))
        .map(|s| {
            let at = |i: usize, v: f64| point(XScale::Point.x(plot, n, i), scale.y(v, plot));
            SeriesRun {
                series: s,
                start: 0,
                points: (0..n).map(|i| at(i, data.cumulative(s, i))).collect(),
                base: (0..n)
                    .map(|i| {
                        at(
                            i,
                            if s == 0 {
                                0.
                            } else {
                                data.cumulative(s - 1, i)
                            },
                        )
                    })
                    .collect(),
            }
        })
        .collect()
}

/// A line along `segments`, or an area's outline (along `segments`, back along `base`).
fn build_path(
    mut builder: PathBuilder,
    start: Point<Pixels>,
    segments: &[CurveSegment],
    base: Option<&[CurveSegment]>,
) -> Option<Path<Pixels>> {
    builder.move_to(start);
    for s in segments {
        builder.cubic_bezier_to(s.to, s.c1, s.c2);
    }
    if let Some(base) = base {
        if let Some(last) = base.last() {
            builder.line_to(last.to);
        }
        for s in base.iter().rev().map(|s| s.reversed()) {
            builder.cubic_bezier_to(s.to, s.c1, s.c2);
        }
        builder.close();
    }
    builder.build().ok()
}

/// A dot: a circle of radius `r` at `c`.
fn circle(c: Point<Pixels>, r: Pixels, color: Hsla) -> PaintQuad {
    quad(
        Bounds::new(c - point(r, r), size(r * 2., r * 2.)),
        r,
        color,
        px(0.),
        transparent_black(),
        Default::default(),
    )
}

/// How an area is filled.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AreaFill {
    /// Its colour at 40% (shadcn's `fillOpacity={0.4}`).
    #[default]
    Solid,
    /// Its colour fading down the area, as shadcn's gradient example: 80% to 10% of the 40%.
    Gradient,
}

fn area_background(color: Hsla, fill: AreaFill) -> Background {
    match fill {
        AreaFill::Solid => color.opacity(0.4).into(),
        AreaFill::Gradient => linear_gradient(
            180.,
            linear_color_stop(color.opacity(0.32), 0.05),
            linear_color_stop(color.opacity(0.04), 0.95),
        ),
    }
}

// ---------------------------------------------------------------------------------------------
// The shared builder

/// Writes a value for the y axis and the tooltip.
type Format = Rc<dyn Fn(f64) -> SharedString>;

/// What every chart with axes takes besides its data.
#[derive(Clone)]
struct Options {
    grid: bool,
    x_axis: bool,
    y_axis: bool,
    legend: bool,
    tooltip: bool,
    label: Option<SharedString>,
    empty_text: SharedString,
    format: Option<Format>,
}

impl Options {
    /// A value in the tooltip (and to assistive technology): the host's way, or in full.
    fn value_text(&self, v: f64) -> SharedString {
        match &self.format {
            Some(f) => f(v),
            None => format_value(v).into(),
        }
    }

    /// A tick's label on the y axis: the host's way, or short.
    fn tick_text(&self, v: f64) -> SharedString {
        match &self.format {
            Some(f) => f(v),
            None => compact_number(v).into(),
        }
    }
}

impl Default for Options {
    fn default() -> Self {
        Self {
            grid: true,
            x_axis: true,
            y_axis: true,
            legend: false,
            tooltip: true,
            label: None,
            empty_text: "No data".into(),
            format: None,
        }
    }
}

/// The builder methods every chart with axes shares.
macro_rules! cartesian_builders {
    ($ty:ty, $kind:expr, $x:expr) => {
        impl $ty {
            /// How it writes a value, in the tooltip and on the y axis (`|v| format!("${v}")
            /// .into()`). Without one, the tooltip has it in full (`1,234,567`) and the axis short
            /// (`1.2M`).
            pub fn format_value(mut self, format: impl Fn(f64) -> SharedString + 'static) -> Self {
                self.options.format = Some(Rc::new(format));
                self
            }

            /// The y axis's labels, one for each grid line from the bottom up.
            pub fn tick_labels(&self) -> Vec<SharedString> {
                self.scale()
                    .ticks
                    .iter()
                    .map(|t| self.options.tick_text(*t))
                    .collect()
            }

            /// The x value nearest `x` in a plot with these bounds (the area inside the axes):
            /// where the pointer puts the tooltip. None without data.
            pub fn nearest(&self, plot: Bounds<Pixels>, x: Pixels) -> Option<usize> {
                if self.data.is_empty() {
                    return None;
                }
                $x.nearest(plot, self.data.len(), x)
            }

            /// What it tells assistive technology it shows, unless given a label.
            pub fn accessibility_summary(&self) -> String {
                self.data.summary($kind, &self.options)
            }

            /// The x values' labels, in order ("Jan", "Feb"…). Labels too long for their space
            /// are cut short with an ellipsis.
            pub fn categories(
                mut self,
                categories: impl IntoIterator<Item = impl Into<SharedString>>,
            ) -> Self {
                self.data.categories = categories.into_iter().map(Into::into).collect();
                self
            }

            /// A series: `key` names it for [`series_color`](Self::series_color), `label` is
            /// what the legend and the tooltip call it, and `values` are its values at each x
            /// in turn (anything not a number, as `f64::NAN`, is a gap). Coloured with the
            /// theme's `chart_1` to `chart_5` in the order added, unless given a colour.
            pub fn series(
                mut self,
                key: impl Into<SharedString>,
                label: impl Into<SharedString>,
                values: impl IntoIterator<Item = impl Into<f64>>,
            ) -> Self {
                self.data.series.push(Series {
                    key: key.into(),
                    label: label.into(),
                    values: values.into_iter().map(Into::into).collect(),
                    color: None,
                });
                self
            }

            /// The colour of the series `key` (otherwise the theme's chart colour for its place).
            pub fn series_color(
                mut self,
                key: impl Into<SharedString>,
                color: impl Into<Hsla>,
            ) -> Self {
                let key = key.into();
                let color = color.into();
                for s in self.data.series.iter_mut().filter(|s| s.key == key) {
                    s.color = Some(color);
                }
                self
            }

            /// Whether it draws the horizontal grid lines (it does unless told not to).
            pub fn grid(mut self, grid: bool) -> Self {
                self.options.grid = grid;
                self
            }

            /// Whether it labels the x values below the plot (it does unless told not to).
            pub fn x_axis(mut self, show: bool) -> Self {
                self.options.x_axis = show;
                self
            }

            /// Whether it labels the grid's values left of the plot (it does unless told not to).
            pub fn y_axis(mut self, show: bool) -> Self {
                self.options.y_axis = show;
                self
            }

            /// Whether it shows a legend under the plot: each series' colour and label.
            pub fn legend(mut self, show: bool) -> Self {
                self.options.legend = show;
                self
            }

            /// Whether the pointer, a tap or the keyboard shows a tooltip with every series'
            /// value at an x (it does unless told not to; the x is still marked without one).
            pub fn tooltip(mut self, show: bool) -> Self {
                self.options.tooltip = show;
                self
            }

            /// What it shows, to assistive technology (otherwise a summary made from the data:
            /// the series, the x values' range and the values' range).
            pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
                self.options.label = Some(label.into());
                self
            }

            /// What it says when there's nothing to draw ("No data" unless told otherwise).
            pub fn empty_text(mut self, text: impl Into<SharedString>) -> Self {
                self.options.empty_text = text.into();
                self
            }
        }

        impl Styled for $ty {
            fn style(&mut self) -> &mut StyleRefinement {
                &mut self.style
            }
        }
    };
}

// ---------------------------------------------------------------------------------------------
// Bar chart

/// shadcn's bar chart: a band for each category, the series' bars side by side in it (or
/// [`stacked`](Self::stacked)), their ends rounded away from the zero line; negative values
/// hang below it. Full width and 250 pixels tall unless styled otherwise.
#[derive(IntoElement)]
pub struct BarChart {
    id: ElementId,
    data: Data,
    options: Options,
    stacked: bool,
    radius: Rems,
    style: StyleRefinement,
}

cartesian_builders!(BarChart, "Bar chart", XScale::Band);

impl BarChart {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            data: Data::default(),
            options: Options::default(),
            stacked: false,
            radius: css(4.),
            style: StyleRefinement::default(),
        }
    }

    /// Stacks the series in one bar per category instead of side by side (positive values up
    /// from zero, negative ones down).
    pub fn stacked(mut self, stacked: bool) -> Self {
        self.stacked = stacked;
        self
    }

    /// The radius of the bars' rounded ends (4 pixels unless told otherwise).
    pub fn radius(mut self, radius: impl Into<Rems>) -> Self {
        self.radius = radius.into();
        self
    }

    /// The value axis this data gets: its ends and grid lines.
    pub fn scale(&self) -> ValueScale {
        let (lo, hi) = if self.stacked {
            self.data.stacked_extent()
        } else {
            self.data.extent()
        };
        ValueScale::new(lo, hi)
    }

    /// The bars as drawn in a plot with these bounds (the area inside the axes).
    pub fn bars(&self, plot: Bounds<Pixels>) -> Vec<BarRect> {
        bar_layout(&self.data, &self.scale(), self.stacked, plot)
    }
}

impl RenderOnce for BarChart {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let scale = self.scale();
        let summary = self.accessibility_summary();
        let marks = Marks::Bars {
            stacked: self.stacked,
            radius: self.radius,
        };
        render_cartesian(
            Cartesian {
                id: self.id,
                data: Rc::new(self.data),
                options: self.options,
                scale,
                summary,
                marks,
                style: self.style,
            },
            window,
            cx,
        )
    }
}

// ---------------------------------------------------------------------------------------------
// Line chart

/// shadcn's line chart: each series a two-pixel line through its values, smooth
/// ([`Curve::Monotone`]) unless told [`Curve::Linear`], with optional dots. A gap in a series
/// breaks its line. The tooltip's x is marked with a thin line and a dot on each series. Full
/// width and 250 pixels tall unless styled otherwise.
#[derive(IntoElement)]
pub struct LineChart {
    id: ElementId,
    data: Data,
    options: Options,
    curve: Curve,
    dots: bool,
    style: StyleRefinement,
}

cartesian_builders!(LineChart, "Line chart", XScale::Point);

impl LineChart {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            data: Data::default(),
            options: Options::default(),
            curve: Curve::Monotone,
            dots: false,
            style: StyleRefinement::default(),
        }
    }

    /// How the lines run between points (smooth unless told otherwise).
    pub fn curve(mut self, curve: Curve) -> Self {
        self.curve = curve;
        self
    }

    /// Whether each value gets a dot on its line (a value alone between gaps always has one).
    pub fn dots(mut self, dots: bool) -> Self {
        self.dots = dots;
        self
    }

    /// The value axis this data gets: its ends and grid lines.
    pub fn scale(&self) -> ValueScale {
        let (lo, hi) = self.data.extent();
        ValueScale::new(lo, hi)
    }

    /// The lines as drawn in a plot with these bounds: each series' unbroken runs of points.
    pub fn lines(&self, plot: Bounds<Pixels>) -> Vec<SeriesRun> {
        line_runs(&self.data, &self.scale(), plot)
    }
}

impl RenderOnce for LineChart {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let scale = self.scale();
        let summary = self.accessibility_summary();
        let marks = Marks::Lines {
            curve: self.curve,
            dots: self.dots,
        };
        render_cartesian(
            Cartesian {
                id: self.id,
                data: Rc::new(self.data),
                options: self.options,
                scale,
                summary,
                marks,
                style: self.style,
            },
            window,
            cx,
        )
    }
}

// ---------------------------------------------------------------------------------------------
// Area chart

/// shadcn's area chart: each series a line, as [`LineChart`] draws it, filled down to the zero
/// line in its colour at 40% (or with a [`AreaFill::Gradient`]), or [`stacked`](Self::stacked)
/// one on another. Full width and 250 pixels tall unless styled otherwise.
#[derive(IntoElement)]
pub struct AreaChart {
    id: ElementId,
    data: Data,
    options: Options,
    curve: Curve,
    dots: bool,
    stacked: bool,
    fill: AreaFill,
    style: StyleRefinement,
}

cartesian_builders!(AreaChart, "Area chart", XScale::Point);

impl AreaChart {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            data: Data::default(),
            options: Options::default(),
            curve: Curve::Monotone,
            dots: false,
            stacked: false,
            fill: AreaFill::Solid,
            style: StyleRefinement::default(),
        }
    }

    /// How the lines run between points (smooth unless told otherwise).
    pub fn curve(mut self, curve: Curve) -> Self {
        self.curve = curve;
        self
    }

    /// Whether each value gets a dot on its line.
    pub fn dots(mut self, dots: bool) -> Self {
        self.dots = dots;
        self
    }

    /// Stacks each series' area on the ones before it instead of each from zero (a gap counts
    /// as zero in a stack).
    pub fn stacked(mut self, stacked: bool) -> Self {
        self.stacked = stacked;
        self
    }

    /// How the areas are filled (flat at 40% unless told otherwise).
    pub fn fill(mut self, fill: AreaFill) -> Self {
        self.fill = fill;
        self
    }

    /// The value axis this data gets: its ends and grid lines.
    pub fn scale(&self) -> ValueScale {
        let (lo, hi) = if self.stacked {
            self.data.cumulative_extent()
        } else {
            self.data.extent()
        };
        ValueScale::new(lo, hi)
    }

    /// The areas as drawn in a plot with these bounds: each series' runs, its line's points
    /// and its bottom edge's.
    pub fn areas(&self, plot: Bounds<Pixels>) -> Vec<SeriesRun> {
        area_runs(&self.data, &self.scale(), self.stacked, plot)
    }
}

impl RenderOnce for AreaChart {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let scale = self.scale();
        let summary = self.accessibility_summary();
        let marks = Marks::Areas {
            curve: self.curve,
            dots: self.dots,
            stacked: self.stacked,
            fill: self.fill,
        };
        render_cartesian(
            Cartesian {
                id: self.id,
                data: Rc::new(self.data),
                options: self.options,
                scale,
                summary,
                marks,
                style: self.style,
            },
            window,
            cx,
        )
    }
}

// ---------------------------------------------------------------------------------------------
// Drawing

/// What a chart draws in its plot.
#[derive(Clone, Copy)]
enum Marks {
    Bars {
        stacked: bool,
        radius: Rems,
    },
    Lines {
        curve: Curve,
        dots: bool,
    },
    Areas {
        curve: Curve,
        dots: bool,
        stacked: bool,
        fill: AreaFill,
    },
}

impl Marks {
    fn x_scale(self) -> XScale {
        match self {
            Marks::Bars { .. } => XScale::Band,
            Marks::Lines { .. } | Marks::Areas { .. } => XScale::Point,
        }
    }
}

/// Something painted in the plot, in order.
enum Paint {
    Quad(PaintQuad),
    Path(Path<Pixels>, Background),
}

/// A chart with axes, ready to draw.
struct Cartesian {
    id: ElementId,
    data: Rc<Data>,
    options: Options,
    scale: ValueScale,
    summary: String,
    marks: Marks,
    style: StyleRefinement,
}

/// What a chart keeps between frames: its focus, the x value its tooltip is at, and where its
/// plot was last drawn (for the pointer).
struct ChartState {
    focus: FocusHandle,
    active: Option<usize>,
    plot: Bounds<Pixels>,
    _blur: Subscription,
}

impl ChartState {
    /// Moves the tooltip, redrawing only if it moved.
    fn set(&mut self, active: Option<usize>, cx: &mut Context<Self>) {
        if self.active != active {
            self.active = active;
            cx.notify();
        }
    }
}

fn render_cartesian(chart: Cartesian, window: &mut Window, cx: &mut App) -> gpui::AnyElement {
    crate::init_soon(cx);
    let theme = cx.theme().clone();
    let Cartesian {
        id,
        data,
        options,
        scale,
        summary,
        marks,
        style,
    } = chart;
    let state = window.use_keyed_state(id.clone(), cx, |window, cx| {
        let focus = cx.focus_handle();
        // The tooltip the keyboard showed goes when the focus does.
        let blur = cx.on_blur(&focus, window, |s: &mut ChartState, _, cx| s.set(None, cx));
        ChartState {
            focus,
            active: None,
            plot: Bounds::default(),
            _blur: blur,
        }
    });
    let n = data.len();
    let empty = data.is_empty();
    let xs = marks.x_scale();
    let (focus, active, last_plot) = {
        let s = state.read(cx);
        (
            s.focus.clone(),
            s.active.filter(|i| *i < n && !empty),
            s.plot,
        )
    };
    let name = id.to_string();
    let label = options.label.clone().unwrap_or_else(|| summary.into());

    let mut root = div()
        .id(id.clone())
        .key_context(CONTEXT)
        .role(Role::Figure)
        .aria_label(label)
        .relative()
        .flex()
        .flex_col()
        .w_full()
        .min_w_0()
        .h(css(250.))
        .pt(css(8.))
        .text_size(theme.text.xs)
        .rounded(theme.radius_md());
    gpui::Refineable::refine(root.style(), &style);

    if empty {
        return root
            .child(
                div()
                    .debug_selector({
                        let name = name.clone();
                        move || format!("{name}-empty")
                    })
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(theme.muted_foreground)
                    .child(options.empty_text),
            )
            .into_any_element();
    }

    // Moves the tooltip to the x value `to` gives from where it is.
    let mover = |to: fn(Option<usize>, usize) -> usize| {
        let state = state.clone();
        move |_: &mut Window, cx: &mut App| {
            state.update(cx, |s, cx| {
                let at = s.active.filter(|i| *i < n);
                s.set(Some(to(at, n)), cx)
            });
        }
    };
    let next = mover(|at, n| at.map_or(0, |i| (i + 1).min(n - 1)));
    let prev = mover(|at, n| at.map_or(n - 1, |i| i.saturating_sub(1)));
    let first = mover(|_, _| 0);
    let last = mover(|_, n| n - 1);
    let ring = focus_ring(cx);
    root = root
        .track_focus(&focus.tab_stop(true))
        .focus_visible(move |s| s.shadow(vec![ring]))
        .aria_description("Left and Right move between values; Escape hides them.")
        .when_some(active, |d, i| d.aria_value(data.describe(i, &options)))
        .on_action(move |_: &NextPoint, w, cx| next(w, cx))
        .on_action(move |_: &PreviousPoint, w, cx| prev(w, cx))
        .on_action(move |_: &FirstPoint, w, cx| first(w, cx))
        .on_action(move |_: &LastPoint, w, cx| last(w, cx))
        .on_action({
            let state = state.clone();
            move |_: &HideTooltip, _, cx| {
                // With nothing to hide, Escape goes on to whatever else wants it (a dialog).
                if state.read(cx).active.is_some() {
                    state.update(cx, |s, cx| s.set(None, cx));
                } else {
                    cx.propagate();
                }
            }
        });

    let colors: Rc<Vec<Hsla>> = Rc::new(
        (0..data.series.len())
            .map(|s| data.color(s, &theme))
            .collect(),
    );

    // The y axis: each tick's short label, right-aligned against the plot, as wide as the
    // widest one.
    let x_axis_h = css(24.);
    let y_axis = options.y_axis.then(|| {
        let labels: Vec<SharedString> = scale.ticks.iter().map(|t| options.tick_text(*t)).collect();
        let rem = window.rem_size();
        let font_size = theme.text.xs.to_pixels(rem);
        let mut style = window.text_style();
        style.font_family = theme.font_family.clone();
        let widest = labels
            .iter()
            .map(|l| {
                window
                    .text_system()
                    .shape_line(l.clone(), font_size, &[style.to_run(l.len())], None)
                    .width
            })
            .fold(px(0.), Pixels::max);
        let margin = css(8.).to_pixels(rem);
        div()
            .relative()
            .flex_none()
            .w(widest + margin)
            .when(options.x_axis, |d| d.mb(x_axis_h))
            .text_color(theme.muted_foreground)
            .children(scale.ticks.iter().zip(labels).map(|(t, l)| {
                div()
                    .absolute()
                    .right_0()
                    .top(relative(scale.fraction(*t)))
                    .mt(css(-8.))
                    .h(css(16.))
                    .pr(margin)
                    .flex()
                    .items_center()
                    .whitespace_nowrap()
                    .child(l)
            }))
    });

    // The plot: grid, the tooltip's band and the marks, painted; the pointer moves the tooltip.
    let plot_canvas = {
        let state = state.clone();
        let data = data.clone();
        let colors = colors.clone();
        let scale = scale.clone();
        let grid = options.grid;
        let border = theme.border;
        let muted = theme.muted;
        let background = theme.background;
        canvas(
            move |bounds, window, cx| {
                state.update(cx, |s, _| s.plot = bounds);
                let rem = window.rem_size();
                let mut paints = Vec::new();
                if grid {
                    for t in &scale.ticks {
                        let y = scale.y(*t, bounds);
                        // The zero line, when there's something below it, a little stronger.
                        let color = if *t == 0. && scale.min < 0. {
                            border
                        } else {
                            border.opacity(0.5)
                        };
                        paints.push(Paint::Quad(fill(
                            Bounds::new(
                                point(bounds.origin.x, y - px(0.5)),
                                size(bounds.size.width, px(1.)),
                            ),
                            color,
                        )));
                    }
                }
                // The tooltip's x: a muted band behind bars, a thin line through points.
                if let Some(i) = active {
                    paints.push(Paint::Quad(match xs {
                        XScale::Band => fill(band(bounds, n, i), muted),
                        XScale::Point => fill(
                            Bounds::new(
                                point(xs.x(bounds, n, i) - px(0.5), bounds.origin.y),
                                size(px(1.), bounds.size.height),
                            ),
                            border,
                        ),
                    }));
                }
                let (runs, curve, dots) = match marks {
                    Marks::Bars { stacked, radius } => {
                        let r = radius.to_pixels(rem);
                        for b in bar_layout(&data, &scale, stacked, bounds) {
                            let r = r.min(b.bounds.size.width / 2.).min(b.bounds.size.height);
                            let (top, bottom) = (
                                if b.round_top { r } else { px(0.) },
                                if b.round_bottom { r } else { px(0.) },
                            );
                            paints.push(Paint::Quad(quad(
                                b.bounds,
                                Corners {
                                    top_left: top,
                                    top_right: top,
                                    bottom_right: bottom,
                                    bottom_left: bottom,
                                },
                                colors[b.series],
                                px(0.),
                                transparent_black(),
                                Default::default(),
                            )));
                        }
                        (Vec::new(), Curve::Linear, false)
                    }
                    Marks::Lines { curve, dots } => (line_runs(&data, &scale, bounds), curve, dots),
                    Marks::Areas {
                        curve,
                        dots,
                        stacked,
                        fill,
                    } => {
                        let runs = area_runs(&data, &scale, stacked, bounds);
                        for r in &runs {
                            let top = curve_segments(&r.points, curve);
                            let base = curve_segments(&r.base, curve);
                            if let Some(path) =
                                build_path(PathBuilder::fill(), r.points[0], &top, Some(&base))
                            {
                                paints.push(Paint::Path(
                                    path,
                                    area_background(colors[r.series], fill),
                                ));
                            }
                        }
                        (runs, curve, dots)
                    }
                };
                // Lines over the fills, then dots over the lines.
                for r in &runs {
                    let segments = curve_segments(&r.points, curve);
                    let stroke = PathBuilder::stroke(px(LINE_WIDTH));
                    if let Some(path) = build_path(stroke, r.points[0], &segments, None) {
                        paints.push(Paint::Path(path, colors[r.series].into()));
                    }
                }
                let dot = css(3.).to_pixels(rem);
                for r in &runs {
                    if dots || r.points.len() == 1 {
                        for p in &r.points {
                            paints.push(Paint::Quad(circle(*p, dot, colors[r.series])));
                        }
                    }
                }
                // The tooltip's x: a bigger dot on each line, ringed in the page's colour.
                if let Some(i) = active {
                    let r = css(4.).to_pixels(rem);
                    for run in &runs {
                        if let Some(p) = i
                            .checked_sub(run.start)
                            .and_then(|k| run.points.get(k))
                            .filter(|_| data.value(run.series, i).is_some())
                        {
                            paints.push(Paint::Quad(circle(*p, r + px(2.), background)));
                            paints.push(Paint::Quad(circle(*p, r, colors[run.series])));
                        }
                    }
                }
                paints
            },
            |_, paints, window, _| {
                for p in paints {
                    match p {
                        Paint::Quad(q) => window.paint_quad(q),
                        Paint::Path(path, background) => window.paint_path(path, background),
                    }
                }
            },
        )
        .absolute()
        .size_full()
    };

    let tooltip = (options.tooltip && active.is_some()).then(|| {
        let i = active.unwrap_or(0);
        // Where its x is across the plot, as last drawn.
        let frac = if last_plot.size.width > px(0.) {
            ((xs.x(last_plot, n, i) - last_plot.origin.x) / last_plot.size.width).clamp(0., 1.)
        } else {
            (i as f32 + 0.5) / n as f32
        };
        let rows = (0..data.series.len()).filter_map(|s| {
            data.value(s, i).map(|v| {
                div()
                    .flex()
                    .items_center()
                    .gap(css(8.))
                    .child(
                        div()
                            .flex_none()
                            .size(css(10.))
                            .rounded(css(2.))
                            .bg(colors[s]),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_color(theme.muted_foreground)
                            .child(data.series[s].label.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .font_family(theme.mono_font_family.clone())
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.foreground)
                            .child(options.value_text(v)),
                    )
            })
        });
        let name = name.clone();
        let card = div()
            .debug_selector(move || format!("{name}-tooltip-{i}"))
            .absolute()
            .top(css(4.))
            // Beside the x value, on the side with more room.
            .map(|d| {
                if frac <= 0.5 {
                    d.left(relative(frac)).ml(css(10.))
                } else {
                    d.right(relative(1. - frac)).mr(css(10.))
                }
            })
            .min_w(css(128.))
            .max_w(css(240.))
            .flex()
            .flex_col()
            .gap(css(6.))
            .px(css(10.))
            .py(css(6.))
            .rounded(theme.radius_lg())
            .border_1()
            .border_color(theme.border.opacity(0.5))
            .bg(theme.background)
            .shadow_lg()
            .child(
                div()
                    .overflow_hidden()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.foreground)
                    .child(data.category(i)),
            )
            .children(rows);
        deferred(card).with_priority(1)
    });

    let plot = div()
        .id(ElementId::NamedChild(
            std::sync::Arc::new(id.clone()),
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
            let state = state.clone();
            move |e: &MouseMoveEvent, _, cx| {
                state.update(cx, |s, cx| {
                    let i = xs.nearest(s.plot, n, e.position.x);
                    s.set(i, cx)
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
        // A tap (or a click) shows the tooltip there, as there's no hover on a touch screen.
        .on_mouse_down(MouseButton::Left, {
            let state = state.clone();
            move |e: &MouseDownEvent, _, cx| {
                state.update(cx, |s, cx| {
                    let i = xs.nearest(s.plot, n, e.position.x);
                    s.set(i, cx)
                });
            }
        })
        // A tap anywhere else hides it again.
        .on_mouse_down_out({
            let state = state.clone();
            move |_, _, cx| state.update(cx, |s, cx| s.set(None, cx))
        })
        .child(plot_canvas)
        .children(tooltip);

    // The x axis: each value's label under its band, cut short or thinned out to fit.
    let x_axis = options.x_axis.then(|| {
        let data = data.clone();
        div()
            .h(x_axis_h)
            .flex_none()
            .text_color(theme.muted_foreground)
            .child(
                canvas(
                    move |bounds, window, _| x_labels(&data, xs, bounds, window),
                    |_, (lines, line_height), window, cx| {
                        for (origin, line) in lines {
                            let _ =
                                line.paint(origin, line_height, TextAlign::Left, None, window, cx);
                        }
                    },
                )
                .size_full(),
            )
    });

    let legend = options.legend.then(|| {
        div()
            .flex()
            .flex_wrap()
            .justify_center()
            .gap_x(css(16.))
            .gap_y(css(4.))
            .pt(css(12.))
            .text_color(theme.foreground)
            .children(data.series.iter().enumerate().map(|(s, series)| {
                div()
                    .flex()
                    .items_center()
                    .gap(css(6.))
                    .min_w_0()
                    .child(
                        div()
                            .flex_none()
                            .size(css(8.))
                            .rounded(css(2.))
                            .bg(colors[s]),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .child(series.label.clone()),
                    )
            }))
    });

    root.child(
        div().flex().flex_1().min_h_0().children(y_axis).child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(plot)
                .children(x_axis),
        ),
    )
    .children(legend)
    .into_any_element()
}

/// The x labels to paint in `bounds` (the axis under the plot): each under its x, as many as
/// fit, each cut short with an ellipsis if it's wider than its slot.
fn x_labels(
    data: &Data,
    xs: XScale,
    bounds: Bounds<Pixels>,
    window: &mut Window,
) -> (Vec<(Point<Pixels>, ShapedLine)>, Pixels) {
    let n = data.len();
    let line_height = window.line_height();
    if n == 0 {
        return (Vec::new(), line_height);
    }
    let style = window.text_style();
    let rem = window.rem_size();
    let font_size = style.font_size.to_pixels(rem);
    let text_system = window.text_system().clone();
    let shape = |text: SharedString| {
        let run = style.to_run(text.len());
        text_system.shape_line(text, font_size, &[run], None)
    };
    let labels: Vec<SharedString> = (0..n).map(|i| data.category(i)).collect();
    let lines: Vec<ShapedLine> = labels.iter().map(|l| shape(l.clone())).collect();
    let widest = lines.iter().map(|l| l.width).fold(px(0.), Pixels::max);
    let band_w = xs.spacing(bounds, n);
    let stride = label_stride(band_w, widest, css(40.).to_pixels(rem));
    let top = bounds.origin.y + css(6.).to_pixels(rem);
    let mut out = Vec::new();
    for i in (0..n).step_by(stride) {
        // Its space: `stride` bands centred on its own, inside the axis, less the gap.
        let centre = xs.x(bounds, n, i);
        let half = band_w * stride as f32 / 2.;
        let from = (centre - half).max(bounds.origin.x);
        let to = (centre + half).min(bounds.right());
        let room = (to - from - px(LABEL_GAP)).max(px(0.));
        let mut line = lines[i].clone();
        if line.width > room {
            let run = [style.to_run(labels[i].len())];
            let mut wrapper = text_system.line_wrapper(style.font(), font_size);
            let (text, runs) =
                wrapper.truncate_line(labels[i].clone(), room, "…", &run, TruncateFrom::End);
            line = text_system.shape_line(text, font_size, &runs, None);
        }
        let x = (centre - line.width / 2.).min(to - line.width).max(from);
        out.push((point(x, top), line));
    }
    (out, line_height)
}
