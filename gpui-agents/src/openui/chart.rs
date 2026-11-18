//! Charts with their data already shaped for drawing: labels and series of numbers (parsed, one
//! per label), slices with their share of the whole, scatter points, and the range an axis has
//! to cover. The shaping follows react-ui's (buildChartData, the pie's labels and values), and
//! every list is capped (MAX_POINTS, MAX_SERIES) so a hostile card can't hand the drawing code a
//! million points.

use super::value::{V, number};
use super::{MAX_POINTS, MAX_SERIES};

/// Bar, line, area, radar and horizontal bar charts: values across categories.
#[derive(Clone, Debug, PartialEq)]
pub struct Bars {
    pub kind: ChartKind,
    /// The categories along the axis, one per point.
    pub labels: Vec<String>,
    /// Each series has exactly one value per label: None where it gave none or not a number.
    pub series: Vec<Series>,
    /// Bars stacked rather than side by side (bar and horizontal bar charts).
    pub stacked: bool,
    /// How a line or area joins its points.
    pub curve: Curve,
    pub x_label: Option<String>,
    pub y_label: Option<String>,
    /// The height the agent asked for, in pixels.
    pub height: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChartKind {
    Bar,
    HorizontalBar,
    Line,
    Area,
    Radar,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Curve {
    #[default]
    Linear,
    Natural,
    Step,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Series {
    pub name: String,
    pub values: Vec<Option<f64>>,
}

impl Bars {
    /// The values the value axis has to show, always including 0: stacked bars by their
    /// totals (positive and negative parts apart), everything else by each value.
    pub fn range(&self) -> (f64, f64) {
        let (mut lo, mut hi) = (0.0f64, 0.0f64);
        if self.stacked {
            for i in 0..self.labels.len() {
                let (mut pos, mut neg) = (0.0, 0.0);
                for v in self
                    .series
                    .iter()
                    .filter_map(|s| s.values.get(i).copied().flatten())
                {
                    if v >= 0.0 {
                        pos += v;
                    } else {
                        neg += v;
                    }
                }
                hi = hi.max(pos);
                lo = lo.min(neg);
            }
        } else {
            for v in self
                .series
                .iter()
                .flat_map(|s| s.values.iter().copied().flatten())
            {
                hi = hi.max(v);
                lo = lo.min(v);
            }
        }
        (lo, hi)
    }
}

/// Pie, donut, radial and single stacked bar charts: parts of a whole.
#[derive(Clone, Debug, PartialEq)]
pub struct Slices {
    pub kind: SlicesKind,
    pub slices: Vec<Slice>,
    /// A half circle rather than a whole one (pie and donut).
    pub semicircle: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlicesKind {
    Pie,
    Donut,
    Radial,
    StackedBar,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Slice {
    pub label: String,
    /// Never negative or NaN: those draw as 0.
    pub value: f64,
}

impl Slices {
    pub fn total(&self) -> f64 {
        self.slices.iter().map(|s| s.value).sum()
    }

    /// Each slice's share of the total, 0 when there's nothing to share.
    pub fn shares(&self) -> Vec<f64> {
        let total = self.total();
        self.slices
            .iter()
            .map(|s| if total > 0.0 { s.value / total } else { 0.0 })
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Scatter {
    pub sets: Vec<ScatterSet>,
    pub x_label: Option<String>,
    pub y_label: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScatterSet {
    pub name: String,
    pub points: Vec<ScatterPoint>,
}

/// A point with finite coordinates (one that isn't a number is left out).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScatterPoint {
    pub x: f64,
    pub y: f64,
    /// A third value, drawn as the point's size.
    pub z: Option<f64>,
}

impl Scatter {
    /// The smallest and largest x and y over every point, or None with no points.
    pub fn bounds(&self) -> Option<((f64, f64), (f64, f64))> {
        let mut pts = self.sets.iter().flat_map(|s| s.points.iter());
        let first = pts.next()?;
        let init = ((first.x, first.x), (first.y, first.y));
        Some(pts.fold(init, |((x0, x1), (y0, y1)), p| {
            ((x0.min(p.x), x1.max(p.x)), (y0.min(p.y), y1.max(p.y)))
        }))
    }
}

/// The chart in a card.
#[derive(Clone, Debug, PartialEq)]
pub enum Chart {
    Bars(Bars),
    Slices(Slices),
    Scatter(Scatter),
}

/// A number from a card's data: a number, or text that reads as one; never NaN or infinite.
pub(crate) fn num(v: &V) -> Option<f64> {
    let n = match v {
        V::Num(n) => *n,
        V::Str(s) => number(s),
        _ => return None,
    };
    n.is_finite().then_some(n)
}

fn as_list(v: &V) -> Vec<V> {
    match v {
        V::Arr(a) => a.iter().take(MAX_POINTS.max(MAX_SERIES)).cloned().collect(),
        V::Undef | V::Null => Vec::new(),
        other => vec![other.clone()],
    }
}

/// react-ui's buildChartData: series as Series(name, values) components, or as rows
/// [category, v1, v2, ...] with the labels naming the series after the first.
pub(crate) fn series(
    labels: &V,
    series: &V,
    text: &mut dyn FnMut(&V) -> String,
) -> (Vec<String>, Vec<Series>) {
    let rows = as_list(series);
    if rows.first().is_some_and(|r| matches!(r, V::Arr(_))) {
        let names: Vec<String> = as_list(labels)
            .iter()
            .skip(1)
            .take(MAX_SERIES)
            .map(&mut *text)
            .collect();
        let rows: Vec<&V> = rows.iter().take(MAX_POINTS).collect();
        let cats = rows.iter().map(|r| text(&r.field("0"))).collect();
        let series = names
            .into_iter()
            .enumerate()
            .map(|(si, name)| Series {
                name,
                values: rows
                    .iter()
                    .map(|r| Some(num(&r.field(&(si + 1).to_string())).unwrap_or(0.0)))
                    .collect(),
            })
            .collect();
        return (cats, series);
    }
    let labels: Vec<String> = as_list(labels)
        .iter()
        .take(MAX_POINTS)
        .map(&mut *text)
        .collect();
    let mut out: Vec<Series> = Vec::new();
    for s in rows.iter().filter_map(V::as_el) {
        let (V::Str(cat), V::Arr(vals)) = (
            s.props.get("category").cloned().unwrap_or_default(),
            s.props.get("values").cloned().unwrap_or_default(),
        ) else {
            continue;
        };
        let values = (0..labels.len())
            .map(|i| vals.get(i).and_then(num))
            .collect();
        let name = text(&V::Str(cat));
        // Two series of one name are one key in the web's chart data: the later wins.
        match out.iter().position(|x| x.name == name) {
            Some(at) => out[at].values = values,
            None if out.len() < MAX_SERIES => out.push(Series { name, values }),
            None => {}
        }
    }
    (labels, out)
}

/// The pie's data: labels with values (a value that isn't a number is 0), or Slice components.
pub(crate) fn slices(labels: &V, values: &V, text: &mut dyn FnMut(&V) -> String) -> Vec<Slice> {
    let (ls, vs) = (as_list(labels), as_list(values));
    let clean = |v: Option<f64>| v.filter(|n| *n > 0.0).unwrap_or(0.0);
    if !ls.is_empty() && !vs.is_empty() {
        return ls
            .iter()
            .take(MAX_POINTS)
            .enumerate()
            .map(|(i, l)| Slice {
                label: text(l),
                value: clean(
                    vs.get(i)
                        .and_then(|v| if let V::Num(n) = v { Some(*n) } else { None }),
                ),
            })
            .collect();
    }
    ls.iter()
        .filter_map(V::as_el)
        .take(MAX_POINTS)
        .map(|s| Slice {
            label: text(&s.props.get("category").cloned().unwrap_or_default()),
            value: clean(s.props.get("value").and_then(num)),
        })
        .collect()
}

/// The scatter's datasets, each point's coordinates read as Number() reads them.
pub(crate) fn scatter(datasets: &V, text: &mut dyn FnMut(&V) -> String) -> Vec<ScatterSet> {
    let props = |v: &V| match v {
        V::El(e) => V::obj(e.props.clone()),
        other => other.clone(),
    };
    as_list(datasets)
        .iter()
        .take(MAX_SERIES)
        .map(|ds| {
            let ds = props(ds);
            let points = as_list(&ds.field("points"))
                .iter()
                .take(MAX_POINTS)
                .filter_map(|p| {
                    let p = props(p);
                    let (x, y) = (num(&p.field("x"))?, num(&p.field("y"))?);
                    let z = match p.field("z") {
                        V::Undef | V::Null => None,
                        z => Some(num(&z)?),
                    };
                    Some(ScatterPoint { x, y, z })
                })
                .collect();
            ScatterSet {
                name: text(&ds.field("name")),
                points,
            }
        })
        .collect()
}
