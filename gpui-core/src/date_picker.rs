//! A date picker: a field that shows the date, and a month to pick it from. One date, or a range
//! of two ([`DatePickerState::range`]).

use std::rc::Rc;

use chrono::{Datelike as _, Duration, Local, NaiveDate, Weekday};
use gpui::{
    App, Context, ElementId, Entity, EventEmitter, FocusHandle, Focusable, Hsla,
    InteractiveElement as _, IntoElement, KeyBinding, ParentElement as _, Rems, RenderOnce, Role,
    SharedString, StatefulInteractiveElement as _, StyleRefinement, Styled, Window, actions, div,
    prelude::FluentBuilder as _, rems,
};

use crate::icon::{Icon, IconName};
use crate::popover::{Popover, PopoverColors};
use crate::press::{Press, focus_ring};
use crate::size::{Disableable, Selectable as _};
use crate::theme::{ActiveTheme as _, Theme};

actions!(
    gpui_core_date_picker,
    [
        PrevDay, NextDay, PrevWeek, NextWeek, PrevMonth, NextMonth, Pick, Close
    ]
);

const CONTEXT: &str = "GpuiCoreDatePicker";
const GRID: &str = "GpuiCoreMonthGrid";

pub(crate) fn init(cx: &mut App) {
    let c = Some(CONTEXT);
    cx.bind_keys([
        KeyBinding::new("left", PrevDay, c),
        KeyBinding::new("right", NextDay, c),
        KeyBinding::new("up", PrevWeek, c),
        KeyBinding::new("down", NextWeek, c),
        KeyBinding::new("pageup", PrevMonth, c),
        KeyBinding::new("pagedown", NextMonth, c),
        KeyBinding::new("enter", Pick, c),
        KeyBinding::new("space", Pick, c),
        KeyBinding::new("escape", Close, c),
    ]);
    let g = Some(GRID);
    cx.bind_keys([
        KeyBinding::new("left", PrevDay, g),
        KeyBinding::new("right", NextDay, g),
        KeyBinding::new("up", PrevWeek, g),
        KeyBinding::new("down", NextWeek, g),
        KeyBinding::new("pageup", PrevMonth, g),
        KeyBinding::new("pagedown", NextMonth, g),
        KeyBinding::new("enter", Pick, g),
        KeyBinding::new("space", Pick, g),
    ]);
}

/// The picked date, or the two ends of a range (the end empty until it's picked).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DateValue {
    start: Option<NaiveDate>,
    end: Option<NaiveDate>,
}

impl DateValue {
    pub fn start(&self) -> Option<NaiveDate> {
        self.start
    }

    pub fn end(&self) -> Option<NaiveDate> {
        self.end
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DatePickerEvent {
    Change(DateValue),
}

/// A date picker's value and month. Make it an entity, draw it with [`DatePicker`], subscribe to
/// its [`DatePickerEvent`]s.
pub struct DatePickerState {
    value: DateValue,
    range: bool,
    open: bool,
    /// The day the keyboard is on in the open month.
    cursor: NaiveDate,
    focus: FocusHandle,
    /// The day weeks start on.
    week_start: Weekday,
    /// Today, when the host keeps its own clock or zone; the Mac's local date otherwise.
    today: Option<NaiveDate>,
}

impl EventEmitter<DatePickerEvent> for DatePickerState {}

impl Focusable for DatePickerState {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

fn today() -> NaiveDate {
    Local::now().date_naive()
}

fn first_of_month(d: NaiveDate) -> NaiveDate {
    d.with_day(1).unwrap_or(d)
}

/// `d` moved by whole months, kept to the month's last day (31 January + 1 month: 28 February).
fn add_months(d: NaiveDate, months: i32) -> NaiveDate {
    let total = d.year() * 12 + d.month0() as i32 + months;
    let (y, m) = (total.div_euclid(12), total.rem_euclid(12) as u32 + 1);
    let mut day = d.day();
    loop {
        if let Some(n) = NaiveDate::from_ymd_opt(y, m, day) {
            return n;
        }
        day -= 1;
    }
}

impl DatePickerState {
    /// Picks one date.
    pub fn new(_: &mut Window, cx: &mut Context<Self>) -> Self {
        crate::init(cx);
        Self {
            value: DateValue::default(),
            range: false,
            open: false,
            cursor: today(),
            focus: cx.focus_handle().tab_stop(true),
            week_start: Weekday::Sun,
            today: None,
        }
    }

    /// Today, for a host whose day isn't the Mac's local date (a display time zone of its own,
    /// a pinned clock): it's the day marked as today, and where the keyboard starts with nothing
    /// picked.
    pub fn today(mut self, date: NaiveDate) -> Self {
        self.today = Some(date);
        if self.value.start.is_none() {
            self.cursor = date;
        }
        self
    }

    /// Changes today (`None` for the Mac's local date), as when the host's day turns over.
    pub fn set_today(&mut self, date: Option<NaiveDate>, cx: &mut Context<Self>) {
        if self.today != date {
            self.today = date;
            cx.notify();
        }
    }

    /// The day marked as today.
    pub fn today_date(&self) -> NaiveDate {
        self.today.unwrap_or_else(today)
    }

    /// The day weeks start on (Sunday unless told otherwise).
    pub fn week_start(mut self, day: Weekday) -> Self {
        self.week_start = day;
        self
    }

    /// Shows the month `date` is in, with the keyboard on `date`, without picking it.
    pub fn show_month(&mut self, date: NaiveDate, cx: &mut Context<Self>) {
        self.cursor = date;
        cx.notify();
    }

    /// The day the keyboard is on (in the month shown).
    pub fn shown(&self) -> NaiveDate {
        self.cursor
    }

    /// Picks a range: the first press its start, the second its end.
    pub fn range(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            range: true,
            ..Self::new(window, cx)
        }
    }

    pub fn value(&self) -> DateValue {
        self.value
    }

    /// Sets the date (and the end of a range) without telling anyone.
    pub fn set_value(
        &mut self,
        start: Option<NaiveDate>,
        end: Option<NaiveDate>,
        cx: &mut Context<Self>,
    ) {
        self.value = DateValue {
            start,
            end: if self.range { end } else { None },
        };
        if let Some(s) = start {
            self.cursor = s;
        }
        cx.notify();
    }

    fn toggle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = !self.open;
        if self.open {
            self.cursor = self.value.start.unwrap_or_else(|| self.today_date());
        }
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn pick(&mut self, day: NaiveDate, cx: &mut Context<Self>) {
        self.cursor = day;
        let v = &mut self.value;
        if self.range {
            match (v.start, v.end) {
                (Some(s), None) => {
                    let (a, b) = if day < s { (day, s) } else { (s, day) };
                    *v = DateValue {
                        start: Some(a),
                        end: Some(b),
                    };
                    self.open = false;
                }
                _ => {
                    *v = DateValue {
                        start: Some(day),
                        end: None,
                    }
                }
            }
        } else {
            *v = DateValue {
                start: Some(day),
                end: None,
            };
            self.open = false;
        }
        cx.emit(DatePickerEvent::Change(self.value));
        cx.notify();
    }

    fn move_cursor(&mut self, days: i64, cx: &mut Context<Self>) {
        self.cursor += Duration::days(days);
        cx.notify();
    }

    fn step(&mut self, days: i64, cx: &mut Context<Self>) {
        if self.open {
            self.cursor += Duration::days(days);
            cx.notify();
        }
    }

    fn label(&self) -> Option<String> {
        let f = |d: NaiveDate| d.format("%b %-d, %Y").to_string();
        match (self.value.start, self.value.end) {
            (Some(s), Some(e)) => Some(format!("{} – {}", f(s), f(e))),
            (Some(s), None) if self.range => Some(format!("{} – …", f(s))),
            (Some(s), None) => Some(f(s)),
            _ => None,
        }
    }
}

/// The focus ring in the host's colour, drawn as given, or the theme's.
fn ring_for(color: Option<Hsla>, cx: &App) -> gpui::BoxShadow {
    let ring = focus_ring(cx);
    match color {
        Some(color) => gpui::BoxShadow { color, ..ring },
        None => ring,
    }
}

fn css(v: f32) -> Rems {
    rems(v / 15.)
}

/// Draws a [`DatePickerState`]: a field like an input, and under it, when open, the month.
#[derive(IntoElement)]
pub struct DatePicker {
    state: Entity<DatePickerState>,
    placeholder: SharedString,
    disabled: bool,
    colors: DatePickerColors,
    style: StyleRefinement,
}

impl DatePicker {
    pub fn new(state: &Entity<DatePickerState>) -> Self {
        Self {
            state: state.clone(),
            placeholder: "Pick a date".into(),
            disabled: false,
            colors: DatePickerColors::default(),
            style: StyleRefinement::default(),
        }
    }

    /// Colours over the theme's, for the field and its month.
    pub fn colors(mut self, colors: DatePickerColors) -> Self {
        self.colors = colors;
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }
}

impl Disableable for DatePicker {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Styled for DatePicker {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for DatePicker {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let s = self.state.clone();
        let (open, label, focus) = {
            let st = s.read(cx);
            (st.open, st.label(), st.focus.clone())
        };
        let id = s.entity_id().as_u64();
        let c = self.colors;
        let ring = ring_for(c.focus_ring, cx);
        let ring_border = c.focus_border.or(c.focus_ring).unwrap_or(theme.ring);
        let muted = c.placeholder.unwrap_or(theme.muted_foreground);
        let fill = c
            .field
            .or_else(|| theme.mode.is_dark().then(|| theme.input.opacity(0.3)));

        let trigger = Press::new(ElementId::NamedInteger("date-picker".into(), id))
            .track_focus(&focus)
            .focus_ring(false)
            .disabled(self.disabled)
            .accessibility_label(
                label
                    .clone()
                    .unwrap_or_else(|| self.placeholder.to_string()),
            )
            .flex()
            .items_center()
            .gap(css(8.))
            .w_full()
            .min_w_0()
            .h(css(36.))
            .px(css(12.))
            .rounded(theme.radius_md())
            .border_1()
            .border_color(c.border.unwrap_or(theme.input))
            .when_some(fill, |d, fill| d.bg(fill))
            .when_some(c.text, |d, text| d.text_color(text))
            .text_size(theme.text.sm)
            .when(self.disabled, |d| d.opacity(0.5))
            .focus_visible(move |st| st.border_color(ring_border).shadow(vec![ring]))
            .child(Icon::new(IconName::Calendar).color(muted))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .text_ellipsis()
                    .whitespace_nowrap()
                    // Clipped to its slot, with room either side for ink past a glyph's advance.
                    .overflow_hidden()
                    .px(css(3.))
                    .mx(css(-3.))
                    .when(label.is_none(), |d| d.text_color(muted))
                    .child(label.unwrap_or_else(|| self.placeholder.to_string())),
            )
            .on_click({
                let s = s.clone();
                move |_, window, cx| s.update(cx, |st, cx| st.toggle(window, cx))
            });

        let mut root = div()
            .id(ElementId::NamedInteger("date-picker-root".into(), id))
            .key_context(CONTEXT)
            .flex()
            .flex_col()
            .min_w_0()
            .on_action(
                window.listener_for(&s, |st: &mut DatePickerState, _: &PrevDay, _, cx| {
                    st.step(-1, cx)
                }),
            )
            .on_action(
                window.listener_for(&s, |st: &mut DatePickerState, _: &NextDay, _, cx| {
                    st.step(1, cx)
                }),
            )
            .on_action(
                window.listener_for(&s, |st: &mut DatePickerState, _: &PrevWeek, _, cx| {
                    st.step(-7, cx)
                }),
            )
            .on_action(
                window.listener_for(&s, |st: &mut DatePickerState, _: &NextWeek, _, cx| {
                    st.step(7, cx)
                }),
            )
            .on_action(
                window.listener_for(&s, |st: &mut DatePickerState, _: &PrevMonth, _, cx| {
                    st.cursor = add_months(st.cursor, -1);
                    cx.notify()
                }),
            )
            .on_action(
                window.listener_for(&s, |st: &mut DatePickerState, _: &NextMonth, _, cx| {
                    st.cursor = add_months(st.cursor, 1);
                    cx.notify()
                }),
            )
            .on_action(
                window.listener_for(&s, |st: &mut DatePickerState, _: &Pick, window, cx| {
                    if st.open {
                        let day = st.cursor;
                        st.pick(day, cx);
                    } else {
                        st.toggle(window, cx);
                    }
                }),
            )
            .on_action(
                window.listener_for(&s, |st: &mut DatePickerState, _: &Close, _, cx| {
                    if st.open {
                        st.open = false;
                        cx.notify();
                    } else {
                        cx.propagate();
                    }
                }),
            )
            .child(
                Popover::new(ElementId::NamedInteger("date-picker-month".into(), id))
                    .open(open)
                    // The keyboard stays on the field, whose keys move through the month.
                    .take_focus(false)
                    .when_some(c.popover, |p, colors| p.colors(colors))
                    .match_trigger_width(false)
                    .on_dismiss({
                        let s = s.clone();
                        move |_, _, cx| {
                            s.update(cx, |st, cx| {
                                st.open = false;
                                cx.notify()
                            })
                        }
                    })
                    .trigger(trigger)
                    .p(css(12.))
                    .when(open, |p| {
                        p.child(month_view(
                            &s,
                            &theme,
                            &MonthOptions {
                                colors: c,
                                ..MonthOptions::default()
                            },
                            cx,
                        ))
                    }),
            );
        gpui::Refineable::refine(root.style(), &self.style);
        root
    }
}

/// A [`DatePickerState`]'s month drawn in place, with no field: for a side panel or a
/// calendar's navigator. It takes the focus with Tab; the arrows move a day or a week, Page Up
/// and Page Down a month, Enter or Space picks. Picks are [`DatePickerEvent::Change`], as from
/// the picker.
#[derive(IntoElement)]
pub struct MonthGrid {
    state: Entity<DatePickerState>,
    options: MonthOptions,
    footer: Option<gpui::AnyElement>,
    style: StyleRefinement,
}

/// A date picker's and a month's colours, each over the theme's when given: for a host with a
/// palette of its own (a light card in a dark app).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DatePickerColors {
    /// The field's fill (the theme's input at 30% in the dark, none in the light).
    pub field: Option<Hsla>,
    /// The field's border (the theme's input).
    pub border: Option<Hsla>,
    /// The field's date (the colour of the text around it).
    pub text: Option<Hsla>,
    /// The placeholder and the calendar icon (the theme's muted foreground).
    pub placeholder: Option<Hsla>,
    /// The keyboard focus ring, drawn as given (the theme's ring at half strength).
    pub focus_ring: Option<Hsla>,
    /// The focused field's border (the focus ring's colour if given, else the theme's ring).
    pub focus_border: Option<Hsla>,
    /// The month's popover.
    pub popover: Option<PopoverColors>,
    /// A picked day's fill and number (the theme's primary and primary foreground).
    pub selected: Option<Hsla>,
    pub selected_text: Option<Hsla>,
    /// The days between a range's ends (the theme's accent).
    pub range: Option<Hsla>,
    /// A day or an arrow under the pointer (the theme's accent).
    pub hover: Option<Hsla>,
    /// Today's number (the day's colour, in semibold, unless given).
    pub today: Option<Hsla>,
    /// A ring round today, as Notion draws it (none unless given). The day the keyboard is on
    /// keeps its own border.
    pub today_ring: Option<Hsla>,
    /// Days outside the month, the weekdays' names and the dots (the theme's muted foreground).
    pub muted: Option<Hsla>,
    /// The border of the day the keyboard is on (the theme's ring at half strength).
    pub cursor: Option<Hsla>,
}

/// How a month is drawn beyond its state: dots under some days, the touch size, the colours.
#[derive(Clone, Default)]
struct MonthOptions {
    dots: Option<Rc<dyn Fn(NaiveDate) -> bool>>,
    touch: bool,
    colors: DatePickerColors,
    day_size: Option<gpui::AbsoluteLength>,
    shown_range: Option<(NaiveDate, NaiveDate)>,
}

impl MonthGrid {
    pub fn new(state: &Entity<DatePickerState>) -> Self {
        Self {
            state: state.clone(),
            options: MonthOptions::default(),
            footer: None,
            style: StyleRefinement::default(),
        }
    }

    /// A day's width and height (36 CSS pixels unless given; at least 44 points with
    /// [`touch`](Self::touch)).
    pub fn day_size(mut self, size: impl Into<gpui::AbsoluteLength>) -> Self {
        self.options.day_size = Some(size.into());
        self
    }

    /// A range drawn for show, as a picked range is: the ends filled, the days between tinted.
    /// Presses still pick as the state does, one day unless it's a range picker: for editing
    /// one end of a range kept elsewhere.
    pub fn shown_range(mut self, start: NaiveDate, end: NaiveDate) -> Self {
        self.options.shown_range = Some((start.min(end), start.max(end)));
        self
    }

    /// Something under the days: Today and Clear buttons, say.
    pub fn footer(mut self, footer: impl IntoElement) -> Self {
        self.footer = Some(footer.into_any_element());
        self
    }

    /// A dot under each day `has` says yes to (a calendar's days with events).
    pub fn dots(mut self, has: impl Fn(NaiveDate) -> bool + 'static) -> Self {
        self.options.dots = Some(Rc::new(has));
        self
    }

    /// Days at least 44 points each way, for a finger.
    pub fn touch(mut self) -> Self {
        self.options.touch = true;
        self
    }

    pub fn colors(mut self, colors: DatePickerColors) -> Self {
        self.options.colors = colors;
        self
    }
}

impl Styled for MonthGrid {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for MonthGrid {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let s = self.state.clone();
        let focus = s.read(cx).focus.clone();
        let ring = ring_for(self.options.colors.focus_ring, cx);
        let mut el = div()
            .id(ElementId::NamedInteger(
                "month-grid".into(),
                s.entity_id().as_u64(),
            ))
            .key_context(GRID)
            .track_focus(&focus)
            .role(Role::Grid)
            .p(css(4.))
            .rounded(theme.radius_md())
            .focus_visible(move |st| st.shadow(vec![ring]))
            .on_action(
                window.listener_for(&s, |st: &mut DatePickerState, _: &PrevDay, _, cx| {
                    st.move_cursor(-1, cx)
                }),
            )
            .on_action(
                window.listener_for(&s, |st: &mut DatePickerState, _: &NextDay, _, cx| {
                    st.move_cursor(1, cx)
                }),
            )
            .on_action(
                window.listener_for(&s, |st: &mut DatePickerState, _: &PrevWeek, _, cx| {
                    st.move_cursor(-7, cx)
                }),
            )
            .on_action(
                window.listener_for(&s, |st: &mut DatePickerState, _: &NextWeek, _, cx| {
                    st.move_cursor(7, cx)
                }),
            )
            .on_action(
                window.listener_for(&s, |st: &mut DatePickerState, _: &PrevMonth, _, cx| {
                    st.cursor = add_months(st.cursor, -1);
                    cx.notify()
                }),
            )
            .on_action(
                window.listener_for(&s, |st: &mut DatePickerState, _: &NextMonth, _, cx| {
                    st.cursor = add_months(st.cursor, 1);
                    cx.notify()
                }),
            )
            .on_action(
                window.listener_for(&s, |st: &mut DatePickerState, _: &Pick, _, cx| {
                    let day = st.cursor;
                    st.pick(day, cx)
                }),
            );
        gpui::Refineable::refine(el.style(), &self.style);
        el.child(month_view(&s, &theme, &self.options, cx))
            .when_some(self.footer, |el, footer| {
                el.child(div().mt(css(8.)).child(footer))
            })
    }
}

/// The month a [`DatePickerState`] is on: its name between the arrows, the weekdays, and six
/// weeks of days to press.
fn month_view(
    s: &Entity<DatePickerState>,
    theme: &Theme,
    options: &MonthOptions,
    cx: &App,
) -> gpui::Div {
    let (cursor, value, week_start, today) = {
        let st = s.read(cx);
        (st.cursor, st.value, st.week_start, st.today_date())
    };
    let month = first_of_month(cursor);
    let start = grid_start(month, week_start);
    let cell: gpui::DefiniteLength = match (options.touch, options.day_size) {
        (true, _) => theme.touch_target.into(),
        (false, Some(size)) => size.into(),
        (false, None) => css(36.).into(),
    };
    // The range drawn: the picked one, else one shown for show.
    let (range_start, range_end) = match (value.start, value.end, options.shown_range) {
        (Some(a), Some(b), _) => (Some(a), Some(b)),
        (_, _, Some((a, b))) => (Some(a), Some(b)),
        _ => (None, None),
    };
    let dots = options.dots.clone();
    let c = options.colors;
    let muted = c.muted.unwrap_or(theme.muted_foreground);
    let selected = c.selected.unwrap_or(theme.primary);
    let selected_text = c.selected_text.unwrap_or(theme.primary_foreground);
    let range = c.range.unwrap_or(theme.accent);
    let hover = c.hover.unwrap_or(theme.accent);
    let cursor_border = c.cursor.unwrap_or(theme.ring.opacity(0.5));
    let day_button = |d: NaiveDate| {
        let in_month = d.month() == month.month();
        // What's picked, for assistive tech; drawn as picked too are a shown range's ends.
        let chosen = value.start == Some(d) || value.end == Some(d);
        let picked = chosen || range_start == Some(d) || range_end == Some(d);
        let between = matches!((range_start, range_end), (Some(a), Some(b)) if d > a && d < b);
        let on = d == cursor;
        let s = s.clone();
        Press::new(ElementId::NamedInteger(
            "date-picker-day".into(),
            d.num_days_from_ce() as u64,
        ))
        .tab_stop(false)
        .focus_ring(false)
        .selected(chosen)
        .accessibility_label(d.format("%A, %B %-d, %Y").to_string())
        .size(cell)
        .flex()
        .items_center()
        .justify_center()
        .rounded(theme.radius_md())
        .text_size(theme.text.sm)
        .when(!in_month, |b| b.text_color(muted))
        .when(between, |b| b.bg(range))
        .when(picked, |b| b.bg(selected).text_color(selected_text))
        .when(!picked, |b| b.hover(move |st| st.bg(hover)))
        .when(on && !picked, |b| b.border_1().border_color(cursor_border))
        .when_some(
            c.today_ring.filter(|_| d == today && !picked && !on),
            |b, ring| b.border_1().border_color(ring),
        )
        .when(d == today && !picked, |b| {
            b.font_weight(gpui::FontWeight::SEMIBOLD)
                .when_some(c.today, |b, today| b.text_color(today))
        })
        .flex_col()
        .debug_selector(move || format!("day-{d}"))
        .child(d.day().to_string())
        .when(dots.as_ref().is_some_and(|has| has(d)), |b| {
            b.child(div().size(css(4.)).rounded_full().bg(if picked {
                selected_text
            } else {
                muted
            }))
        })
        .on_click(move |_, _, cx| s.update(cx, |st, cx| st.pick(d, cx)))
    };
    let nav = |id: &'static str, icon: IconName, label: &'static str, months: i32| {
        let s = s.clone();
        Press::new(ElementId::NamedInteger(
            id.into(),
            id_hash(id) ^ id_hash(label),
        ))
        .tab_stop(false)
        .accessibility_label(label)
        .size(css(28.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(theme.radius_md())
        .hover(move |st| st.bg(hover))
        .child(Icon::new(icon))
        .on_click(move |_, _, cx| {
            s.update(cx, |st, cx| {
                st.cursor = add_months(st.cursor, months);
                cx.notify()
            })
        })
    };
    div()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .mb(css(8.))
                .child(nav(
                    "date-picker-prev",
                    IconName::ChevronLeft,
                    "Previous month",
                    -1,
                ))
                .child(
                    div()
                        .text_size(theme.text.sm)
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(month.format("%B %Y").to_string()),
                )
                .child(nav(
                    "date-picker-next",
                    IconName::ChevronRight,
                    "Next month",
                    1,
                )),
        )
        .child(div().flex().children((0..7).map(|i| {
            let w = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"]
                [((week_start.num_days_from_sunday() + i) % 7) as usize];
            div()
                .w(cell)
                .flex()
                .justify_center()
                .text_size(theme.text.xs)
                .text_color(muted)
                .child(w)
        })))
        .children((0..6).map(|w| {
            div()
                .flex()
                .children((0..7).map(|d| day_button(start + Duration::days(w * 7 + d))))
        }))
}

/// The first day the grid of `month` shows: the week's first day on or before the 1st.
fn grid_start(month: NaiveDate, week_start: Weekday) -> NaiveDate {
    let first = first_of_month(month);
    let lead = (first.weekday().num_days_from_sunday() + 7 - week_start.num_days_from_sunday()) % 7;
    first - Duration::days(lead as i64)
}

fn id_hash(s: &str) -> u64 {
    s.bytes().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ b as u64).wrapping_mul(0x100000001b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_grid_starts_on_the_weeks_first_day() {
        let d = |y, m, d| NaiveDate::from_ymd_opt(y, m, d).unwrap();
        // November 2026 starts on a Sunday.
        assert_eq!(grid_start(d(2026, 11, 15), Weekday::Sun), d(2026, 11, 1));
        assert_eq!(grid_start(d(2026, 11, 15), Weekday::Mon), d(2026, 10, 26));
        // June 2026 starts on a Monday.
        assert_eq!(grid_start(d(2026, 6, 3), Weekday::Mon), d(2026, 6, 1));
        assert_eq!(grid_start(d(2026, 6, 3), Weekday::Sun), d(2026, 5, 31));
    }

    #[test]
    fn months_keep_to_their_last_day() {
        let d = |y, m, d| NaiveDate::from_ymd_opt(y, m, d).unwrap();
        assert_eq!(add_months(d(2026, 1, 31), 1), d(2026, 2, 28));
        assert_eq!(add_months(d(2024, 1, 31), 1), d(2024, 2, 29));
        assert_eq!(add_months(d(2026, 1, 15), -1), d(2025, 12, 15));
        assert_eq!(add_months(d(2026, 12, 1), 1), d(2027, 1, 1));
    }
}
