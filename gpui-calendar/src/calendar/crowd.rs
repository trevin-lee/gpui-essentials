//! The list behind "+N": the side-by-side events that didn't fit their day's width, in a
//! popover beside the "+N" (a sheet on touch). Each row is the event's colour, title and time;
//! a click opens the event, as a double click on its chip does, and a drag carries it into the
//! grid as a drag of the chip would.

use gpui::*;
use gpui_core::{DismissReason, Popover, PopoverAnchor, Press, Sheet, Theme};

use super::Calendar;
use crate::model::*;
use crate::paint::{Area, Target};
use crate::time;

/// How wide the list's rows are, in CSS pixels.
const LIST_WIDTH: f32 = 260.;

/// The list open from a "+N": the events it stands for, and where the "+N" was drawn.
#[derive(Clone, Debug)]
pub(crate) struct Crowd {
    pub ids: Vec<EventId>,
    pub anchor: Bounds<Pixels>,
}

impl Calendar {
    pub(crate) fn open_crowd(
        &mut self,
        ids: Vec<EventId>,
        anchor: Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.crowd = Some(Crowd { ids, anchor });
        cx.notify();
    }

    pub(crate) fn close_crowd(&mut self, cx: &mut Context<Self>) {
        if self.crowd.take().is_some() {
            cx.notify();
        }
    }

    /// Where the "+N" standing for an event that didn't fit beside the others was drawn in the
    /// last frame; none if the event was drawn (or isn't on screen).
    pub fn hidden_bounds(&self, id: &str) -> Option<Bounds<Pixels>> {
        self.shared
            .crowds
            .borrow()
            .iter()
            .find(|(_, ids)| ids.iter().any(|i| i.as_ref() == id))
            .map(|(b, _)| *b)
    }

    /// Opens the list behind the "+N" that stands for an event that didn't fit, as a click on
    /// the "+N" does. False if the last frame drew the event, or nothing for it.
    pub fn open_hidden(&mut self, id: &str, cx: &mut Context<Self>) -> bool {
        let found = self
            .shared
            .crowds
            .borrow()
            .iter()
            .find(|(_, ids)| ids.iter().any(|i| i.as_ref() == id))
            .cloned();
        match found {
            Some((anchor, ids)) => {
                self.open_crowd(ids, anchor, cx);
                true
            }
            None => false,
        }
    }

    /// The events listed behind a "+N", if one is open.
    pub fn crowd_events(&self) -> Option<&[EventId]> {
        self.crowd.as_ref().map(|c| c.ids.as_slice())
    }

    /// A press on a row: the event is selected, and if the press then moves it's a drag of the
    /// event, held by its start, as if its chip had been pressed there.
    fn press_crowd_row(&mut self, id: EventId, at: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(e) = self.event(&id) else { return };
        let read_only = self.is_read_only(e);
        let start = match e.time {
            EventTime::Timed { start, .. } => Some(time::local(start, self.options.time_zone)),
            EventTime::AllDay { .. } => None,
        };
        self.select(Some(Selection::Event(id.clone())), cx);
        let held = start
            .and_then(|(date, minute)| self.point_at(date, minute))
            .unwrap_or(at);
        self.drag = Some(super::Drag::Press {
            at: held,
            target: Target::Event {
                id,
                area: Area::Grid,
                resize: false,
                read_only,
            },
        });
        cx.notify();
    }

    pub(super) fn render_crowd(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let crowd = self.crowd.clone()?;
        let theme = Theme::global(cx);
        let c = theme.colors;
        let touch = self.options.touch;
        let this = cx.entity().downgrade();
        let tz = self.options.time_zone;
        let hour12 = self.options.hour12;
        let rows = crowd.ids.iter().enumerate().filter_map(|(i, id)| {
            let e = self.event(id)?;
            let color: Hsla = rgb(self.color_of(e)).into();
            let when = match e.time {
                EventTime::Timed { start, end } => time::format_range(
                    start.with_timezone(&tz).time(),
                    end.with_timezone(&tz).time(),
                    hour12,
                ),
                EventTime::AllDay { .. } => "All day".into(),
            };
            let (press, open) = (this.clone(), this.clone());
            let (id_press, id_open) = (id.clone(), id.clone());
            let label = SharedString::from(self.describe_crowd_row(e, &when));
            Some(
                Press::new(("crowd-row", i))
                    .accessibility_label(label)
                    .focus_on_press(false)
                    .flex()
                    .items_center()
                    .gap(rems(8. / 15.))
                    .w_full()
                    .min_h(if touch {
                        DefiniteLength::from(px(44.))
                    } else {
                        DefiniteLength::from(rems(30. / 15.))
                    })
                    .px(rems(8. / 15.))
                    .rounded(rems(6. / 15.))
                    .text_size(rems(13. / 15.))
                    .hover(|d| d.bg(c.accent))
                    .debug_selector(move || format!("crowd-row-{i}"))
                    .on_mouse_down(MouseButton::Left, move |e, _, cx| {
                        press
                            .update(cx, |c, cx| {
                                c.press_crowd_row(id_press.clone(), e.position, cx)
                            })
                            .ok();
                    })
                    .on_click(move |_, window, cx| {
                        open.update(cx, |c, cx| {
                            c.drag = None;
                            c.crowd = None;
                            c.open_event(id_open.clone(), cx);
                            c.focus.focus(window, cx);
                        })
                        .ok();
                    })
                    .child(
                        div()
                            .flex_none()
                            .size(rems(8. / 15.))
                            .rounded_full()
                            .bg(color),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_weight(FontWeight::MEDIUM)
                            .child(e.title.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_color(c.muted_foreground)
                            .child(SharedString::from(when)),
                    ),
            )
        });
        let count = crowd.ids.len();
        let heading = div()
            .px(rems(8. / 15.))
            .pt(rems(4. / 15.))
            .pb(rems(4. / 15.))
            .truncate()
            .text_size(rems(12. / 15.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(c.muted_foreground)
            .child(SharedString::from(if count == 1 {
                "1 more event".to_string()
            } else {
                format!("{count} more events")
            }));
        let list = div()
            .flex()
            .flex_col()
            .debug_selector(|| "crowd-list".into())
            .child(heading)
            .children(rows);
        // Where the "+N" is this frame, if it's still there; else where it was. The list
        // hangs beside it (it's as tall as the events it stands for), or below or above it
        // where neither side has room.
        let anchor = self
            .shared
            .crowds
            .borrow()
            .iter()
            .find(|(_, ids)| ids == &crowd.ids)
            .map_or(crowd.anchor, |(b, _)| *b);
        // A press on the "+N" itself is the calendar's to toggle the list with, not one outside.
        let dismiss = {
            let this = this.clone();
            move |reason: &DismissReason, window: &mut Window, cx: &mut App| {
                if *reason == DismissReason::ClickOutside
                    && anchor.contains(&window.mouse_position())
                {
                    return;
                }
                this.update(cx, |c, cx| c.close_crowd(cx)).ok();
            }
        };
        let scale = f32::from(window.rem_size()) / 15.;
        let pad = 4.;
        let width = px((LIST_WIDTH + pad * 2.) * scale + 2.);
        let tall = px(((count as f32 * 30. + 30.).min(360.) + pad * 2.) * scale + 2.);
        let gap = px(4. * scale);
        let anchor: PopoverAnchor =
            match super::panel::beside(anchor, width, tall, window.viewport_size(), gap) {
                Some(p) => p.into(),
                None => Bounds::new(
                    anchor.origin,
                    size(anchor.size.width.min(px(1.)), anchor.size.height),
                )
                .into(),
            };
        Some(if touch {
            Sheet::new("calendar-crowd")
                .open(true)
                .take_focus(false)
                .accessibility_label("More events")
                .on_dismiss(dismiss)
                .child(list)
                .into_any_element()
        } else {
            Popover::new("calendar-crowd")
                .open(true)
                .anchor(anchor)
                .take_focus(false)
                .accessibility_label("More events")
                .on_dismiss(dismiss)
                .p(rems(pad / 15.))
                .child(div().w(rems(LIST_WIDTH / 15.)).child(list))
                .into_any_element()
        })
    }

    /// "Vendor call, 2:30 to 3:30 PM, Work calendar": a row of the list, said.
    fn describe_crowd_row(&self, e: &Event, when: &str) -> String {
        let mut out = format!("{}, {}", e.title, when.replace(" – ", " to "));
        if let Some(c) = self.calendar(&e.calendar) {
            out.push_str(&format!(", {} calendar", c.name));
        }
        out
    }
}
