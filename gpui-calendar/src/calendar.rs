//! The calendar itself: an entity that holds the host's events, the view and the period on
//! screen, the selection and any drag in progress. It lays events out once per change (not per
//! frame), paints them (`paint.rs`), and turns the mouse and keyboard into proposals it hands
//! the host as [`CalendarEvent`]s. It never changes an event itself: the host applies a move
//! and sets the events again. The host's calendars decide each event's colour, whether it's
//! shown and whether it can be edited.

use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::time::Duration as StdDuration;
// GPUI's clock: std's Instant, but in a browser (which has none) web-time's.
#[cfg(not(target_family = "wasm"))]
use std::time::Instant;
#[cfg(target_family = "wasm")]
use web_time::Instant;

use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_core::{
    Button, ButtonGroup, ButtonVariants as _, DatePickerState, DismissReason, DropdownMenu,
    IconName, MenuSelect, Popover, PopoverAlign, PopupMenuItem, Press, Selectable as _, Sheet,
    Sizable as _, Theme,
};

use crate::find::Query;
use crate::layout::{self, Column};
use crate::model::*;

mod crowd;
pub(crate) mod drag;
mod menu;
mod mini;
mod panel;
use crate::paint::{self, Area, Geo, Ghost, Shared, Target};
use crate::theme::{CalendarColors, palette_color};
use crate::time::{self, DstChange};

actions!(
    calendar,
    [
        /// Moves the selection a day back; with nothing selected, goes to the previous period.
        SelectLeft,
        /// Moves the selection a day on; with nothing selected, goes to the next period.
        SelectRight,
        /// Moves the selection earlier (a slot on the time grid, a week in the month).
        SelectUp,
        /// Moves the selection later.
        SelectDown,
        /// Goes to today.
        Today,
        DayView,
        WeekView,
        MonthView,
        /// Shows the agenda: two weeks of events as a list.
        AgendaView,
        PreviousPeriod,
        NextPeriod,
        /// Proposes a new event at the selected slot, or at the next round half hour today.
        NewEvent,
        /// Opens the selected event, or makes one in the selected slot.
        OpenSelected,
        /// Asks the host to delete the selected event.
        DeleteSelected,
        /// Drops the drag in progress, or else the selection.
        Cancel,
        /// Asks the host to focus its search field ([`CalendarEvent::Find`]).
        Find,
        /// Opens the mini month to jump to a date (where it's behind the title).
        GoToDate,
        /// With the event panel open, asks the host to edit its event ([`CalendarEvent::Edit`]),
        /// as the panel's Edit does. Nothing for a read-only event.
        EditPanelEvent,
        /// Opens the menu of what's selected (an event's Open, Edit and Delete, or a slot's New
        /// event), as a right-click does.
        ShowMenu,
    ]
);

const CONTEXT: &str = "Calendar";

/// The calendar's keys. Call once at startup ([`crate::init`] does). A host remaps any of them
/// by binding the same actions to other keys in the `Calendar` context.
pub fn bind_keys(cx: &mut App) {
    let c = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("left", SelectLeft, c),
        KeyBinding::new("right", SelectRight, c),
        KeyBinding::new("up", SelectUp, c),
        KeyBinding::new("down", SelectDown, c),
        KeyBinding::new("t", Today, c),
        KeyBinding::new("d", DayView, c),
        KeyBinding::new("w", WeekView, c),
        KeyBinding::new("m", MonthView, c),
        KeyBinding::new("a", AgendaView, c),
        KeyBinding::new("k", PreviousPeriod, c),
        KeyBinding::new("p", PreviousPeriod, c),
        KeyBinding::new("j", NextPeriod, c),
        KeyBinding::new("n", NewEvent, c),
        KeyBinding::new("enter", OpenSelected, c),
        KeyBinding::new("backspace", DeleteSelected, c),
        KeyBinding::new("delete", DeleteSelected, c),
        KeyBinding::new("escape", Cancel, c),
        KeyBinding::new("g", GoToDate, c),
        KeyBinding::new("e", EditPanelEvent, c),
        KeyBinding::new("shift-f10", ShowMenu, c),
        KeyBinding::new(
            if cfg!(any(target_os = "macos", target_os = "ios")) {
                "cmd-f"
            } else {
                "ctrl-f"
            },
            Find,
            c,
        ),
    ]);
}

/// What the calendar tells its host. Every change to an event is a proposal: the host decides,
/// and updates the events it gave the calendar if it agrees. Nothing is proposed for an event
/// that's read-only or in a read-only calendar.
#[derive(Clone, Debug, PartialEq)]
pub enum CalendarEvent {
    /// The view or the period on screen changed: supply events for this range (recurring
    /// events expanded).
    RangeChange {
        view: CalendarView,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    },
    /// An event was selected, or the event selection was dropped.
    Select(Option<EventId>),
    /// Double-click or Enter on an event.
    Open(EventId),
    /// Someone dragged out empty time, double-clicked a slot, or pressed Enter on one. The
    /// calendar is [`CalendarOptions::default_calendar`] or the first that's shown and can be
    /// edited; none when the host gave no calendars.
    Create {
        calendar: Option<CalendarId>,
        time: EventTime,
    },
    /// An event was dragged somewhere else. For an occurrence of a recurring event, `scope`
    /// is the person's answer to "this event, this and following, or all?"; otherwise
    /// [`EditScope::This`].
    Move {
        event: EventRef,
        time: EventTime,
        scope: EditScope,
    },
    /// An event's end was dragged.
    Resize {
        event: EventRef,
        time: EventTime,
        scope: EditScope,
    },
    /// Delete or Backspace was pressed with this event selected.
    Delete { event: EventRef, scope: EditScope },
    /// The event panel's Edit: the host opens its own editor.
    Edit(EventRef),
    /// The event panel moved the event to another calendar.
    ChangeCalendar {
        event: EventRef,
        calendar: CalendarId,
        scope: EditScope,
    },
    /// The event panel answered the invitation for the attendee marked `me`.
    Rsvp {
        event: EventRef,
        response: Response,
        scope: EditScope,
    },
    /// One of an event's links was clicked: the host opens what it points at.
    OpenLink { event: EventRef, link: EventLink },
    /// A web address in the event panel's place or notes was clicked: the host opens it (in
    /// the browser, say). The calendar never opens anything itself.
    OpenUrl { event: EventRef, url: SharedString },
    /// ⌘F (Ctrl-F off the Mac): focus the host's search field, and call
    /// [`Calendar::set_query`] as it changes. The calendar takes no typing itself unless the
    /// host asks it to with [`Calendar::type_query`].
    Find,
    /// The query changed while the calendar was taking typing ([`Calendar::type_query`]).
    QueryChange(SharedString),
}

/// The event panel: which event, where it hangs from, and whether its calendar list is open.
#[derive(Clone, Debug)]
pub(crate) struct Panel {
    id: EventId,
    anchor: Bounds<Pixels>,
    picking_calendar: bool,
}

/// Which edit is waiting for the recurring-event question's answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum EditKind {
    Move,
    Resize,
    Delete,
    ChangeCalendar(CalendarId),
    Rsvp(Response),
}

/// An edit to an occurrence of a recurring event, held until the person says which
/// occurrences it's for. Its ghost stays where the drag left it.
#[derive(Clone, Debug)]
pub(crate) struct Asking {
    kind: EditKind,
    event: EventRef,
    time: Option<EventTime>,
    ghost: Option<Ghost>,
    /// Where the question hangs from: where the finger or pointer let go, or the event.
    anchor: Bounds<Pixels>,
    /// The answer the arrow keys are on.
    highlighted: usize,
}

/// A drag in progress, or a press that might become one.
#[derive(Clone, Debug)]
pub(crate) enum Drag {
    Press {
        /// Where the press went down, which a drag must move a few pixels from to start.
        at: Point<Pixels>,
        target: Target,
        /// Where the event is held from, when that isn't the press (a row in the "+N" list
        /// holds its event by its start, on the grid behind the list).
        held: Option<Point<Pixels>>,
    },
    Create {
        date: NaiveDate,
        from: f32,
        to: f32,
    },
    CreateDays {
        area: Area,
        from: NaiveDate,
        to: NaiveDate,
    },
    Move {
        id: EventId,
        original: EventTime,
        /// Where it was grabbed.
        area: Area,
        /// Where it is now: on the week's grid, the all-day row or the hours, either way.
        over: Area,
        grab: (NaiveDate, f32),
        now: (NaiveDate, f32),
    },
    Resize {
        id: EventId,
        original: EventTime,
        area: Area,
        now: (NaiveDate, f32),
        /// How many minutes below the event's end it was grabbed (a finger takes the handle
        /// from beside it), kept so the end doesn't jump to the finger.
        shift: f32,
    },
}

/// What an event chip says, worked out when the layout is.
#[derive(Clone, Debug)]
pub(crate) struct Chip {
    pub id: EventId,
    pub title: SharedString,
    /// "9 – 10:30 AM" on the grid, "9:30a" in the month.
    pub time: SharedString,
    pub location: Option<SharedString>,
    pub links: Rc<[EventLink]>,
    pub color: u32,
    pub read_only: bool,
    /// Only all-day events change length from a span's right edge: a timed one's end has a
    /// time a day can't express.
    pub all_day: bool,
    pub end: DateTime<Utc>,
    /// Whether the find query (if any) finds it.
    pub found: bool,
    /// The user's answer to it, where it's an invitation they didn't send (an attendee marked
    /// `me` who isn't its organizer).
    pub answer: Option<Response>,
}

/// A timed event's piece on one day of the grid.
#[derive(Clone, Debug)]
pub(crate) struct TimedPiece {
    pub chip: usize,
    pub col: u32,
    /// Wall-clock minutes on that day.
    pub top: f32,
    pub bottom: f32,
    pub place: Column,
    /// Whether the event ends on this piece (and so can be resized from it).
    pub ends: bool,
}

/// An event drawn across days: in the grid's all-day row, or on a week of the month (where a
/// timed event within one day is a `dot` row).
#[derive(Clone, Debug)]
pub(crate) struct SpanPiece {
    pub chip: usize,
    /// The week row in the month view; 0 on the grid.
    pub row: u32,
    pub first: i32,
    pub last: i32,
    pub lane: usize,
    pub cont_left: bool,
    pub cont_right: bool,
    pub dot: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct LayoutKey {
    version: u64,
    view: CalendarView,
    first: NaiveDate,
    days: u32,
    tz: chrono_tz::Tz,
    hour12: bool,
    min_minutes: u32,
    query: Option<Rc<Query>>,
}

/// The events on screen, laid out. Rebuilt when the events, the period or the options change.
#[derive(Debug)]
pub(crate) struct Laid {
    key: LayoutKey,
    pub chips: Vec<Chip>,
    pub timed: Vec<TimedPiece>,
    pub spans: Vec<SpanPiece>,
    pub dst: Vec<(u32, DstChange)>,
    /// The agenda's days, in order: only days with events, and today.
    pub agenda: Vec<AgendaDay>,
}

/// A day in the agenda and its events, all-day ones first and then by start.
#[derive(Clone, Debug)]
pub(crate) struct AgendaDay {
    pub date: NaiveDate,
    pub rows: Vec<AgendaRow>,
}

/// An event's row on one day of the agenda, with what its time column says.
#[derive(Clone, Debug)]
pub(crate) struct AgendaRow {
    pub chip: usize,
    /// "9:30 AM", "All day" or "Until".
    pub start: SharedString,
    /// "10:30 AM", "Day 2 of 3", or nothing.
    pub end: SharedString,
    /// When it starts, for a timed event that starts on the row's day; none for one that's
    /// all day or began on an earlier day. Where the line at the current time goes.
    pub at: Option<DateTime<Utc>>,
}

/// Where a touch pan stands, as far as swiping goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Swiping {
    /// No pan, or one that isn't a swipe.
    No,
    /// A pan that hasn't moved yet.
    Undecided,
    /// A sideways pan under the finger.
    Yes,
}

/// A swipe easing to rest: from one offset to another over [`SETTLE`], timed by GPUI's clock and
/// drawn a frame at a time; then, if it turned the period, the new one easing in.
#[derive(Clone, Copy, Debug)]
struct Settle {
    from: f32,
    to: f32,
    start: Instant,
    /// The period it turns to (1 next, -1 previous) once it's off the side.
    turn: Option<i32>,
    width: f32,
}

/// How long a swipe takes to ease out of the way, and the next period in.
const SETTLE: StdDuration = StdDuration::from_millis(200);

/// The speed at which a swipe is a flick, in points per second, and the stretch of the pan
/// it's measured over.
const FLICK_SPEED: f32 = 500.;
const FLICK_WINDOW: StdDuration = StdDuration::from_millis(80);

/// The time now, by GPUI's clock: the wall clock when the calendar was made, plus the time
/// GPUI has counted since (so tests that move GPUI's clock move the calendar's), never behind
/// the wall clock (which keeps going while a Mac sleeps and GPUI's clock doesn't).
#[derive(Clone)]
struct Clock {
    executor: BackgroundExecutor,
    utc: DateTime<Utc>,
    at: Instant,
}

impl Clock {
    fn now(&self) -> DateTime<Utc> {
        let counted = self.executor.now().duration_since(self.at);
        let since = self.utc + Duration::from_std(counted).unwrap_or_default();
        since.max(Utc::now())
    }
}

/// What a button in the narrow header does.
type HeaderAction = Rc<dyn Fn(&mut Calendar, &mut Context<Calendar>)>;

/// A calendar: day, week, N-day and month views of the host's events.
pub struct Calendar {
    calendars: Vec<CalendarSource>,
    calendar_index: HashMap<CalendarId, usize>,
    events: Vec<Event>,
    /// Timed events by start, all-day ones by start date: what the culling searches.
    timed_order: Vec<usize>,
    all_day_order: Vec<usize>,
    longest_timed: Duration,
    longest_all_day: Duration,
    by_id: HashMap<EventId, usize>,
    version: u64,

    view: CalendarView,
    anchor: NaiveDate,
    pub options: CalendarOptions,
    colors: Option<CalendarColors>,
    header: bool,
    pub(crate) all_day_expanded: bool,

    selection: Option<Selection>,
    pub(crate) drag: Option<Drag>,
    hover_cursor: CursorStyle,
    now_override: Option<DateTime<Utc>>,
    laid: Option<Rc<Laid>>,
    last_range: Option<(DateTime<Utc>, DateTime<Utc>)>,

    focus: FocusHandle,
    /// What the canvas tells assistive technology, frame by frame.
    pub(crate) a11y: Rc<crate::a11y::A11yFrame>,
    pub(crate) shared: Rc<Shared>,
    _tick: Option<Task<()>>,
    clock: Clock,

    /// Where a touch that's dragging something landed, and whether it has moved off it yet.
    touch_from: Option<Point<Pixels>>,
    touch_moved: bool,
    /// A sideways swipe: how far it has carried the body, whether one is under the finger,
    /// where it was when (for its speed), and the easing that settles it.
    pub(crate) swipe: f32,
    swiping: Swiping,
    swipe_track: VecDeque<(Instant, f32)>,
    settle: Option<Settle>,
    /// The period beside this one, laid out while a swipe shows it.
    neighbour: Option<Rc<Laid>>,
    /// An edit to a recurring event waiting for "this, following or all".
    asking: Option<Asking>,
    /// The event whose details the panel shows, and where it hangs from.
    panel: Option<Panel>,
    /// The list of side-by-side events that didn't fit, open from their "+N".
    crowd: Option<crowd::Crowd>,
    /// The menu a right-click or Shift-F10 opened.
    menu: Option<menu::Menu>,
    /// Where the pointer or finger last was in a drag.
    last_point: Point<Pixels>,
    /// What the events are filtered by, if anything.
    query: Option<Rc<Query>>,
    /// The mini month: its state (made on first use), the anchor it was last shown, its week
    /// start, whether it's open behind the title, and the subscription to its picks.
    pub(crate) mini: Option<Entity<DatePickerState>>,
    mini_shown: Option<(NaiveDate, chrono::Weekday)>,
    mini_open: bool,
    /// That a press outside the open mini month closed it, for the title the press may be on:
    /// it's that press's, so it doesn't open it again. Cleared each frame.
    mini_dismissed: bool,
    /// Whether the mini month was beside the calendar in the last frame.
    mini_side: bool,
    _mini_sub: Option<Subscription>,
    /// Typing goes into the query: the host asked for it with `type_query`.
    typing: bool,
}

impl EventEmitter<CalendarEvent> for Calendar {}

impl Focusable for Calendar {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Calendar {
    /// An empty calendar on today's week.
    pub fn new(cx: &mut Context<Self>) -> Self {
        let options = CalendarOptions::default();
        let anchor = Utc::now().with_timezone(&options.time_zone).date_naive();
        let clock = Clock {
            executor: cx.background_executor().clone(),
            utc: Utc::now(),
            at: cx.background_executor().now(),
        };
        // The line at the current time moves once a minute, on the minute; nothing else draws
        // on its own.
        let tick = cx.spawn({
            let clock = clock.clone();
            async move |this, cx| {
                loop {
                    let now = clock.now();
                    let into = now.timestamp_subsec_millis() as u64
                        + (now.timestamp().rem_euclid(60) as u64) * 1000;
                    cx.background_executor()
                        .timer(StdDuration::from_millis(60_000 - into))
                        .await;
                    if this.update(cx, |_, cx| cx.notify()).is_err() {
                        break;
                    }
                }
            }
        });
        Self {
            calendars: Vec::new(),
            calendar_index: HashMap::new(),
            events: Vec::new(),
            timed_order: Vec::new(),
            all_day_order: Vec::new(),
            longest_timed: Duration::zero(),
            longest_all_day: Duration::zero(),
            by_id: HashMap::new(),
            version: 0,
            view: CalendarView::Week,
            anchor,
            options,
            colors: None,
            header: true,
            all_day_expanded: false,
            selection: None,
            drag: None,
            hover_cursor: CursorStyle::Arrow,
            now_override: None,
            laid: None,
            last_range: None,
            // Tab reaches the header's controls, the mini month, then the calendar (its grid,
            // its month, its list), then an open panel.
            focus: cx.focus_handle().tab_stop(true).tab_index(2),
            a11y: Rc::default(),
            shared: Rc::default(),
            _tick: Some(tick),
            clock,
            touch_from: None,
            touch_moved: false,
            swipe: 0.,
            swiping: Swiping::No,
            swipe_track: VecDeque::new(),
            settle: None,
            neighbour: None,
            asking: None,
            panel: None,
            crowd: None,
            menu: None,
            last_point: Point::default(),
            query: None,
            mini: None,
            mini_shown: None,
            mini_open: false,
            mini_dismissed: false,
            mini_side: false,
            _mini_sub: None,
            typing: false,
        }
    }

    pub fn with_events(mut self, events: Vec<Event>) -> Self {
        self.load(events);
        self
    }

    pub fn with_view(mut self, view: CalendarView) -> Self {
        self.view = view;
        self
    }

    /// Shows the period around this date.
    pub fn with_date(mut self, date: NaiveDate) -> Self {
        self.anchor = date;
        self
    }

    pub fn with_options(mut self, f: impl FnOnce(&mut CalendarOptions)) -> Self {
        let tz = self.options.time_zone;
        f(&mut self.options);
        if self.options.time_zone != tz {
            self.anchor = self.today();
        }
        self
    }

    /// The host's calendars: their colours, which are shown and which can be edited.
    pub fn with_calendars(mut self, calendars: Vec<CalendarSource>) -> Self {
        self.load_calendars(calendars);
        self
    }

    /// The header with the period's name, Today, previous and next, and the views (on by
    /// default). Without it the host draws its own and calls [`Calendar::go_to`] and the rest.
    pub fn with_header(mut self, header: bool) -> Self {
        self.header = header;
        self
    }

    /// Colours of its own, in place of the theme's.
    pub fn with_colors(mut self, colors: CalendarColors) -> Self {
        self.colors = Some(colors);
        self
    }

    /// Pretends it's this moment, for pictures and tests. The line at the current time, today's
    /// mark and the dimming of past events follow it.
    pub fn with_now(mut self, now: DateTime<Utc>) -> Self {
        self.set_now(Some(now));
        self
    }

    pub fn set_now(&mut self, now: Option<DateTime<Utc>>) {
        self.now_override = now;
        if now.is_some() {
            self._tick = None;
        }
    }

    pub fn now(&self) -> DateTime<Utc> {
        self.now_override.unwrap_or_else(|| self.clock.now())
    }

    /// Today in the display zone.
    pub fn today(&self) -> NaiveDate {
        self.now()
            .with_timezone(&self.options.time_zone)
            .date_naive()
    }

    // The calendars.

    fn load_calendars(&mut self, calendars: Vec<CalendarSource>) {
        self.calendar_index = calendars
            .iter()
            .enumerate()
            .map(|(i, c)| (c.id.clone(), i))
            .collect();
        self.calendars = calendars;
        // Which events show, and their colours, follow the calendars.
        let events = std::mem::take(&mut self.events);
        self.load(events);
    }

    /// Replaces the calendars: say a calendar was hidden, recoloured or made read-only.
    pub fn set_calendars(&mut self, calendars: Vec<CalendarSource>, cx: &mut Context<Self>) {
        self.load_calendars(calendars);
        cx.notify();
    }

    pub fn calendars(&self) -> &[CalendarSource] {
        &self.calendars
    }

    pub fn calendar(&self, id: &str) -> Option<&CalendarSource> {
        self.calendar_index.get(id).map(|&i| &self.calendars[i])
    }

    /// Whether an event's calendar is shown. An event whose calendar isn't in the list is.
    pub fn is_shown(&self, event: &Event) -> bool {
        self.calendar(&event.calendar).is_none_or(|c| c.visible)
    }

    /// Whether an event can't be changed: it's read-only, or its calendar is.
    pub fn is_read_only(&self, event: &Event) -> bool {
        event.read_only || self.calendar(&event.calendar).is_some_and(|c| c.read_only)
    }

    /// The colour an event is drawn in, as 0xRRGGBB: its own, its calendar's, or a palette
    /// colour picked by its calendar id when the host gave that calendar none.
    pub fn color_of(&self, event: &Event) -> u32 {
        event.color.unwrap_or_else(|| {
            self.calendar(&event.calendar)
                .map(|c| c.color)
                .unwrap_or_else(|| palette_color(&event.calendar))
        })
    }

    /// The calendar a new event goes in: `Some(None)` with no calendars given, `None` when
    /// every calendar is read-only (and so nothing can be made).
    fn new_event_calendar(&self) -> Option<Option<CalendarId>> {
        if self.calendars.is_empty() {
            return Some(None);
        }
        // Shown as well as editable: an event made in a hidden calendar would vanish as soon
        // as the host added it.
        let usable = |c: &&CalendarSource| c.visible && !c.read_only;
        self.options
            .default_calendar
            .as_ref()
            .and_then(|id| self.calendar(id))
            .filter(usable)
            .or_else(|| self.calendars.iter().find(usable))
            .map(|c| Some(c.id.clone()))
    }

    fn propose_create(&self, time: EventTime, cx: &mut Context<Self>) {
        if let Some(calendar) = self.new_event_calendar() {
            cx.emit(CalendarEvent::Create { calendar, time });
        }
    }

    /// An event the host could change: shown, and neither it nor its calendar read-only.
    fn editable(&self, id: &str) -> Option<&Event> {
        self.event(id)
            .filter(|e| self.is_shown(e) && !self.is_read_only(e))
    }

    // Find.

    /// Filters by text: the events whose title, place, notes or link labels contain every word of
    /// `query`, ignoring case and accents. The time grid dims the others and marks what it
    /// found; the month and the agenda show only what it found. An empty query shows all.
    pub fn set_query(&mut self, query: &str, cx: &mut Context<Self>) {
        let next = Query::new(query).map(Rc::new);
        if next != self.query {
            self.query = next;
            self.laid = None;
            self.neighbour = None;
            cx.notify();
        }
    }

    /// The query, as given; empty without one.
    pub fn query(&self) -> &str {
        self.query.as_ref().map_or("", |q| q.text.as_str())
    }

    /// Whether the query finds an event: always, without a query.
    pub fn is_found(&self, event: &Event) -> bool {
        self.query.as_ref().is_none_or(|q| q.matches(event))
    }

    /// The shown events in the period on screen that the query finds.
    pub fn found(&self) -> Vec<&Event> {
        let (start, end) = self.visible_range();
        let mut out = self.events_in(start, end);
        out.retain(|e| self.is_found(e));
        out
    }

    /// Takes typing into the query until Enter or Escape (Escape clears it), sending
    /// [`CalendarEvent::QueryChange`] as it changes. Only for a host with no search field of
    /// its own: by default the calendar takes no typing.
    pub fn type_query(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.typing = true;
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub fn is_typing_query(&self) -> bool {
        self.typing
    }

    fn query_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        if !self.typing {
            return;
        }
        let k = &event.keystroke;
        let mut text = self.query().to_string();
        match k.key.as_str() {
            "escape" => {
                self.typing = false;
                text.clear();
            }
            "enter" => self.typing = false,
            "backspace" => {
                text.pop();
            }
            _ if k.modifiers.control || k.modifiers.platform || k.modifiers.function => return,
            _ => match &k.key_char {
                Some(c) if !c.chars().any(char::is_control) => text.push_str(c),
                _ => return,
            },
        }
        cx.stop_propagation();
        if text != self.query() {
            self.set_query(&text, cx);
            cx.emit(CalendarEvent::QueryChange(text.into()));
        }
        cx.notify();
    }

    /// While the calendar takes typing, a pill at the bottom of the body says what it's
    /// finding.
    fn render_query(&self, colors: CalendarColors, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.typing {
            return None;
        }
        let theme = Theme::global(cx);
        let found = self.found().len();
        let count = match (self.query.is_some(), found) {
            (false, _) => String::new(),
            (true, 1) => "1 event".into(),
            (true, n) => format!("{n} events"),
        };
        let pill = div()
            .min_w_0()
            .max_w(rems(320. / 15.))
            .flex()
            .items_center()
            .gap(rems(8. / 15.))
            .px(rems(10. / 15.))
            .h(rems(30. / 15.))
            .rounded_full()
            .bg(theme.colors.popover)
            .border_1()
            .border_color(colors.find)
            .shadow_md()
            .text_size(rems(13. / 15.))
            .debug_selector(|| "calendar-query".into())
            .child(
                div()
                    .flex_none()
                    .text_color(colors.muted_text)
                    .child("Find"),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(colors.text)
                    .child(format!("{}▏", self.query())),
            )
            .when(!count.is_empty(), |d| {
                d.child(div().flex_none().text_color(colors.muted_text).child(count))
            });
        // At the bottom right of the body, never wider than it.
        Some(
            div()
                .absolute()
                .bottom(rems(12. / 15.))
                .left(rems(12. / 15.))
                .right(rems(12. / 15.))
                .flex()
                .justify_end()
                .child(pill)
                .into_any_element(),
        )
    }

    // The events.

    fn load(&mut self, events: Vec<Event>) {
        self.events = events;
        self.version += 1;
        if let Some(p) = &self.panel
            && !self.events.iter().any(|e| e.id == p.id)
        {
            self.panel = None;
        }
        self.by_id = self
            .events
            .iter()
            .enumerate()
            .map(|(i, e)| (e.id.clone(), i))
            .collect();
        let (mut timed, mut all_day) = (Vec::new(), Vec::new());
        self.longest_timed = Duration::zero();
        self.longest_all_day = Duration::zero();
        for (i, e) in self.events.iter().enumerate() {
            if !self.is_shown(e) {
                continue;
            }
            match e.time {
                EventTime::Timed { start, end } => {
                    self.longest_timed = self.longest_timed.max(end - start);
                    timed.push(i);
                }
                EventTime::AllDay { start, end } => {
                    self.longest_all_day = self.longest_all_day.max(end - start);
                    all_day.push(i);
                }
            }
        }
        let start_of = |i: &usize| match self.events[*i].time {
            EventTime::Timed { start, .. } => start.timestamp(),
            EventTime::AllDay { start, .. } => start.num_days_from_ce() as i64,
        };
        timed.sort_by_key(start_of);
        all_day.sort_by_key(start_of);
        self.timed_order = timed;
        self.all_day_order = all_day;
        self.laid = None;
        if let Some(Selection::Event(id)) = &self.selection
            && !self.event(id).is_some_and(|e| self.is_shown(e))
        {
            self.selection = None;
        }
    }

    /// Replaces the events: the host's answer to a [`CalendarEvent::RangeChange`], or to a
    /// proposal it agreed to.
    pub fn set_events(&mut self, events: Vec<Event>, cx: &mut Context<Self>) {
        self.load(events);
        cx.notify();
    }

    /// Adds an event, or replaces the one with its id.
    pub fn upsert_event(&mut self, event: Event, cx: &mut Context<Self>) {
        let mut events = std::mem::take(&mut self.events);
        match self.by_id.get(&event.id) {
            Some(&i) => events[i] = event,
            None => events.push(event),
        }
        self.set_events(events, cx);
    }

    pub fn remove_event(&mut self, id: &str, cx: &mut Context<Self>) {
        let mut events = std::mem::take(&mut self.events);
        events.retain(|e| e.id.as_ref() != id);
        self.set_events(events, cx);
    }

    /// Every event the host gave, its hidden calendars' included.
    pub fn events(&self) -> &[Event] {
        &self.events
    }

    pub fn event(&self, id: &str) -> Option<&Event> {
        self.by_id.get(id).map(|&i| &self.events[i])
    }

    /// The shown events that touch an instant range, culled with a binary search.
    pub fn events_in(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Vec<&Event> {
        let tz = self.options.time_zone;
        let mut out = Vec::new();
        let from = start - self.longest_timed;
        let at = self
            .timed_order
            .partition_point(|&i| match self.events[i].time {
                EventTime::Timed { start, .. } => start < from,
                _ => true,
            });
        for &i in &self.timed_order[at..] {
            let EventTime::Timed { start: s, end: e } = self.events[i].time else {
                continue;
            };
            if s >= end {
                break;
            }
            if e > start || (s == e && s >= start) {
                out.push(&self.events[i]);
            }
        }
        // All-day dates are the same everywhere, so compare them to the dates the range covers
        // in the display zone.
        let first = start.with_timezone(&tz).date_naive();
        let last = (end - Duration::nanoseconds(1))
            .with_timezone(&tz)
            .date_naive();
        let from = first - self.longest_all_day;
        let at = self
            .all_day_order
            .partition_point(|&i| match self.events[i].time {
                EventTime::AllDay { start, .. } => start < from,
                _ => true,
            });
        for &i in &self.all_day_order[at..] {
            let EventTime::AllDay { start: s, end: e } = self.events[i].time else {
                continue;
            };
            if s > last {
                break;
            }
            if e > first {
                out.push(&self.events[i]);
            }
        }
        out
    }

    // The view and the period.

    pub fn view(&self) -> CalendarView {
        self.view
    }

    pub fn date(&self) -> NaiveDate {
        self.anchor
    }

    /// The first date on screen and how many there are.
    pub fn visible_days(&self) -> (NaiveDate, u32) {
        time::visible_days(self.view, self.anchor, self.options.week_start)
    }

    /// The instants on screen: what the host supplies events for.
    pub fn visible_range(&self) -> (DateTime<Utc>, DateTime<Utc>) {
        time::visible_range(
            self.view,
            self.anchor,
            self.options.week_start,
            self.options.time_zone,
        )
    }

    /// What the header calls the period on screen.
    pub fn title(&self) -> String {
        time::title(self.view, self.anchor, self.options.week_start)
    }

    /// A shorter name for the period, as the narrow header shows it.
    pub fn short_title(&self) -> String {
        time::short_title(self.view, self.anchor, self.options.week_start)
    }

    pub fn set_view(&mut self, view: CalendarView, cx: &mut Context<Self>) {
        self.view = view;
        self.range_changed(cx);
    }

    /// Shows the period around `date`.
    pub fn go_to(&mut self, date: NaiveDate, cx: &mut Context<Self>) {
        self.anchor = date;
        self.range_changed(cx);
    }

    pub fn go_to_today(&mut self, cx: &mut Context<Self>) {
        self.go_to(self.today(), cx);
        self.scroll_to_now();
    }

    pub fn previous(&mut self, cx: &mut Context<Self>) {
        self.go_to(time::step(self.view, self.anchor, -1), cx);
    }

    pub fn next(&mut self, cx: &mut Context<Self>) {
        self.go_to(time::step(self.view, self.anchor, 1), cx);
    }

    pub fn set_options(&mut self, f: impl FnOnce(&mut CalendarOptions), cx: &mut Context<Self>) {
        f(&mut self.options);
        self.laid = None;
        self.range_changed(cx);
    }

    fn range_changed(&mut self, cx: &mut Context<Self>) {
        self.announce_range(cx);
        cx.notify();
    }

    fn announce_range(&mut self, cx: &mut Context<Self>) {
        let range = self.visible_range();
        if self.last_range != Some(range) {
            self.last_range = Some(range);
            cx.emit(CalendarEvent::RangeChange {
                view: self.view,
                start: range.0,
                end: range.1,
            });
            self.all_day_expanded = false;
            // A new agenda opens at today, or at its top.
            self.shared.agenda_scroll.set(None);
        }
    }

    /// Scrolls the time grid so this wall-clock time is at the top.
    pub fn scroll_to_minute(&mut self, minute: f32) {
        self.shared.scroll.set(Some(minute));
    }

    /// The minute at the top of the time grid.
    pub fn scroll_minute(&self) -> f32 {
        self.shared.scroll.get().unwrap_or(0.)
    }

    fn scroll_to_now(&mut self) {
        // Done at the next layout, once the grid's height is known.
        self.shared.scroll.set(None);
    }

    // The selection.

    pub fn selection(&self) -> Option<&Selection> {
        self.selection.as_ref()
    }

    pub fn selected_event(&self) -> Option<&EventId> {
        match &self.selection {
            Some(Selection::Event(id)) => Some(id),
            _ => None,
        }
    }

    pub fn select(&mut self, selection: Option<Selection>, cx: &mut Context<Self>) {
        let before = self.selected_event().cloned();
        self.selection = selection;
        let after = self.selected_event().cloned();
        if before != after {
            cx.emit(CalendarEvent::Select(after));
        }
        cx.notify();
    }

    /// What a drag in progress would do if it were let go now: the event it moves (none for a
    /// new one) and where.
    pub fn drag_proposal(&self) -> Option<(Option<EventId>, EventTime)> {
        let tz = self.options.time_zone;
        let step = self.options.snap();
        match self.drag.as_ref()? {
            Drag::Press { .. } => None,
            Drag::Create { date, from, to } => {
                Some((None, time::created(*date, *from, *to, step, tz)))
            }
            Drag::CreateDays { from, to, .. } => Some((None, time::created_days(*from, *to))),
            Drag::Move {
                id,
                original,
                area,
                over,
                grab,
                now,
            } => Some((
                Some(id.clone()),
                match (*over, original) {
                    (over, _) if over == *area => {
                        time::moved(*original, *grab, *now, *area != Area::Grid, step, tz)
                    }
                    // Into the all-day row: that whole day.
                    (Area::AllDay, _) => EventTime::all_day(now.0, now.0 + Duration::days(1)),
                    // Into the hours: starting where it's let go, an all-day event as long
                    // as a new one, a timed one as long as it was.
                    (Area::Grid, time) => {
                        let start = time::at_minutes(now.0, time::snap(now.1, step), tz);
                        let length = match time {
                            EventTime::Timed { start, end } => *end - *start,
                            EventTime::AllDay { .. } => self.options.default_duration,
                        };
                        EventTime::timed(start, start + length)
                    }
                    _ => time::moved(*original, *grab, *now, *area != Area::Grid, step, tz),
                },
            )),
            Drag::Resize {
                id,
                original,
                now,
                shift,
                ..
            } => {
                let to = (now.0, now.1 - shift);
                Some((Some(id.clone()), time::resized(*original, to, step, tz)))
            }
        }
    }

    pub fn is_dragging(&self) -> bool {
        matches!(&self.drag, Some(d) if !matches!(d, Drag::Press { .. }))
    }

    // Where things are on screen, from the last frame.

    /// The window point at a wall-clock minute on a date on the time grid, or the middle of the
    /// date's cell in the month view.
    pub fn point_at(&self, date: NaiveDate, minute: f32) -> Option<Point<Pixels>> {
        self.shared.geo.get()?.point_at(date, minute)
    }

    /// Where an event's chip was drawn in the last frame (its first piece), for a host's
    /// popover.
    pub fn event_bounds(&self, id: &str) -> Option<Bounds<Pixels>> {
        self.shared
            .hits
            .borrow()
            .iter()
            .find(|h| h.id.as_ref() == id)
            .map(|h| h.bounds)
    }

    /// Where one of an event's links was drawn in the last frame: its row when the chip lists
    /// its links, or the chip's badge when that one link is all it has. None when it's only
    /// counted.
    pub fn link_bounds(&self, id: &str, index: usize) -> Option<Bounds<Pixels>> {
        self.shared
            .links
            .borrow()
            .iter()
            .find(|l| l.id.as_ref() == id && l.index == index)
            .map(|l| l.bounds)
    }

    // Laying out.

    /// The shortest an event is drawn on the grid, in minutes: tall enough for a line of text.
    fn min_minutes(&self) -> u32 {
        (paint::MIN_CHIP_HEIGHT / self.options.hour_height.max(8.) * 60.).ceil() as u32
    }

    fn chip(&self, e: &Event, month: bool) -> Chip {
        let tz = self.options.time_zone;
        let hour12 = self.options.hour12;
        let (time, end) = match e.time {
            EventTime::Timed { start, end } => {
                let s = start.with_timezone(&tz).time();
                let t = if month {
                    time::format_time_short(s, hour12)
                } else {
                    time::format_range(s, end.with_timezone(&tz).time(), hour12)
                };
                (t, end)
            }
            EventTime::AllDay { end, .. } => (String::new(), time::day_start(end, tz)),
        };
        Chip {
            id: e.id.clone(),
            title: if e.title.is_empty() {
                "(No title)".into()
            } else {
                e.title.clone()
            },
            time: time.into(),
            location: e.location.clone(),
            links: e.links.as_slice().into(),
            color: self.color_of(e),
            read_only: self.is_read_only(e),
            all_day: e.time.is_all_day(),
            found: self.is_found(e),
            end,
            answer: e.my_answer(),
        }
    }

    /// The period a swipe is bringing in, laid out, so it can be drawn beside the one leaving:
    /// the next one when the view is pushed left, the previous one when pushed right. Kept
    /// while the swipe lasts; none for the agenda, which scrolls its own way.
    fn swipe_neighbour(&mut self) -> Option<paint::Neighbour> {
        if self.swipe == 0. || matches!(self.view, CalendarView::Agenda(_)) {
            self.neighbour = None;
            return None;
        }
        let dir = if self.swipe < 0. { 1 } else { -1 };
        let anchor = time::step(self.view, self.anchor, dir);
        let (first, days) = time::visible_days(self.view, anchor, self.options.week_start);
        let key = LayoutKey {
            version: self.version,
            view: self.view,
            first,
            days,
            tz: self.options.time_zone,
            hour12: self.options.hour12,
            min_minutes: self.min_minutes(),
            query: self.query.clone(),
        };
        let laid = match &self.neighbour {
            Some(laid) if laid.key == key => laid.clone(),
            _ => {
                let laid = Rc::new(self.build_layout(key));
                self.neighbour = Some(laid.clone());
                laid
            }
        };
        Some(paint::Neighbour {
            dir,
            first,
            days,
            month: anchor.month(),
            laid,
        })
    }

    pub(crate) fn layout(&mut self) -> Rc<Laid> {
        let (first, days) = self.visible_days();
        let key = LayoutKey {
            version: self.version,
            view: self.view,
            first,
            days,
            tz: self.options.time_zone,
            hour12: self.options.hour12,
            min_minutes: self.min_minutes(),
            query: self.query.clone(),
        };
        if let Some(laid) = &self.laid
            && laid.key == key
        {
            return laid.clone();
        }
        let laid = Rc::new(self.build_layout(key));
        self.laid = Some(laid.clone());
        laid
    }

    fn build_layout(&self, key: LayoutKey) -> Laid {
        let tz = key.tz;
        let (first, days) = (key.first, key.days);
        let last = first + Duration::days(days as i64 - 1);
        let (start, end) = (
            time::day_start(first, tz),
            time::day_start(last + Duration::days(1), tz),
        );
        let month = key.view == CalendarView::Month;
        let agenda = matches!(key.view, CalendarView::Agenda(_));
        let mut visible = self.events_in(start, end);
        // The month and the agenda show only what the query finds; the grid dims the rest.
        if (month || agenda) && key.query.is_some() {
            visible.retain(|e| self.is_found(e));
        }
        // Each agenda day's rows, with a key to sort them by: all-day first, then by start.
        let mut agenda_rows: Vec<Vec<(AgendaRow, (u8, i64))>> = if agenda {
            vec![Vec::new(); days as usize]
        } else {
            Vec::new()
        };
        let mut chips = Vec::with_capacity(visible.len());
        let mut timed = Vec::new();
        let mut spans: Vec<(SpanPiece, (u8, i64, i64))> = Vec::new();
        let col_of = |d: NaiveDate| (d - first).num_days() as i32;
        let min = key.min_minutes as f32;

        for e in visible {
            let chip = chips.len();
            chips.push(self.chip(e, month));
            let (d0, d1) = time::event_days(&e.time, tz);
            let (c0, c1) = (col_of(d0), col_of(d1));
            let order_start = match e.time {
                EventTime::Timed { start, .. } => start.timestamp(),
                EventTime::AllDay { start, .. } => time::day_start(start, tz).timestamp(),
            };
            if agenda {
                for c in c0.max(0)..=c1.min(days as i32 - 1) {
                    let date = first + Duration::days(c as i64);
                    let row = self.agenda_row(e, chip, date, (d0, d1));
                    let all_day = (row.start.as_ref() == "All day") as u8;
                    agenda_rows[c as usize].push((row, (1 - all_day, order_start)));
                }
                continue;
            }
            let push_span = |spans: &mut Vec<_>, row: u32, lo: i32, hi: i32, dot: bool| {
                let (a, b) = (c0.max(lo), c1.min(hi));
                if a > b {
                    return;
                }
                let piece = SpanPiece {
                    chip,
                    row,
                    first: a - lo,
                    last: b - lo,
                    lane: 0,
                    cont_left: c0 < lo,
                    cont_right: c1 > hi,
                    dot,
                };
                // Spans across days before single days, longer first, then by start.
                let sort = (dot as u8, -((c1 - c0) as i64), order_start);
                spans.push((piece, sort));
            };
            if month {
                let dot = !e.time.is_all_day() && d0 == d1;
                for row in 0..days / 7 {
                    let lo = row as i32 * 7;
                    push_span(&mut spans, row, lo, lo + 6, dot);
                }
            } else if time::in_all_day_row(&e.time) {
                push_span(&mut spans, 0, 0, days as i32 - 1, false);
            } else if let EventTime::Timed { start, end } = e.time {
                for c in c0.max(0)..=c1.min(days as i32 - 1) {
                    let date = first + Duration::days(c as i64);
                    let top = time::wall_minutes(start, date, tz);
                    // In a repeated hour the end can read earlier than the start.
                    let bottom = time::wall_minutes(end, date, tz).max(top);
                    timed.push(TimedPiece {
                        chip,
                        col: c as u32,
                        top,
                        bottom,
                        place: Column {
                            column: 0,
                            span: 1,
                            columns: 1,
                        },
                        ends: c == c1,
                    });
                }
            }
        }

        // Side by side columns, a day at a time.
        let mut by_col: Vec<Vec<usize>> = vec![Vec::new(); days as usize];
        for (i, p) in timed.iter().enumerate() {
            by_col[p.col as usize].push(i);
        }
        for members in by_col {
            let iv: Vec<(f32, f32)> = members
                .iter()
                .map(|&i| (timed[i].top, timed[i].bottom.max(timed[i].top + min)))
                .collect();
            for (k, place) in layout::columns(&iv).into_iter().enumerate() {
                timed[members[k]].place = place;
            }
        }
        // Earlier and wider pieces first, so later ones paint over them as on paper.
        timed.sort_by(|a, b| {
            a.col
                .cmp(&b.col)
                .then(a.place.column.cmp(&b.place.column))
                .then(a.top.total_cmp(&b.top))
        });

        // Rows of spans, a week at a time.
        spans.sort_by(|a, b| {
            a.0.row
                .cmp(&b.0.row)
                .then(a.0.first.cmp(&b.0.first))
                .then(a.1.cmp(&b.1))
        });
        let mut out_spans = Vec::with_capacity(spans.len());
        let mut i = 0;
        while i < spans.len() {
            let row = spans[i].0.row;
            let j = i + spans[i..].iter().take_while(|s| s.0.row == row).count();
            let ranges: Vec<(i32, i32)> =
                spans[i..j].iter().map(|s| (s.0.first, s.0.last)).collect();
            for (k, lane) in layout::lanes(&ranges).into_iter().enumerate() {
                let mut piece = spans[i + k].0.clone();
                piece.lane = lane;
                out_spans.push(piece);
            }
            i = j;
        }

        let today = self.today();
        let agenda = agenda_rows
            .into_iter()
            .enumerate()
            .filter_map(|(c, mut rows)| {
                let date = first + Duration::days(c as i64);
                if rows.is_empty() && (date != today || key.query.is_some()) {
                    return None;
                }
                rows.sort_by_key(|r| r.1);
                Some(AgendaDay {
                    date,
                    rows: rows.into_iter().map(|r| r.0).collect(),
                })
            })
            .collect();

        let dst = if month || !key.view.is_time_grid() {
            Vec::new()
        } else {
            (0..days)
                .filter_map(|c| {
                    time::dst_change(first + Duration::days(c as i64), tz).map(|d| (c, d))
                })
                .collect()
        };

        Laid {
            key,
            chips,
            timed,
            spans: out_spans,
            dst,
            agenda,
        }
    }

    /// What the agenda's time column says for an event on one of its days.
    fn agenda_row(
        &self,
        e: &Event,
        chip: usize,
        date: NaiveDate,
        (d0, d1): (NaiveDate, NaiveDate),
    ) -> AgendaRow {
        let tz = self.options.time_zone;
        let hour12 = self.options.hour12;
        let clock = |t: DateTime<Utc>| time::format_time(t.with_timezone(&tz).time(), hour12);
        let span = (d1 - d0).num_days() + 1;
        let nth = (date - d0).num_days() + 1;
        let (start, end) = match e.time {
            EventTime::AllDay { .. } if span > 1 => {
                ("All day".into(), format!("Day {nth} of {span}"))
            }
            EventTime::AllDay { .. } => ("All day".into(), String::new()),
            EventTime::Timed { start, end } if d0 == d1 => (clock(start), clock(end)),
            EventTime::Timed { start, .. } if date == d0 => (clock(start), String::new()),
            EventTime::Timed { end, .. } if date == d1 => ("Until".into(), clock(end)),
            EventTime::Timed { .. } => ("All day".into(), format!("Day {nth} of {span}")),
        };
        let at = match e.time {
            EventTime::Timed { start, .. } if date == d0 => Some(start),
            _ => None,
        };
        AgendaRow {
            chip,
            start: start.into(),
            end: end.into(),
            at,
        }
    }

    // The mouse.

    fn mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.focus.focus(window, cx);
        // A press outside the question drops it, and does nothing else. One outside the panel
        // closes it and goes on to do what it does.
        if self.asking.is_some() {
            self.dismiss_question(cx);
            return;
        }
        if self.panel.is_some() {
            self.close_panel(cx);
        }
        let Some(geo) = self.shared.geo.get() else {
            return;
        };
        let target = paint::target_at(&geo, &self.shared, event.position);
        // A press outside the list closes it; one on its "+N" toggles it.
        let reopen = match &target {
            Target::Crowd(ids, _) => self.crowd.as_ref().is_none_or(|c| &c.ids != ids),
            Target::More(date) => self.crowd.as_ref().is_none_or(|c| c.day != Some(*date)),
            _ => false,
        };
        self.close_crowd(cx);
        let double = event.click_count >= 2;
        match &target {
            Target::Event { id, .. } => {
                self.select(Some(Selection::Event(id.clone())), cx);
                if double {
                    self.open_event(id.clone(), cx);
                    return;
                }
            }
            // A link opens when it's let go without a drag; a drag from it moves its event.
            Target::Link { id, .. } => self.select(Some(Selection::Event(id.clone())), cx),
            Target::Expand => {
                self.all_day_expanded = true;
                cx.notify();
                return;
            }
            Target::Crowd(ids, anchor) => {
                if reopen {
                    self.open_crowd(ids.clone(), *anchor, cx);
                }
                return;
            }
            // A month day's "+N more" lists the day's events beside it (a click on it again
            // closes the list).
            Target::More(date) => {
                if reopen {
                    let anchor = self
                        .shared
                        .mores
                        .borrow()
                        .iter()
                        .find(|(_, d)| d == date)
                        .map_or(
                            Bounds::new(event.position, size(px(1.), px(1.))),
                            |(b, _)| *b,
                        );
                    self.open_day_list(*date, anchor, cx);
                }
                return;
            }
            Target::Date(date) => {
                let date = *date;
                self.view = CalendarView::Day;
                self.go_to(date, cx);
                return;
            }
            Target::Grid { date, minute } if double => {
                let start = time::snap_floor(*minute, self.options.slot_minutes as f32);
                let length = self.options.default_duration.num_minutes() as f32;
                let tz = self.options.time_zone;
                let time = EventTime::timed(
                    time::at_minutes(*date, start, tz),
                    time::at_minutes(*date, start + length, tz),
                );
                self.propose_create(time, cx);
                return;
            }
            Target::Day { date, .. } if double => {
                self.propose_create(time::created_days(*date, *date), cx);
                return;
            }
            Target::Nothing => return,
            _ => {}
        }
        self.drag = Some(Drag::Press {
            at: event.position,
            target,
            held: None,
        });
        self.hand_over(event.position, gpui_dnd::Backend::Mouse, 4., window, cx);
        cx.notify();
    }

    pub(crate) fn mouse_move(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(geo) = self.shared.geo.get() else {
            return;
        };
        match &self.drag {
            None => {
                let cursor = paint::cursor_at(&geo, &self.shared, event.position);
                if cursor != self.hover_cursor {
                    self.hover_cursor = cursor;
                    cx.notify();
                }
            }
            // A press let go outside the window, before it was a drag.
            Some(Drag::Press { .. }) if event.pressed_button != Some(MouseButton::Left) => {
                self.finish(cx)
            }
            // gpui-dnd carries a press past its threshold, and the drag after it.
            Some(_) => {}
        }
    }

    /// Carries a press or a drag to `p`. A press becomes a drag once it has moved a few pixels,
    /// or at once when `now` (a long press, or a touch on a selected event).
    fn drag_to(&mut self, drag: Drag, p: Point<Pixels>, now: bool, cx: &mut Context<Self>) {
        self.last_point = p;
        let Some(geo) = self.shared.geo.get() else {
            return;
        };
        let next = match drag {
            // The agenda is a list: nothing in it is dragged.
            Drag::Press { .. } if geo.kind == paint::Kind::Agenda => return,
            Drag::Press { at, target, held } => {
                let threshold = 4. * geo.s;
                let moved = (f32::from(p.x - at.x)).hypot(f32::from(p.y - at.y));
                if moved < threshold && !now {
                    return;
                }
                // A drag from the list of what didn't fit: the list gives way to it.
                self.crowd = None;
                let target = match target {
                    Target::Link {
                        id,
                        area,
                        read_only,
                        ..
                    } => Target::Event {
                        id,
                        area,
                        resize: false,
                        read_only,
                    },
                    other => other,
                };
                match target {
                    Target::Event {
                        id,
                        area,
                        resize,
                        read_only,
                        ..
                    } => {
                        if read_only {
                            return;
                        }
                        let Some(original) = self.editable(&id).map(|e| e.time) else {
                            return;
                        };
                        if resize {
                            let grab = geo.pos(held.unwrap_or(at), area);
                            let shift = match original {
                                EventTime::Timed { end, .. } if area == Area::Grid => {
                                    let tz = self.options.time_zone;
                                    grab.1 - time::wall_minutes(end, grab.0, tz)
                                }
                                _ => 0.,
                            };
                            Drag::Resize {
                                id,
                                original,
                                area,
                                now: geo.pos(p, area),
                                shift,
                            }
                        } else {
                            let over = over_area(&geo, area, p);
                            Drag::Move {
                                id,
                                original,
                                area,
                                over,
                                grab: geo.pos(held.unwrap_or(at), area),
                                now: geo.pos(p, over),
                            }
                        }
                    }
                    Target::Grid { .. } | Target::Day { .. }
                        if self.new_event_calendar().is_none() =>
                    {
                        return;
                    }
                    Target::Grid { date, minute } => Drag::Create {
                        date,
                        from: minute,
                        to: geo.pos(p, Area::Grid).1,
                    },
                    Target::Day { date, area } => Drag::CreateDays {
                        area,
                        from: date,
                        to: geo.pos(p, area).0,
                    },
                    _ => return,
                }
            }
            Drag::Move {
                id,
                original,
                area,
                grab,
                ..
            } => {
                let over = over_area(&geo, area, p);
                Drag::Move {
                    id,
                    original,
                    area,
                    over,
                    grab,
                    now: geo.pos(p, over),
                }
            }
            Drag::Resize {
                id,
                original,
                area,
                shift,
                ..
            } => Drag::Resize {
                id,
                original,
                area,
                now: geo.pos(p, area),
                shift,
            },
            Drag::Create { date, from, .. } => Drag::Create {
                date,
                from,
                to: geo.pos(p, Area::Grid).1,
            },
            Drag::CreateDays { area, from, .. } => Drag::CreateDays {
                area,
                from,
                to: geo.pos(p, area).0,
            },
        };
        // Near the grid's top or bottom edge, or past it, gpui-dnd scrolls the hours for these.
        self.shared.drag_scrolls.set(
            geo.kind == paint::Kind::Grid
                && matches!(
                    next,
                    Drag::Create { .. }
                        | Drag::Move {
                            over: Area::Grid,
                            ..
                        }
                        | Drag::Resize {
                            area: Area::Grid,
                            ..
                        }
                ),
        );
        self.drag = Some(next);
        cx.notify();
    }

    /// A press let go before it was a drag: a click. A drag's release is gpui-dnd's.
    pub(crate) fn mouse_up(&mut self, _: &MouseUpEvent, cx: &mut Context<Self>) {
        if matches!(self.drag, Some(Drag::Press { .. })) {
            self.finish(cx);
        }
    }

    fn finish(&mut self, cx: &mut Context<Self>) {
        let proposal = self.drag_proposal();
        let ghost = self.ghost();
        let Some(drag) = self.drag.take() else {
            return;
        };
        match drag {
            // On touch, a tap on the slot already picked makes an event there, as a second tap
            // does in a phone's calendar: the first only picks it (a double click on the Mac).
            Drag::Press { target, .. } => match target {
                Target::Grid { date, minute } => {
                    let slot = self.options.slot_minutes.max(1) as f32;
                    let minute = time::snap_floor(minute, slot) as u32;
                    let picked = Selection::Slot { date, minute };
                    if self.options.touch && self.selection.as_ref() == Some(&picked) {
                        let tz = self.options.time_zone;
                        let start = time::at_minutes(date, minute as f32, tz);
                        self.propose_create(
                            EventTime::timed(start, start + self.options.default_duration),
                            cx,
                        );
                    } else {
                        self.select(Some(picked), cx);
                    }
                }
                Target::Day { date, .. } => {
                    let picked = Selection::Slot { date, minute: 0 };
                    if self.options.touch && self.selection.as_ref() == Some(&picked) {
                        self.propose_create(time::created_days(date, date), cx);
                    } else {
                        self.select(Some(picked), cx)
                    }
                }
                Target::Link { id, index, .. } => {
                    if let Some(e) = self.event(&id)
                        && let Some(link) = e.links.get(index)
                    {
                        cx.emit(CalendarEvent::OpenLink {
                            event: e.to_ref(),
                            link: link.clone(),
                        });
                    }
                }
                _ => {}
            },
            Drag::Move { original, .. } | Drag::Resize { original, .. } => {
                // The host may have made it read-only while it was dragged.
                if let Some((Some(id), time)) = proposal
                    && time != original
                    && let Some(event) = self.editable(&id).map(Event::to_ref)
                {
                    let kind = if matches!(drag, Drag::Move { .. }) {
                        EditKind::Move
                    } else {
                        EditKind::Resize
                    };
                    // The question opens beside the ghost, never over it.
                    let anchor = self
                        .shared
                        .ghost
                        .get()
                        .unwrap_or_else(|| Bounds::new(self.last_point, size(px(1.), px(1.))));
                    self.propose_edit(kind, event, Some(time), ghost, anchor, cx);
                }
            }
            Drag::Create { .. } | Drag::CreateDays { .. } => {
                if let Some((None, time)) = proposal {
                    self.propose_create(time, cx);
                }
            }
        }
        cx.notify();
    }

    fn scroll_by(&mut self, minutes: f32, geo: &Geo) {
        let max = (time::DAY_MINUTES - geo.visible_minutes()).max(0.);
        let at = (self.scroll_minute() + minutes).clamp(0., max);
        self.shared.scroll.set(Some(at));
    }

    fn scroll(&mut self, event: &ScrollEvent, cx: &mut Context<Self>) {
        let Some(geo) = self.shared.geo.get() else {
            return;
        };
        if event.source == ScrollSource::Touch && self.swipe_scroll(event, &geo, cx) {
            return;
        }
        let delta = event.delta.pixel_delta(px(20.)).y;
        if geo.kind == paint::Kind::Agenda {
            let at = self.shared.agenda_scroll.get().unwrap_or(0.) - f32::from(delta);
            self.shared
                .agenda_scroll
                .set(Some(at.clamp(0., self.shared.agenda_max.get())));
            cx.notify();
            return;
        }
        if geo.kind != paint::Kind::Grid {
            return;
        }
        let minutes = -f32::from(delta) / f32::from(geo.hour_h) * 60.;
        self.scroll_by(minutes, &geo);
        cx.notify();
    }

    // Touch.

    /// A long press: on an event it picks the event up, on empty time it makes one an hour
    /// long (the usual length), and gpui-dnd then follows the finger with either
    /// (`drag::long_press`). The drag for gpui-dnd to begin, if it took it.
    pub(crate) fn long_press(
        &mut self,
        event: &LongPressEvent,
        cx: &mut Context<Self>,
    ) -> Option<drag::Held> {
        let geo = self.shared.geo.get()?;
        // The rest of the long press is gpui-dnd's drag.
        if event.phase != TouchPhase::Started || self.drag.is_some() {
            return None;
        }
        let target = match paint::target_at(&geo, &self.shared, event.start_position) {
            Target::Link {
                id,
                area,
                read_only,
                ..
            } => Target::Event {
                id,
                area,
                resize: false,
                read_only,
            },
            other => other,
        };
        let cursor = self.drag_cursor(&target, &geo)?;
        if let Target::Event { id, .. } = &target {
            self.select(Some(Selection::Event(id.clone())), cx);
        }
        self.touch_from = Some(event.start_position);
        self.touch_moved = false;
        let at = event.start_position;
        self.drag_to(
            Drag::Press {
                at,
                target,
                held: None,
            },
            at,
            true,
            cx,
        );
        if let Some(Drag::Create { from, to, .. }) = &mut self.drag {
            *from = time::snap_floor(*from, self.options.snap());
            *to = *from + self.options.default_duration.num_minutes() as f32;
        }
        cx.notify();
        Some(self.held(at, cursor, cx))
    }

    /// A touch landing on the selected event, or on its handle: it drags the event (or its
    /// end) straight away rather than scrolling, gpui-dnd taking it once the finger moves. Let
    /// go without moving, it opens the event.
    pub(crate) fn touch_drag(
        &mut self,
        event: &TouchDragEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(geo) = self.shared.geo.get() else {
            return false;
        };
        match event.phase {
            TouchPhase::Started => {
                if !self.options.touch || geo.kind == paint::Kind::Agenda || self.drag.is_some() {
                    return false;
                }
                let at = event.start_position;
                let target = paint::target_at(&geo, &self.shared, at);
                let Target::Event { id, read_only, .. } = &target else {
                    return false;
                };
                if *read_only || self.selected_event() != Some(id) || self.editable(id).is_none() {
                    return false;
                }
                self.touch_from = Some(at);
                self.touch_moved = false;
                self.drag = Some(Drag::Press {
                    at,
                    target,
                    held: None,
                });
                self.hand_over(at, gpui_dnd::Backend::Touch, 8., window, cx);
                true
            }
            _ => self.touch_follow(event.phase, event.position, &geo, cx),
        }
    }

    /// The rest of a touch on the selected event before it's a drag (gpui-dnd follows it
    /// once it is): a tap on it, let go without moving, opens it.
    fn touch_follow(
        &mut self,
        phase: TouchPhase,
        p: Point<Pixels>,
        geo: &Geo,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(from) = self.touch_from else {
            return false;
        };
        if !matches!(self.drag, Some(Drag::Press { .. })) {
            return false;
        }
        match phase {
            TouchPhase::Started | TouchPhase::Moved => {
                self.touch_moved |= (p - from).magnitude() as f32 >= 8. * geo.s;
            }
            TouchPhase::Ended => {
                self.touch_from = None;
                if !self.touch_moved
                    && let Some(Drag::Press {
                        target: Target::Event { id, .. },
                        ..
                    }) = self.drag.take()
                {
                    self.open_event(id, cx);
                } else {
                    self.drag = None;
                }
                cx.notify();
            }
            TouchPhase::Cancelled => {
                self.touch_from = None;
                self.drag = None;
                cx.notify();
            }
        }
        true
    }

    /// A finger's sideways pan: the body follows it, and past a fifth of the width (or flicked)
    /// the next or previous period slides in. Returns whether the pan was a sideways one.
    fn swipe_scroll(&mut self, event: &ScrollEvent, geo: &Geo, cx: &mut Context<Self>) -> bool {
        let delta = event.delta.pixel_delta(px(20.));
        let (dx, dy) = (f32::from(delta.x), f32::from(delta.y));
        if event.touch_phase == TouchPhase::Started {
            self.swiping = if self.drag.is_none() {
                Swiping::Undecided
            } else {
                Swiping::No
            };
        }
        // GPUI locks a pan to the way it first moves.
        if self.swiping == Swiping::Undecided && (dx != 0. || dy != 0.) {
            self.swiping = if dx.abs() > dy.abs() {
                self.settle = None;
                self.swipe_track.clear();
                Swiping::Yes
            } else {
                Swiping::No
            };
        }
        if self.swiping != Swiping::Yes {
            if matches!(event.touch_phase, TouchPhase::Ended | TouchPhase::Cancelled) {
                self.swiping = Swiping::No;
            }
            return self.swiping == Swiping::Undecided;
        }
        let now = cx.background_executor().now();
        self.swipe += dx;
        self.swipe_track.push_back((now, self.swipe));
        while self
            .swipe_track
            .front()
            .is_some_and(|(t, _)| now.duration_since(*t) > FLICK_WINDOW)
        {
            self.swipe_track.pop_front();
        }
        match event.touch_phase {
            TouchPhase::Started | TouchPhase::Moved => cx.notify(),
            TouchPhase::Ended | TouchPhase::Cancelled => {
                self.swiping = Swiping::No;
                // The day area's width (the gutter stays), or the month's.
                let width = f32::from(geo.bounds.right() - geo.gutter);
                let far = self.swipe.abs() > width / 5.;
                let speed = self.swipe_speed();
                let flicked = speed.abs() >= FLICK_SPEED && speed.signum() == self.swipe.signum();
                let turn = (event.touch_phase == TouchPhase::Ended && (far || flicked))
                    .then_some(if self.swipe < 0. { 1 } else { -1 });
                self.swipe_track.clear();
                self.settle = Some(Settle {
                    from: self.swipe,
                    to: turn.map_or(0., |dir| -(dir as f32) * width),
                    start: now,
                    turn,
                    width,
                });
                cx.notify();
            }
        }
        true
    }

    /// How fast the swipe was going over its last moments, in points per second: the same
    /// however often the touches came.
    fn swipe_speed(&self) -> f32 {
        let (Some((t0, x0)), Some((t1, x1))) = (self.swipe_track.front(), self.swipe_track.back())
        else {
            return 0.;
        };
        let dt = t1.duration_since(*t0).as_secs_f32();
        if dt <= 0. { 0. } else { (x1 - x0) / dt }
    }

    /// Moves a settling swipe on to where it is at this frame, asking for the next frame while
    /// it moves. At rest it asks for none. With Reduce Motion on (`App::reduce_motion`, which
    /// the host sets from the system's setting) it lands at once, the new period with it.
    fn advance_settle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(settle) = self.settle else {
            return;
        };
        let now = cx.background_executor().now();
        let still = cx.reduce_motion();
        let t = if still {
            1.
        } else {
            (now.duration_since(settle.start).as_secs_f32() / SETTLE.as_secs_f32()).min(1.)
        };
        let eased = 1. - (1. - t).powi(3);
        self.swipe = settle.from + (settle.to - settle.from) * eased;
        if t < 1. {
            window.request_animation_frame();
            return;
        }
        self.settle = None;
        if let Some(dir) = settle.turn {
            // The new period comes in from the side the finger pushed from.
            self.go_to(time::step(self.view, self.anchor, dir), cx);
            if still {
                self.swipe = 0.;
                return;
            }
            let from = dir as f32 * settle.width;
            self.swipe = from;
            self.settle = Some(Settle {
                from,
                to: 0.,
                start: now,
                turn: None,
                width: settle.width,
            });
            window.request_animation_frame();
        }
    }

    // The keyboard.

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.menu.is_some() {
            self.close_menu(cx);
        } else if self.mini_open {
            self.mini_open = false;
            cx.notify();
        } else if self.asking.is_some() {
            self.dismiss_question(cx);
        } else if self.crowd.is_some() {
            self.close_crowd(cx);
        } else if self.panel.is_some() {
            self.close_panel(cx);
        } else if self.drag.take().is_some() {
            // A press gpui-dnd holds, not yet a drag, is forgotten too.
            gpui_dnd::cancel(window, cx);
            cx.notify();
        } else if self.selection.is_some() {
            self.select(None, cx);
        }
    }

    /// The date and minute the selection is at, if any.
    fn selection_place(&self) -> Option<(NaiveDate, f32)> {
        let tz = self.options.time_zone;
        match self.selection.as_ref()? {
            Selection::Slot { date, minute } => Some((*date, *minute as f32)),
            Selection::Event(id) => {
                let e = self.event(id)?;
                Some(match e.time {
                    EventTime::Timed { start, .. } => time::local(start, tz),
                    EventTime::AllDay { start, .. } => (start, 0.),
                })
            }
        }
    }

    /// The events on a date, all-day first and then by start.
    fn events_on(&self, date: NaiveDate) -> Vec<&Event> {
        let tz = self.options.time_zone;
        let mut on = self.events_in(
            time::day_start(date, tz),
            time::day_start(date + Duration::days(1), tz),
        );
        on.sort_by_key(|e| match e.time {
            EventTime::AllDay { .. } => (0, 0),
            EventTime::Timed { start, .. } => (1, start.timestamp()),
        });
        on
    }

    /// Arrow keys: a slot moves by a day or a slot (a week in the month); an event selection
    /// moves to the next event that way, or to the slot there when there's none.
    fn move_selection(&mut self, dx: i32, dy: i32, cx: &mut Context<Self>) {
        if matches!(self.view, CalendarView::Agenda(_))
            && (dy != 0 || self.selected_event().is_some())
        {
            self.move_in_agenda(if dx + dy > 0 { 1 } else { -1 }, cx);
            return;
        }
        let month = self.view == CalendarView::Month;
        let slot = self.options.slot_minutes.max(1) as f32;
        let Some((date, minute)) = self.selection_place() else {
            if dx != 0 && dy == 0 {
                // Nothing selected: left and right go to the previous and next period.
                self.go_to(time::step(self.view, self.anchor, dx), cx);
                return;
            }
            let (first, days) = self.visible_days();
            let today = self.today();
            let in_view = today >= first && (today - first).num_days() < days as i64;
            let date = if in_view { today } else { first };
            let now = time::local(self.now(), self.options.time_zone).1;
            let minute = time::snap_floor(if in_view { now } else { 9. * 60. }, slot);
            self.select_slot(date, minute, cx);
            return;
        };
        if let Some(Selection::Event(id)) = self.selection.clone() {
            if dy != 0 && !month {
                let on = self.events_on(date);
                let at = on.iter().position(|e| e.id == id);
                let next = at.and_then(|i| {
                    let j = i as i64 + dy as i64;
                    (j >= 0).then(|| on.get(j as usize)).flatten()
                });
                if let Some(e) = next {
                    let id = e.id.clone();
                    self.select_event_and_reveal(id, cx);
                } else {
                    let to = (time::snap_floor(minute, slot) + dy as f32 * slot)
                        .clamp(0., time::DAY_MINUTES - slot);
                    self.select_slot(date, to, cx);
                }
                return;
            }
            let to = date
                + Duration::days(if month {
                    dx as i64 + 7 * dy as i64
                } else {
                    dx as i64
                });
            let nearest = self
                .events_on(to)
                .into_iter()
                .min_by_key(|e| match e.time {
                    EventTime::Timed { start, .. } => {
                        (time::local(start, self.options.time_zone).1 - minute).abs() as i64
                    }
                    EventTime::AllDay { .. } => 24 * 60,
                })
                .map(|e| e.id.clone());
            match nearest {
                Some(id) => self.select_event_and_reveal(id, cx),
                None => self.select_slot(to, time::snap_floor(minute, slot), cx),
            }
            return;
        }
        let (to, minute) = if month {
            (date + Duration::days(dx as i64 + 7 * dy as i64), minute)
        } else {
            (
                date + Duration::days(dx as i64),
                (minute + dy as f32 * slot).clamp(0., time::DAY_MINUTES - slot),
            )
        };
        self.select_slot(to, minute, cx);
    }

    /// The arrows in the agenda: the next or previous event in the list, or the first from
    /// today on when nothing is selected.
    fn move_in_agenda(&mut self, dir: i32, cx: &mut Context<Self>) {
        let laid = self.layout();
        let mut order: Vec<(NaiveDate, EventId)> = Vec::new();
        for day in &laid.agenda {
            for row in &day.rows {
                let id = &laid.chips[row.chip].id;
                if !order.iter().any(|(_, o)| o == id) {
                    order.push((day.date, id.clone()));
                }
            }
        }
        if order.is_empty() {
            return;
        }
        let at = self
            .selected_event()
            .and_then(|id| order.iter().position(|(_, o)| o == id));
        let next = match at {
            Some(i) => (i as i64 + dir as i64).clamp(0, order.len() as i64 - 1) as usize,
            None => {
                let today = self.today();
                order.iter().position(|(d, _)| *d >= today).unwrap_or(0)
            }
        };
        let id = order[next].1.clone();
        self.shared.agenda_reveal.replace(Some(id.clone()));
        self.select(Some(Selection::Event(id)), cx);
    }

    fn select_slot(&mut self, date: NaiveDate, minute: f32, cx: &mut Context<Self>) {
        self.reveal(date, minute, cx);
        self.select(
            Some(Selection::Slot {
                date,
                minute: minute as u32,
            }),
            cx,
        );
    }

    fn select_event_and_reveal(&mut self, id: EventId, cx: &mut Context<Self>) {
        if let Some(e) = self.event(&id) {
            let (date, minute) = match e.time {
                EventTime::Timed { start, .. } => time::local(start, self.options.time_zone),
                EventTime::AllDay { start, .. } => (start, -1.),
            };
            self.reveal(date, minute, cx);
        }
        self.select(Some(Selection::Event(id)), cx);
    }

    /// Brings a date into the period on screen, and a minute into the grid's scroll.
    fn reveal(&mut self, date: NaiveDate, minute: f32, cx: &mut Context<Self>) {
        let (first, days) = self.visible_days();
        if date < first || (date - first).num_days() >= days as i64 {
            self.go_to(date, cx);
        }
        if minute >= 0.
            && self.view.is_time_grid()
            && let Some(geo) = self.shared.geo.get()
        {
            let top = self.scroll_minute();
            let shown = geo.visible_minutes();
            let slot = self.options.slot_minutes.max(1) as f32;
            if minute < top {
                self.scroll_by(minute - top, &geo);
            } else if minute + slot > top + shown {
                self.scroll_by(minute + slot - (top + shown), &geo);
            }
        }
    }

    /// `n`: a new event of the usual length at the selected slot (a day in the month), or, with
    /// no slot selected, at the next round half hour today, bringing today into view.
    pub fn new_event(&mut self, cx: &mut Context<Self>) {
        if self.asking.is_some() {
            return;
        }
        let tz = self.options.time_zone;
        let length = self.options.default_duration;
        let time = match self.selection.clone() {
            Some(Selection::Slot { date, .. }) if self.view == CalendarView::Month => {
                time::created_days(date, date)
            }
            Some(Selection::Slot { date, minute }) => {
                let start = time::at_minutes(date, minute as f32, tz);
                EventTime::timed(start, start + length)
            }
            _ => {
                let (today, now) = time::local(self.now(), tz);
                let start_minute = time::snap_ceil(now + 0.001, 30.);
                let start = time::at_minutes(today, start_minute, tz);
                let (date, minute) = time::local(start, tz);
                self.reveal(date, minute, cx);
                EventTime::timed(start, start + length)
            }
        };
        self.propose_create(time, cx);
    }

    fn open_selected(&mut self, cx: &mut Context<Self>) {
        match self.selection.clone() {
            Some(Selection::Event(id)) => self.open_event(id, cx),
            Some(Selection::Slot { date, minute }) => {
                let tz = self.options.time_zone;
                let time = if self.view == CalendarView::Month {
                    time::created_days(date, date)
                } else {
                    let start = time::at_minutes(date, minute as f32, tz);
                    EventTime::timed(start, start + self.options.default_duration)
                };
                self.propose_create(time, cx);
            }
            None => {}
        }
    }

    pub(crate) fn delete_selected(&mut self, cx: &mut Context<Self>) {
        if self.asking.is_some() {
            return;
        }
        // With the panel open, Delete is its Delete: for the event it shows, and the panel
        // closes rather than stay over the question or a deleted event.
        if self.panel.is_some() {
            self.panel_edit(EditKind::Delete, cx);
            return;
        }
        if let Some(event) = self
            .selected_event()
            .and_then(|id| self.editable(id))
            .map(Event::to_ref)
        {
            let anchor = self.event_bounds(&event.id).unwrap_or_else(|| {
                self.shared.geo.get().map_or(Bounds::default(), |g| {
                    Bounds::new(g.bounds.center(), size(px(1.), px(1.)))
                })
            });
            self.propose_edit(EditKind::Delete, event, None, None, anchor, cx);
        }
    }

    // Recurring events.

    /// Sends an edit to the host, or first asks which occurrences it's for when the event is
    /// one of a recurring series.
    fn propose_edit(
        &mut self,
        kind: EditKind,
        event: EventRef,
        time: Option<EventTime>,
        ghost: Option<Ghost>,
        anchor: Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if event.series.is_none() {
            self.emit_edit(kind, event, time, EditScope::This, cx);
            return;
        }
        // A host that allows one answer isn't asked a question with one answer.
        if let [scope] = self.options.edit_scopes()[..] {
            self.emit_edit(kind, event, time, scope, cx);
            cx.notify();
            return;
        }
        self.asking = Some(Asking {
            kind,
            event,
            time,
            ghost,
            anchor,
            highlighted: 0,
        });
        cx.notify();
    }

    fn emit_edit(
        &mut self,
        kind: EditKind,
        event: EventRef,
        time: Option<EventTime>,
        scope: EditScope,
        cx: &mut Context<Self>,
    ) {
        cx.emit(match (kind, time) {
            (EditKind::Move, Some(time)) => CalendarEvent::Move { event, time, scope },
            (EditKind::Resize, Some(time)) => CalendarEvent::Resize { event, time, scope },
            (EditKind::ChangeCalendar(calendar), _) => CalendarEvent::ChangeCalendar {
                event,
                calendar,
                scope,
            },
            (EditKind::Rsvp(response), _) => CalendarEvent::Rsvp {
                event,
                response,
                scope,
            },
            _ => CalendarEvent::Delete { event, scope },
        });
    }

    // The event panel.

    /// What opening an event does: tells the host, and shows the panel if it's on.
    pub(crate) fn open_event(&mut self, id: EventId, cx: &mut Context<Self>) {
        cx.emit(CalendarEvent::Open(id.clone()));
        if self.options.event_panel {
            self.open_panel(id, cx);
        }
    }

    /// Shows an event's details in the panel, whether or not `event_panel` is on.
    pub fn open_panel(&mut self, id: EventId, cx: &mut Context<Self>) {
        if self.event(&id).is_none() {
            return;
        }
        let anchor = self.event_bounds(&id).unwrap_or_else(|| {
            self.shared.geo.get().map_or(Bounds::default(), |g| {
                Bounds::new(g.bounds.center(), size(px(1.), px(1.)))
            })
        });
        self.panel = Some(Panel {
            id,
            anchor,
            picking_calendar: false,
        });
        cx.notify();
    }

    pub fn close_panel(&mut self, cx: &mut Context<Self>) {
        if self.panel.take().is_some() {
            cx.notify();
        }
    }

    /// The panel's Edit: the panel closes and the host is asked to open its editor on the
    /// event. Nothing without the panel, or for an event that can't be edited.
    pub fn edit_panel_event(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.panel.as_ref().map(|p| p.id.clone()) else {
            return;
        };
        let Some(event) = self.editable(&id).map(Event::to_ref) else {
            return;
        };
        self.panel = None;
        cx.emit(CalendarEvent::Edit(event));
        cx.notify();
    }

    /// The event the panel shows, if it's open.
    pub fn panel_event(&self) -> Option<&EventId> {
        self.panel.as_ref().map(|p| &p.id)
    }

    /// A panel action that edits the event: the panel closes, and the edit goes to the host or
    /// first asks which occurrences, for a repeating event.
    fn panel_edit(&mut self, kind: EditKind, cx: &mut Context<Self>) {
        let Some(p) = self.panel.clone() else { return };
        let Some(event) = self.event(&p.id).map(Event::to_ref) else {
            return;
        };
        // An answer to an invitation doesn't need the event to be editable.
        let allowed = matches!(kind, EditKind::Rsvp(_)) || self.editable(&p.id).is_some();
        if !allowed {
            return;
        }
        // An answer to a one-off invitation keeps the panel open; anything that asks closes it,
        // so the question has the event to itself.
        if !(matches!(kind, EditKind::Rsvp(_)) && event.series.is_none()) {
            self.panel = None;
        }
        self.propose_edit(kind, event, None, None, p.anchor, cx);
        cx.notify();
    }

    /// Whether the calendar is asking which occurrences of a recurring event an edit is for.
    pub fn is_asking(&self) -> bool {
        self.asking.is_some()
    }

    /// Answers the question: the held edit goes to the host with this scope.
    pub fn answer(&mut self, scope: EditScope, cx: &mut Context<Self>) {
        if let Some(a) = self.asking.take() {
            self.emit_edit(a.kind, a.event, a.time, scope, cx);
            cx.notify();
        }
    }

    /// Drops the question and the edit with it: the event stays where it was.
    pub fn dismiss_question(&mut self, cx: &mut Context<Self>) {
        if self.asking.take().is_some() {
            cx.notify();
        }
    }

    fn highlight_answer(&mut self, by: i32, cx: &mut Context<Self>) {
        let n = self.options.edit_scopes().len() as i32;
        if let Some(a) = &mut self.asking {
            a.highlighted = (a.highlighted as i32 + by).rem_euclid(n) as usize;
            cx.notify();
        }
    }

    /// The question: on the Mac a popover where the edit was let go; on touch a sheet.
    fn render_question(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let asking = self.asking.as_ref()?;
        let theme = Theme::global(cx);
        let c = theme.colors;
        let touch = self.options.touch;
        let title = match asking.kind {
            EditKind::Move => "Move a repeating event",
            EditKind::Resize => "Change a repeating event",
            EditKind::Delete => "Delete a repeating event",
            EditKind::ChangeCalendar(_) => "Move a repeating event to another calendar",
            EditKind::Rsvp(_) => "Answer for a repeating event",
        };
        let this = cx.entity().downgrade();
        let scopes = self.options.edit_scopes();
        let rows = scopes.into_iter().enumerate().map(|(i, scope)| {
            let this = this.clone();
            let highlighted = i == asking.highlighted;
            Press::new(("scope", i))
                .flex()
                .items_center()
                .w_full()
                .min_h(if touch {
                    DefiniteLength::from(px(44.))
                } else {
                    DefiniteLength::from(rems(30. / 15.))
                })
                .px(rems(10. / 15.))
                .rounded(rems(6. / 15.))
                .text_size(rems(13. / 15.))
                .when(highlighted && !touch, |d| d.bg(c.accent))
                .hover(|d| d.bg(c.accent))
                .debug_selector(move || format!("scope-{i}"))
                .on_click(move |_, window, cx| {
                    this.update(cx, |c, cx| {
                        c.answer(scope, cx);
                        c.focus.focus(window, cx);
                    })
                    .ok();
                })
                .child(div().truncate().child(scope.label()))
        });
        // Drawn over the window, outside the calendar's place in Tab's order: put after it.
        let rows = after_calendar(div().flex().flex_col().children(rows));
        let heading = div()
            .px(rems(10. / 15.))
            .pt(rems(6. / 15.))
            .pb(rems(4. / 15.))
            .truncate()
            .text_size(rems(12. / 15.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(c.muted_foreground)
            .child(title);
        let dismiss = {
            let this = this.clone();
            move |_: &DismissReason, _: &mut Window, cx: &mut App| {
                this.update(cx, |c, cx| c.dismiss_question(cx)).ok();
            }
        };
        Some(if touch {
            let cancel = {
                let this = this.clone();
                Press::new("scope-cancel")
                    .flex()
                    .items_center()
                    .justify_center()
                    .w_full()
                    .min_h(px(44.))
                    .mt(rems(6. / 15.))
                    .rounded(rems(6. / 15.))
                    .bg(c.secondary)
                    .text_size(rems(13. / 15.))
                    .on_click(move |_, _, cx| {
                        this.update(cx, |c, cx| c.dismiss_question(cx)).ok();
                    })
                    .child("Cancel")
            };
            Sheet::new("calendar-question")
                .accessibility_label(title)
                .open(true)
                .take_focus(false)
                .on_dismiss(dismiss)
                .child(heading)
                .child(rows)
                .child(cancel)
                .into_any_element()
        } else {
            // Below the edit's ghost, or above it, never over it; hung from its left edge, so
            // a ghost across a week doesn't make the question as wide.
            let a = asking.anchor;
            Popover::new("calendar-question")
                .accessibility_label(title)
                .open(true)
                .anchor(Bounds::new(
                    a.origin,
                    size(a.size.width.min(px(1.)), a.size.height),
                ))
                .take_focus(false)
                .on_dismiss(dismiss)
                .p(rems(4. / 15.))
                .min_w(rems(14.))
                .max_w(rems(20.))
                .child(heading)
                .child(rows)
                .into_any_element()
        })
    }

    // Drawing.

    /// The ghost of the drag in progress: the proposal, drawn where it would land.
    fn ghost(&self) -> Option<Ghost> {
        if let Some(a) = &self.asking {
            return a.ghost.clone();
        }
        let (id, time) = self.drag_proposal()?;
        let tz = self.options.time_zone;
        let (title, color) = match id.as_ref().and_then(|id| self.event(id)) {
            Some(e) => (e.title.clone(), self.color_of(e)),
            None => {
                let calendar = self.new_event_calendar().flatten();
                let color = calendar
                    .as_ref()
                    .and_then(|id| self.calendar(id))
                    .map_or(crate::theme::PALETTE[0], |c| c.color);
                ("New event".into(), color)
            }
        };
        let label = match time {
            EventTime::Timed { start, end } => time::format_range(
                start.with_timezone(&tz).time(),
                end.with_timezone(&tz).time(),
                self.options.hour12,
            ),
            EventTime::AllDay { start, end } => {
                let days = (end - start).num_days();
                if days == 1 {
                    "All day".into()
                } else {
                    format!("{days} days")
                }
            }
        };
        Some(Ghost {
            moving: id,
            time,
            title,
            label: label.into(),
            color,
        })
    }

    /// The header for a narrow calendar or one taken by touch: the title (a short one when
    /// narrow), Today, the period arrows (not on touch, where a swipe changes the period), and
    /// the views (behind one menu button when narrow, with 3 Days among them). Drawn with plain elements, not gpui-core's
    /// buttons; on touch each control is 44 points tall.
    fn render_compact_header(
        &self,
        narrow: bool,
        colors: CalendarColors,
        cx: &mut Context<Self>,
    ) -> Div {
        let touch = self.options.touch;
        let this = cx.entity().downgrade();
        let on = move |f: HeaderAction| {
            let this = this.clone();
            move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                this.update(cx, |c, cx| {
                    f(c, cx);
                    c.focus.focus(window, cx);
                })
                .ok();
            }
        };
        // Each a button to assistive technology, with a name where its text is a glyph, and
        // reached by Tab; a press leaves the keyboard with the calendar.
        let control = |id: ElementId, label: SharedString, name: &str, selected: bool| {
            Press::new(id)
                .accessibility_label(SharedString::from(name.to_string()))
                .selected(selected)
                .focus_on_press(false)
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .h(rems(if touch { 44. } else { 32. } / 15.))
                .min_w(rems(if touch { 44. } else { 0. } / 15.))
                // Tighter when narrow, so four views, Today and the arrows fit a 320-point phone.
                .px(rems(if narrow { 5. } else { 8. } / 15.))
                .rounded(rems(8. / 15.))
                .text_size(rems(13. / 15.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(if selected {
                    colors.text
                } else {
                    colors.muted_text
                })
                .when(selected, |d| d.bg(colors.highlight))
                .cursor_pointer()
                .child(label)
        };
        let view = self.view;
        let views: &[(CalendarView, &str)] = &[
            (CalendarView::Day, "Day"),
            (CalendarView::Week, "Week"),
            (CalendarView::Month, "Month"),
            (CalendarView::AGENDA, "Agenda"),
        ];
        let title = if narrow {
            self.short_title()
        } else {
            self.title()
        };
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(rems(6. / 15.))
            .h(rems(if touch { 56. } else { 52. } / 15.))
            .px(rems(12. / 15.))
            .border_b_1()
            .border_color(colors.line)
            .child({
                fn style<E: Styled>(e: E, colors: CalendarColors) -> E {
                    e.flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(rems(1.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(colors.text)
                }
                if self.options.mini_month {
                    style(self.title_press(title.clone(), cx), colors).into_any_element()
                } else {
                    style(div(), colors)
                        .debug_selector(|| "calendar-title".into())
                        .child(SharedString::from(title))
                        .into_any_element()
                }
            })
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(rems(2. / 15.))
                    .debug_selector(|| "calendar-controls".into())
                    .child(
                        control("today".into(), "Today".into(), "Today", false)
                            .on_click(on(Rc::new(|c, cx| c.go_to_today(cx)))),
                    )
                    .when(!touch, |d| {
                        d.child(
                            control("previous".into(), "‹".into(), "Previous", false)
                                .on_click(on(Rc::new(|c, cx| c.previous(cx)))),
                        )
                        .child(
                            control("next".into(), "›".into(), "Next", false)
                                .on_click(on(Rc::new(|c, cx| c.next(cx)))),
                        )
                    })
                    .map(|d| {
                        if narrow {
                            d.child(self.view_menu(control, colors, cx))
                        } else {
                            d.child(
                                div()
                                    .flex()
                                    .items_center()
                                    .ml(rems(4. / 15.))
                                    .p(rems(2. / 15.))
                                    .rounded(rems(10. / 15.))
                                    .border_1()
                                    .border_color(colors.line)
                                    .children(views.iter().enumerate().map(|(i, &(v, label))| {
                                        let on_view = match (v, view) {
                                            (CalendarView::Week, CalendarView::Days(_)) => true,
                                            (CalendarView::Agenda(_), CalendarView::Agenda(_)) => {
                                                true
                                            }
                                            _ => v == view,
                                        };
                                        control(("view", i).into(), label.into(), label, on_view)
                                            .debug_selector(move || {
                                                format!("calendar-view-{label}")
                                            })
                                            .on_click(on(Rc::new(move |c, cx| c.set_view(v, cx))))
                                    })),
                            )
                        }
                    }),
            )
    }

    /// The narrow header's views, behind one button named for the view shown ("3 Days ▾"): Day,
    /// 3 Days (a phone's week, readable), Week, Month and List. The menu's rows are a finger
    /// tall on touch.
    fn view_menu(
        &self,
        control: impl Fn(ElementId, SharedString, &str, bool) -> Press,
        colors: CalendarColors,
        cx: &mut Context<Self>,
    ) -> DropdownMenu {
        const VIEWS: [(&str, CalendarView, &str); 5] = [
            ("day", CalendarView::Day, "Day"),
            ("days-3", CalendarView::Days(3), "3 Days"),
            ("week", CalendarView::Week, "Week"),
            ("month", CalendarView::Month, "Month"),
            ("list", CalendarView::AGENDA, "List"),
        ];
        let view = self.view;
        let shown = |v: CalendarView| match (v, view) {
            (CalendarView::Agenda(_), CalendarView::Agenda(_)) => true,
            _ => v == view,
        };
        let name: SharedString = match view {
            CalendarView::Days(n) if n != 3 => format!("{n} Days").into(),
            _ => VIEWS
                .iter()
                .find(|(_, v, _)| shown(*v))
                .map_or("View", |(_, _, label)| *label)
                .into(),
        };
        let trigger = control(
            "calendar-views".into(),
            format!("{name} ▾").into(),
            &format!("View: {name}"),
            false,
        )
        .ml(rems(4. / 15.))
        .border_1()
        .border_color(colors.line)
        .debug_selector(|| "calendar-views".into());
        let this = cx.entity().downgrade();
        DropdownMenu::new("calendar-view-menu")
            .trigger(trigger)
            .accessibility_label("View")
            .touch(self.options.touch)
            .align(PopoverAlign::End)
            .items(
                VIEWS
                    .iter()
                    .map(|&(id, v, label)| PopupMenuItem::new(id, label).checked(shown(v))),
            )
            .on_select(move |pick: &MenuSelect, window, cx| {
                let Some(&(_, v, _)) = VIEWS.iter().find(|(id, _, _)| *id == pick.id.as_ref())
                else {
                    return;
                };
                this.update(cx, |c, cx| {
                    c.set_view(v, cx);
                    c.focus.focus(window, cx);
                })
                .ok();
            })
    }

    /// Whether the calendar is laid out for a narrow place: from the width it was last drawn
    /// at, or the window's before the first frame.
    fn is_narrow(&self, window: &Window) -> bool {
        let width = self
            .shared
            .geo
            .get()
            .map_or(window.viewport_size().width, |g| g.bounds.size.width);
        width < px(paint::NARROW * f32::from(window.rem_size()) / 15.)
    }

    /// The title as the button that opens the mini month behind it: a press opens it (on the
    /// way down, so a press that closes it doesn't open it again), and so do Enter and Space
    /// when Tab has brought the keyboard here. VoiceOver hears "November 2026, go to a date".
    fn title_press(&self, title: String, cx: &mut Context<Self>) -> Press {
        let toggle = cx.listener(|c, e: &ClickEvent, window, cx| {
            if e.is_keyboard() {
                c.toggle_mini_month(window, cx);
            }
        });
        Press::new("calendar-title")
            .accessibility_label(SharedString::from(format!("{title}, go to a date")))
            .focus_on_press(false)
            .debug_selector(|| "calendar-title".into())
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|c, _, window, cx| {
                    cx.stop_propagation();
                    // The press that closed it from outside (on this title) is done.
                    if std::mem::take(&mut c.mini_dismissed) {
                        return;
                    }
                    c.toggle_mini_month(window, cx);
                }),
            )
            .on_click(toggle)
            .child(SharedString::from(title))
    }

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::global(cx);
        let colors = theme.colors;
        let this = cx.entity().downgrade();
        let on = move |f: fn(&mut Calendar, &mut Window, &mut Context<Calendar>)| {
            let this = this.clone();
            move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                this.update(cx, |c, cx| {
                    f(c, window, cx);
                    c.focus.focus(window, cx);
                })
                .ok();
            }
        };
        let view = self.view;
        let views = [
            CalendarView::Day,
            CalendarView::Week,
            CalendarView::Month,
            CalendarView::AGENDA,
        ];
        let pick = cx.entity().downgrade();
        div()
            .flex()
            .flex_none()
            .items_center()
            .justify_between()
            .h(rems(52. / 15.))
            .px(rems(16. / 15.))
            .border_b_1()
            .border_color(colors.border)
            .gap(rems(12. / 15.))
            .child({
                fn style<E: Styled>(e: E, colors: &gpui_core::Colors) -> E {
                    e.flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(rems(18. / 15.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(colors.foreground)
                }
                // Where the mini month has no room beside the days, it's behind the title, as
                // in the compact header.
                if self.options.mini_month && !self.mini_side {
                    style(self.title_press(self.title(), cx), &colors).into_any_element()
                } else {
                    style(div(), &colors)
                        .debug_selector(|| "calendar-title".into())
                        .child(SharedString::from(self.title()))
                        .into_any_element()
                }
            })
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(rems(6. / 15.))
                    .debug_selector(|| "calendar-controls".into())
                    .child(
                        Button::new("today")
                            .label("Today")
                            .outline()
                            .small()
                            .on_click(on(|c, _, cx| c.go_to_today(cx))),
                    )
                    .child(
                        Button::new("previous")
                            .icon(IconName::ChevronLeft)
                            .accessibility_label("Previous")
                            .ghost()
                            .small()
                            .on_click(on(|c, _, cx| c.previous(cx))),
                    )
                    .child(
                        Button::new("next")
                            .icon(IconName::ChevronRight)
                            .accessibility_label("Next")
                            .ghost()
                            .small()
                            .on_click(on(|c, _, cx| c.next(cx))),
                    )
                    .child(
                        ButtonGroup::new("views")
                            .outline()
                            .small()
                            .children(views.into_iter().enumerate().map(|(i, v)| {
                                let on_view = match (v, view) {
                                    (CalendarView::Week, CalendarView::Days(_)) => true,
                                    (CalendarView::Agenda(_), CalendarView::Agenda(_)) => true,
                                    _ => v == view,
                                };
                                Button::new(("view", i)).label(v.label()).selected(on_view)
                            }))
                            .on_click(move |picked: &Vec<usize>, window, cx| {
                                if let Some(&i) = picked.first() {
                                    pick.update(cx, |c, cx| {
                                        c.set_view(views[i], cx);
                                        c.focus.focus(window, cx);
                                    })
                                    .ok();
                                }
                            }),
                    ),
            )
    }
}

/// An overlay's content (drawn over the window, so out of the calendar's own place in Tab's
/// order), put after the calendar's other stops: in the window's first group, after the
/// calendar's (header 0, mini month 1, days 2).
pub(crate) fn after_calendar(content: impl IntoElement) -> Div {
    div()
        .tab_group()
        .child(div().tab_group().tab_index(3).child(content))
}

/// Which part of the calendar a move is over at `p`: on the week's grid, the all-day row
/// above the hours or the hours below it, whichever it was grabbed in; elsewhere where it was
/// grabbed.
fn over_area(geo: &Geo, grabbed: Area, p: Point<Pixels>) -> Area {
    match grabbed {
        Area::Grid | Area::AllDay if geo.kind == paint::Kind::Grid => {
            if p.y < geo.grid_top {
                Area::AllDay
            } else {
                Area::Grid
            }
        }
        other => other,
    }
}

impl Render for Calendar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self
            .colors
            .unwrap_or_else(|| CalendarColors::from_theme(cx));
        self.advance_settle(window, cx);
        let laid = self.layout();
        if self.last_range.is_none() {
            // The first frame tells the host what to supply.
            self.announce_range(cx);
        }
        let (first, days) = self.visible_days();
        let now = self.now();
        let tz = self.options.time_zone;
        let scene = paint::Scene {
            kind: match self.view {
                CalendarView::Month => paint::Kind::Month,
                CalendarView::Agenda(_) => paint::Kind::Agenda,
                _ => paint::Kind::Grid,
            },
            first,
            days,
            month: self.anchor.month(),
            today: self.today(),
            now,
            now_minute: time::local(now, tz).1,
            options: self.options.clone(),
            colors,
            family: Theme::global(cx).font_family.clone(),
            laid,
            selection: self.selection.clone(),
            ghost: self.ghost(),
            cursor: match &self.drag {
                Some(Drag::Move { .. }) => CursorStyle::ClosedHand,
                Some(Drag::Resize {
                    area: Area::Grid, ..
                }) => CursorStyle::ResizeUpDown,
                Some(Drag::Resize { .. }) => CursorStyle::ResizeLeftRight,
                Some(Drag::Create { .. } | Drag::CreateDays { .. }) => CursorStyle::Arrow,
                _ => self.hover_cursor,
            },
            all_day_expanded: self.all_day_expanded,
            header_narrow: self.header.then(|| self.is_narrow(window)),
            swipe: self.swipe,
            neighbour: self.swipe_neighbour(),
            lanes: None,
            find: self.query.clone().map(|q| (q, colors.find.opacity(0.38))),
        };
        let this = cx.entity().downgrade();
        let body = crate::a11y::Accessible::new(
            paint::body(scene, self.shared.clone(), this.clone(), self.a11y.clone()).into_element(),
            this,
            self.accessibility_label(),
            self.a11y.clone(),
        );
        let question = self.render_question(cx);
        let query = self.render_query(colors, cx);
        let panel = self.render_panel(window, cx);
        let crowd = self.render_crowd(window, cx);
        let menu = self.render_menu(cx);
        let (aside, mini) = self.render_mini(window, cx);
        let ring = self.focus.is_focused(window) && window.last_input_was_keyboard();
        let root = div()
            .id("calendar")
            .tab_group()
            .relative()
            .flex()
            .size_full()
            .bg(colors.background)
            .text_color(colors.text)
            .track_focus(&self.focus)
            .key_context(if self.typing {
                "CalendarQuery"
            } else {
                CONTEXT
            })
            .capture_key_down(cx.listener(|c, e: &KeyDownEvent, _, cx| c.query_key(e, cx)))
            .on_action(cx.listener(|c, _: &SelectLeft, _, cx| c.move_selection(-1, 0, cx)))
            .on_action(cx.listener(|c, _: &SelectRight, _, cx| c.move_selection(1, 0, cx)))
            .on_action(cx.listener(|c, _: &SelectUp, _, cx| {
                if c.asking.is_some() {
                    c.highlight_answer(-1, cx)
                } else {
                    c.move_selection(0, -1, cx)
                }
            }))
            .on_action(cx.listener(|c, _: &SelectDown, _, cx| {
                if c.asking.is_some() {
                    c.highlight_answer(1, cx)
                } else {
                    c.move_selection(0, 1, cx)
                }
            }))
            .on_action(cx.listener(|c, _: &Today, _, cx| c.go_to_today(cx)))
            .on_action(cx.listener(|c, _: &DayView, _, cx| c.set_view(CalendarView::Day, cx)))
            .on_action(cx.listener(|c, _: &WeekView, _, cx| c.set_view(CalendarView::Week, cx)))
            .on_action(cx.listener(|c, _: &MonthView, _, cx| c.set_view(CalendarView::Month, cx)))
            .on_action(cx.listener(|c, _: &AgendaView, _, cx| c.set_view(CalendarView::AGENDA, cx)))
            .on_action(cx.listener(|c, _: &PreviousPeriod, _, cx| c.previous(cx)))
            .on_action(cx.listener(|c, _: &NextPeriod, _, cx| c.next(cx)))
            .on_action(cx.listener(|c, _: &NewEvent, _, cx| c.new_event(cx)))
            .on_action(cx.listener(|c, _: &OpenSelected, _, cx| {
                match c.asking.as_ref().map(|a| a.highlighted) {
                    Some(i) => {
                        let scopes = c.options.edit_scopes();
                        c.answer(scopes[i.min(scopes.len() - 1)], cx)
                    }
                    None => c.open_selected(cx),
                }
            }))
            .on_action(cx.listener(|c, _: &DeleteSelected, _, cx| c.delete_selected(cx)))
            .on_action(cx.listener(|c, _: &Cancel, window, cx| c.cancel(window, cx)))
            .on_action(cx.listener(|_, _: &Find, _, cx| cx.emit(CalendarEvent::Find)))
            .on_action(cx.listener(|c, _: &GoToDate, window, cx| c.toggle_mini_month(window, cx)))
            .on_action(cx.listener(|c, _: &EditPanelEvent, _, cx| c.edit_panel_event(cx)))
            .on_action(cx.listener(|c, _: &ShowMenu, _, cx| c.show_menu(cx)))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|c, e: &MouseDownEvent, window, cx| {
                    c.focus.focus(window, cx);
                    c.right_click(e.position, cx);
                }),
            )
            .children(aside)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .when(self.header, |d| {
                        let narrow = self.is_narrow(window);
                        // First in the order Tab goes in.
                        let header = div().flex_none().tab_group().tab_index(0);
                        d.child(if narrow || self.options.touch {
                            header.child(self.render_compact_header(narrow, colors, cx))
                        } else {
                            header.child(self.render_header(cx).into_any_element())
                        })
                    })
                    .child(
                        div()
                            .id("calendar-body")
                            .relative()
                            .flex_1()
                            .min_h_0()
                            .overflow_hidden()
                            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
                            .on_scroll(cx.listener(|c, e, _, cx| c.scroll(e, cx)))
                            .child(body)
                            .children(query)
                            .when(ring, |d| {
                                // Where the keyboard is, when Tab brought it: a ring inside the
                                // days, in the theme's focus colour.
                                d.child(
                                    div()
                                        .absolute()
                                        .inset_0()
                                        .border_2()
                                        .border_color(Theme::global(cx).focus_ring()),
                                )
                            }),
                    ),
            )
            .children(panel)
            .children(crowd)
            .children(menu)
            .children(mini)
            .children(question);
        // The calendar's drags run on gpui-dnd: its own provider (the window's first drawn
        // one does the work), and the calendar as their target.
        gpui_dnd::DndProvider::new(self.drop_target(root, cx))
    }
}
