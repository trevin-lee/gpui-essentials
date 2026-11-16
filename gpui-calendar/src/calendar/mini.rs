//! The mini month: gpui-core's `MonthGrid` to jump to a date, beside the calendar when it's
//! wide and behind the title when it's narrow. It shows the calendar's period, dots the days
//! the host's events fall on (never fetching anything), and a pick goes to that date.

use std::collections::HashSet;

use chrono::{Datelike, Duration, NaiveDate};
use gpui::*;
use gpui_core::{
    DatePickerEvent, DatePickerState, DismissReason, MonthGrid, Popover, Sheet, Theme,
};

use super::Calendar;
use crate::time;

/// How wide the side with the mini month is, in CSS pixels.
const SIDE: f32 = 248.;

impl Calendar {
    /// Opens or closes the mini month behind the title, or, where it's beside the calendar,
    /// gives it the keyboard (`g`, or a click on the narrow header's title).
    pub fn toggle_mini_month(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.options.mini_month {
            return;
        }
        if self.mini_beside(window) {
            if let Some(state) = &self.mini {
                window.focus(&state.focus_handle(cx), cx);
            }
            return;
        }
        self.mini_open = !self.mini_open;
        cx.notify();
    }

    /// Whether the mini month has room beside the calendar: the whole width (the days, and the
    /// side when it's there) must leave the days as wide as a calendar that isn't narrow.
    /// Measured on the whole, so showing the side can't make it not fit, and back.
    fn mini_beside(&self, window: &Window) -> bool {
        if self.options.touch {
            return false;
        }
        let scale = f32::from(window.rem_size()) / 15.;
        let days = self
            .shared
            .geo
            .get()
            .map_or(window.viewport_size().width, |g| g.bounds.size.width);
        let whole = days
            + if self.mini_side {
                px(SIDE * scale)
            } else {
                px(0.)
            };
        whole >= px((crate::paint::NARROW + SIDE) * scale)
    }

    /// Whether the mini month is open behind the title.
    pub fn is_mini_month_open(&self) -> bool {
        self.mini_open
    }

    /// The mini month's state, made the first time and remade when the week's first day
    /// changes; shown on the calendar's date, with that date marked.
    fn mini_state(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<DatePickerState> {
        let week_start = self.options.week_start;
        let remake = self.mini_shown.is_none_or(|(_, start)| start != week_start);
        if remake || self.mini.is_none() {
            let state = cx.new(|cx| DatePickerState::new(window, cx).week_start(week_start));
            self._mini_sub = Some(cx.subscribe(&state, |c, _, event, cx| {
                let DatePickerEvent::Change(value) = event;
                if let Some(date) = value.start() {
                    c.mini_open = false;
                    c.go_to(date, cx);
                }
            }));
            self.mini = Some(state);
            self.mini_shown = None;
        }
        let state = self.mini.clone().expect("made above");
        // In step with the calendar's period, without anyone being told.
        if self.mini_shown != Some((self.anchor, week_start)) {
            let anchor = self.anchor;
            state.update(cx, |s, cx| s.set_value(Some(anchor), None, cx));
            self.mini_shown = Some((anchor, week_start));
        }
        state
    }

    /// The days in and around the month the mini month shows that the host's shown events
    /// fall on.
    pub(crate) fn busy_days(&self, shown: NaiveDate) -> HashSet<NaiveDate> {
        let tz = self.options.time_zone;
        let first = shown.with_day(1).unwrap_or(shown) - Duration::days(7);
        let last = first + Duration::days(7 * 7);
        let mut out = HashSet::new();
        let (start, end) = (time::day_start(first, tz), time::day_start(last, tz));
        for e in self.events_in(start, end) {
            let (a, b) = time::event_days(&e.time, tz);
            let mut d = a.max(first);
            while d <= b.min(last) {
                out.insert(d);
                d += Duration::days(1);
            }
        }
        out
    }

    /// The mini month, drawn: beside the calendar (the first element), or open behind the
    /// title (the second).
    pub(super) fn render_mini(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Option<AnyElement>, Option<AnyElement>) {
        if !self.options.mini_month {
            return (None, None);
        }
        let beside = self.mini_beside(window);
        self.mini_side = beside;
        if !beside && !self.mini_open {
            return (None, None);
        }
        let state = self.mini_state(window, cx);
        let busy = self.busy_days(state.read(cx).shown());
        let mut grid = MonthGrid::new(&state).dots(move |d| busy.contains(&d));
        if self.options.touch {
            grid = grid.touch();
        }
        let c = Theme::global(cx).colors;
        if beside {
            let aside = div()
                // After the header, before the days, for Tab.
                .tab_group()
                .tab_index(1)
                .flex_none()
                .w(rems(SIDE / 15.))
                .h_full()
                .p(rems(12. / 15.))
                .border_r_1()
                .border_color(c.border)
                .debug_selector(|| "calendar-mini".into())
                .child(grid)
                .into_any_element();
            return (Some(aside), None);
        }
        let this = cx.entity().downgrade();
        let dismiss = move |_: &DismissReason, _: &mut Window, cx: &mut App| {
            this.update(cx, |c, cx| {
                c.mini_open = false;
                cx.notify();
            })
            .ok();
        };
        let content = div()
            .p(rems(4. / 15.))
            .debug_selector(|| "calendar-mini".into())
            .child(grid);
        let open = if self.options.touch {
            Sheet::new("calendar-mini-sheet")
                .accessibility_label("Go to a date")
                .open(true)
                .take_focus(false)
                .on_dismiss(dismiss)
                .child(content)
                .into_any_element()
        } else {
            // Under the header, at its left: where the title is.
            let anchor = self.shared.geo.get().map_or(Bounds::default(), |g| {
                Bounds::new(
                    point(g.bounds.left() + px(12.), g.bounds.top()),
                    size(px(1.), px(1.)),
                )
            });
            Popover::new("calendar-mini-popover")
                .accessibility_label("Go to a date")
                .open(true)
                .anchor(anchor)
                .take_focus(false)
                .max_height(rems(30.))
                .on_dismiss(dismiss)
                .p(rems(4. / 15.))
                .child(content)
                .into_any_element()
        };
        (None, Some(open))
    }
}
