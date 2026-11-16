//! The event panel: an event's details (time, calendar, place, notes, links, guests) and what
//! can be done with it, as a popover beside the event on the Mac and a sheet on touch. Every
//! action goes to the host as a [`CalendarEvent`]; the panel changes nothing itself.

use chrono::{Datelike, Duration};
use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_core::{
    Button, ButtonGroup, ButtonVariants as _, ControlSize, DismissReason, Icon, IconName, Popover,
    PopoverAnchor, Press, Selectable as _, Sheet, Sizable as _, Theme,
};

use super::{Calendar, CalendarEvent, EditKind};
use crate::model::*;
use crate::time;

/// The height of a line of the panel's text, and of its title's, in CSS pixels: what its icons
/// centre on.
const LINE: f32 = 20.;
const TITLE_LINE: f32 = 22.;

/// How wide the panel's content is, in CSS pixels.
const WIDTH: f32 = 284.;

/// Where a panel `width` wide and about `tall` high hangs beside `event`: right of it, else left
/// of it, top-aligned with it and moved up as far as it must to end inside the window (never
/// above its top). None where neither side has room. The point is for gpui-core's `Popover`,
/// which puts the panel `gap` below it.
fn beside(
    event: Bounds<Pixels>,
    width: Pixels,
    tall: Pixels,
    view: Size<Pixels>,
    gap: Pixels,
) -> Option<Point<Pixels>> {
    let (space, edge) = (px(6.), px(8.));
    let top = event.top().min(view.height - edge - tall).max(edge);
    let x = if view.width - event.right() - space - edge >= width {
        event.right() + space
    } else if event.left() - space - edge >= width {
        event.left() - space - width
    } else {
        return None;
    };
    Some(point(x, top - gap))
}

impl Calendar {
    /// When an event happens, in words: "Wednesday, November 18 · 11 AM – 12 PM".
    fn when(&self, e: &Event) -> String {
        let tz = self.options.time_zone;
        let hour12 = self.options.hour12;
        let day = |d: chrono::NaiveDate| {
            format!(
                "{}, {} {}",
                time::weekday_name(d.weekday(), false),
                time::month_name(d.month()),
                d.day()
            )
        };
        let short = |d: chrono::NaiveDate| {
            format!(
                "{}, {} {}",
                time::weekday_name(d.weekday(), true),
                time::month_short(d.month()),
                d.day()
            )
        };
        match e.time {
            EventTime::AllDay { start, end } => {
                let last = end - Duration::days(1);
                if last <= start {
                    format!("{} · All day", day(start))
                } else {
                    format!("{} – {} · All day", short(start), short(last))
                }
            }
            EventTime::Timed { start, end } => {
                let (s, f) = (start.with_timezone(&tz), end.with_timezone(&tz));
                if s.date_naive() == f.date_naive() || f == s {
                    format!(
                        "{} · {}",
                        day(s.date_naive()),
                        time::format_range(s.time(), f.time(), hour12)
                    )
                } else {
                    format!(
                        "{}, {} – {}, {}",
                        short(s.date_naive()),
                        time::format_time(s.time(), hour12),
                        short(f.date_naive()),
                        time::format_time(f.time(), hour12)
                    )
                }
            }
        }
    }

    /// Text with what the find query found in it marked, as the grid marks it.
    fn marked(&self, text: SharedString, cx: &App) -> StyledText {
        let ranges = self
            .query
            .as_ref()
            .map(|q| q.ranges(&text))
            .unwrap_or_default();
        let colors = self
            .colors
            .unwrap_or_else(|| crate::theme::CalendarColors::from_theme(cx));
        let style = HighlightStyle {
            background_color: Some(colors.find.opacity(0.38)),
            ..Default::default()
        };
        StyledText::new(text).with_highlights(ranges.into_iter().map(|r| (r, style)))
    }

    /// The panel for the event it's open on, if any.
    pub(super) fn render_panel(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let panel = self.panel.as_ref()?;
        let e = self.event(&panel.id)?.clone();
        let theme = Theme::global(cx);
        let c = theme.colors;
        let touch = self.options.touch;
        // A narrow calendar has no room beside an event: the panel is a sheet there too.
        let sheet = touch || self.is_narrow(window);
        let view = window.viewport_size();
        let scale = f32::from(window.rem_size()) / 15.;
        // The body scrolls within the window, between the title and the footer.
        let body_max = (view.height * if sheet { 0.6 } else { 0.85 } - px(190.))
            .min(px(380. * scale))
            .max(px(80.));
        let row_h = if touch { px(44.) } else { px(28.) };
        let editable = self.editable(&e.id).is_some();
        let color: Hsla = rgb(self.color_of(&e)).into();
        let this = cx.entity().downgrade();
        let act = |f: fn(&mut Calendar, &mut Context<Calendar>)| {
            let this = this.clone();
            move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                this.update(cx, |c, cx| {
                    f(c, cx);
                    // The keyboard stays with the calendar, whatever the panel did.
                    c.focus.focus(window, cx);
                })
                .ok();
            }
        };
        let muted = c.muted_foreground;
        // A row: its icon centred on the row's first line, `first` tall (a line of text, or a
        // pressable row's full height).
        let line = |key: &'static str, icon: IconName, first: DefiniteLength, child: AnyElement| {
            div()
                .flex()
                .items_start()
                .gap(rems(8. / 15.))
                .min_w_0()
                .child(
                    div()
                        .flex_none()
                        .flex()
                        .items_center()
                        .h(first)
                        .text_size(rems(14. / 15.))
                        .debug_selector(move || format!("panel-icon-{key}"))
                        .child(Icon::new(icon).color(muted)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .debug_selector(move || format!("panel-first-{key}"))
                        .child(child),
                )
        };
        // One line of the panel's text.
        let text_line: DefiniteLength = rems(LINE / 15.).into();
        let row_line: DefiniteLength = row_h.into();

        // The title, and the close button.
        let title_line = px(TITLE_LINE * scale);
        let close_size = if touch { px(44.) } else { px(26.) };
        let close = Press::new("panel-close")
            .accessibility_label("Close")
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(close_size)
            // Its middle on the title's first line, however tall the target.
            .my((title_line - close_size) / 2.)
            .rounded(rems(6. / 15.))
            .hover(|d| d.bg(c.accent))
            .debug_selector(|| "panel-close".into())
            .on_click(act(|c, cx| c.close_panel(cx)))
            .child(
                div()
                    .text_size(rems(14. / 15.))
                    .child(Icon::new(IconName::X).color(muted)),
            );
        let head = div()
            .flex()
            .items_start()
            .gap(rems(8. / 15.))
            .child(
                div()
                    .flex_none()
                    .mt(rems((TITLE_LINE - 12.) / 2. / 15.))
                    .size(rems(12. / 15.))
                    .rounded(rems(3. / 15.))
                    .bg(color),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(rems(16. / 15.))
                    .line_height(rems(TITLE_LINE / 15.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .line_clamp(3)
                    .debug_selector(|| "panel-title".into())
                    .child(self.marked(
                        if e.title.is_empty() {
                            SharedString::from("(No title)")
                        } else {
                            e.title.clone()
                        },
                        cx,
                    )),
            )
            .child(close);

        let mut body = div().flex().flex_col().gap(rems(10. / 15.));
        // The answer and the actions stay in view below the body, which scrolls.
        let mut footer = div().flex().flex_col().gap(rems(10. / 15.));
        // When, and whether it repeats.
        body = body.child(line(
            "when",
            IconName::Clock,
            text_line,
            div()
                .flex()
                .flex_col()
                .child(div().child(self.when(&e)))
                .when(e.series.is_some(), |d| {
                    d.child(div().text_color(muted).child("Repeats"))
                })
                .into_any_element(),
        ));
        // The calendar, and the others it can move to.
        let source = self.calendar(&e.calendar).cloned();
        let others: Vec<CalendarSource> = self
            .calendars()
            .iter()
            .filter(|s| s.id != e.calendar && !s.read_only && s.visible)
            .cloned()
            .collect();
        let dot = |rgb_: u32| {
            div()
                .flex_none()
                .size(rems(9. / 15.))
                .rounded_full()
                .bg(rgb(rgb_))
        };
        let cal_name: SharedString = source
            .as_ref()
            .map_or(e.calendar.clone(), |s| s.name.clone());
        let can_move = editable && !others.is_empty();
        let mut cal = div().flex().flex_col().gap(rems(2. / 15.)).child(
            div()
                .flex()
                .items_center()
                .gap(rems(8. / 15.))
                .min_h(row_h)
                .child(dot(self.color_of(&e)))
                .child(div().flex_1().min_w_0().truncate().child(cal_name))
                .when(can_move, |d| {
                    d.child(
                        Press::new("panel-change-calendar")
                            .debug_selector(|| "panel-change".into())
                            .flex_none()
                            .flex()
                            .items_center()
                            .min_h(row_h)
                            .px(rems(8. / 15.))
                            .rounded(rems(6. / 15.))
                            .text_color(muted)
                            .hover(|d| d.bg(c.accent))
                            .on_click(act(|c, cx| {
                                if let Some(p) = &mut c.panel {
                                    p.picking_calendar = !p.picking_calendar;
                                    cx.notify();
                                }
                            }))
                            .child(if panel.picking_calendar {
                                "Cancel"
                            } else {
                                "Change"
                            }),
                    )
                }),
        );
        if can_move && panel.picking_calendar {
            for (i, other) in others.into_iter().enumerate() {
                let this = this.clone();
                let id = other.id.clone();
                cal = cal.child(
                    Press::new(("panel-calendar", i))
                        .flex()
                        .items_center()
                        .gap(rems(8. / 15.))
                        .min_h(row_h)
                        .px(rems(6. / 15.))
                        .rounded(rems(6. / 15.))
                        .hover(|d| d.bg(c.accent))
                        .debug_selector(move || format!("panel-calendar-{i}"))
                        .on_click(move |_, window, cx| {
                            let id = id.clone();
                            this.update(cx, |c, cx| {
                                c.panel_edit(EditKind::ChangeCalendar(id), cx);
                                c.focus.focus(window, cx);
                            })
                            .ok();
                        })
                        .child(dot(other.color))
                        .child(div().min_w_0().truncate().child(other.name)),
                );
            }
        }
        body = body.child(line(
            "calendar",
            IconName::Calendar,
            row_line,
            cal.into_any_element(),
        ));
        if let Some(place) = &e.location {
            body = body.child(line(
                "place",
                IconName::MapPin,
                text_line,
                div().child(place.clone()).into_any_element(),
            ));
        }
        if let Some(notes) = &e.notes {
            body = body.child(line(
                "notes",
                IconName::TextAlignStart,
                text_line,
                div()
                    .text_color(c.popover_foreground.opacity(0.9))
                    .child(self.marked(notes.clone(), cx))
                    .into_any_element(),
            ));
        }
        // Links, each opening what it points at.
        if !e.links.is_empty() {
            let mut links = div().flex().flex_col();
            for (i, link) in e.links.iter().enumerate() {
                let this = this.clone();
                let r = e.to_ref();
                let l = link.clone();
                let icon = match link.kind {
                    LinkKind::Doc => IconName::FileText,
                    LinkKind::Agent => IconName::Bot,
                };
                links = links.child(
                    Press::new(("panel-link", i))
                        .flex()
                        .items_center()
                        .gap(rems(8. / 15.))
                        .min_h(row_h)
                        .px(rems(6. / 15.))
                        .rounded(rems(6. / 15.))
                        .hover(|d| d.bg(c.accent))
                        .debug_selector(move || format!("panel-link-{i}"))
                        .on_click(move |_, window, cx| {
                            let (event, link) = (r.clone(), l.clone());
                            this.update(cx, |c, cx| {
                                cx.emit(CalendarEvent::OpenLink { event, link });
                                c.focus.focus(window, cx);
                            })
                            .ok();
                        })
                        .child(Icon::new(icon).color(muted))
                        .child(div().min_w_0().truncate().child(link.label.clone())),
                );
            }
            body = body.child(line(
                "links",
                IconName::Link,
                row_line,
                links.into_any_element(),
            ));
        }
        // Guests, and the answer of the one using the calendar.
        if !e.attendees.is_empty() {
            let going = e
                .attendees
                .iter()
                .filter(|a| a.response == Response::Accepted)
                .count();
            let n = e.attendees.len();
            let mut guests = div()
                .flex()
                .flex_col()
                .gap(rems(4. / 15.))
                .child(div().child(format!(
                    "{n} {} · {going} going",
                    if n == 1 { "guest" } else { "guests" }
                )));
            for (i, a) in e.attendees.iter().enumerate() {
                let (mark, mark_color): (&str, Hsla) = match a.response {
                    Response::Accepted => ("Going", c.success),
                    Response::Tentative => ("Maybe", rgb(0xe2a336).into()),
                    Response::Declined => ("No", c.destructive),
                    Response::NeedsAction => ("—", muted),
                };
                let name = if a.me {
                    format!("{} (you)", a.name)
                } else {
                    a.name.to_string()
                };
                guests = guests.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(rems(8. / 15.))
                        .min_w_0()
                        .debug_selector(move || format!("panel-guest-{i}"))
                        .child(div().flex_1().min_w_0().truncate().child(name))
                        .when(a.organizer, |d| {
                            d.child(div().flex_none().text_color(muted).child("Organizer"))
                        })
                        .child(div().flex_none().text_color(mark_color).child(mark)),
                );
            }
            body = body.child(line(
                "guests",
                IconName::Users,
                text_line,
                guests.into_any_element(),
            ));
            if let Some(me) = e.attendees.iter().find(|a| a.me) {
                let answers = [Response::Accepted, Response::Tentative, Response::Declined];
                let current = answers.iter().position(|r| *r == me.response);
                let this = this.clone();
                let size = if touch {
                    ControlSize::Touch
                } else {
                    ControlSize::Small
                };
                footer = footer.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(rems(8. / 15.))
                        .flex_wrap()
                        .child(div().text_color(muted).child("Going?"))
                        .child(
                            div().debug_selector(|| "panel-rsvp".into()).child(
                                ButtonGroup::new("panel-rsvp")
                                    .outline()
                                    .with_size(size)
                                    .children(["Yes", "Maybe", "No"].into_iter().enumerate().map(
                                        |(i, label)| {
                                            Button::new(("rsvp", i))
                                                .label(label)
                                                .selected(current == Some(i))
                                        },
                                    ))
                                    .on_click(move |picked: &Vec<usize>, window, cx| {
                                        if let Some(&i) = picked.first() {
                                            this.update(cx, |c, cx| {
                                                c.panel_edit(EditKind::Rsvp(answers[i]), cx);
                                                c.focus.focus(window, cx);
                                            })
                                            .ok();
                                        }
                                    }),
                            ),
                        ),
                );
            }
        }
        // What can be done with it.
        if editable {
            let size = if touch {
                ControlSize::Touch
            } else {
                ControlSize::Small
            };
            let this_edit = this.clone();
            let r = e.to_ref();
            footer = footer.child(
                div()
                    .flex()
                    .gap(rems(8. / 15.))
                    .pt(rems(4. / 15.))
                    .debug_selector(|| "panel-actions".into())
                    .child(
                        Button::new("panel-edit")
                            .label("Edit")
                            .outline()
                            .with_size(size)
                            .on_click(move |_, window, cx| {
                                let event = r.clone();
                                this_edit
                                    .update(cx, |c, cx| {
                                        c.panel = None;
                                        cx.emit(CalendarEvent::Edit(event));
                                        c.focus.focus(window, cx);
                                        cx.notify();
                                    })
                                    .ok();
                            }),
                    )
                    .child(
                        div().debug_selector(|| "panel-delete".into()).child(
                            Button::new("panel-delete")
                                .label("Delete")
                                .ghost()
                                .with_size(size)
                                .on_click(act(|c, cx| c.panel_edit(EditKind::Delete, cx))),
                        ),
                    ),
            );
        }

        let content = div()
            .flex()
            .flex_col()
            .gap(rems(12. / 15.))
            .p(rems(8. / 15.))
            .text_size(rems(13. / 15.))
            .line_height(rems(LINE / 15.))
            .debug_selector(|| "panel".into())
            .child(head)
            .child(
                div()
                    .id("panel-body")
                    .max_h(body_max)
                    .overflow_y_scroll()
                    .child(body),
            )
            .child(footer.debug_selector(|| "panel-footer".into()));
        let dismiss = {
            let this = this.clone();
            move |_: &DismissReason, _: &mut Window, cx: &mut App| {
                this.update(cx, |c, cx| c.close_panel(cx)).ok();
            }
        };
        Some(if sheet {
            Sheet::new("calendar-panel")
                .open(true)
                .take_focus(false)
                .on_dismiss(dismiss)
                .child(content)
                .into_any_element()
        } else {
            // Beside the event where there's room; below or above it (never over it) where
            // there isn't.
            let pad = 4.;
            let width = px((WIDTH + pad * 2.) * scale + 2.);
            let tall = body_max + px(190. * scale);
            let gap = px(4. * scale);
            let anchor: PopoverAnchor = match beside(panel.anchor, width, tall, view, gap) {
                Some(p) => p.into(),
                // From the event's left edge, so a long span doesn't widen it.
                None => {
                    let a = panel.anchor;
                    Bounds::new(a.origin, size(a.size.width.min(px(1.)), a.size.height)).into()
                }
            };
            Popover::new("calendar-panel")
                .open(true)
                .anchor(anchor)
                .take_focus(false)
                .max_height(rems((f32::from(tall) / scale + 20.) / 15.))
                .on_dismiss(dismiss)
                .p(rems(pad / 15.))
                .child(div().w(rems(WIDTH / 15.)).child(content))
                .into_any_element()
        })
    }
}
