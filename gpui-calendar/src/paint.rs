//! Drawing the calendar and finding what's under the pointer. Everything inside the body is
//! painted on one canvas: the grid, the events and the text, culled to what's on screen. The
//! frame's geometry and the chips' bounds are kept for the mouse handlers.
//!
//! Sizes are CSS pixels times the window's rem over 15, so the calendar follows the
//! app's zoom as everything else does.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc, Weekday};
use gpui::*;

use crate::calendar::{AgendaRow, Calendar, Chip, Laid, SpanPiece};
use crate::find::Query;
use crate::layout;
use crate::model::{CalendarOptions, EventId, EventLink, EventTime, LinkKind, Selection};
use crate::theme::{CalendarColors, ChipColors, mix};
use crate::time::{self, DstChange};

/// The shortest a timed event is drawn, in CSS pixels at the default hour height: room for a
/// line of text.
pub(crate) const MIN_CHIP_HEIGHT: f32 = 20.;

const GUTTER: f32 = 56.;
/// The gutter when the calendar is narrow, as on a phone.
const GUTTER_NARROW: f32 = 46.;
/// The second zone's column, left of the hours, when there is one.
const SECOND: f32 = 56.;
const SECOND_NARROW: f32 = 50.;
/// Below this width (in CSS pixels) the calendar lays itself out for a phone: a narrower
/// gutter, shorter labels, and the compact header.
pub(crate) const NARROW: f32 = 600.;
const HEAD: f32 = 40.;
const LANE: f32 = 22.;
/// The narrowest a column of side-by-side events gets, in CSS pixels (44 points on touch);
/// and the narrowest the strip for "+N" beside them is.
const MIN_COLUMN: f32 = 36.;
const MIN_STRIP: f32 = 18.;
/// How wide the chevron at a span's continuing end is, in CSS pixels.
const CHEVRON: f32 = 8.;
/// The most of the calendar under its day names the all-day row takes, expanded or not.
const MAX_ALL_DAY_SHARE: f32 = 0.4;
const MONTH_HEAD: f32 = 28.;
const MONTH_LANE: f32 = 20.;
const MONTH_DATE: f32 = 28.;
const TITLE: f32 = 12.;
const TITLE_LEADING: f32 = 15.;
const SMALL: f32 = 11.25;
const SMALL_LEADING: f32 = 14.;
const LABEL: f32 = 13.125;
/// A link's row in a chip tall enough to list them.
const LINK_ROW: f32 = 17.;
/// A link's mark: a page for a doc, a ring for an agent, a chain for several links.
const GLYPH: f32 = 11.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Grid,
    Month,
    Agenda,
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
    /// "+N" beside side-by-side events, for those that didn't fit: which they are, and where
    /// it was drawn.
    Crowd(Vec<EventId>, Bounds<Pixels>),
    /// One of an event's links.
    Link {
        id: EventId,
        index: usize,
        area: Area,
        read_only: bool,
    },
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

/// Where one of an event's links was drawn.
#[derive(Clone, Debug)]
pub(crate) struct LinkHit {
    pub id: EventId,
    pub index: usize,
    pub bounds: Bounds<Pixels>,
    pub area: Area,
    pub read_only: bool,
}

/// What the canvas and the mouse handlers share.
#[derive(Default)]
pub(crate) struct Shared {
    pub geo: Cell<Option<Geo>>,
    /// The minute at the top of the time grid; none until the first frame places it.
    pub scroll: Cell<Option<f32>>,
    pub hits: RefCell<Vec<Hit>>,
    pub links: RefCell<Vec<LinkHit>>,
    pub mores: RefCell<Vec<(Bounds<Pixels>, NaiveDate)>>,
    pub dates: RefCell<Vec<(Bounds<Pixels>, NaiveDate)>>,
    pub expand: RefCell<Vec<Bounds<Pixels>>>,
    /// "+N" for side-by-side events that didn't fit, and which they are.
    pub crowds: RefCell<Vec<(Bounds<Pixels>, Vec<EventId>)>>,
    /// Where the drag's ghost was drawn in the last frame.
    pub ghost: Cell<Option<Bounds<Pixels>>>,
    /// How far the agenda is scrolled, in pixels; none until a frame places it (at today).
    pub agenda_scroll: Cell<Option<f32>>,
    /// The farthest the agenda scrolls, from the last frame.
    pub agenda_max: Cell<f32>,
    /// An event the next agenda frame scrolls into view.
    pub agenda_reveal: RefCell<Option<EventId>>,
}

/// A frame's geometry.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Geo {
    pub kind: Kind,
    pub bounds: Bounds<Pixels>,
    pub s: f32,
    pub first: NaiveDate,
    pub days: u32,
    /// Where the first day's column starts: past the gutter, carried by a swipe.
    pub left: Pixels,
    /// The gutter's right edge, which a swipe doesn't move: the hours and the zone are drawn
    /// left of it, and the days right of it.
    pub gutter: Pixels,
    /// How wide the second zone's column is, at the gutter's left; none without one.
    pub second: Pixels,
    pub col_w: Pixels,
    pub head_bottom: Pixels,
    pub all_day_bottom: Pixels,
    pub grid_top: Pixels,
    pub grid_bottom: Pixels,
    pub hour_h: Pixels,
    pub scroll: f32,
    pub rows: u32,
    pub row_h: Pixels,
    /// Taken by touch: small things take taps around them, and only the selected event
    /// shows a handle to change its end.
    pub touch: bool,
}

/// The smallest a touch target is, in points (CSS pixels): Apple's 44.
pub(crate) const TOUCH_TARGET: f32 = 44.;

/// `b` grown about its centre to at least `min` each way.
fn inflate(b: Bounds<Pixels>, min: Pixels) -> Bounds<Pixels> {
    let w = b.size.width.max(min);
    let h = b.size.height.max(min);
    Bounds::new(
        point(b.center().x - w / 2., b.center().y - h / 2.),
        size(w, h),
    )
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

    pub(crate) fn col_x(&self, col: i32) -> Pixels {
        self.left + self.col_w * col as f32
    }

    pub(crate) fn date(&self, col: i32) -> NaiveDate {
        self.first + Duration::days(col as i64)
    }

    pub(crate) fn row_top(&self, row: u32) -> Pixels {
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
            Kind::Agenda => None,
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

/// The period beside the one on screen, drawn next to it while a swipe carries it in.
#[derive(Clone)]
pub(crate) struct Neighbour {
    /// 1 for the next period (to the right), -1 for the previous one (to the left).
    pub dir: i32,
    pub first: NaiveDate,
    pub days: u32,
    pub month: u32,
    pub laid: Rc<Laid>,
}

/// Everything a frame draws, made by the entity's render.
#[derive(Clone)]
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
    /// Whether the header was drawn for a narrow calendar, if there is a header: when the
    /// body turns out to be the other width, the calendar draws again with the right one.
    pub header_narrow: Option<bool>,
    /// How far a sideways swipe has carried the body, in pixels.
    pub swipe: f32,
    pub neighbour: Option<Neighbour>,
    /// The all-day row's lanes, when they're another period's: the period a swipe brings in
    /// keeps the one on screen's, so their hours line up.
    pub lanes: Option<usize>,
    /// The find query and the colour its finds are marked in, while there is one.
    pub find: Option<(Rc<Query>, Hsla)>,
}

impl Scene {
    /// How many rows the all-day row stacks before "+N": `all_day_rows`, or all of them when
    /// it's expanded or a single day; never more than leave the hours most of a calendar
    /// `height` tall (CSS pixels at scale `s`), so a crowded day can't push them off.
    fn all_day_rows(&self, height: Pixels, s: f32) -> usize {
        let wanted = if self.all_day_expanded || self.days == 1 {
            usize::MAX
        } else {
            self.options.all_day_rows.max(1)
        };
        let room = (f32::from(height) / s - HEAD) * MAX_ALL_DAY_SHARE - 8.;
        wanted.min(((room / LANE).floor() as usize).max(1))
    }

    fn all_day_lanes(&self) -> usize {
        self.lanes.unwrap_or_else(|| {
            self.laid
                .spans
                .iter()
                .map(|s| s.lane + 1)
                .max()
                .unwrap_or(0)
        })
    }

    fn selected(&self, id: &EventId) -> bool {
        matches!(&self.selection, Some(Selection::Event(s)) if s == id)
    }

    /// Whether a find query leaves this chip out (dimmed on the grid).
    fn dimmed(&self, chip: &Chip) -> bool {
        self.find.is_some() && !chip.found
    }

    /// Marks found words while drawing a found chip's text.
    fn mark(&self, chip: &Chip) -> Option<(Rc<Query>, Hsla)> {
        self.find.clone().filter(|_| chip.found)
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

/// The frame's geometry. A swipe carries the time grid's days by `swipe` while its gutter
/// stays, and carries the month and the agenda whole.
fn geometry(
    scene: &Scene,
    bounds: Bounds<Pixels>,
    swipe: Pixels,
    rem: Pixels,
    shared: &Shared,
) -> Geo {
    let s = f32::from(rem) / 15.;
    let u = |v: f32| px(v * s);
    let bounds = if scene.kind == Kind::Grid {
        bounds
    } else {
        Bounds::new(point(bounds.origin.x + swipe, bounds.origin.y), bounds.size)
    };
    match scene.kind {
        Kind::Grid => {
            let narrow = bounds.size.width < u(NARROW);
            let second = if scene.options.secondary_zone.is_some() {
                u(if narrow { SECOND_NARROW } else { SECOND })
            } else {
                px(0.)
            };
            let gutter = u(if narrow { GUTTER_NARROW } else { GUTTER }) + second;
            let head_bottom = bounds.top() + u(HEAD);
            let lanes = scene
                .all_day_lanes()
                .min(scene.all_day_rows(bounds.size.height, s))
                .max(1);
            let all_day_bottom = head_bottom + u(LANE * lanes as f32 + 8.);
            let hour_h = u(scene.options.hour_height.max(8.));
            let col_w = (bounds.size.width - gutter) / scene.days.max(1) as f32;
            let mut geo = Geo {
                kind: Kind::Grid,
                bounds,
                s,
                first: scene.first,
                days: scene.days,
                left: bounds.left() + gutter + swipe,
                gutter: bounds.left() + gutter,
                second,
                col_w,
                head_bottom,
                all_day_bottom,
                grid_top: all_day_bottom,
                grid_bottom: bounds.bottom(),
                hour_h,
                scroll: 0.,
                rows: 0,
                row_h: px(0.),
                touch: scene.options.touch,
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
                gutter: bounds.left(),
                second: px(0.),
                col_w: bounds.size.width / 7.,
                head_bottom,
                all_day_bottom: head_bottom,
                grid_top: head_bottom,
                grid_bottom: bounds.bottom(),
                hour_h: px(1.),
                scroll: 0.,
                rows,
                row_h: (bounds.bottom() - head_bottom) / rows as f32,
                touch: scene.options.touch,
            }
        }
        Kind::Agenda => Geo {
            kind: Kind::Agenda,
            bounds,
            s,
            first: scene.first,
            days: scene.days,
            left: bounds.left(),
            gutter: bounds.left(),
            second: px(0.),
            col_w: bounds.size.width,
            head_bottom: bounds.top(),
            all_day_bottom: bounds.top(),
            grid_top: bounds.top(),
            grid_bottom: bounds.bottom(),
            hour_h: px(1.),
            scroll: 0.,
            rows: 0,
            row_h: px(0.),
            touch: scene.options.touch,
        },
    }
}

/// What's under a point.
pub(crate) fn target_at(geo: &Geo, shared: &Shared, p: Point<Pixels>) -> Target {
    if !geo.bounds.contains(&p) {
        return Target::Nothing;
    }
    // The agenda's day headers stay over the rows scrolling under them.
    if geo.kind == Kind::Agenda
        && let Some((_, date)) = shared.dates.borrow().iter().find(|(b, _)| b.contains(&p))
    {
        return Target::Date(*date);
    }
    // With touch, small things take taps in a 44-point square round them, after anything a
    // tap lands on exactly.
    let target = px(TOUCH_TARGET * geo.s);
    let near = |b: &Bounds<Pixels>, exact: bool| {
        if exact {
            b.contains(&p)
        } else {
            geo.touch && inflate(*b, target).contains(&p)
        }
    };
    let link_at = |exact: bool| {
        shared
            .links
            .borrow()
            .iter()
            .rev()
            .find(|l| near(&l.bounds, exact))
            .map(|link| Target::Link {
                id: link.id.clone(),
                index: link.index,
                area: link.area,
                read_only: link.read_only,
            })
    };
    // The selected event's handle reaches past its edge on touch.
    if geo.touch
        && let Some(hit) = shared
            .hits
            .borrow()
            .iter()
            .rev()
            .find(|h| h.resize.is_some_and(|r| r.contains(&p)))
    {
        return Target::Event {
            id: hit.id.clone(),
            area: hit.area,
            resize: true,
            read_only: hit.read_only,
        };
    }
    // Links are drawn over their events.
    if let Some(link) = link_at(true) {
        return link;
    }
    // So is "+N" in a day too narrow for a strip beside its events.
    let crowd_at = |exact: bool| {
        shared
            .crowds
            .borrow()
            .iter()
            .find(|(b, _)| near(b, exact))
            .map(|(b, ids)| Target::Crowd(ids.clone(), *b))
    };
    if let Some(crowd) = crowd_at(true) {
        return crowd;
    }
    let event_at = |exact: bool| {
        let hits = shared.hits.borrow();
        let found = if exact {
            hits.iter().rev().find(|h| h.bounds.contains(&p))
        } else {
            // The nearest of those whose square takes the tap.
            hits.iter()
                .filter(|h| near(&h.bounds, false))
                .min_by(|a, b| {
                    let d = |h: &Hit| (h.bounds.center() - p).magnitude();
                    d(a).total_cmp(&d(b))
                })
        };
        found.map(|hit| Target::Event {
            id: hit.id.clone(),
            area: hit.area,
            resize: hit.resize.is_some_and(|r| r.contains(&p)),
            read_only: hit.read_only,
        })
    };
    if let Some(event) = event_at(true) {
        return event;
    }
    for exact in [true, false] {
        if !exact && !geo.touch {
            break;
        }
        if !exact && let Some(crowd) = crowd_at(false) {
            return crowd;
        }
        if let Some((_, date)) = shared.mores.borrow().iter().find(|(b, _)| near(b, exact)) {
            return Target::More(*date);
        }
        if shared.expand.borrow().iter().any(|b| near(b, exact)) {
            return Target::Expand;
        }
        if let Some((_, date)) = shared.dates.borrow().iter().find(|(b, _)| near(b, exact)) {
            return Target::Date(*date);
        }
        if !exact {
            if let Some(link) = link_at(false) {
                return link;
            }
            if let Some(event) = event_at(false) {
                return event;
            }
        }
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
        Target::Event { .. }
        | Target::Link { .. }
        | Target::More(_)
        | Target::Date(_)
        | Target::Expand
        | Target::Crowd(..) => CursorStyle::PointingHand,
        _ => CursorStyle::Arrow,
    }
}

/// The calendar's body: one canvas, and the listeners that follow a drag anywhere in the
/// window.
pub(crate) fn body(
    scene: Scene,
    shared: Rc<Shared>,
    this: WeakEntity<Calendar>,
    a11y: Rc<crate::a11y::A11yFrame>,
) -> impl IntoElement {
    let prepaint_shared = shared.clone();
    let prepaint_this = this.clone();
    canvas(
        move |bounds, window, cx| {
            let geo = geometry(
                &scene,
                bounds,
                px(scene.swipe),
                window.rem_size(),
                &prepaint_shared,
            );
            prepaint_shared.geo.set(Some(geo));
            let narrow = bounds.size.width < px(NARROW * f32::from(window.rem_size()) / 15.);
            if scene.header_narrow.is_some_and(|n| n != narrow) {
                let this = prepaint_this.clone();
                window.defer(cx, move |_, cx| {
                    this.update(cx, |_, cx| cx.notify()).ok();
                });
            }
            // With a screen reader on, the frame is laid out here first, without paint, so
            // what it's told (built from where things go) is this frame's, not the last's.
            // Without one, nothing extra runs.
            if window.is_a11y_active() || crate::a11y::forced() {
                draw(window, cx, &scene, &geo, bounds, &prepaint_shared, true);
                let items = prepaint_this
                    .upgrade()
                    .map(|c| c.read(cx).accessibility_items())
                    .unwrap_or_default();
                a11y.set(items, window.scale_factor());
            } else {
                a11y.set(Vec::new(), window.scale_factor());
            }
            let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
            (scene, geo, hitbox)
        },
        move |bounds, (scene, geo, hitbox), window, cx| {
            window.set_cursor_style(scene.cursor, &hitbox);
            draw(window, cx, &scene, &geo, bounds, &shared, false);
            // Touch: a long press, and a drag claimed as the finger lands.
            let (on_long, on_drag) = (this.clone(), this.clone());
            window.on_mouse_event(move |event: &LongPressEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble
                    && on_long
                        .update(cx, |c, cx| c.long_press(event, cx))
                        .unwrap_or(false)
                {
                    window.prevent_default();
                    cx.stop_propagation();
                }
            });
            window.on_mouse_event(move |event: &TouchDragEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble
                    && on_drag
                        .update(cx, |c, cx| c.touch_drag(event, cx))
                        .unwrap_or(false)
                {
                    window.prevent_default();
                    cx.stop_propagation();
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

/// Draws a frame's days (and the gutter), noting where everything went in `shared` for the
/// mouse and for assistive technology; `dry`, it only notes where things go.
fn draw(
    window: &mut Window,
    cx: &mut App,
    scene: &Scene,
    geo: &Geo,
    bounds: Bounds<Pixels>,
    shared: &Shared,
    dry: bool,
) {
    shared.hits.borrow_mut().clear();
    shared.links.borrow_mut().clear();
    shared.mores.borrow_mut().clear();
    shared.dates.borrow_mut().clear();
    shared.expand.borrow_mut().clear();
    shared.crowds.borrow_mut().clear();
    shared.ghost.set(None);
    window.with_content_mask(Some(ContentMask { bounds }), |window| {
        let mut ink = Ink {
            window,
            cx,
            s: geo.s,
            family: scene.family.clone(),
            mark: None,
            dry,
        };
        // Days are drawn right of the gutter only, and the gutter after them, still.
        let days_area =
            Bounds::from_corners(point(geo.gutter, bounds.top()), bounds.bottom_right());
        ink.window
            .with_content_mask(Some(ContentMask { bounds: days_area }), |window| {
                let mut ink = Ink {
                    window,
                    cx: ink.cx,
                    s: ink.s,
                    family: ink.family.clone(),
                    mark: ink.mark.clone(),
                    dry: ink.dry,
                };
                // The period a swipe brings in, beside this one. Drawn first: what
                // it leaves in the shared hit lists is replaced by this period's
                // below, and nothing takes a press while a swipe moves.
                if let Some(n) = &scene.neighbour {
                    let mut beside = scene.clone();
                    beside.first = n.first;
                    beside.days = n.days;
                    beside.month = n.month;
                    beside.laid = n.laid.clone();
                    beside.selection = None;
                    beside.ghost = None;
                    beside.neighbour = None;
                    beside.lanes = Some(scene.all_day_lanes());
                    // Beside this period's days: a day area's width along for the grid, a
                    // whole width for the month.
                    let along = (bounds.right() - geo.gutter) * n.dir as f32;
                    let beside_geo = geometry(
                        &beside,
                        bounds,
                        px(scene.swipe) + along,
                        ink.window.rem_size(),
                        shared,
                    );
                    match beside.kind {
                        Kind::Grid => paint_grid(&mut ink, &beside, &beside_geo, shared),
                        Kind::Month => paint_month(&mut ink, &beside, &beside_geo, shared),
                        Kind::Agenda => {}
                    }
                    shared.hits.borrow_mut().clear();
                    shared.links.borrow_mut().clear();
                    shared.mores.borrow_mut().clear();
                    shared.dates.borrow_mut().clear();
                    shared.expand.borrow_mut().clear();
                    shared.crowds.borrow_mut().clear();
                }
                match scene.kind {
                    Kind::Grid => paint_grid(&mut ink, scene, geo, shared),
                    Kind::Month => paint_month(&mut ink, scene, geo, shared),
                    Kind::Agenda => paint_agenda(&mut ink, scene, geo, shared),
                }
            });
        if scene.kind == Kind::Grid {
            paint_gutter(&mut ink, scene, geo);
        }
    });
}

/// Every line of text a frame draws, kept by the overflow tests.
#[cfg(test)]
pub(crate) mod drawn {
    use std::cell::RefCell;

    use gpui::{Bounds, Pixels};

    #[derive(Clone, Debug)]
    pub struct Text {
        pub text: String,
        pub bounds: Bounds<Pixels>,
        /// The content mask it was drawn under: past it, the text is cut off.
        pub mask: Bounds<Pixels>,
    }

    thread_local! {
        pub static TEXTS: RefCell<Option<Vec<Text>>> = const { RefCell::new(None) };
    }

    /// Starts keeping the text drawn on this thread.
    pub fn record() {
        TEXTS.with(|t| *t.borrow_mut() = Some(Vec::new()));
    }

    /// The text drawn since `record`, and starts again.
    pub fn take() -> Vec<Text> {
        TEXTS.with(|t| t.borrow_mut().replace(Vec::new()).unwrap_or_default())
    }
}

/// The fewest letters a cut line keeps before its "…"; with less room it draws nothing.
pub(crate) const MIN_KEPT: usize = 2;

/// The characters a cut line keeps before its ellipsis.
pub(crate) fn kept_chars(cut: &str) -> usize {
    cut.trim_end_matches('…').trim_end().chars().count()
}

/// The pen: quads and text at the calendar's scale.
struct Ink<'a> {
    window: &'a mut Window,
    cx: &'a mut App,
    s: f32,
    family: SharedString,
    /// While set, text is drawn with what the query finds in it marked in this colour.
    mark: Option<(Rc<Query>, Hsla)>,
    /// Lays the frame out without drawing it: where things go (the hit lists) without any
    /// paint, for assistive technology, which needs them before the frame is painted.
    dry: bool,
}

impl Ink<'_> {
    fn u(&self, v: f32) -> Pixels {
        px(v * self.s)
    }

    /// Notes a line of text for the overflow tests; nothing outside them.
    #[cfg_attr(not(test), allow(unused_variables))]
    fn note(&self, text: &str, at: Point<Pixels>, width: Pixels, height: Pixels) {
        #[cfg(test)]
        if self.dry {
            return;
        }
        #[cfg(test)]
        drawn::TEXTS.with(|t| {
            if let Some(log) = t.borrow_mut().as_mut() {
                log.push(drawn::Text {
                    text: text.to_string(),
                    bounds: Bounds::new(at, size(width, height)),
                    mask: self.window.content_mask().bounds,
                });
            }
        });
    }

    fn fill(&mut self, b: Bounds<Pixels>, color: Hsla) {
        if self.dry {
            return;
        }
        self.window.paint_quad(fill(b, color));
    }

    fn rounded(&mut self, b: Bounds<Pixels>, radius: impl Into<Corners<Pixels>>, color: Hsla) {
        if self.dry {
            return;
        }
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
        if self.dry {
            return;
        }
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

    /// The runs for `text`: one, or, while marking, the found words with a background of the
    /// mark's colour.
    fn runs(&self, text: &str, weight: FontWeight, color: Hsla) -> Vec<TextRun> {
        let Some((query, mark)) = &self.mark else {
            return vec![self.run(text.len(), weight, color)];
        };
        let mut out = Vec::new();
        let mut at = 0;
        for r in query.ranges(text) {
            if r.start > at {
                out.push(self.run(r.start - at, weight, color));
            }
            let mut found = self.run(r.end - r.start, weight, color);
            found.background_color = Some(*mark);
            out.push(found);
            at = r.end;
        }
        if at < text.len() || out.is_empty() {
            out.push(self.run(text.len() - at, weight, color));
        }
        out
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
        let mut runs = self.runs(text, weight, color);
        if let Some(w) = max_w {
            if w <= px(1.) {
                return None;
            }
            let mut wrapper = self
                .window
                .text_system()
                .line_wrapper(runs[0].font.clone(), font_size);
            let (cut, cut_runs) = wrapper.truncate_line(line, w, "…", &runs, TruncateFrom::End);
            let next = cut_runs.into_owned();
            if next.is_empty() {
                return None;
            }
            // A lone "…", or one letter and "…", says nothing: leave the text out.
            if cut.as_ref() != text && kept_chars(&cut) < MIN_KEPT {
                return None;
            }
            line = cut;
            runs = next;
        }
        let shaped = self
            .window
            .text_system()
            .shape_line(line, font_size, &runs, None);
        // Too narrow for even the ellipsis: draw nothing rather than spill over.
        if max_w.is_some_and(|w| shaped.width > w) {
            return None;
        }
        Some(shaped)
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
        self.note(&shaped.text.clone(), at, width, leading);
        if self.dry {
            return width;
        }
        if self.mark.is_some() {
            let _ =
                shaped.paint_background(at, leading, TextAlign::Left, None, self.window, self.cx);
        }
        let _ = shaped.paint(at, leading, TextAlign::Left, None, self.window, self.cx);
        width
    }

    /// One line of text ending at `right`, cut with an ellipsis where it would pass `left`.
    #[allow(clippy::too_many_arguments)]
    fn text_right(
        &mut self,
        text: &str,
        size: f32,
        leading: f32,
        weight: FontWeight,
        color: Hsla,
        (left, right): (Pixels, Pixels),
        top: Pixels,
    ) -> Pixels {
        let Some(shaped) = self.shape(text, size, weight, color, Some(right - left)) else {
            return px(0.);
        };
        let width = shaped.width;
        let leading = self.u(leading);
        self.note(
            &shaped.text.clone(),
            point(right - width, top),
            width,
            leading,
        );
        if self.dry {
            return width;
        }
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
        let runs = self.runs(text, weight, color);
        let font_size = self.u(size);
        let Ok(lines) = self.window.text_system().shape_text(
            text.to_string().into(),
            font_size,
            &runs,
            Some(width),
            Some(max_lines),
        ) else {
            return 0;
        };
        let leading = self.u(leading);
        let mut used = 0;
        let mut y = at.y;
        for line in lines {
            self.note(
                text,
                point(at.x, y),
                line.width(),
                line.size(leading).height,
            );
            if self.mark.is_some() && !self.dry {
                let _ = line.paint_background(
                    point(at.x, y),
                    leading,
                    TextAlign::Left,
                    None,
                    self.window,
                    self.cx,
                );
            }
            if !self.dry {
                let _ = line.paint(
                    point(at.x, y),
                    leading,
                    TextAlign::Left,
                    None,
                    self.window,
                    self.cx,
                );
            }
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

/// What a chip says.
pub(crate) struct ChipText<'a> {
    pub title: &'a str,
    pub time: &'a str,
    pub location: Option<&'a str>,
    pub links: &'a [EventLink],
}

/// A link's mark, or the mark for several.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Glyph {
    Doc,
    Agent,
    Links,
}

impl From<LinkKind> for Glyph {
    fn from(kind: LinkKind) -> Self {
        match kind {
            LinkKind::Doc => Glyph::Doc,
            LinkKind::Agent => Glyph::Agent,
        }
    }
}

/// Draws a mark `GLYPH` high with its left edge at `x`, centred on `mid`.
fn paint_glyph(ink: &mut Ink, glyph: Glyph, x: Pixels, mid: Pixels, color: Hsla) {
    let g = ink.u(GLYPH);
    let stroke = ink.u(1.2).max(px(1.));
    match glyph {
        Glyph::Doc => {
            // A page: an outline with two lines of text on it.
            let w = g * 0.78;
            let page = Bounds::new(point(x + (g - w) / 2., mid - g / 2.), size(w, g));
            ink.outlined(page, ink.u(1.5), transparent_black(), stroke, color);
            for k in 1..3 {
                let y = page.top() + g * (k as f32 * 0.3);
                ink.fill(
                    Bounds::new(point(page.left() + w * 0.25, y), size(w * 0.5, stroke)),
                    color,
                );
            }
        }
        Glyph::Agent => {
            // A ring with a dot: someone, rather than something.
            let ring = Bounds::new(point(x, mid - g / 2.), size(g, g));
            ink.outlined(ring, g / 2., transparent_black(), stroke, color);
            let d = g * 0.36;
            ink.rounded(
                Bounds::new(point(x + (g - d) / 2., mid - d / 2.), size(d, d)),
                d / 2.,
                color,
            );
        }
        Glyph::Links => {
            // A chain: two links side by side.
            let (w, h) = (g * 0.66, g * 0.46);
            for k in 0..2 {
                let link = Bounds::new(point(x + g * 0.34 * k as f32, mid - h / 2.), size(w, h));
                ink.outlined(link, h / 2., transparent_black(), stroke, color);
            }
        }
    }
}

/// How wide [`paint_badge`] would draw.
fn badge_width(ink: &mut Ink, links: &[EventLink]) -> Pixels {
    match links.len() {
        0 => px(0.),
        1 => ink.u(GLYPH),
        n => ink.u(GLYPH + 3.) + ink.measure(&n.to_string(), SMALL, FontWeight::MEDIUM),
    }
}

/// The links on a chip too small to list them: one link's mark, or a chain and the count.
/// Right-aligned at `right`; returns where it starts.
fn paint_badge(
    ink: &mut Ink,
    links: &[EventLink],
    right: Pixels,
    mid: Pixels,
    color: Hsla,
) -> Pixels {
    let w = badge_width(ink, links);
    let x = right - w;
    match links {
        [] => {}
        [one] => paint_glyph(ink, one.kind.into(), x, mid, color),
        many => {
            paint_glyph(ink, Glyph::Links, x, mid, color);
            ink.text(
                &many.len().to_string(),
                SMALL,
                SMALL_LEADING,
                FontWeight::MEDIUM,
                color,
                point(x + ink.u(GLYPH + 3.), mid - ink.u(SMALL_LEADING) / 2.),
                None,
            );
        }
    }
    x
}

/// Where a chip drew its links, as (bounds, index) pairs. A badge for a single link is that
/// link; a badge for several isn't a link (a click on it selects the event).
type LinkRects = Vec<(Bounds<Pixels>, usize)>;

/// A timed event's chip on the grid. Tall enough, it lists its links under its text; smaller,
/// it shows a badge.
fn paint_timed_chip(
    ink: &mut Ink,
    rect: Bounds<Pixels>,
    text: &ChipText,
    cc: ChipColors,
    background: Hsla,
    faded: bool,
) -> LinkRects {
    let mut out = Vec::new();
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
        return out;
    }
    let h = rect.size.height;
    let (fg, muted) = (cc.text.opacity(alpha), cc.muted_text.opacity(alpha));
    ink.window
        .with_content_mask(Some(ContentMask { bounds: rect }), |window| {
            let mut ink = Ink {
                window,
                cx: ink.cx,
                s: ink.s,
                family: ink.family.clone(),
                mark: ink.mark.clone(),
                dry: ink.dry,
            };
            let pad = ink.u(3.);
            let links = text.links;
            if h >= ink.u(TITLE_LEADING + SMALL_LEADING + 5.) {
                // Links listed under the text when every one of them fits, and the chip is wide
                // enough for a label; otherwise a badge by the title.
                let rows = ink.u(LINK_ROW) * links.len() as f32;
                let list = !links.is_empty()
                    && w >= ink.u(60.)
                    && h >= pad * 2. + ink.u(TITLE_LEADING + SMALL_LEADING) + rows;
                let badge = if links.is_empty() || list || w < ink.u(48.) {
                    px(0.)
                } else {
                    badge_width(&mut ink, links) + ink.u(6.)
                };
                let reserved = if list { rows } else { px(0.) };
                // The title over as many lines as leave room for the time (and the links).
                let room = h - pad * 2. - ink.u(SMALL_LEADING) - reserved;
                // Narrow chips keep the title to a line: wrapping there breaks words apart.
                let lines = if w < ink.u(84.) || badge > px(0.) {
                    1
                } else {
                    ((f32::from(room) / f32::from(ink.u(TITLE_LEADING))).floor() as usize)
                        .clamp(1, 3)
                };
                let top = rect.top() + pad;
                let used = ink.wrapped(
                    text.title,
                    TITLE,
                    TITLE_LEADING,
                    FontWeight::MEDIUM,
                    fg,
                    point(x, top),
                    w - badge,
                    lines,
                );
                if badge > px(0.) {
                    let mid = top + ink.u(TITLE_LEADING) / 2.;
                    let at = paint_badge(&mut ink, links, x + w, mid, muted);
                    if links.len() == 1 {
                        out.push((badge_hit(&ink, at, x + w, mid), 0));
                    }
                }
                let mut y = top + ink.u(TITLE_LEADING) * used.max(1) as f32;
                ink.text(
                    text.time,
                    SMALL,
                    SMALL_LEADING,
                    FontWeight::NORMAL,
                    muted,
                    point(x, y),
                    Some(w),
                );
                y += ink.u(SMALL_LEADING);
                if let Some(location) = text.location
                    && y + ink.u(SMALL_LEADING) + reserved <= rect.bottom() - pad
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
                    y += ink.u(SMALL_LEADING);
                }
                if list {
                    for (i, link) in links.iter().enumerate() {
                        let row = Bounds::new(point(x, y), size(w, ink.u(LINK_ROW)));
                        let mid = row.top() + row.size.height / 2.;
                        paint_glyph(&mut ink, link.kind.into(), x, mid, muted);
                        let lx = x + ink.u(GLYPH + 5.);
                        ink.text(
                            &link.label,
                            SMALL,
                            SMALL_LEADING,
                            FontWeight::MEDIUM,
                            fg,
                            point(lx, mid - ink.u(SMALL_LEADING) / 2.),
                            Some(x + w - lx),
                        );
                        out.push((row, i));
                        y += ink.u(LINK_ROW);
                    }
                }
            } else {
                // One line: the title, then the time if there's room, and the links' badge at
                // the end.
                let y = rect.top() + ((h - ink.u(TITLE_LEADING)) / 2.).max(px(0.));
                let mid = y + ink.u(TITLE_LEADING) / 2.;
                let badge = if links.is_empty() || w < ink.u(48.) {
                    px(0.)
                } else {
                    badge_width(&mut ink, links) + ink.u(6.)
                };
                let used = ink.text(
                    text.title,
                    TITLE,
                    TITLE_LEADING,
                    FontWeight::MEDIUM,
                    fg,
                    point(x, y),
                    Some(w - badge),
                );
                let gap = ink.u(6.);
                let left = w - badge - used - gap;
                if left > ink.u(28.) {
                    ink.text(
                        text.time,
                        SMALL,
                        TITLE_LEADING,
                        FontWeight::NORMAL,
                        muted,
                        point(x + used + gap, y + ink.u(0.5)),
                        Some(left),
                    );
                }
                if badge > px(0.) {
                    let at = paint_badge(&mut ink, links, x + w, mid, muted);
                    if links.len() == 1 {
                        out.push((badge_hit(&ink, at, x + w, mid), 0));
                    }
                }
            }
        });
    out
}

/// The clickable area round a badge: a little more than the mark.
fn badge_hit(ink: &Ink, left: Pixels, right: Pixels, mid: Pixels) -> Bounds<Pixels> {
    let half = ink.u(LINK_ROW) / 2.;
    Bounds::from_corners(
        point(left - ink.u(3.), mid - half),
        point(right + ink.u(3.), mid + half),
    )
}

/// A chip across days: the all-day row, and spans in the month.
#[allow(clippy::too_many_arguments)]
fn paint_span_chip(
    ink: &mut Ink,
    rect: Bounds<Pixels>,
    title: &str,
    links: &[EventLink],
    cc: ChipColors,
    open_left: bool,
    open_right: bool,
    faded: bool,
) -> LinkRects {
    let mut out = Vec::new();
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
    let y = rect.top() + ((rect.size.height - ink.u(TITLE_LEADING)) / 2.).max(px(0.));
    // An end that goes on past the week (into the next row of the month, or the next
    // period) says so: a chevron pointing on, where there's room for one beside the title.
    let chevron = ink.u(CHEVRON);
    let (on_left, on_right) = if rect.size.width >= ink.u(CHEVRON * 4.) {
        (open_left, open_right)
    } else {
        (false, false)
    };
    let muted = cc.muted_text.opacity(alpha);
    if on_left {
        ink.text(
            "‹",
            TITLE,
            TITLE_LEADING,
            FontWeight::MEDIUM,
            muted,
            point(rect.left() + ink.u(3.), y),
            Some(chevron),
        );
    }
    if on_right {
        ink.text_right(
            "›",
            TITLE,
            TITLE_LEADING,
            FontWeight::MEDIUM,
            muted,
            (rect.right() - chevron - ink.u(3.), rect.right() - ink.u(3.)),
            y,
        );
    }
    let x = rect.left()
        + ink.u(if open_left { 6. } else { 9. })
        + if on_left { chevron } else { px(0.) };
    let w = rect.right() - x - ink.u(4.) - if on_right { chevron } else { px(0.) };
    let mid = rect.top() + rect.size.height / 2.;
    let badge = if links.is_empty() || w < ink.u(48.) {
        px(0.)
    } else {
        badge_width(ink, links) + ink.u(6.)
    };
    ink.text(
        title,
        TITLE,
        TITLE_LEADING,
        FontWeight::MEDIUM,
        cc.text.opacity(alpha),
        point(x, y),
        Some(w - badge),
    );
    if badge > px(0.) {
        let at = paint_badge(ink, links, x + w, mid, cc.muted_text.opacity(alpha));
        if links.len() == 1 {
            out.push((badge_hit(ink, at, x + w, mid), 0));
        }
    }
    out
}

/// "+3 more", or "+3" where that doesn't fit.
fn more_label(ink: &mut Ink, hidden: usize, room: Pixels) -> String {
    let long = format!("+{hidden} more");
    if ink.measure(&long, SMALL, FontWeight::MEDIUM) <= room {
        long
    } else {
        format!("+{hidden}")
    }
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
    // Its own days only: a swipe draws the next period's beside them.
    ink.fill(
        Bounds::from_corners(point(geo.left, b.top()), point(geo.col_x(days), b.bottom())),
        c.background,
    );

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
                mark: ink.mark.clone(),
                dry: ink.dry,
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

            // Hour lines (their labels are the gutter's).
            for h in 1..24 {
                let y = geo.y_of(h as f32 * 60.);
                if y < geo.grid_top - ink.u(10.) || y > b.bottom() + ink.u(10.) {
                    continue;
                }
                ink.hline(geo.left, geo.col_x(days), y, c.line);
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
                mark: ink.mark.clone(),
                dry: ink.dry,
            };
            let mut hits = shared.hits.borrow_mut();
            // No column narrower than a readable minimum: a group needing more shows those
            // that fit, and "+N" for the rest.
            let avail = geo.col_w - margin;
            let narrowest = ink.u(if geo.touch { TOUCH_TARGET } else { MIN_COLUMN });
            let strip = ink.u(MIN_STRIP);
            let squeeze = |columns: usize| {
                layout::squeeze(
                    columns,
                    f32::from(avail),
                    f32::from(narrowest),
                    f32::from(strip),
                )
            };
            let mut hidden: Vec<HiddenPiece> = Vec::new();
            for piece in &scene.laid.timed {
                let top = geo.y_of(piece.top);
                let bottom = geo.y_of(piece.bottom.max(piece.top + min));
                if bottom < geo.grid_top || top > b.bottom() {
                    continue;
                }
                let chip = &scene.laid.chips[piece.chip];
                let x0 = geo.col_x(piece.col as i32);
                let fit = squeeze(piece.place.columns);
                let Some((left, width)) = fit.place(piece.place) else {
                    hidden.push(HiddenPiece {
                        col: piece.col,
                        top,
                        bottom,
                        id: chip.id.clone(),
                        fit,
                    });
                    continue;
                };
                let rect = Bounds::from_corners(
                    point(x0 + px(left), top),
                    point(x0 + px(left + width), bottom),
                );
                let selected = scene.selected(&chip.id);
                let faded = scene.moving(&chip.id) || scene.dimmed(chip);
                let cc = ChipColors::new(chip.color, c, selected, scene.past(chip));
                let text = ChipText {
                    title: &chip.title,
                    time: &chip.time,
                    location: chip.location.as_deref(),
                    links: &chip.links,
                };
                ink.mark = scene.mark(chip);
                let links = paint_timed_chip(&mut ink, rect, &text, cc, c.background, faded);
                ink.mark = None;
                if scene.find.is_some() && chip.found {
                    ink.outlined(rect, ink.u(4.), transparent_black(), ink.u(1.5), c.find);
                }
                push_links(shared, chip, links, &grid_area, Area::Grid);
                let resize = (piece.ends && !chip.read_only)
                    .then(|| {
                        if !geo.touch {
                            Some(Bounds::from_corners(
                                point(rect.left(), rect.bottom() - ink.u(6.)),
                                rect.bottom_right(),
                            ))
                        } else if selected && !faded {
                            // A handle at the bottom, taking a 44-point band across the edge.
                            paint_handle(&mut ink, point(rect.center().x, rect.bottom()), cc);
                            let half = ink.u(TOUCH_TARGET / 2.);
                            Some(Bounds::from_corners(
                                point(rect.left(), rect.bottom() - half),
                                point(rect.right(), rect.bottom() + half),
                            ))
                        } else {
                            None
                        }
                    })
                    .flatten();
                hits.push(Hit {
                    id: chip.id.clone(),
                    bounds: rect.intersect(&grid_area),
                    area: Area::Grid,
                    resize,
                    read_only: chip.read_only,
                });
            }
            drop(hits);
            paint_crowds(&mut ink, scene, geo, shared, hidden);

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
                        let text = ChipText {
                            title: &ghost.title,
                            time: &ghost.label,
                            location: None,
                            links: &[],
                        };
                        note_ghost(shared, rect);
                        paint_timed_chip(&mut ink, rect, &text, ghost_cc, c.background, false);
                        ink.outlined(rect, ink.u(4.), transparent_black(), ink.u(1.5), accent);
                    }
                    d += Duration::days(1);
                }
            }

            // Now: a quiet line across the week, a strong one across today.
            if let Some(col) = now_col {
                ink.fill(
                    Bounds::new(
                        point(geo.left, now_y),
                        size(geo.col_x(days) - geo.left, px(1.)),
                    ),
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

    // Lines between days, through the all-day row and the grid.
    for i in 0..days {
        ink.vline(geo.col_x(i), geo.head_bottom, b.bottom(), c.line);
    }

    // The head: each day's name and date.
    for i in 0..days {
        let date = geo.date(i);
        let x = geo.col_x(i) + ink.u(10.);
        let mid = b.top() + ink.u(HEAD) / 2.;
        let name = time::weekday_name(date.weekday(), days > 1);
        let number = date.day().to_string();
        let today = date == scene.today;
        let room = geo.col_w - ink.u(14.);
        let name_w = ink.measure(name, LABEL, FontWeight::NORMAL);
        let nw = ink.measure(&number, LABEL, FontWeight::SEMIBOLD);
        let badge = if today {
            ink.u(24.).max(nw + ink.u(10.))
        } else {
            nw
        };
        if name_w + ink.u(6.) + badge > room {
            // Too narrow for "Mon 16" on a line: the name over the number, centred, as a
            // phone's week does; the name's first letter where even that's too wide.
            paint_stacked_head(ink, scene, geo, i, name, &number, today);
        } else {
            let w = ink.text(
                name,
                LABEL,
                18.,
                FontWeight::NORMAL,
                c.muted_text,
                point(x, mid - ink.u(9.)),
                Some(room),
            );
            // The same gap after the weekday on every day, today's badge included.
            let nx = x + w + ink.u(6.);
            let number_right = if today {
                let pill = Bounds::new(point(nx, mid - ink.u(12.)), size(badge, ink.u(24.)));
                ink.rounded(pill, ink.u(12.), c.now);
                ink.text(
                    &number,
                    LABEL,
                    24.,
                    FontWeight::SEMIBOLD,
                    c.on_now,
                    point(pill.left() + (badge - nw) / 2., pill.top()),
                    Some(nw + px(1.)),
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
                    Some(nw + px(1.)),
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

    let max_rows = scene.all_day_rows(geo.bounds.size.height, geo.s);
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
            let label = more_label(ink, f.hidden, geo.col_w - ink.u(10.));
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
        paint_span_ghost(ink, scene, geo, shared, ghost, 0, row_top(0));
    }
    // The line under the all-day row.
    ink.hline(geo.left, geo.col_x(days), geo.grid_top - px(1.), c.line);
}

/// A timed event that didn't fit beside the others it overlaps.
struct HiddenPiece {
    col: u32,
    top: Pixels,
    bottom: Pixels,
    id: EventId,
    fit: layout::Squeeze,
}

/// "+N" for the events that didn't fit, one for each run of them on a day: in the strip at the
/// right of the columns that show, from the first one's start to the last one's end.
fn paint_crowds(
    ink: &mut Ink,
    scene: &Scene,
    geo: &Geo,
    shared: &Shared,
    mut hidden: Vec<HiddenPiece>,
) {
    let c = &scene.colors;
    hidden.sort_by(|a, b| {
        a.col
            .cmp(&b.col)
            .then(f32::from(a.top).total_cmp(&f32::from(b.top)))
    });
    let mut runs: Vec<(u32, Pixels, Pixels, Vec<EventId>, layout::Squeeze)> = Vec::new();
    for h in hidden {
        match runs.last_mut() {
            // Runs closer than a label's height are one: "+N"s don't stack.
            Some(run) if run.0 == h.col && h.top < run.2 + ink.u(18.) => {
                run.2 = run.2.max(h.bottom);
                if !run.3.contains(&h.id) {
                    run.3.push(h.id);
                }
            }
            _ => runs.push((h.col, h.top, h.bottom, vec![h.id], h.fit)),
        }
    }
    let gap = ink.u(2.);
    let pill_h = ink.u(18.);
    for (col, top, bottom, ids, fit) in runs {
        let x0 = geo.col_x(col as i32);
        let left = x0 + px(fit.events) + gap;
        let rect = Bounds::new(
            point(left, top),
            size(
                (x0 + px(fit.events + fit.chip) - left).max(px(1.)),
                (bottom - top).max(pill_h),
            ),
        );
        // "+3", or just "+" in a strip too narrow for the count.
        let mut label = format!("+{}", ids.len());
        let mut w = ink.measure(&label, SMALL, FontWeight::SEMIBOLD);
        if w > rect.size.width - ink.u(2.) {
            label = "+".into();
            w = ink.measure(&label, SMALL, FontWeight::SEMIBOLD);
        }
        ink.rounded(rect, ink.u(4.), c.highlight);
        ink.outlined(rect, ink.u(4.), transparent_black(), px(1.), c.line);
        ink.text(
            &label,
            SMALL,
            SMALL_LEADING,
            FontWeight::SEMIBOLD,
            c.text,
            point(
                rect.left() + ((rect.size.width - w) / 2.).max(px(0.)),
                rect.top() + ink.u(2.),
            ),
            Some(rect.size.width),
        );
        shared.crowds.borrow_mut().push((rect, ids));
    }
}

/// A day's name over its number, centred in its column, for narrow columns.
fn paint_stacked_head(
    ink: &mut Ink,
    scene: &Scene,
    geo: &Geo,
    i: i32,
    name: &str,
    number: &str,
    today: bool,
) {
    let c = &scene.colors;
    let top = geo.bounds.top();
    let center = geo.col_x(i) + geo.col_w / 2.;
    let room = geo.col_w - ink.u(4.);
    let first_letter: String = name.chars().take(1).collect();
    let name = if ink.measure(name, SMALL, FontWeight::MEDIUM) <= room {
        name.to_string()
    } else {
        first_letter
    };
    let w = ink.measure(&name, SMALL, FontWeight::MEDIUM);
    ink.text(
        &name,
        SMALL,
        SMALL_LEADING,
        FontWeight::MEDIUM,
        c.muted_text,
        point(center - w / 2., top + ink.u(3.)),
        Some(room),
    );
    let nw = ink.measure(number, TITLE, FontWeight::SEMIBOLD);
    let y = top + ink.u(18.);
    if today {
        let d = ink.u(20.).max(nw + ink.u(8.)).min(room);
        let pill = Bounds::new(point(center - d / 2., y), size(d, ink.u(20.)));
        ink.rounded(pill, ink.u(10.), c.now);
        ink.text(
            number,
            TITLE,
            20.,
            FontWeight::SEMIBOLD,
            c.on_now,
            point(center - nw / 2., y),
            Some(room),
        );
    } else {
        ink.text(
            number,
            TITLE,
            20.,
            FontWeight::SEMIBOLD,
            c.text,
            point(center - nw / 2., y),
            Some(room),
        );
    }
}

/// The handle a selected event shows on touch, where dragging changes its end.
fn paint_handle(ink: &mut Ink, at: Point<Pixels>, cc: ChipColors) {
    let r = ink.u(5.);
    ink.outlined(
        Bounds::new(point(at.x - r, at.y - r), size(r * 2., r * 2.)),
        r,
        cc.text,
        ink.u(2.),
        cc.accent,
    );
}

/// The time grid's gutter: the zone, "All day", the hours and the time now. A swipe leaves it
/// where it is.
fn paint_gutter(ink: &mut Ink, scene: &Scene, geo: &Geo) {
    let c = &scene.colors;
    let b = geo.bounds;
    let edge = geo.gutter;
    ink.fill(
        Bounds::from_corners(b.origin, point(edge, b.bottom())),
        c.background,
    );
    // The second zone's column: its abbreviation, a quiet line between, and its hours below.
    let second_right = b.left() + geo.second - ink.u(6.);
    let second_hours = scene.options.secondary_zone.map(|zone| {
        // The hours as they are on today, if it's on screen, or the first day.
        let date = scene
            .today_col()
            .map_or(scene.first, |i| scene.first + Duration::days(i as i64));
        let label = time::zone_label(date, zone);
        let room = (b.left() + ink.u(2.), second_right);
        let label = zone_fit(ink, label, room);
        ink.text_right(
            &label,
            SMALL,
            SMALL_LEADING,
            FontWeight::NORMAL,
            c.muted_text.opacity(0.75),
            room,
            b.top() + (ink.u(HEAD) - ink.u(SMALL_LEADING)) / 2.,
        );
        ink.vline(
            b.left() + geo.second,
            geo.grid_top,
            b.bottom(),
            c.faint_line,
        );
        time::secondary_hours(date, scene.options.time_zone, zone, scene.options.hour12)
    });
    // The zone, by the days' names.
    let primary = (b.left() + geo.second + ink.u(2.), edge - ink.u(8.));
    let label = time::zone_label(scene.first, scene.options.time_zone);
    let label = zone_fit(ink, label, primary);
    ink.text_right(
        &label,
        SMALL,
        SMALL_LEADING,
        FontWeight::NORMAL,
        c.muted_text,
        primary,
        b.top() + (ink.u(HEAD) - ink.u(SMALL_LEADING)) / 2.,
    );
    // "All day", where the gutter has room for it.
    if ink.measure("All day", SMALL, FontWeight::NORMAL) <= edge - b.left() - ink.u(12.) {
        ink.text_right(
            "All day",
            SMALL,
            SMALL_LEADING,
            FontWeight::NORMAL,
            c.muted_text,
            (b.left(), edge - ink.u(8.)),
            geo.head_bottom + ink.u(7.),
        );
    }
    // The line under the all-day row runs on through the gutter.
    ink.hline(b.left(), edge, geo.grid_top - px(1.), c.line);
    let viewport = Bounds::from_corners(point(b.left(), geo.grid_top), point(edge, b.bottom()));
    let now_shown = scene.today_col().is_some();
    let now_y = geo.y_of(scene.now_minute);
    ink.window
        .with_content_mask(Some(ContentMask { bounds: viewport }), |window| {
            let mut ink = Ink {
                window,
                cx: ink.cx,
                s: ink.s,
                family: ink.family.clone(),
                mark: ink.mark.clone(),
                dry: ink.dry,
            };
            // The time now's pill, kept whole inside the grid like the hours.
            let pill_h = ink.u(16.);
            let pill_top = (now_y - pill_h / 2.).max(geo.grid_top);
            // The hours, out of the way of the time now.
            for h in 1..24 {
                let y = geo.y_of(h as f32 * 60.);
                if y < geo.grid_top || y > b.bottom() + ink.u(10.) {
                    continue;
                }
                // Centred on its line, but never cut by the grid's top: a line at the very top
                // (the grid scrolled to 8 AM) keeps its label, just below it.
                let top = (y - ink.u(SMALL_LEADING / 2.)).max(geo.grid_top);
                // Far enough that the two labels don't touch.
                let bottom = top + ink.u(SMALL_LEADING);
                if now_shown && bottom + ink.u(1.) > pill_top && top < pill_top + pill_h + ink.u(1.)
                {
                    continue;
                }
                ink.text_right(
                    &time::format_hour(h, scene.options.hour12),
                    SMALL,
                    SMALL_LEADING,
                    FontWeight::NORMAL,
                    c.muted_text,
                    primary,
                    top,
                );
                if let Some(hours) = &second_hours {
                    ink.text_right(
                        &hours[h as usize],
                        SMALL,
                        SMALL_LEADING,
                        FontWeight::NORMAL,
                        c.muted_text.opacity(0.75),
                        (b.left() + ink.u(2.), second_right),
                        top,
                    );
                }
            }
            // The time now.
            if now_shown && now_y >= geo.grid_top && now_y <= b.bottom() {
                let t = scene.now.with_timezone(&scene.options.time_zone).time();
                let label = time::format_clock(t, scene.options.hour12);
                let w = ink.measure(&label, 10.5, FontWeight::SEMIBOLD);
                let (pad, h) = (ink.u(4.), pill_h);
                let right = edge - ink.u(4.);
                let pill = Bounds::from_corners(
                    point(right - w - pad * 2., pill_top),
                    point(right, pill_top + h),
                );
                ink.rounded(pill, h / 2., c.now);
                ink.text(
                    &label,
                    10.5,
                    16.,
                    FontWeight::SEMIBOLD,
                    c.on_now,
                    point(pill.left() + pad, pill.top()),
                    Some(w + px(1.)),
                );
            }
        });
}

/// A zone's label as it fits its column: "GMT+10:30" becomes "+10:30" where it's too wide.
fn zone_fit(ink: &mut Ink, label: String, (left, right): (Pixels, Pixels)) -> String {
    if ink.measure(&label, SMALL, FontWeight::NORMAL) > right - left
        && let Some(offset) = label.strip_prefix("GMT")
    {
        offset.to_string()
    } else {
        label
    }
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
    let faded = scene.moving(&chip.id) || scene.dimmed(chip);
    let past = scene.past(chip);
    let cc = ChipColors::new(chip.color, c, selected, past);
    let ring = scene.find.is_some() && chip.found && !span.dot;
    for (a, z) in layout::visible_runs(span.first, span.last, span.lane, fits) {
        let open_left = a == span.first && span.cont_left;
        let open_right = z == span.last && span.cont_right;
        let x0 = geo.col_x(a) + if open_left { px(0.) } else { ink.u(3.) };
        let x1 = geo.col_x(z + 1) - if open_right { px(0.) } else { ink.u(4.) };
        let rect = Bounds::from_corners(point(x0, top), point(x1, top + lane_h - ink.u(2.)));
        ink.mark = scene.mark(chip);
        let links = if span.dot {
            paint_dot_row(ink, rect, chip, cc, selected, faded)
        } else {
            paint_span_chip(
                ink,
                rect,
                &chip.title,
                &chip.links,
                cc,
                open_left,
                open_right,
                faded,
            )
        };
        ink.mark = None;
        if ring {
            ink.outlined(rect, ink.u(4.), transparent_black(), ink.u(1.5), c.find);
        }
        push_links(shared, chip, links, &geo.bounds, area);
        let resize = (chip.all_day && !chip.read_only && z == span.last && !span.cont_right)
            .then(|| {
                if !geo.touch {
                    Some(Bounds::from_corners(
                        point(rect.right() - ink.u(6.), rect.top()),
                        rect.bottom_right(),
                    ))
                } else if selected && !faded {
                    paint_handle(ink, point(rect.right(), rect.center().y), cc);
                    let half = ink.u(TOUCH_TARGET / 2.);
                    Some(Bounds::from_corners(
                        point(rect.right() - half, rect.center().y - half),
                        point(rect.right() + half, rect.center().y + half),
                    ))
                } else {
                    None
                }
            })
            .flatten();
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
) -> LinkRects {
    let mut out = Vec::new();
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
    let badge = if chip.links.is_empty() || w < ink.u(60.) {
        px(0.)
    } else {
        badge_width(ink, &chip.links) + ink.u(6.)
    };
    if badge > px(0.) {
        let at = paint_badge(ink, &chip.links, x + w, mid, cc.muted_text.opacity(alpha));
        if chip.links.len() == 1 {
            out.push((badge_hit(ink, at, x + w, mid), 0));
        }
    }
    let w = w - badge;
    // A narrow day keeps its room for the title: the time goes.
    let tw = if rect.size.width < ink.u(90.) {
        px(0.)
    } else {
        ink.text(
            &chip.time,
            SMALL,
            TITLE_LEADING,
            FontWeight::NORMAL,
            cc.muted_text.opacity(alpha),
            point(x, y + ink.u(0.5)),
            Some(w),
        )
    };
    let gap = if tw > px(0.) { ink.u(5.) } else { px(0.) };
    ink.text(
        &chip.title,
        TITLE,
        TITLE_LEADING,
        FontWeight::MEDIUM,
        cc.text.opacity(alpha),
        point(x + tw + gap, y),
        Some(w - tw - gap),
    );
    out
}

/// Keeps where a chip drew its links, for the mouse.
fn push_links(shared: &Shared, chip: &Chip, links: LinkRects, clip: &Bounds<Pixels>, area: Area) {
    let mut hits = shared.links.borrow_mut();
    for (bounds, index) in links {
        let bounds = bounds.intersect(clip);
        if bounds.size.width > px(0.) && bounds.size.height > px(0.) {
            hits.push(LinkHit {
                id: chip.id.clone(),
                index,
                bounds,
                area,
                read_only: chip.read_only,
            });
        }
    }
}

/// The ghost of an all-day drag: the days it would cover, tinted, and its chip over the
/// first row.
fn paint_span_ghost(
    ink: &mut Ink,
    scene: &Scene,
    geo: &Geo,
    shared: &Shared,
    ghost: &Ghost,
    row: u32,
    top: Pixels,
) {
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
    note_ghost(shared, rect);
    let label = format!("{}  {}", ghost.title, ghost.label);
    paint_span_chip(ink, rect, &label, &[], cc, false, false, false);
    ink.outlined(rect, ink.u(4.), transparent_black(), ink.u(1.5), accent);
}

/// Keeps where the ghost was drawn: all of it, across the days and weeks it covers.
fn note_ghost(shared: &Shared, rect: Bounds<Pixels>) {
    let all = match shared.ghost.get() {
        Some(b) => b.union(&rect),
        None => rect,
    };
    shared.ghost.set(Some(all));
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
                let label = more_label(ink, f.hidden, geo.col_w - ink.u(10.));
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
            paint_span_ghost(ink, scene, geo, shared, ghost, row, top);
        }
    }
}

// The agenda: a list of days, each with its events.

const AGENDA_HEAD: f32 = 34.;
const AGENDA_PAD_X: f32 = 16.;
const AGENDA_ROW_PAD: f32 = 8.;
const AGENDA_DAY_GAP: f32 = 6.;
const AGENDA_EMPTY: f32 = 30.;
const PILL_H: f32 = 22.;
const PILL_GAP: f32 = 6.;

/// One row's measurements, worked out before anything is drawn.
struct AgendaPlace {
    day: usize,
    /// None for a day's header, or for "Nothing planned" with `empty`.
    row: Option<usize>,
    empty: bool,
    top: f32,
    height: f32,
    title_lines: usize,
    /// Each link's pill: left and top inside the row's text column, and width.
    pills: Vec<(f32, f32, f32)>,
}

/// The agenda's columns at this width, in pixels from the row's left: where the time column,
/// the colour bar and the text start, and how wide the text is.
struct AgendaColumns {
    time_w: Pixels,
    bar_x: Pixels,
    text_x: Pixels,
    text_w: Pixels,
}

fn agenda_columns(ink: &Ink, width: Pixels) -> AgendaColumns {
    let narrow = width < ink.u(480.);
    let pad = ink.u(if narrow { 12. } else { AGENDA_PAD_X });
    let time_w = ink.u(if narrow { 60. } else { 78. });
    let bar_x = pad + time_w + ink.u(8.);
    let text_x = bar_x + ink.u(14.);
    AgendaColumns {
        time_w,
        bar_x,
        text_x,
        text_w: (width - text_x - pad).max(px(0.)),
    }
}

fn pill_width(ink: &mut Ink, link: &EventLink) -> Pixels {
    ink.u(8. + GLYPH + 5. + 8.) + ink.measure(&link.label, SMALL, FontWeight::MEDIUM)
}

/// Where everything in the agenda goes, from its top, unscrolled.
fn agenda_places(ink: &mut Ink, scene: &Scene, cols: &AgendaColumns) -> (Vec<AgendaPlace>, f32) {
    let s = ink.s;
    let text_w = f32::from(cols.text_w);
    let mut out = Vec::new();
    let mut y = 0.;
    for (d, day) in scene.laid.agenda.iter().enumerate() {
        out.push(AgendaPlace {
            day: d,
            row: None,
            empty: false,
            top: y,
            height: AGENDA_HEAD * s,
            title_lines: 0,
            pills: Vec::new(),
        });
        y += AGENDA_HEAD * s;
        if day.rows.is_empty() {
            out.push(AgendaPlace {
                day: d,
                row: None,
                empty: true,
                top: y,
                height: AGENDA_EMPTY * s,
                title_lines: 0,
                pills: Vec::new(),
            });
            y += AGENDA_EMPTY * s;
        }
        for (r, row) in day.rows.iter().enumerate() {
            let chip = &scene.laid.chips[row.chip];
            let title_w = f32::from(ink.measure(&chip.title, TITLE, FontWeight::MEDIUM));
            let title_lines = if title_w > text_w { 2 } else { 1 };
            let mut h = TITLE_LEADING * s * title_lines as f32;
            if chip.location.is_some() {
                h += SMALL_LEADING * s;
            }
            let mut pills = Vec::new();
            if !chip.links.is_empty() {
                let (mut px_, mut py) = (0., h + 4. * s);
                for link in chip.links.iter() {
                    let w = f32::from(pill_width(ink, link)).min(text_w);
                    if px_ > 0. && px_ + w > text_w {
                        px_ = 0.;
                        py += (PILL_H + 4.) * s;
                    }
                    pills.push((px_, py, w));
                    px_ += w + PILL_GAP * s;
                }
                h = py + PILL_H * s;
            }
            let time_h = (TITLE_LEADING + SMALL_LEADING) * s;
            let height = h.max(time_h) + AGENDA_ROW_PAD * s * 2.;
            out.push(AgendaPlace {
                day: d,
                row: Some(r),
                empty: false,
                top: y,
                height,
                title_lines,
                pills,
            });
            y += height;
        }
        y += AGENDA_DAY_GAP * s;
    }
    (out, y)
}

/// What a day's header says: "Wednesday, November 18", or "Wed, Nov 18" when narrow.
fn agenda_day_label(date: NaiveDate, narrow: bool) -> String {
    if narrow {
        format!(
            "{}, {} {}",
            time::weekday_name(date.weekday(), true),
            time::month_short(date.month()),
            date.day()
        )
    } else {
        format!(
            "{}, {} {}",
            time::weekday_name(date.weekday(), false),
            time::month_name(date.month()),
            date.day()
        )
    }
}

fn paint_agenda(ink: &mut Ink, scene: &Scene, geo: &Geo, shared: &Shared) {
    let c = &scene.colors;
    let b = geo.bounds;
    ink.fill(b, c.background);
    let cols = agenda_columns(ink, b.size.width);
    let narrow = b.size.width < ink.u(480.);
    let pad = cols.bar_x - cols.time_w - ink.u(8.);
    let (places, content) = agenda_places(ink, scene, &cols);
    let view_h = f32::from(b.size.height);
    let max = (content - view_h).max(0.);
    shared.agenda_max.set(max);

    if places.is_empty() {
        ink.text(
            "No events",
            TITLE,
            TITLE_LEADING,
            FontWeight::MEDIUM,
            c.muted_text,
            point(b.left() + pad, b.top() + ink.u(16.)),
            Some(b.size.width - pad * 2.),
        );
        shared.agenda_scroll.set(Some(0.));
        return;
    }

    // Where it's scrolled: today's header at the top the first time, then wherever it was
    // left, with an event the keyboard selected brought into view.
    let mut scroll = shared.agenda_scroll.get().unwrap_or_else(|| {
        places
            .iter()
            .find(|p| p.row.is_none() && !p.empty && scene.laid.agenda[p.day].date >= scene.today)
            .map_or(0., |p| p.top)
    });
    if let Some(id) = shared.agenda_reveal.borrow_mut().take()
        && let Some(p) = places.iter().find(|p| {
            p.row
                .is_some_and(|r| scene.laid.chips[scene.laid.agenda[p.day].rows[r].chip].id == id)
        })
    {
        let head = AGENDA_HEAD * ink.s;
        if p.top - head < scroll {
            scroll = p.top - head;
        } else if p.top + p.height > scroll + view_h {
            scroll = p.top + p.height - view_h;
        }
    }
    let scroll = scroll.clamp(0., max);
    shared.agenda_scroll.set(Some(scroll));
    let y_of = |top: f32| b.top() + px(top - scroll);

    // The rows, under the band at the top where a day's header always is.
    let rows_area = Bounds::from_corners(
        point(b.left(), b.top() + ink.u(AGENDA_HEAD)),
        b.bottom_right(),
    );
    for place in &places {
        let top = y_of(place.top);
        let bottom = top + px(place.height);
        if bottom < rows_area.top() || top > b.bottom() {
            continue;
        }
        let day = &scene.laid.agenda[place.day];
        if place.empty {
            ink.text(
                "Nothing planned",
                SMALL,
                SMALL_LEADING,
                FontWeight::NORMAL,
                c.muted_text,
                point(b.left() + cols.text_x, top + ink.u(6.)),
                Some(cols.text_w),
            );
            continue;
        }
        let Some(r) = place.row else { continue };
        let row = &day.rows[r];
        ink.window
            .with_content_mask(Some(ContentMask { bounds: rows_area }), |window| {
                let mut ink = Ink {
                    window,
                    cx: ink.cx,
                    s: ink.s,
                    family: ink.family.clone(),
                    mark: ink.mark.clone(),
                    dry: ink.dry,
                };
                ink.mark = scene.mark(&scene.laid.chips[row.chip]);
                paint_agenda_row(&mut ink, scene, shared, &cols, row, place, top, rows_area);
            });
    }

    // The days' headers, each held at the top while its day scrolls under it.
    let heads: Vec<&AgendaPlace> = places
        .iter()
        .filter(|p| p.row.is_none() && !p.empty)
        .collect();
    for (i, place) in heads.iter().enumerate() {
        let head_h = px(place.height);
        let mut top = y_of(place.top).max(b.top());
        if let Some(next) = heads.get(i + 1) {
            top = top.min(y_of(next.top) - head_h);
        }
        if top + head_h < b.top() || top > b.bottom() {
            continue;
        }
        let rect = Bounds::new(point(b.left(), top), size(b.size.width, head_h));
        ink.fill(rect, c.background);
        ink.hline(b.left(), b.right(), rect.bottom() - px(1.), c.faint_line);
        let date = scene.laid.agenda[place.day].date;
        let today = date == scene.today;
        let label = agenda_day_label(date, narrow);
        let ty = top + (head_h - ink.u(18.)) / 2.;
        let w = ink.text(
            &label,
            LABEL,
            18.,
            FontWeight::SEMIBOLD,
            if today { c.now } else { c.text },
            point(b.left() + pad, ty),
            Some(b.size.width - pad * 2.),
        );
        if today {
            let x = b.left() + pad + w + ink.u(8.);
            ink.text(
                "Today",
                SMALL,
                18.,
                FontWeight::MEDIUM,
                c.muted_text,
                point(x, ty),
                Some(b.right() - pad - x),
            );
        }
        shared.dates.borrow_mut().push((rect, date));
    }
}

#[allow(clippy::too_many_arguments)]
fn paint_agenda_row(
    ink: &mut Ink,
    scene: &Scene,
    shared: &Shared,
    cols: &AgendaColumns,
    row: &AgendaRow,
    place: &AgendaPlace,
    top: Pixels,
    b: Bounds<Pixels>,
) {
    let c = &scene.colors;
    let chip = &scene.laid.chips[row.chip];
    let selected = scene.selected(&chip.id);
    let cc = ChipColors::new(chip.color, c, false, scene.past(chip));
    let pad = cols.bar_x - cols.time_w - ink.u(8.);
    let rect = Bounds::new(
        point(b.left() + pad / 2., top),
        size(b.size.width - pad, px(place.height)),
    );
    // `b` is the area rows show in: hits outside it are under a header.
    if selected {
        ink.rounded(rect, ink.u(6.), mix(c.background, cc.accent, 0.18));
    }
    let y = top + ink.u(AGENDA_ROW_PAD);
    // The time column.
    let tx = b.left() + pad;
    ink.text(
        &row.start,
        SMALL,
        TITLE_LEADING,
        FontWeight::MEDIUM,
        cc.text,
        point(tx, y),
        Some(cols.time_w),
    );
    ink.text(
        &row.end,
        SMALL,
        SMALL_LEADING,
        FontWeight::NORMAL,
        cc.muted_text,
        point(tx, y + ink.u(TITLE_LEADING)),
        Some(cols.time_w),
    );
    // The calendar's colour.
    let bar = Bounds::new(
        point(b.left() + cols.bar_x, y + ink.u(1.)),
        size(
            ink.u(3.),
            px(place.height) - ink.u(AGENDA_ROW_PAD * 2. + 2.),
        ),
    );
    ink.rounded(bar, ink.u(1.5), cc.accent);
    // The title, the place and the links.
    let x = b.left() + cols.text_x;
    let used = ink.wrapped(
        &chip.title,
        TITLE,
        TITLE_LEADING,
        FontWeight::MEDIUM,
        cc.text,
        point(x, y),
        cols.text_w,
        place.title_lines,
    );
    let mut ly = y + ink.u(TITLE_LEADING) * used.max(1) as f32;
    if let Some(location) = &chip.location {
        ink.text(
            location,
            SMALL,
            SMALL_LEADING,
            FontWeight::NORMAL,
            cc.muted_text,
            point(x, ly),
            Some(cols.text_w),
        );
        ly += ink.u(SMALL_LEADING);
    }
    let _ = ly;
    let mut links = Vec::new();
    for (i, (link, &(lx, ly, w))) in chip.links.iter().zip(&place.pills).enumerate() {
        let pill = Bounds::new(point(x + px(lx), y + px(ly)), size(px(w), ink.u(PILL_H)));
        ink.outlined(
            pill,
            ink.u(PILL_H / 2.),
            mix(c.background, cc.accent, 0.12),
            px(1.),
            mix(c.background, cc.accent, 0.35),
        );
        let mid = pill.top() + pill.size.height / 2.;
        paint_glyph(
            ink,
            link.kind.into(),
            pill.left() + ink.u(8.),
            mid,
            cc.muted_text,
        );
        let label_x = pill.left() + ink.u(8. + GLYPH + 5.);
        ink.text(
            &link.label,
            SMALL,
            SMALL_LEADING,
            FontWeight::MEDIUM,
            cc.text,
            point(label_x, mid - ink.u(SMALL_LEADING) / 2.),
            Some(pill.right() - ink.u(8.) - label_x),
        );
        links.push((pill, i));
    }
    push_links(shared, chip, links, &b, Area::Month);
    shared.hits.borrow_mut().push(Hit {
        id: chip.id.clone(),
        bounds: rect.intersect(&b),
        area: Area::Month,
        resize: None,
        read_only: chip.read_only,
    });
}
