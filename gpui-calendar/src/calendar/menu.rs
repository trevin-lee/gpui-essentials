//! The menu a right-click opens on an event or on empty time, as in Google Calendar, and that
//! Shift-F10 opens on what's selected: Open, Edit and Delete for an event (Open alone for one
//! that can't be changed), New event for empty time. Each does what its key does: Enter, `e`,
//! Delete and `n`, and a repeating event's Delete still asks which occurrences first. Where the
//! event or the day is behind a "+N" (side by side, a month day's "+N more", or the all-day
//! row's), "Show all" opens that list, so the keyboard reaches what a click on the "+N" does.

use chrono::NaiveDate;
use gpui::*;
use gpui_core::{MenuSelect, Popover, PopupMenu, PopupMenuItem};

use super::{Calendar, CalendarEvent, CalendarView};
use crate::model::*;
use crate::paint::{self, Target};
use crate::time;

/// The open menu: what it's for, and the box it hangs from.
#[derive(Clone, Debug)]
pub(super) struct Menu {
    on: MenuOn,
    anchor: Bounds<Pixels>,
}

/// The list behind a "+N" that "Show all" opens.
#[derive(Clone, Debug, PartialEq)]
enum Behind {
    /// The side-by-side events that didn't fit, with this one among them.
    Crowd(EventId),
    /// A month day's events, behind its "+N more".
    Day(NaiveDate),
    /// The all-day row's, behind its "+N".
    AllDay,
}

#[derive(Clone, Debug, PartialEq)]
enum MenuOn {
    Event(EventId),
    /// The selected empty slot (a time on the grid, or a day).
    Slot,
}

impl Calendar {
    /// A right-click at `p`: selects what's there and opens its menu there. Nothing on the
    /// agenda's empty space, the header or a "+N", and nothing while a drag or the repeating
    /// question is on.
    pub(super) fn right_click(&mut self, p: Point<Pixels>, cx: &mut Context<Self>) {
        if self.asking.is_some() || self.drag.is_some() {
            return;
        }
        let Some(geo) = self.shared.geo.get() else {
            return;
        };
        let on = match paint::target_at(&geo, &self.shared, p) {
            Target::Event { id, .. } | Target::Link { id, .. } => {
                self.select(Some(Selection::Event(id.clone())), cx);
                MenuOn::Event(id)
            }
            Target::Grid { date, minute } if self.new_event_calendar().is_some() => {
                let slot = self.options.slot_minutes.max(1) as f32;
                let minute = time::snap_floor(minute, slot) as u32;
                self.select(Some(Selection::Slot { date, minute }), cx);
                MenuOn::Slot
            }
            Target::Day { date, .. } if self.new_event_calendar().is_some() => {
                self.select(Some(Selection::Slot { date, minute: 0 }), cx);
                MenuOn::Slot
            }
            _ => return,
        };
        self.close_panel(cx);
        self.crowd = None;
        self.menu = Some(Menu {
            on,
            anchor: Bounds::new(p, size(px(1.), px(1.))),
        });
        cx.notify();
    }

    /// Shift-F10: the menu of what's selected, hung from it.
    pub(super) fn show_menu(&mut self, cx: &mut Context<Self>) {
        if self.asking.is_some() || self.drag.is_some() {
            return;
        }
        let center = || {
            self.shared.geo.get().map_or(Bounds::default(), |g| {
                Bounds::new(g.bounds.center(), size(px(1.), px(1.)))
            })
        };
        let (on, anchor) = match self.selection.clone() {
            Some(Selection::Event(id)) => {
                // One that didn't fit hangs it from its "+N".
                let at = self
                    .event_bounds(&id)
                    .or_else(|| self.hidden_bounds(&id))
                    .unwrap_or_else(center);
                (MenuOn::Event(id), at)
            }
            Some(Selection::Slot { date, minute })
                if self.new_event_calendar().is_some() || self.behind_day(date).is_some() =>
            {
                let at = self
                    .point_at(date, minute as f32)
                    .map_or_else(center, |p| Bounds::new(p, size(px(1.), px(1.))));
                (MenuOn::Slot, at)
            }
            _ => return,
        };
        self.close_panel(cx);
        self.menu = Some(Menu { on, anchor });
        cx.notify();
    }

    pub(super) fn close_menu(&mut self, cx: &mut Context<Self>) {
        if self.menu.take().is_some() {
            cx.notify();
        }
    }

    /// Whether the menu is open, and the ids of its entries (for tests and hosts' own checks).
    pub fn menu_items(&self) -> Option<Vec<&'static str>> {
        let menu = self.menu.as_ref()?;
        let mut items = match &menu.on {
            MenuOn::Event(id) if self.editable(id).is_some() => vec!["open", "edit", "delete"],
            MenuOn::Event(_) => vec!["open"],
            MenuOn::Slot if self.new_event_calendar().is_some() => vec!["new"],
            MenuOn::Slot => vec![],
        };
        if self.behind(&menu.on).is_some() {
            items.push("all");
        }
        Some(items)
    }

    /// The list behind a "+N" that what the menu is on belongs to, and how many it holds.
    fn behind(&self, on: &MenuOn) -> Option<(Behind, usize)> {
        let date = match (on, &self.selection) {
            (_, Some(Selection::Slot { date, .. })) => Some(*date),
            _ => self.selection_place().map(|(d, _)| d),
        };
        if let MenuOn::Event(id) = on {
            if let Some(n) = self
                .shared
                .crowds
                .borrow()
                .iter()
                .find(|(_, ids)| ids.contains(id))
                .map(|(_, ids)| ids.len())
            {
                return Some((Behind::Crowd(id.clone()), n));
            }
            // An all-day event the row had no room for.
            let all_day = self.event(id).is_some_and(|e| e.time.is_all_day());
            if all_day
                && self.view.is_time_grid()
                && !self.all_day_expanded
                && self.event_bounds(id).is_none()
                && !self.shared.expand.borrow().is_empty()
            {
                return Some((Behind::AllDay, 0));
            }
        }
        date.and_then(|d| self.behind_day(d))
    }

    /// A month day's "+N more" list, if the last frame drew one for `date`: its day, and how
    /// many events it lists.
    fn behind_day(&self, date: NaiveDate) -> Option<(Behind, usize)> {
        if self.view != CalendarView::Month
            || !self.shared.mores.borrow().iter().any(|(_, d)| *d == date)
        {
            return None;
        }
        let n = self
            .events_on(date)
            .into_iter()
            .filter(|e| self.is_shown(e) && self.is_found(e))
            .count();
        Some((Behind::Day(date), n))
    }

    /// Picks an entry of the open menu by its id, as a click on it does.
    pub fn pick_menu_item(&mut self, item: &str, cx: &mut Context<Self>) {
        let Some(menu) = self.menu.take() else {
            return;
        };
        match (item, menu.on) {
            ("open", MenuOn::Event(id)) => self.open_event(id, cx),
            ("edit", MenuOn::Event(id)) => {
                if let Some(event) = self.editable(&id).map(Event::to_ref) {
                    cx.emit(CalendarEvent::Edit(event));
                }
            }
            ("delete", MenuOn::Event(id)) => {
                self.select(Some(Selection::Event(id)), cx);
                self.delete_selected(cx);
            }
            ("new", MenuOn::Slot) => self.new_event(cx),
            ("all", on) => match self.behind(&on).map(|(b, _)| b) {
                Some(Behind::Crowd(id)) => {
                    if self.open_hidden(&id, cx) {
                        self.highlight_row(&id);
                    }
                }
                Some(Behind::Day(date)) => {
                    if self.open_more(date, cx)
                        && let MenuOn::Event(id) = on
                    {
                        self.highlight_row(&id);
                    }
                }
                Some(Behind::AllDay) => self.all_day_expanded = true,
                None => {}
            },
            _ => {}
        }
        cx.notify();
    }

    pub(super) fn render_menu(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu = self.menu.as_ref()?;
        let items = self.menu_items()?;
        let all: SharedString = match self.behind(&menu.on) {
            Some((Behind::Crowd(_), n)) => format!("Show {n} more events").into(),
            Some((Behind::Day(_), n)) => format!("Show all {n} events").into(),
            _ => "Show all all-day events".into(),
        };
        let entry = |id: &'static str| match id {
            "open" => PopupMenuItem::new("open", "Open").shortcut("↩"),
            "edit" => PopupMenuItem::new("edit", "Edit").shortcut("E"),
            "delete" => PopupMenuItem::new("delete", "Delete").shortcut("⌫"),
            "all" => PopupMenuItem::new("all", all.clone()),
            _ => PopupMenuItem::new("new", "New event").shortcut("N"),
        };
        let (pick, dismiss) = (cx.entity().downgrade(), cx.entity().downgrade());
        let list = PopupMenu::new("calendar-menu")
            .items(items.into_iter().map(entry))
            .on_select(move |pick_: &MenuSelect, window, cx| {
                pick.update(cx, |c, cx| {
                    c.pick_menu_item(&pick_.id, cx);
                    c.focus.focus(window, cx);
                })
                .ok();
            });
        Some(
            Popover::new("calendar-menu")
                .open(true)
                .anchor(menu.anchor)
                .accessibility_label(match menu.on {
                    MenuOn::Event(_) => "Event actions",
                    MenuOn::Slot => "Actions",
                })
                .on_dismiss(move |_, window, cx| {
                    dismiss
                        .update(cx, |c, cx| {
                            c.close_menu(cx);
                            c.focus.focus(window, cx);
                        })
                        .ok();
                })
                .p(rems(0.25))
                .child(list)
                .into_any_element(),
        )
    }
}
