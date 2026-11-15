//! Drawing the calendar and finding what's under the pointer. Everything inside the body is
//! painted on one canvas: the grid, the events and the text, culled to what's on screen. The
//! frame's geometry and the chips' bounds are kept for the mouse handlers.
//!
//! Sizes are CSS pixels times the window's rem over 15, so the calendar follows the
//! app's zoom as everything else does.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc, Weekday};
use gpui_kit::*;

use crate::calendar::{Calendar, Chip, Laid, SpanPiece};
use crate::layout;
use crate::model::{CalendarOptions, EventId, EventTime, Selection};
use crate::theme::{CalendarColors, ChipColors, mix};
use crate::time::{self, DstChange};

/// The shortest a timed event is drawn, in CSS pixels at the default hour height: room for a
/// line of text.
pub(crate) const MIN_CHIP_HEIGHT: f32 = 20.;

const GUTTER: f32 = 56.;
const HEAD: f32 = 40.;
const LANE: f32 = 22.;
const MONTH_HEAD: f32 = 28.;
const MONTH_LANE: f32 = 20.;
const MONTH_DATE: f32 = 28.;
const TITLE: f32 = 12.;
const TITLE_LEADING: f32 = 15.;
const SMALL: f32 = 11.25;
const SMALL_LEADING: f32 = 14.;
const LABEL: f32 = 13.125;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Grid,
    Month,
}

/// Which part of the calendar a drag is in, which decides how it moves things.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Area {
    /// The time grid: by snapped minutes.
    Grid,
    /// The grid's all-day row: by days.
    AllDay,
    /// The month: by days.
    Month,
}

/// What a press landed on.
#[derive(Clone, Debug)]
pub(crate) enum Target {
    Event {
        id: EventId,
        area: Area,
        resize: bool,
        read_only: bool,
    },
    /// "+N more" on a day.
    More(NaiveDate),
    /// A day's name or number.
    Date(NaiveDate),
    /// "+N" in the all-day row, which opens the row to show everything.
    Expand,
    Grid {
        date: NaiveDate,
        minute: f32,
    },
    /// An empty part of the all-day row or of a month cell.
    Day {
        date: NaiveDate,
        area: Area,
    },
    Nothing,
}

/// Where an event was drawn, for the mouse.
#[derive(Clone, Debug)]
pub(crate) struct Hit {
    pub id: EventId,
    pub bounds: Bounds<Pixels>,
    pub area: Area,
    /// The strip that resizes it, if it can be.
    pub resize: Option<Bounds<Pixels>>,
    pub read_only: bool,
}

/// What the canvas and the mouse handlers share.
#[derive(Default)]
pub(crate) struct Shared {
    pub geo: Cell<Option<Geo>>,
    /// The minute at the top of the time grid; none until the first frame places it.
    pub scroll: Cell<Option<f32>>,
    pub hits: RefCell<Vec<Hit>>,
    pub mores: RefCell<Vec<(Bounds<Pixels>, NaiveDate)>>,
    pub dates: RefCell<Vec<(Bounds<Pixels>, NaiveDate)>>,
    pub expand: RefCell<Vec<Bounds<Pixels>>>,
}

/// A frame's geometry.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Geo {
    pub kind: Kind,
    pub bounds: Bounds<Pixels>,
    pub s: f32,
    pub first: NaiveDate,
    pub days: u32,
    /// Where the first day's column starts.
    pub left: Pixels,
    pub col_w: Pixels,
    pub head_bottom: Pixels,
    pub all_day_bottom: Pixels,
    pub grid_top: Pixels,
    pub grid_bottom: Pixels,
    pub hour_h: Pixels,
    pub scroll: f32,
    pub rows: u32,
    pub row_h: Pixels,
}

impl Geo {
    pub fn y_of(&self, minute: f32) -> Pixels {
        self.grid_top + self.hour_h * ((minute - self.scroll) / 60.)
    }

    pub fn minute_at(&self, y: Pixels) -> f32 {
        f32::from(y - self.grid_top) / f32::from(self.hour_h) * 60. + self.scroll
    }

    pub fn visible_minutes(&self) -> f32 {
        f32::from(self.grid_bottom - self.grid_top) / f32::from(self.hour_h) * 60.
    }

    fn col_at(&self, x: Pixels) -> i32 {
        (f32::from(x - self.left) / f32::from(self.col_w)).floor() as i32
    }

    fn col_x(&self, col: i32) -> Pixels {
        self.left + self.col_w * col as f32
    }

    fn date(&self, col: i32) -> NaiveDate {
        self.first + Duration::days(col as i64)
    }

    fn row_top(&self, row: u32) -> Pixels {
        self.head_bottom + self.row_h * row as f32
    }

    /// The date and minute under a point, clamped into the area: where a drag is.
    pub fn pos(&self, p: Point<Pixels>, area: Area) -> (NaiveDate, f32) {
        match area {
            Area::Grid | Area::AllDay => {
                let col = self.col_at(p.x).clamp(0, self.days as i32 - 1);
                let minute = if area == Area::Grid {
                    self.minute_at(p.y).clamp(0., time::DAY_MINUTES)
                } else {
                    0.
                };
                (self.date(col), minute)
            }
            Area::Month => {
                let col = self.col_at(p.x).clamp(0, 6);
                let row =
                    (f32::from(p.y - self.head_bottom) / f32::from(self.row_h)).floor() as i32;
                let row = row.clamp(0, self.rows as i32 - 1);
                (self.date(row * 7 + col), 0.)
            }
        }
    }

    pub fn point_at(&self, date: NaiveDate, minute: f32) -> Option<Point<Pixels>> {
        let i = (date - self.first).num_days() as i32;
        if i < 0 || i >= self.days as i32 {
            return None;
        }
        match self.kind {
            Kind::Grid => {
                let x = self.col_x(i) + self.col_w / 2.;
                let y = if minute < 0. {
                    (self.head_bottom + self.all_day_bottom) / 2.
                } else {
                    self.y_of(minute)
                };
                Some(point(x, y))
            }
            Kind::Month => {
                let (row, col) = (i / 7, i % 7);
                Some(point(
                    self.col_x(col) + self.col_w / 2.,
                    self.row_top(row as u32) + self.row_h / 2.,
                ))
            }
        }
    }
}

/// The proposal of a drag in progress, drawn where it would land.
#[derive(Clone, Debug)]
pub(crate) struct Ghost {
    /// The event it moves, which is drawn faded in its old place; none for a new one.
    pub moving: Option<EventId>,
    pub time: EventTime,
    pub title: SharedString,
    pub label: SharedString,
    pub color: u32,
}

/// Everything a frame draws, made by the entity's render.
pub(crate) struct Scene {
    pub kind: Kind,
    pub first: NaiveDate,
    pub days: u32,
    /// The month the month view is of: days outside it are quieter.
    pub month: u32,
    pub today: NaiveDate,
    pub now: DateTime<Utc>,
    pub now_minute: f32,
    pub options: CalendarOptions,
    pub colors: CalendarColors,
    pub family: SharedString,
    pub laid: Rc<Laid>,
    pub selection: Option<Selection>,
    pub ghost: Option<Ghost>,
    pub cursor: CursorStyle,
    pub all_day_expanded: bool,
}

impl Scene {
    fn all_day_rows(&self) -> usize {
        if self.all_day_expanded || self.days == 1 {
            usize::MAX
        } else {
            self.options.all_day_rows.max(1)
        }
    }

    fn all_day_lanes(&self) -> usize {
        self.laid
            .spans
            .iter()
            .map(|s| s.lane + 1)
            .max()
            .unwrap_or(0)
    }

    fn selected(&self, id: &EventId) -> bool {
        matches!(&self.selection, Some(Selection::Event(s)) if s == id)
    }

    fn moving(&self, id: &EventId) -> bool {
        self.ghost
            .as_ref()
            .is_some_and(|g| g.moving.as_ref() == Some(id))
    }

    fn past(&self, chip: &Chip) -> bool {
        self.options.dim_past && chip.end <= self.now
    }

    fn today_col(&self) -> Option<i32> {
        let i = (self.today - self.first).num_days();
        (i >= 0 && i < self.days as i64).then_some(i as i32)
    }
}

fn geometry(scene: &Scene, bounds: Bounds<Pixels>, rem: Pixels, shared: &Shared) -> Geo {
    let s = f32::from(rem) / 15.;
    let u = |v: f32| px(v * s);
    match scene.kind {
        Kind::Grid => {
            let gutter = u(GUTTER);
            let head_bottom = bounds.top() + u(HEAD);
            let lanes = scene.all_day_lanes().min(scene.all_day_rows()).max(1);
            let all_day_bottom = head_bottom + u(LANE * lanes as f32 + 8.);
            let hour_h = u(scene.options.hour_height.max(8.));
            let col_w = (bounds.size.width - gutter) / scene.days.max(1) as f32;
            let mut geo = Geo {
                kind: Kind::Grid,
                bounds,
                s,
                first: scene.first,
                days: scene.days,
                left: bounds.left() + gutter,
                col_w,
                head_bottom,
                all_day_bottom,
                grid_top: all_day_bottom,
                grid_bottom: bounds.bottom(),
                hour_h,
                scroll: 0.,
                rows: 0,
                row_h: px(0.),
            };
            let shown = geo.visible_minutes();
            let max = (time::DAY_MINUTES - shown).max(0.);
            let scroll = match shared.scroll.get() {
                Some(at) => at,
                // It opens with the current time a third of the way down, or at the host's
                // chosen time when today isn't on screen.
                None if scene.today_col().is_some() => scene.now_minute - shown / 3.,
                None => time::minutes_of(scene.options.scroll_time),
            }
            .clamp(0., max);
            shared.scroll.set(Some(scroll));
            geo.scroll = scroll;
            geo
        }
        Kind::Month => {
            let head_bottom = bounds.top() + u(MONTH_HEAD);
            let rows = (scene.days / 7).max(1);
            Geo {
                kind: Kind::Month,
                bounds,
                s,
                first: scene.first,
                days: scene.days,
                left: bounds.left(),
                col_w: bounds.size.width / 7.,
                head_bottom,
                all_day_bottom: head_bottom,
                grid_top: head_bottom,
                grid_bottom: bounds.bottom(),
                hour_h: px(1.),
                scroll: 0.,
                rows,
                row_h: (bounds.bottom() - head_bottom) / rows as f32,
            }
        }
    }
}

/// What's under a point.
pub(crate) fn target_at(geo: &Geo, shared: &Shared, p: Point<Pixels>) -> Target {
    if !geo.bounds.contains(&p) {
        return Target::Nothing;
    }
    if let Some(hit) = shared
        .hits
        .borrow()
        .iter()
        .rev()
        .find(|h| h.bounds.contains(&p))
    {
        return Target::Event {
            id: hit.id.clone(),
            area: hit.area,
            resize: hit.resize.is_some_and(|r| r.contains(&p)),
            read_only: hit.read_only,
        };
    }
    if let Some((_, date)) = shared.mores.borrow().iter().find(|(b, _)| b.contains(&p)) {
        return Target::More(*date);
    }
    if shared.expand.borrow().iter().any(|b| b.contains(&p)) {
        return Target::Expand;
    }
    if let Some((_, date)) = shared.dates.borrow().iter().find(|(b, _)| b.contains(&p)) {
        return Target::Date(*date);
    }
    match geo.kind {
        Kind::Grid if p.x >= geo.left => {
            if p.y >= geo.grid_top {
                let (date, minute) = geo.pos(p, Area::Grid);
                Target::Grid { date, minute }
            } else if p.y >= geo.head_bottom {
                Target::Day {
                    date: geo.pos(p, Area::AllDay).0,
                    area: Area::AllDay,
                }
            } else {
                Target::Nothing
            }
        }
        Kind::Month if p.y >= geo.head_bottom => Target::Day {
            date: geo.pos(p, Area::Month).0,
            area: Area::Month,
        },
        _ => Target::Nothing,
    }
}

/// The pointer's shape over a point, while nothing is dragged.
pub(crate) fn cursor_at(geo: &Geo, shared: &Shared, p: Point<Pixels>) -> CursorStyle {
    match target_at(geo, shared, p) {
        Target::Event {
            resize: true, area, ..
        } => {
            if area == Area::Grid {
                CursorStyle::ResizeUpDown
            } else {
                CursorStyle::ResizeLeftRight
            }
        }
        Target::Event { .. } | Target::More(_) | Target::Date(_) | Target::Expand => {
            CursorStyle::PointingHand
        }
        _ => CursorStyle::Arrow,
    }
}

/// The calendar's body: one canvas, and the listeners that follow a drag anywhere in the
/// window.
pub(crate) fn body(
    scene: Scene,
    shared: Rc<Shared>,
    this: WeakEntity<Calendar>,
) -> impl IntoElement {
    let prepaint_shared = shared.clone();
    canvas(
        move |bounds, window, _| {
            let geo = geometry(&scene, bounds, window.rem_size(), &prepaint_shared);
            prepaint_shared.geo.set(Some(geo));
            let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
            (scene, geo, hitbox)
        },
        move |bounds, (scene, geo, hitbox), window, cx| {
            window.set_cursor_style(scene.cursor, &hitbox);
            shared.hits.borrow_mut().clear();
            shared.mores.borrow_mut().clear();
            shared.dates.borrow_mut().clear();
            shared.expand.borrow_mut().clear();
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                let mut ink = Ink {
                    window,
                    cx,
                    s: geo.s,
                    family: scene.family.clone(),
                };
                match scene.kind {
                    Kind::Grid => paint_grid(&mut ink, &scene, &geo, &shared),
                    Kind::Month => paint_month(&mut ink, &scene, &geo, &shared),
                }
            });
            let (on_move, on_up) = (this.clone(), this);
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                if phase == DispatchPhase::Bubble {
                    on_move.update(cx, |c, cx| c.mouse_move(event, cx)).ok();
                }
            });
            window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
                    on_up.update(cx, |c, cx| c.mouse_up(event, cx)).ok();
                }
            });
        },
    )
    .size_full()
}

/// The pen: quads and text at the calendar's scale.
struct Ink<'a> {
    window: &'a mut Window,
    cx: &'a mut App,
    s: f32,
    family: SharedString,
}

impl Ink<'_> {
    fn u(&self, v: f32) -> Pixels {
        px(v * self.s)
    }

    fn fill(&mut self, b: Bounds<Pixels>, color: Hsla) {
        self.window.paint_quad(fill(b, color));
    }

    fn rounded(&mut self, b: Bounds<Pixels>, radius: impl Into<Corners<Pixels>>, color: Hsla) {
        self.window.paint_quad(quad(
            b,
            radius,
            color,
            px(0.),
            transparent_black(),
            BorderStyle::default(),
        ));
    }

    fn outlined(
        &mut self,
        b: Bounds<Pixels>,
        radius: Pixels,
        fill: Hsla,
        width: Pixels,
        border: Hsla,
    ) {
        self.window
            .paint_quad(quad(b, radius, fill, width, border, BorderStyle::default()));
    }

    fn hline(&mut self, x0: Pixels, x1: Pixels, y: Pixels, color: Hsla) {
        self.fill(
            Bounds::from_corners(point(x0, y), point(x1, y + px(1.))),
            color,
        );
    }

    fn vline(&mut self, x: Pixels, y0: Pixels, y1: Pixels, color: Hsla) {
        self.fill(
            Bounds::from_corners(point(x, y0), point(x + px(1.), y1)),
            color,
        );
    }

    fn font(&self, weight: FontWeight) -> Font {
        Font {
            weight,
            ..font(self.family.clone())
        }
    }

    fn run(&self, len: usize, weight: FontWeight, color: Hsla) -> TextRun {
        TextRun {
            len,
            font: self.font(weight),
            color,
            background_color: None,
            underline: None,
            strikethrough: None,
        }
    }

    fn shape(
        &mut self,
        text: &str,
        size: f32,
        weight: FontWeight,
        color: Hsla,
        max_w: Option<Pixels>,
    ) -> Option<ShapedLine> {
        if text.is_empty() {
            return None;
        }
        let font_size = self.u(size);
        let mut line: SharedString = text.to_string().into();
        let mut run = self.run(line.len(), weight, color);
        if let Some(w) = max_w {
            if w <= px(1.) {
                return None;
            }
            let mut wrapper = self
                .window
                .text_system()
                .line_wrapper(run.font.clone(), font_size);
            let runs = [run];
            let (cut, cut_runs) = wrapper.truncate_line(line, w, "…", &runs, TruncateFrom::End);
            let next = cut_runs.into_owned().into_iter().next()?;
            line = cut;
            run = next;
        }
        Some(
            self.window
                .text_system()
                .shape_line(line, font_size, &[run], None),
        )
    }

    /// One line of text from its top left, cut with an ellipsis at `max_w`. Returns its width.
    #[allow(clippy::too_many_arguments)]
    fn text(
        &mut self,
        text: &str,
        size: f32,
        leading: f32,
        weight: FontWeight,
        color: Hsla,
        at: Point<Pixels>,
        max_w: Option<Pixels>,
    ) -> Pixels {
        let Some(shaped) = self.shape(text, size, weight, color, max_w) else {
            return px(0.);
        };
        let width = shaped.width;
        let leading = self.u(leading);
        let _ = shaped.paint(at, leading, TextAlign::Left, None, self.window, self.cx);
        width
    }

    /// One line of text ending at `right`.
    #[allow(clippy::too_many_arguments)]
    fn text_right(
        &mut self,
        text: &str,
        size: f32,
        leading: f32,
        weight: FontWeight,
        color: Hsla,
        right: Pixels,
        top: Pixels,
    ) -> Pixels {
        let Some(shaped) = self.shape(text, size, weight, color, None) else {
            return px(0.);
        };
        let width = shaped.width;
        let leading = self.u(leading);
        let _ = shaped.paint(
            point(right - width, top),
            leading,
            TextAlign::Left,
            None,
            self.window,
            self.cx,
        );
        width
    }

    fn measure(&mut self, text: &str, size: f32, weight: FontWeight) -> Pixels {
        self.shape(text, size, weight, transparent_black(), None)
            .map_or(px(0.), |s| s.width)
    }

    /// Text wrapped to `width` in at most `max_lines` lines. Returns the lines it took.
    #[allow(clippy::too_many_arguments)]
    fn wrapped(
        &mut self,
        text: &str,
        size: f32,
        leading: f32,
        weight: FontWeight,
        color: Hsla,
        at: Point<Pixels>,
        width: Pixels,
        max_lines: usize,
    ) -> usize {
        if max_lines <= 1 {
            self.text(text, size, leading, weight, color, at, Some(width));
            return 1;
        }
        let run = self.run(text.len(), weight, color);
        let font_size = self.u(size);
        let Ok(lines) = self.window.text_system().shape_text(
            text.to_string().into(),
            font_size,
            &[run],
            Some(width),
            Some(max_lines),
        ) else {
            return 0;
        };
        let leading = self.u(leading);
        let mut used = 0;
        let mut y = at.y;
        for line in lines {
            let _ = line.paint(
                point(at.x, y),
                leading,
                TextAlign::Left,
                None,
                self.window,
                self.cx,
            );
            let n = line.wrap_boundaries.len() + 1;
            used += n;
            y += leading * n as f32;
        }
        used.min(max_lines)
    }
}

fn is_weekend(d: NaiveDate) -> bool {
    matches!(d.weekday(), Weekday::Sat | Weekday::Sun)
}

/// A timed event's chip on the grid.
#[allow(clippy::too_many_arguments)]
fn paint_timed_chip(
    ink: &mut Ink,
    rect: Bounds<Pixels>,
    title: &str,
    time_label: &str,
    location: Option<&str>,
    cc: ChipColors,
    background: Hsla,
    faded: bool,
) {
    let alpha = if faded { 0.35 } else { 1. };
    let r = ink.u(4.);
    // A hairline of the background round each chip keeps overlapping ones apart.
    ink.outlined(rect, r, cc.fill.opacity(alpha), px(1.), background);
    let bar = Bounds::new(
        point(rect.left() + px(1.), rect.top() + px(1.)),
        size(ink.u(3.), (rect.size.height - px(2.)).max(px(0.))),
    );
    ink.rounded(
        bar,
        Corners {
            top_left: r,
            bottom_left: r,
            top_right: px(0.),
            bottom_right: px(0.),
        },
        cc.accent.opacity(alpha),
    );
    let x = rect.left() + ink.u(9.);
    let w = rect.size.width - ink.u(12.);
    if w < ink.u(14.) {
        return;
    }
    let h = rect.size.height;
    let (text, muted) = (cc.text.opacity(alpha), cc.muted_text.opacity(alpha));
    ink.window
        .with_content_mask(Some(ContentMask { bounds: rect }), |window| {
            let mut ink = Ink {
                window,
                cx: ink.cx,
                s: ink.s,
                family: ink.family.clone(),
            };
            let pad = ink.u(3.);
            if h >= ink.u(TITLE_LEADING + SMALL_LEADING + 5.) {
                // The title over as many lines as leave room for the time.
                let room = h - pad * 2. - ink.u(SMALL_LEADING);
                // Narrow chips keep the title to a line: wrapping there breaks words apart.
                let lines = if w < ink.u(84.) {
                    1
                } else {
                    ((f32::from(room) / f32::from(ink.u(TITLE_LEADING))).floor() as usize)
                        .clamp(1, 3)
                };
                let used = ink.wrapped(
                    title,
                    TITLE,
                    TITLE_LEADING,
                    FontWeight::MEDIUM,
                    text,
                    point(x, rect.top() + pad),
                    w,
                    lines,
                );
                let mut y = rect.top() + pad + ink.u(TITLE_LEADING) * used.max(1) as f32;
                ink.text(
                    time_label,
                    SMALL,
                    SMALL_LEADING,
                    FontWeight::NORMAL,
                    muted,
                    point(x, y),
                    Some(w),
                );
                y += ink.u(SMALL_LEADING);
                if let Some(location) = location
                    && y + ink.u(SMALL_LEADING) <= rect.bottom()
                {
                    ink.text(
                        location,
                        SMALL,
                        SMALL_LEADING,
                        FontWeight::NORMAL,
                        muted,
                        point(x, y),
                        Some(w),
                    );
                }
            } else {
                // One line: the title, then the time if there's room.
                let y = rect.top() + ((h - ink.u(TITLE_LEADING)) / 2.).max(px(0.));
                let used = ink.text(
                    title,
                    TITLE,
                    TITLE_LEADING,
                    FontWeight::MEDIUM,
                    text,
                    point(x, y),
                    Some(w),
                );
                let gap = ink.u(6.);
                let left = w - used - gap;
                if left > ink.u(28.) {
                    ink.text(
                        time_label,
                        SMALL,
                        TITLE_LEADING,
                        FontWeight::NORMAL,
                        muted,
                        point(x + used + gap, y + ink.u(0.5)),
                        Some(left),
                    );
                }
            }
        });
}

/// A chip across days: the all-day row, and spans in the month.
#[allow(clippy::too_many_arguments)]
fn paint_span_chip(
    ink: &mut Ink,
    rect: Bounds<Pixels>,
    title: &str,
    cc: ChipColors,
    open_left: bool,
    open_right: bool,
    faded: bool,
) {
    let alpha = if faded { 0.35 } else { 1. };
    let r = ink.u(4.);
    let corners = Corners {
        top_left: if open_left { px(0.) } else { r },
        bottom_left: if open_left { px(0.) } else { r },
        top_right: if open_right { px(0.) } else { r },
        bottom_right: if open_right { px(0.) } else { r },
    };
    ink.rounded(rect, corners, cc.fill.opacity(alpha));
    if !open_left {
        let bar = Bounds::new(rect.origin, size(ink.u(3.), rect.size.height));
        ink.rounded(
            bar,
            Corners {
                top_left: r,
                bottom_left: r,
                top_right: px(0.),
                bottom_right: px(0.),
            },
            cc.accent.opacity(alpha),
        );
    }
    let x = rect.left() + ink.u(if open_left { 6. } else { 9. });
    let w = rect.right() - x - ink.u(4.);
    let y = rect.top() + ((rect.size.height - ink.u(TITLE_LEADING)) / 2.).max(px(0.));
    ink.text(
        title,
        TITLE,
        TITLE_LEADING,
        FontWeight::MEDIUM,
        cc.text.opacity(alpha),
        point(x, y),
        Some(w),
    );
}

/// A ghost: the drag's proposal, outlined in its colour.
fn ghost_colors(color: u32, colors: &CalendarColors) -> (ChipColors, Hsla) {
    let accent: Hsla = rgb(color).into();
    (
        ChipColors {
            accent,
            // Opaque, blended onto the grid's background: text under a see-through ghost
            // collides with its own.
            fill: mix(colors.background, accent, 0.42),
            text: colors.text,
            muted_text: mix(colors.text, accent, 0.3),
        },
        accent,
    )
}

fn paint_grid(ink: &mut Ink, scene: &Scene, geo: &Geo, shared: &Shared) {
    let c = &scene.colors;
    let b = geo.bounds;
    let days = scene.days as i32;
    ink.fill(b, c.background);

    // Weekends, under everything.
    for i in 0..days {
        if is_weekend(geo.date(i)) {
            let x = geo.col_x(i);
            ink.fill(
                Bounds::from_corners(point(x, geo.head_bottom), point(x + geo.col_w, b.bottom())),
                c.off_day,
            );
        }
    }

    let viewport = Bounds::from_corners(point(b.left(), geo.grid_top), b.bottom_right());
    let grid_area = Bounds::from_corners(point(geo.left, geo.grid_top), b.bottom_right());
    let now_col = scene.today_col();
    let now_y = geo.y_of(scene.now_minute);

    ink.window
        .with_content_mask(Some(ContentMask { bounds: viewport }), |window| {
            let mut ink = Ink {
                window,
                cx: ink.cx,
                s: ink.s,
                family: ink.family.clone(),
            };
            // The hours the clocks skip or repeat.
            for (col, change) in &scene.laid.dst {
                let (from, to, label) = match *change {
                    DstChange::Skipped { from, to } => (from, to, "Clocks go forward".to_string()),
                    DstChange::Repeated { from, to } => (
                        from,
                        to,
                        format!(
                            "{} happens twice",
                            time::format_hour(from / 60, scene.options.hour12)
                        ),
                    ),
                };
                let x = geo.col_x(*col as i32);
                let band = Bounds::from_corners(
                    point(x + px(1.), geo.y_of(from as f32)),
                    point(x + geo.col_w, geo.y_of(to as f32)),
                );
                ink.fill(band, c.highlight);
                // Hatching: short diagonal-ish ticks read as "not normal time" without colour.
                let mut tx = band.left() + ink.u(4.);
                while tx < band.right() {
                    ink.fill(
                        Bounds::new(point(tx, band.top()), size(px(1.), band.size.height)),
                        c.faint_line.opacity(0.5),
                    );
                    tx += ink.u(6.);
                }
                ink.text(
                    &label,
                    SMALL,
                    SMALL_LEADING,
                    FontWeight::MEDIUM,
                    c.muted_text,
                    point(band.left() + ink.u(6.), band.top() + ink.u(4.)),
                    Some(band.size.width - ink.u(12.)),
                );
            }

            // Hour lines, and their labels in the gutter.
            for h in 1..24 {
                let y = geo.y_of(h as f32 * 60.);
                if y < geo.grid_top - ink.u(10.) || y > b.bottom() + ink.u(10.) {
                    continue;
                }
                ink.hline(geo.left, b.right(), y, c.line);
                let near_now = now_col.is_some() && (y - now_y).abs() < ink.u(12.);
                let label_top = y - ink.u(SMALL_LEADING / 2.);
                if !near_now && label_top >= geo.grid_top {
                    ink.text_right(
                        &time::format_hour(h, scene.options.hour12),
                        SMALL,
                        SMALL_LEADING,
                        FontWeight::NORMAL,
                        c.muted_text,
                        geo.left - ink.u(8.),
                        y - ink.u(SMALL_LEADING / 2.),
                    );
                }
            }

            // The selected slot.
            if let Some(Selection::Slot { date, minute }) = &scene.selection {
                let i = (*date - scene.first).num_days() as i32;
                if (0..days).contains(&i) {
                    let slot = scene.options.slot_minutes.max(1) as f32;
                    let x = geo.col_x(i);
                    let rect = Bounds::from_corners(
                        point(x + px(2.), geo.y_of(*minute as f32) + px(1.)),
                        point(
                            x + geo.col_w - px(2.),
                            geo.y_of(*minute as f32 + slot) - px(1.),
                        ),
                    );
                    ink.outlined(rect, ink.u(4.), c.highlight, ink.u(1.5), c.selection);
                }
            }
        });

    // Events, in the grid area only.
    let min = (MIN_CHIP_HEIGHT / scene.options.hour_height.max(8.)) * 60.;
    let margin = ink.u(6.);
    ink.window
        .with_content_mask(Some(ContentMask { bounds: grid_area }), |window| {
            let mut ink = Ink {
                window,
                cx: ink.cx,
                s: ink.s,
                family: ink.family.clone(),
            };
            let mut hits = shared.hits.borrow_mut();
            for piece in &scene.laid.timed {
                let top = geo.y_of(piece.top);
                let bottom = geo.y_of(piece.bottom.max(piece.top + min));
                if bottom < geo.grid_top || top > b.bottom() {
                    continue;
                }
                let chip = &scene.laid.chips[piece.chip];
                let x0 = geo.col_x(piece.col as i32);
                let avail = geo.col_w - margin;
                let rect = Bounds::from_corners(
                    point(x0 + avail * piece.place.left(), top),
                    point(
                        x0 + avail * (piece.place.left() + piece.place.width()),
                        bottom,
                    ),
                );
                let selected = scene.selected(&chip.id);
                let faded = scene.moving(&chip.id);
                let cc = ChipColors::new(chip.color, c, selected, scene.past(chip));
                paint_timed_chip(
                    &mut ink,
                    rect,
                    &chip.title,
                    &chip.time,
                    chip.location.as_deref(),
                    cc,
                    c.background,
                    faded,
                );
                let resize = (piece.ends && !chip.read_only).then(|| {
                    Bounds::from_corners(
                        point(rect.left(), rect.bottom() - ink.u(6.)),
                        rect.bottom_right(),
                    )
                });
                hits.push(Hit {
                    id: chip.id.clone(),
                    bounds: rect.intersect(&grid_area),
                    area: Area::Grid,
                    resize,
                    read_only: chip.read_only,
                });
            }
            drop(hits);

            // The ghost, a piece per day it touches, full width.
            if let Some(ghost) = &scene.ghost
                && let EventTime::Timed { start, end } = ghost.time
                && !time::in_all_day_row(&ghost.time)
            {
                let tz = scene.options.time_zone;
                let (d0, d1) = time::timed_days(start, end, tz);
                let (ghost_cc, accent) = ghost_colors(ghost.color, c);
                let mut d = d0;
                while d <= d1 {
                    let i = (d - scene.first).num_days() as i32;
                    if (0..days).contains(&i) {
                        let top = time::wall_minutes(start, d, tz);
                        let bottom = time::wall_minutes(end, d, tz).max(top + min);
                        let x0 = geo.col_x(i);
                        let rect = Bounds::from_corners(
                            point(x0 + px(1.), geo.y_of(top)),
                            point(x0 + geo.col_w - margin, geo.y_of(bottom)),
                        );
                        paint_timed_chip(
                            &mut ink,
                            rect,
                            &ghost.title,
                            &ghost.label,
                            None,
                            ghost_cc,
                            c.background,
                            false,
                        );
                        ink.outlined(rect, ink.u(4.), transparent_black(), ink.u(1.5), accent);
                    }
                    d += Duration::days(1);
                }
            }

            // Now: a quiet line across the week, a strong one across today.
            if let Some(col) = now_col {
                ink.fill(
                    Bounds::new(point(geo.left, now_y), size(b.right() - geo.left, px(1.))),
                    c.now.opacity(0.35),
                );
                let x = geo.col_x(col);
                ink.fill(
                    Bounds::new(point(x, now_y - px(1.)), size(geo.col_w, ink.u(2.))),
                    c.now,
                );
                let r = ink.u(5.);
                ink.rounded(
                    Bounds::new(point(x - r, now_y - r), size(r * 2., r * 2.)),
                    r,
                    c.now,
                );
            }
        });

    // The time now, in the gutter.
    if now_col.is_some() && now_y >= geo.grid_top && now_y <= b.bottom() {
        ink.window
            .with_content_mask(Some(ContentMask { bounds: viewport }), |window| {
                let mut ink = Ink {
                    window,
                    cx: ink.cx,
                    s: ink.s,
                    family: ink.family.clone(),
                };
                let t = scene.now.with_timezone(&scene.options.time_zone).time();
                let label = time::format_clock(t, scene.options.hour12);
                let w = ink.measure(&label, 10.5, FontWeight::SEMIBOLD);
                let (pad, h) = (ink.u(4.), ink.u(16.));
                let right = geo.left - ink.u(4.);
                let pill = Bounds::from_corners(
                    point(right - w - pad * 2., now_y - h / 2.),
                    point(right, now_y + h / 2.),
                );
                ink.rounded(pill, h / 2., c.now);
                ink.text(
                    &label,
                    10.5,
                    16.,
                    FontWeight::SEMIBOLD,
                    c.on_now,
                    point(pill.left() + pad, pill.top()),
                    None,
                );
            });
    }

    // Lines between days, through the all-day row and the grid.
    for i in 0..days {
        ink.vline(geo.col_x(i), geo.head_bottom, b.bottom(), c.line);
    }

    // The head: each day's name and date.
    let label = time::zone_label(scene.first, scene.options.time_zone);
    ink.text_right(
        &label,
        SMALL,
        SMALL_LEADING,
        FontWeight::NORMAL,
        c.muted_text,
        geo.left - ink.u(8.),
        b.top() + (ink.u(HEAD) - ink.u(SMALL_LEADING)) / 2.,
    );
    for i in 0..days {
        let date = geo.date(i);
        let x = geo.col_x(i) + ink.u(10.);
        let mid = b.top() + ink.u(HEAD) / 2.;
        let name = time::weekday_name(date.weekday(), days > 1);
        let w = ink.text(
            name,
            LABEL,
            18.,
            FontWeight::NORMAL,
            c.muted_text,
            point(x, mid - ink.u(9.)),
            Some(geo.col_w - ink.u(14.)),
        );
        let number = date.day().to_string();
        // The same gap after the weekday on every day, today's badge included.
        let nx = x + w + ink.u(6.);
        let number_right = if date == scene.today {
            let nw = ink.measure(&number, LABEL, FontWeight::SEMIBOLD);
            let d = ink.u(24.).max(nw + ink.u(10.));
            let pill = Bounds::new(point(nx, mid - ink.u(12.)), size(d, ink.u(24.)));
            ink.rounded(pill, ink.u(12.), c.now);
            ink.text(
                &number,
                LABEL,
                24.,
                FontWeight::SEMIBOLD,
                c.on_now,
                point(pill.left() + (d - nw) / 2., pill.top()),
                None,
            );
            pill.right()
        } else {
            nx + ink.text(
                &number,
                LABEL,
                18.,
                FontWeight::SEMIBOLD,
                c.text,
                point(nx, mid - ink.u(9.)),
                None,
            )
        };
        // A day the clocks change says how long it is.
        let length = time::day_length(date, scene.options.time_zone).num_hours();
        if length != 24 {
            let note_x = number_right + ink.u(8.);
            ink.text(
                &format!("{length} hours"),
                SMALL,
                18.,
                FontWeight::MEDIUM,
                c.now,
                point(note_x, mid - ink.u(8.)),
                Some(geo.col_x(i) + geo.col_w - note_x - ink.u(4.)),
            );
        }
        if days > 1 {
            shared.dates.borrow_mut().push((
                Bounds::from_corners(
                    point(geo.col_x(i), b.top()),
                    point(geo.col_x(i) + geo.col_w, geo.head_bottom),
                ),
                date,
            ));
        }
    }

    // The all-day row.
    ink.text_right(
        "All day",
        SMALL,
        SMALL_LEADING,
        FontWeight::NORMAL,
        c.muted_text,
        geo.left - ink.u(8.),
        geo.head_bottom + ink.u(7.),
    );
    let max_rows = scene.all_day_rows();
    let ranges: Vec<(i32, i32)> = scene.laid.spans.iter().map(|s| (s.first, s.last)).collect();
    let lanes: Vec<usize> = scene.laid.spans.iter().map(|s| s.lane).collect();
    let fits = layout::fit(&ranges, &lanes, scene.days as usize, max_rows);
    let row_top = |lane: usize| geo.head_bottom + px(4. * geo.s) + px(LANE * geo.s) * lane as f32;
    for span in &scene.laid.spans {
        paint_span(
            ink,
            scene,
            geo,
            shared,
            span,
            &fits,
            row_top(span.lane),
            Area::AllDay,
        );
    }
    for (i, f) in fits.iter().enumerate() {
        if f.hidden > 0 {
            let x = geo.col_x(i as i32);
            let y = row_top(f.shown_rows);
            let label = format!("+{} more", f.hidden);
            let w = ink.text(
                &label,
                SMALL,
                LANE - 2.,
                FontWeight::MEDIUM,
                c.muted_text,
                point(x + ink.u(8.), y),
                Some(geo.col_w - ink.u(10.)),
            );
            shared.expand.borrow_mut().push(Bounds::new(
                point(x + ink.u(4.), y),
                size(w + ink.u(8.), ink.u(LANE - 2.)),
            ));
        }
    }
    if let Some(ghost) = &scene.ghost
        && time::in_all_day_row(&ghost.time)
    {
        paint_span_ghost(ink, scene, geo, ghost, 0, row_top(0));
    }
    // The line under the all-day row.
    ink.hline(b.left(), b.right(), geo.grid_top - px(1.), c.line);
}

/// One span, in the pieces its days' fits let show.
#[allow(clippy::too_many_arguments)]
fn paint_span(
    ink: &mut Ink,
    scene: &Scene,
    geo: &Geo,
    shared: &Shared,
    span: &SpanPiece,
    fits: &[layout::DayFit],
    top: Pixels,
    area: Area,
) {
    let c = &scene.colors;
    let chip = &scene.laid.chips[span.chip];
    let lane_h = ink.u(if area == Area::Month {
        MONTH_LANE
    } else {
        LANE
    });
    let selected = scene.selected(&chip.id);
    let faded = scene.moving(&chip.id);
    let past = scene.past(chip);
    let cc = ChipColors::new(chip.color, c, selected, past);
    for (a, z) in layout::visible_runs(span.first, span.last, span.lane, fits) {
        let open_left = a == span.first && span.cont_left;
        let open_right = z == span.last && span.cont_right;
        let x0 = geo.col_x(a) + if open_left { px(0.) } else { ink.u(3.) };
        let x1 = geo.col_x(z + 1) - if open_right { px(0.) } else { ink.u(4.) };
        let rect = Bounds::from_corners(point(x0, top), point(x1, top + lane_h - ink.u(2.)));
        if span.dot {
            paint_dot_row(ink, rect, chip, cc, selected, faded);
        } else {
            paint_span_chip(ink, rect, &chip.title, cc, open_left, open_right, faded);
        }
        let resize =
            (chip.all_day && !chip.read_only && z == span.last && !span.cont_right).then(|| {
                Bounds::from_corners(
                    point(rect.right() - ink.u(6.), rect.top()),
                    rect.bottom_right(),
                )
            });
        shared.hits.borrow_mut().push(Hit {
            id: chip.id.clone(),
            bounds: rect,
            area,
            resize,
            read_only: chip.read_only,
        });
    }
}

/// A timed event on one day of the month: a dot in its colour, its time and its title.
fn paint_dot_row(
    ink: &mut Ink,
    rect: Bounds<Pixels>,
    chip: &Chip,
    cc: ChipColors,
    selected: bool,
    faded: bool,
) {
    let alpha = if faded { 0.35 } else { 1. };
    let r = ink.u(4.);
    if selected {
        ink.rounded(rect, r, cc.fill.opacity(alpha));
    }
    let mid = rect.top() + rect.size.height / 2.;
    let dot = ink.u(6.);
    ink.rounded(
        Bounds::new(
            point(rect.left() + ink.u(5.), mid - dot / 2.),
            size(dot, dot),
        ),
        dot / 2.,
        cc.accent.opacity(alpha),
    );
    let x = rect.left() + ink.u(15.);
    let y = mid - ink.u(TITLE_LEADING) / 2.;
    let w = rect.right() - x - ink.u(3.);
    let tw = ink.text(
        &chip.time,
        SMALL,
        TITLE_LEADING,
        FontWeight::NORMAL,
        cc.muted_text.opacity(alpha),
        point(x, y + ink.u(0.5)),
        Some(w),
    );
    let gap = ink.u(5.);
    ink.text(
        &chip.title,
        TITLE,
        TITLE_LEADING,
        FontWeight::MEDIUM,
        cc.text.opacity(alpha),
        point(x + tw + gap, y),
        Some(w - tw - gap),
    );
}

/// The ghost of an all-day drag: the days it would cover, tinted, and its chip over the
/// first row.
fn paint_span_ghost(ink: &mut Ink, scene: &Scene, geo: &Geo, ghost: &Ghost, row: u32, top: Pixels) {
    let c = &scene.colors;
    let (d0, d1) = time::event_days(&ghost.time, scene.options.time_zone);
    let lo = row as i32 * 7;
    let hi = if scene.kind == Kind::Month {
        lo + 6
    } else {
        scene.days as i32 - 1
    };
    let a = ((d0 - scene.first).num_days() as i32).max(lo);
    let z = ((d1 - scene.first).num_days() as i32).min(hi);
    if a > z {
        return;
    }
    let (cc, accent) = ghost_colors(ghost.color, c);
    let lane = ink.u(if scene.kind == Kind::Month {
        MONTH_LANE
    } else {
        LANE
    });
    let rect = Bounds::from_corners(
        point(geo.col_x(a - lo) + ink.u(3.), top),
        point(geo.col_x(z - lo + 1) - ink.u(4.), top + lane - ink.u(2.)),
    );
    let label = format!("{}  {}", ghost.title, ghost.label);
    paint_span_chip(ink, rect, &label, cc, false, false, false);
    ink.outlined(rect, ink.u(4.), transparent_black(), ink.u(1.5), accent);
}

fn paint_month(ink: &mut Ink, scene: &Scene, geo: &Geo, shared: &Shared) {
    let c = &scene.colors;
    let b = geo.bounds;
    ink.fill(b, c.background);

    // The weekday names.
    for col in 0..7 {
        let date = geo.date(col);
        ink.text(
            time::weekday_name(date.weekday(), true),
            SMALL,
            SMALL_LEADING,
            FontWeight::MEDIUM,
            c.muted_text,
            point(
                geo.col_x(col) + ink.u(10.),
                b.top() + (ink.u(MONTH_HEAD) - ink.u(SMALL_LEADING)) / 2.,
            ),
            Some(geo.col_w - ink.u(12.)),
        );
    }

    // The cells.
    let ghost_days = scene
        .ghost
        .as_ref()
        .map(|g| time::event_days(&g.time, scene.options.time_zone));
    for row in 0..geo.rows {
        let top = geo.row_top(row);
        for col in 0..7 {
            let i = row as i32 * 7 + col;
            let date = geo.date(i);
            let x = geo.col_x(col);
            let cell = Bounds::new(point(x, top), size(geo.col_w, geo.row_h));
            if date.month() != scene.month {
                ink.fill(cell, c.off_day);
            }
            if let Some((a, z)) = ghost_days
                && date >= a
                && date <= z
            {
                ink.fill(cell, c.highlight);
            }
            if matches!(&scene.selection, Some(Selection::Slot { date: d, .. }) if *d == date) {
                let inset = Bounds::from_corners(
                    point(cell.left() + px(2.), cell.top() + px(2.)),
                    point(cell.right() - px(1.), cell.bottom() - px(1.)),
                );
                ink.outlined(inset, ink.u(4.), c.highlight, ink.u(1.5), c.selection);
            }
            // The date: "Nov 1" on a month's first day.
            let number = if date.day() == 1 {
                format!("{} {}", time::month_short(date.month()), date.day())
            } else {
                date.day().to_string()
            };
            let tx = x + ink.u(8.);
            let ty = top + ink.u(5.);
            let quiet = date.month() != scene.month;
            let w = if date == scene.today {
                let nw = ink.measure(&number, TITLE, FontWeight::SEMIBOLD);
                let d = ink.u(22.).max(nw + ink.u(10.));
                let pill = Bounds::new(point(tx - ink.u(3.), ty), size(d, ink.u(22.)));
                ink.rounded(pill, ink.u(11.), c.now);
                ink.text(
                    &number,
                    TITLE,
                    22.,
                    FontWeight::SEMIBOLD,
                    c.on_now,
                    point(pill.left() + (d - nw) / 2., pill.top()),
                    None,
                );
                d
            } else {
                ink.text(
                    &number,
                    TITLE,
                    22.,
                    FontWeight::MEDIUM,
                    if quiet { c.muted_text } else { c.text },
                    point(tx, ty),
                    None,
                )
            };
            shared.dates.borrow_mut().push((
                Bounds::new(point(tx - ink.u(4.), ty), size(w + ink.u(8.), ink.u(22.))),
                date,
            ));
        }
    }

    // Lines.
    for row in 0..geo.rows {
        ink.hline(b.left(), b.right(), geo.row_top(row), c.line);
    }
    for col in 1..7 {
        ink.vline(geo.col_x(col), geo.head_bottom, b.bottom(), c.line);
    }

    // Events, a week at a time, with what doesn't fit counted.
    let lane_h = ink.u(MONTH_LANE);
    let usable = geo.row_h - ink.u(MONTH_DATE + 2.);
    let max_rows = (f32::from(usable) / f32::from(lane_h)).floor().max(0.) as usize;
    let spans = &scene.laid.spans;
    let mut i = 0;
    while i < spans.len() {
        let row = spans[i].row;
        let j = i + spans[i..].iter().take_while(|s| s.row == row).count();
        let week = &spans[i..j];
        let ranges: Vec<(i32, i32)> = week.iter().map(|s| (s.first, s.last)).collect();
        let lanes: Vec<usize> = week.iter().map(|s| s.lane).collect();
        let fits = layout::fit(&ranges, &lanes, 7, max_rows);
        let content_top = geo.row_top(row) + ink.u(MONTH_DATE);
        for span in week {
            let top = content_top + lane_h * span.lane as f32;
            let mut moved = span.clone();
            moved.row = 0;
            paint_span(ink, scene, geo, shared, &moved, &fits, top, Area::Month);
        }
        for (col, f) in fits.iter().enumerate() {
            if f.hidden > 0 {
                let x = geo.col_x(col as i32);
                let y = content_top + lane_h * f.shown_rows as f32;
                let label = format!("+{} more", f.hidden);
                let w = ink.text(
                    &label,
                    SMALL,
                    MONTH_LANE - 2.,
                    FontWeight::MEDIUM,
                    c.muted_text,
                    point(x + ink.u(8.), y),
                    Some(geo.col_w - ink.u(10.)),
                );
                let date = geo.date(row as i32 * 7 + col as i32);
                shared.mores.borrow_mut().push((
                    Bounds::new(
                        point(x + ink.u(4.), y),
                        size(w + ink.u(8.), lane_h - ink.u(2.)),
                    ),
                    date,
                ));
            }
        }
        i = j;
    }

    // The ghost over each week it touches.
    if let Some(ghost) = &scene.ghost {
        for row in 0..geo.rows {
            let top = geo.row_top(row) + ink.u(MONTH_DATE);
            paint_span_ghost(ink, scene, geo, ghost, row, top);
        }
    }
}
