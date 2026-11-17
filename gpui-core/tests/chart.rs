//! Charts: nice tick values and short labels, or the host's; a bar chart's bars where Recharts
//! puts them (grouped, stacked, below a zero line); line and area charts' points, gaps, stacks
//! and monotone curves; the x value under the pointer; the tooltip following the pointer, a tap
//! and the keyboard; empty data; and nothing redrawing while idle.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gpui::SharedString;
use gpui::{
    AppContext as _, Bounds, Context, Entity, IntoElement, KeyDownEvent, KeyUpEvent, Keystroke,
    Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement as _,
    Pixels, Point, Render, Styled as _, TestAppContext, VisualTestContext, Window, div, point, px,
    size,
};
use gpui_core::chart::{
    BarRect, CurveSegment, compact_number, curve_segments, format_value, label_stride, nice_ticks,
};
use gpui_core::{AreaChart, BarChart, Curve, LineChart, Root};

fn plot(w: f32, h: f32) -> Bounds<Pixels> {
    Bounds::new(point(px(0.), px(0.)), size(px(w), px(h)))
}

/// `bar`'s bounds as (x, y, width, height), each within half a pixel of `want`.
#[track_caller]
fn assert_bar(bar: &BarRect, want: (f32, f32, f32, f32)) {
    let b = bar.bounds;
    let got = (
        f32::from(b.origin.x),
        f32::from(b.origin.y),
        f32::from(b.size.width),
        f32::from(b.size.height),
    );
    let close = |a: f32, b: f32| (a - b).abs() <= 0.5;
    assert!(
        close(got.0, want.0)
            && close(got.1, want.1)
            && close(got.2, want.2)
            && close(got.3, want.3),
        "series {} category {}: got {got:?}, want {want:?}",
        bar.series,
        bar.category
    );
}

fn two_by_two() -> BarChart {
    BarChart::new("c")
        .categories(["A", "B"])
        .series("a", "Desktop", [100., 50.])
        .series("b", "Mobile", [50., 100.])
}

// ---------------------------------------------------------------------------------------------
// Numbers

#[test]
fn ticks_are_round_and_cover_the_data() {
    assert_eq!(nice_ticks(0., 186., 5), [0., 50., 100., 150., 200.]);
    assert_eq!(nice_ticks(0., 305., 5), [0., 100., 200., 300., 400.]);
    assert_eq!(nice_ticks(0., 1., 5), [0., 0.2, 0.4, 0.6, 0.8, 1.]);
    assert_eq!(
        nice_ticks(0., 1_234_567., 5),
        [0., 250_000., 500_000., 750_000., 1_000_000., 1_250_000.]
    );
    assert_eq!(
        nice_ticks(-12., 305., 5),
        [-100., 0., 100., 200., 300., 400.],
        "zero stays a tick when the data crosses it"
    );
    assert_eq!(
        nice_ticks(100., 0., 5),
        nice_ticks(0., 100., 5),
        "either order"
    );
    assert_eq!(
        nice_ticks(0., 0., 5),
        nice_ticks(0., 1., 5),
        "all zero: 0 to 1"
    );
    assert!(nice_ticks(f64::NAN, 1., 5).is_empty());
    assert!(nice_ticks(0., f64::INFINITY, 5).is_empty());
}

#[test]
fn axis_labels_are_short() {
    for (v, want) in [
        (0., "0"),
        (186., "186"),
        (1200., "1.2k"),
        (12_345., "12.3k"),
        (250_000., "250k"),
        (3_400_000., "3.4M"),
        (1_250_000., "1.25M"),
        (999_950., "1M"),
        (2.5e9, "2.5B"),
        (7e12, "7T"),
        (-1500., "-1.5k"),
        (0.25, "0.25"),
        (-0., "0"),
        (f64::NAN, "–"),
    ] {
        assert_eq!(compact_number(v), want, "{v}");
    }
}

#[test]
fn tooltip_values_are_in_full() {
    assert_eq!(format_value(1_234_567.891), "1,234,567.89");
    assert_eq!(format_value(-1234.), "-1,234");
    assert_eq!(format_value(1000.), "1,000");
    assert_eq!(format_value(999.), "999");
    assert_eq!(format_value(0.5), "0.5");
}

#[test]
fn crowded_x_labels_are_thinned_out() {
    // Each fits its 50 pixel band.
    assert_eq!(label_stride(px(50.), px(30.), px(40.)), 1);
    // A 100 pixel label in 10 pixel bands takes 40 (cut short) and a gap: every fifth.
    assert_eq!(label_stride(px(10.), px(100.), px(40.)), 5);
    assert_eq!(
        label_stride(px(0.), px(30.), px(40.)),
        1,
        "no width: no division by it"
    );
}

// ---------------------------------------------------------------------------------------------
// Geometry

#[test]
fn grouped_bars_sit_side_by_side_in_their_band() {
    let chart = two_by_two();
    assert_eq!(chart.scale().ticks, [0., 20., 40., 60., 80., 100.]);
    let bars = chart.bars(plot(200., 100.));
    assert_eq!(bars.len(), 4);
    // Bands of 100: 10 empty each side, two 38 pixel bars 4 apart.
    assert_bar(&bars[0], (10., 0., 38., 100.));
    assert_bar(&bars[1], (52., 50., 38., 50.));
    assert_bar(&bars[2], (110., 50., 38., 50.));
    assert_bar(&bars[3], (152., 0., 38., 100.));
    assert!(bars.iter().all(|b| b.round_top && !b.round_bottom));
    assert_eq!((bars[1].series, bars[1].category), (1, 0));
}

#[test]
fn stacked_bars_stack_and_only_the_top_one_is_rounded() {
    let chart = two_by_two().stacked(true);
    assert_eq!(chart.scale().ticks, [0., 50., 100., 150.]);
    let bars = chart.bars(plot(200., 150.));
    assert_eq!(bars.len(), 4);
    assert_bar(&bars[0], (10., 50., 80., 100.));
    assert_bar(&bars[1], (10., 0., 80., 50.));
    assert_bar(&bars[2], (110., 100., 80., 50.));
    assert_bar(&bars[3], (110., 0., 80., 100.));
    assert!(!bars[0].round_top && bars[1].round_top);
    assert!(!bars[2].round_top && bars[3].round_top);
}

#[test]
fn negative_bars_hang_below_the_zero_line() {
    let chart = BarChart::new("c")
        .categories(["A", "B"])
        .series("a", "Net", [-50., 100.]);
    let scale = chart.scale();
    assert_eq!(scale.ticks, [-50., 0., 50., 100.]);
    let area = plot(200., 150.);
    assert_eq!(scale.y(0., area), px(100.));
    let bars = chart.bars(area);
    assert_bar(&bars[0], (10., 100., 80., 50.));
    assert!(
        bars[0].round_bottom && !bars[0].round_top,
        "rounded away from zero"
    );
    assert_bar(&bars[1], (110., 0., 80., 100.));

    // Stacked, negatives go down from zero and positives up, each rounded at its far end.
    let chart = BarChart::new("c")
        .categories(["A"])
        .series("a", "In", [40.])
        .series("b", "Out", [-20.])
        .series("c", "Fees", [-10.])
        .stacked(true);
    assert_eq!(chart.scale().ticks, [-40., -20., 0., 20., 40.]);
    let bars = chart.bars(plot(100., 80.));
    assert_bar(&bars[0], (10., 0., 80., 40.));
    assert_bar(&bars[1], (10., 40., 80., 20.));
    assert_bar(&bars[2], (10., 60., 80., 10.));
    assert!(bars[0].round_top && !bars[1].round_bottom && bars[2].round_bottom);
}

#[test]
fn huge_values_scale_without_overflowing() {
    let chart = BarChart::new("c")
        .categories(["A", "B"])
        .series("a", "Views", [3.4e12, 1.2e12]);
    let scale = chart.scale();
    assert_eq!(*scale.ticks.last().unwrap(), 4e12);
    let labels: Vec<String> = scale.ticks.iter().map(|t| compact_number(*t)).collect();
    assert_eq!(labels, ["0", "1T", "2T", "3T", "4T"]);
    let bars = chart.bars(plot(200., 100.));
    assert_bar(&bars[0], (10., 15., 80., 85.));
}

#[test]
fn a_single_point_fills_the_plot() {
    let chart = BarChart::new("c")
        .categories(["Only"])
        .series("a", "A", [42.]);
    let bars = chart.bars(plot(100., 100.));
    assert_eq!(bars.len(), 1);
    assert_eq!(bars[0].bounds.origin.x, px(10.));
    assert_eq!(bars[0].bounds.size.width, px(80.));
    assert_eq!(chart.nearest(plot(100., 100.), px(99.)), Some(0));
}

#[test]
fn empty_or_missing_data_draws_nothing() {
    let none = BarChart::new("c");
    assert!(none.bars(plot(100., 100.)).is_empty());
    assert_eq!(none.nearest(plot(100., 100.), px(50.)), None);
    assert_eq!(none.scale().ticks.first(), Some(&0.));
    assert_eq!(none.accessibility_summary(), "Bar chart, no data");

    let gaps =
        BarChart::new("c")
            .categories(["A", "B"])
            .series("a", "A", [f64::NAN, f64::INFINITY]);
    assert!(gaps.bars(plot(100., 100.)).is_empty());
    assert_eq!(gaps.nearest(plot(100., 100.), px(50.)), None);

    // A gap in one series leaves its place empty; the others keep theirs.
    let some = BarChart::new("c")
        .categories(["A", "B"])
        .series("a", "A", [f64::NAN, 10.])
        .series("b", "B", [5.]);
    let bars = some.bars(plot(200., 100.));
    let at: Vec<(usize, usize)> = bars.iter().map(|b| (b.series, b.category)).collect();
    assert_eq!(at, [(1, 0), (0, 1)]);
}

#[test]
fn the_pointer_picks_the_band_under_it() {
    let chart = two_by_two();
    let area = Bounds::new(point(px(40.), px(0.)), size(px(200.), px(100.)));
    assert_eq!(chart.nearest(area, px(50.)), Some(0));
    assert_eq!(chart.nearest(area, px(139.)), Some(0));
    assert_eq!(chart.nearest(area, px(140.)), Some(1));
    assert_eq!(chart.nearest(area, px(239.)), Some(1));
    assert_eq!(
        chart.nearest(area, px(0.)),
        Some(0),
        "left of the plot: the first"
    );
    assert_eq!(
        chart.nearest(area, px(900.)),
        Some(1),
        "right of it: the last"
    );
}

#[test]
fn it_summarises_itself_to_assistive_technology() {
    assert_eq!(
        two_by_two().accessibility_summary(),
        "Bar chart of Desktop and Mobile: 2 points, A to B; values from 50 to 100"
    );
}

#[track_caller]
fn assert_point(got: Point<Pixels>, want: (f32, f32)) {
    let (x, y) = (f32::from(got.x), f32::from(got.y));
    assert!(
        (x - want.0).abs() <= 0.5 && (y - want.1).abs() <= 0.5,
        "got ({x}, {y}), want {want:?}"
    );
}

#[test]
fn lines_run_through_their_points_twelve_pixels_in_from_the_ends() {
    let chart = LineChart::new("c")
        .categories(["A", "B", "C"])
        .series("a", "A", [0., 50., 100.]);
    let lines = chart.lines(plot(224., 100.));
    assert_eq!(lines.len(), 1);
    let pts = &lines[0].points;
    assert_point(pts[0], (12., 100.));
    assert_point(pts[1], (112., 50.));
    assert_point(pts[2], (212., 0.));
    assert!(lines[0].base.is_empty(), "a line has no bottom edge");
}

#[test]
fn a_gap_breaks_a_line() {
    let chart = LineChart::new("c").series("a", "A", [10., f64::NAN, 30., 40.]);
    let lines = chart.lines(plot(324., 100.));
    let runs: Vec<(usize, usize)> = lines.iter().map(|r| (r.start, r.points.len())).collect();
    assert_eq!(runs, [(0, 1), (2, 2)], "a run alone, then a run of two");
}

#[test]
fn point_charts_pick_the_nearest_point() {
    let chart = LineChart::new("c").series("a", "A", [1., 2., 3.]);
    let area = plot(224., 100.);
    // Points at 12, 112 and 212.
    assert_eq!(chart.nearest(area, px(61.)), Some(0));
    assert_eq!(chart.nearest(area, px(63.)), Some(1));
    assert_eq!(chart.nearest(area, px(170.)), Some(2));
    assert_eq!(chart.nearest(area, px(-50.)), Some(0));
    assert_eq!(chart.nearest(area, px(999.)), Some(2));
    assert_eq!(LineChart::new("e").nearest(area, px(5.)), None);
}

#[test]
fn a_single_point_sits_in_the_middle() {
    let chart = LineChart::new("c")
        .categories(["Now"])
        .series("a", "A", [5.]);
    let lines = chart.lines(plot(200., 100.));
    assert_point(lines[0].points[0], (100., 0.));
    assert_eq!(chart.nearest(plot(200., 100.), px(3.)), Some(0));
    assert!(LineChart::new("e").lines(plot(200., 100.)).is_empty());
}

#[test]
fn monotone_curves_never_overshoot_and_flatten_at_peaks() {
    let pts: Vec<Point<Pixels>> = [(0., 100.), (30., 90.), (100., 20.), (120., 0.)]
        .iter()
        .map(|(x, y)| point(px(*x), px(*y)))
        .collect();
    let segs = curve_segments(&pts, Curve::Monotone);
    assert_eq!(segs.len(), 3);
    for s in &segs {
        let (lo, hi) = (s.from.y.min(s.to.y), s.from.y.max(s.to.y));
        for c in [s.c1, s.c2] {
            assert!(
                c.y >= lo && c.y <= hi,
                "{c:?} outside {lo:?}..{hi:?} in {s:?}"
            );
            assert!(
                c.x >= s.from.x && c.x <= s.to.x,
                "{c:?} goes backwards in {s:?}"
            );
        }
    }
    // A lopsided peak: the curve is flat there, so it doesn't rise past it.
    let peak: Vec<Point<Pixels>> = [(0., 100.), (50., 0.), (150., 80.)]
        .iter()
        .map(|(x, y)| point(px(*x), px(*y)))
        .collect();
    let segs = curve_segments(&peak, Curve::Monotone);
    assert_eq!(segs[0].c2.y, px(0.));
    assert_eq!(segs[1].c1.y, px(0.));
}

#[test]
fn straight_curves_keep_their_control_points_on_the_line() {
    let pts = [
        point(px(0.), px(0.)),
        point(px(30.), px(60.)),
        point(px(60.), px(0.)),
    ];
    let segs = curve_segments(&pts, Curve::Linear);
    assert_eq!(
        segs[0],
        CurveSegment {
            from: pts[0],
            c1: point(px(10.), px(20.)),
            c2: point(px(20.), px(40.)),
            to: pts[1],
        }
    );
    // Two points are joined straight even when asked for a smooth curve.
    let two = curve_segments(&pts[..2], Curve::Monotone);
    assert_eq!(two, curve_segments(&pts[..2], Curve::Linear));
    assert!(curve_segments(&pts[..1], Curve::Monotone).is_empty());
}

#[test]
fn areas_fill_down_to_zero_or_onto_the_one_below() {
    let flat = AreaChart::new("c")
        .series("a", "A", [20., 40.])
        .series("b", "B", [10., 20.]);
    assert_eq!(flat.scale().ticks, [0., 10., 20., 30., 40.]);
    let areas = flat.areas(plot(224., 80.));
    assert_eq!(areas.len(), 2);
    for a in &areas {
        assert!(
            a.base.iter().all(|p| p.y == px(80.)),
            "each down to the zero line"
        );
    }

    let stacked = flat.stacked(true);
    assert_eq!(stacked.scale().ticks, [0., 20., 40., 60.]);
    let areas = stacked.areas(plot(224., 60.));
    // One pixel a unit: A's top at 20 and 40, B's on it at 30 and 60.
    assert_point(areas[0].points[0], (12., 40.));
    assert_point(areas[0].points[1], (212., 20.));
    assert_point(areas[1].points[0], (12., 30.));
    assert_point(areas[1].points[1], (212., 0.));
    assert_eq!(areas[1].base, areas[0].points, "B sits on A");
    assert!(areas[0].base.iter().all(|p| p.y == px(60.)));
}

#[test]
fn negative_values_dip_below_the_zero_line() {
    let chart = AreaChart::new("c").series("a", "A", [-50., 100.]);
    let scale = chart.scale();
    assert_eq!(scale.ticks, [-50., 0., 50., 100.]);
    let areas = chart.areas(plot(224., 150.));
    assert_point(areas[0].points[0], (12., 150.));
    assert!(
        areas[0].base.iter().all(|p| p.y == px(100.)),
        "the base is the zero line"
    );
}

#[test]
fn the_host_writes_the_values() {
    let dollars = |v: f64| -> SharedString { format!("${v}").into() };
    let bars = two_by_two().format_value(dollars);
    assert_eq!(
        bars.tick_labels(),
        ["$0", "$20", "$40", "$60", "$80", "$100"]
    );
    assert_eq!(
        bars.accessibility_summary(),
        "Bar chart of Desktop and Mobile: 2 points, A to B; values from $50 to $100"
    );
    let line = LineChart::new("c")
        .series("a", "Views", [1_200., 3_400_000.])
        .format_value(dollars);
    assert_eq!(
        line.tick_labels().last().map(|s| s.as_ref()),
        Some("$4000000")
    );
    let area = AreaChart::new("c")
        .series("a", "A", [0.5])
        .format_value(dollars);
    assert!(area.accessibility_summary().ends_with("from $0.5 to $0.5"));
    // Without one: short on the axis, in full in the tooltip and the summary.
    let plain = LineChart::new("c").series("a", "Views", [1_200., 3_400_000.]);
    assert_eq!(plain.tick_labels().last().map(|s| s.as_ref()), Some("4M"));
    assert!(
        plain
            .accessibility_summary()
            .ends_with("from 1,200 to 3,400,000")
    );
}

// ---------------------------------------------------------------------------------------------
// In a window

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Bar,
    Line,
    Area,
    Empty,
}

struct Panel {
    kind: Kind,
    draws: Rc<Cell<usize>>,
}

const MONTHS: [&str; 4] = ["Jan", "Feb", "Mar", "Apr"];
const DESKTOP: [f64; 4] = [186., 305., 237., 73.];
const MOBILE: [f64; 4] = [80., 200., 120., 190.];

impl Render for Panel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.draws.set(self.draws.get() + 1);
        let chart = match self.kind {
            Kind::Empty => BarChart::new("sales").into_any_element(),
            Kind::Bar => BarChart::new("sales")
                .categories(MONTHS)
                .series("desktop", "Desktop", DESKTOP)
                .series("mobile", "Mobile", MOBILE)
                .legend(true)
                .into_any_element(),
            Kind::Line => LineChart::new("sales")
                .categories(MONTHS)
                .series("desktop", "Desktop", DESKTOP)
                .series("mobile", "Mobile", MOBILE)
                .dots(true)
                .into_any_element(),
            Kind::Area => AreaChart::new("sales")
                .categories(MONTHS)
                .series("desktop", "Desktop", DESKTOP)
                .series("mobile", "Mobile", MOBILE)
                .stacked(true)
                .into_any_element(),
        };
        div()
            .size_full()
            .p(px(20.))
            .child(div().w(px(400.)).child(chart))
    }
}

struct Opened<'a> {
    kind: Kind,
    draws: Rc<Cell<usize>>,
    vcx: &'a mut VisualTestContext,
}

fn open(cx: &mut TestAppContext, kind: Kind) -> Opened<'_> {
    cx.update(gpui_core::init);
    let draws = Rc::new(Cell::new(0));
    let d = draws.clone();
    let made: Rc<RefCell<Option<Entity<Panel>>>> = Rc::default();
    let m = made.clone();
    let (_, vcx) = cx.add_window_view(move |window, cx| {
        let panel = cx.new(|_| Panel { kind, draws: d });
        *m.borrow_mut() = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    vcx.run_until_parked();
    Opened { kind, draws, vcx }
}

impl Opened<'_> {
    fn plot(&mut self) -> Bounds<Pixels> {
        self.vcx
            .debug_bounds("sales-plot")
            .expect("the plot is drawn")
    }

    /// Where x value `i` of 4 is: its band's middle, or its point (12 pixels in at the ends).
    fn at(&mut self, i: usize, dx: f32) -> Point<Pixels> {
        let p = self.plot();
        let x = if self.kind == Kind::Bar {
            p.origin.x + p.size.width / 4. * (i as f32 + 0.5)
        } else {
            p.origin.x + px(12.) + (p.size.width - px(24.)) / 3. * i as f32
        };
        point(x + px(dx), p.center().y)
    }

    fn hover(&mut self, at: Point<Pixels>) {
        self.vcx.simulate_event(MouseMoveEvent {
            position: at,
            pressed_button: None,
            modifiers: Modifiers::none(),
        });
        self.vcx.run_until_parked();
    }

    fn tap(&mut self, at: Point<Pixels>) {
        self.vcx.simulate_event(MouseDownEvent {
            button: MouseButton::Left,
            position: at,
            modifiers: Modifiers::none(),
            click_count: 1,
            first_mouse: false,
        });
        self.vcx.simulate_event(MouseUpEvent {
            button: MouseButton::Left,
            position: at,
            modifiers: Modifiers::none(),
            click_count: 1,
        });
        self.vcx.run_until_parked();
    }

    fn keys(&mut self, keys: &'static str) {
        for k in keys.split(' ') {
            let keystroke = Keystroke::parse(k).unwrap();
            self.vcx.simulate_event(KeyDownEvent {
                keystroke: keystroke.clone(),
                is_held: false,
                prefer_character_input: false,
            });
            self.vcx.simulate_event(KeyUpEvent { keystroke });
        }
        self.vcx.run_until_parked();
    }

    /// Which x value the tooltip shows, if any.
    fn tooltip(&mut self) -> Option<usize> {
        const NAMES: [&str; 4] = [
            "sales-tooltip-0",
            "sales-tooltip-1",
            "sales-tooltip-2",
            "sales-tooltip-3",
        ];
        (0..4).find(|i| self.vcx.debug_bounds(NAMES[*i]).is_some())
    }

    /// Delivers one display frame and says how many things had asked for it.
    fn frame(&mut self) -> usize {
        self.vcx.executor().advance_clock(Duration::from_millis(16));
        let asked = self.vcx.update(|window, cx| window.simulate_next_frame(cx));
        self.vcx.run_until_parked();
        asked
    }
}

#[gpui::test]
fn the_tooltip_follows_the_pointer_and_goes_when_it_leaves(cx: &mut TestAppContext) {
    let mut o = open(cx, Kind::Bar);
    assert_eq!(o.tooltip(), None, "no tooltip before the pointer comes");
    let at = o.at(1, 0.);
    o.hover(at);
    assert_eq!(o.tooltip(), Some(1));
    let at = o.at(2, -20.);
    o.hover(at);
    assert_eq!(o.tooltip(), Some(2), "the nearest x as the pointer moves");
    let tooltip = o.vcx.debug_bounds("sales-tooltip-2").unwrap();
    let plot = o.plot();
    assert!(
        tooltip.right() <= plot.right() && tooltip.origin.x >= plot.origin.x - px(60.),
        "the tooltip stays by the plot: {tooltip:?} in {plot:?}"
    );
    let outside = point(px(5.), px(5.));
    o.hover(outside);
    assert_eq!(o.tooltip(), None);
}

#[gpui::test]
fn a_tap_shows_the_tooltip_and_a_tap_elsewhere_hides_it(cx: &mut TestAppContext) {
    let mut o = open(cx, Kind::Bar);
    let at = o.at(3, 0.);
    o.tap(at);
    assert_eq!(o.tooltip(), Some(3));
    o.tap(point(px(5.), px(5.)));
    assert_eq!(o.tooltip(), None);
}

#[gpui::test]
fn the_keyboard_moves_the_tooltip(cx: &mut TestAppContext) {
    let mut o = open(cx, Kind::Bar);
    o.keys("tab right");
    assert_eq!(o.tooltip(), Some(0), "Right from nothing: the first");
    o.keys("right right");
    assert_eq!(o.tooltip(), Some(2));
    o.keys("left");
    assert_eq!(o.tooltip(), Some(1));
    o.keys("end right");
    assert_eq!(o.tooltip(), Some(3), "it stops at the end");
    o.keys("home left");
    assert_eq!(o.tooltip(), Some(0), "and at the start");
    o.keys("escape");
    assert_eq!(o.tooltip(), None);
    o.keys("left");
    assert_eq!(o.tooltip(), Some(3), "Left from nothing: the last");
}

#[gpui::test]
fn without_data_it_says_so_and_ignores_keys(cx: &mut TestAppContext) {
    let mut o = open(cx, Kind::Empty);
    assert!(o.vcx.debug_bounds("sales-empty").is_some());
    assert!(o.vcx.debug_bounds("sales-plot").is_none());
    o.keys("tab right");
    assert_eq!(o.tooltip(), None);
}

#[gpui::test]
fn nothing_redraws_while_idle(cx: &mut TestAppContext) {
    let mut o = open(cx, Kind::Bar);
    for _ in 0..10 {
        assert_eq!(o.frame(), 0, "asked for a frame before anything happened");
    }
    let at = o.at(1, 0.);
    o.hover(at);
    assert_eq!(o.tooltip(), Some(1));
    for _ in 0..60 {
        assert_eq!(o.frame(), 0, "asked for a frame with the tooltip up");
    }
    let shown = o.draws.get();
    for dx in [-10., -5., 5., 10.] {
        let at = o.at(1, dx);
        o.hover(at);
    }
    assert_eq!(
        o.draws.get(),
        shown,
        "redrew while the pointer stayed on the same x"
    );
    let at = o.at(2, 0.);
    o.hover(at);
    assert!(o.draws.get() > shown, "didn't redraw for another x");
}

#[gpui::test]
fn a_line_chart_follows_the_pointer_and_the_keyboard(cx: &mut TestAppContext) {
    let mut o = open(cx, Kind::Line);
    // Nearer the second point than the first.
    let at = o.at(1, -30.);
    o.hover(at);
    assert_eq!(o.tooltip(), Some(1));
    let at = o.at(3, 0.);
    o.hover(at);
    assert_eq!(o.tooltip(), Some(3));
    o.hover(point(px(5.), px(5.)));
    assert_eq!(o.tooltip(), None);
    o.keys("tab right right");
    assert_eq!(o.tooltip(), Some(1));
    o.keys("end escape");
    assert_eq!(o.tooltip(), None);
}

#[gpui::test]
fn an_area_chart_redraws_only_for_another_x(cx: &mut TestAppContext) {
    let mut o = open(cx, Kind::Area);
    let at = o.at(2, 0.);
    o.tap(at);
    assert_eq!(o.tooltip(), Some(2));
    for _ in 0..30 {
        assert_eq!(o.frame(), 0, "asked for a frame with the tooltip up");
    }
    let shown = o.draws.get();
    for dx in [-20., -8., 8., 20.] {
        let at = o.at(2, dx);
        o.hover(at);
    }
    assert_eq!(o.draws.get(), shown, "redrew while nearest the same point");
    let at = o.at(1, 0.);
    o.hover(at);
    assert_eq!(o.tooltip(), Some(1));
}
