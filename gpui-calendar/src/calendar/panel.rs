//! The event panel: an event's details (time, calendar, place, notes, links, guests) and what
//! can be done with it, as a popover beside the event on the Mac and a sheet on touch (or as
//! the host's `panel_style` says). Every action goes to the host as a [`CalendarEvent`]; the
//! panel changes nothing itself.

use std::ops::Range;

use chrono::{Datelike, Duration, Offset as _};
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
pub(super) fn beside(
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

/// What the guest list says above its names, as Google Calendar does: "4 guests", then how
/// they answered, "2 yes, 1 maybe, 1 awaiting", leaving out the answers nobody gave.
pub(super) fn guest_summary(guests: &[Attendee]) -> (String, String) {
    let n = guests.len();
    let count = format!("{n} {}", if n == 1 { "guest" } else { "guests" });
    let answers = [
        (Response::Accepted, "yes"),
        (Response::Tentative, "maybe"),
        (Response::Declined, "no"),
        (Response::NeedsAction, "awaiting"),
    ]
    .into_iter()
    .filter_map(|(response, word)| {
        let k = guests.iter().filter(|a| a.response == response).count();
        (k > 0).then(|| format!("{k} {word}"))
    })
    .collect::<Vec<_>>()
    .join(", ");
    (count, answers)
}

/// The web addresses in text ("https://…", "http://…" and "www.…") as byte ranges of it,
/// each with the address to open: a "www." one gets "https://". A sentence's punctuation after
/// one isn't part of it, nor a closing bracket it didn't open.
pub(super) fn find_urls(text: &str) -> Vec<(Range<usize>, String)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < text.len() {
        let rest = &text[i..];
        let at_word = text[..i]
            .chars()
            .next_back()
            .is_none_or(|c| c.is_whitespace() || "([<\"'".contains(c));
        let prefix = ["https://", "http://", "www."].into_iter().find(|p| {
            rest.get(..p.len())
                .is_some_and(|head| head.eq_ignore_ascii_case(p))
        });
        if at_word && let Some(prefix) = prefix {
            let mut end = rest
                .find(|c: char| c.is_whitespace() || "<>\"".contains(c))
                .unwrap_or(rest.len());
            while let Some(last) = rest[..end].chars().next_back() {
                let found = &rest[..end];
                let unopened = |open: char, close: char| {
                    found.matches(open).count() < found.matches(close).count()
                };
                let trailing = match last {
                    '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '"' => true,
                    ')' => unopened('(', ')'),
                    ']' => unopened('[', ']'),
                    _ => false,
                };
                if !trailing {
                    break;
                }
                end -= last.len_utf8();
            }
            if end > prefix.len() {
                let found = &rest[..end];
                let url = if prefix == "www." {
                    format!("https://{found}")
                } else {
                    found.to_string()
                };
                out.push((i..i + end, url));
                i += end;
                continue;
            }
        }
        i += rest.chars().next().map_or(1, char::len_utf8);
    }
    out
}

/// Two lists of highlights, each in order and not overlapping, as one such list: where they
/// overlap, the two styles together.
fn merge_highlights(
    a: &[(Range<usize>, HighlightStyle)],
    b: &[(Range<usize>, HighlightStyle)],
) -> Vec<(Range<usize>, HighlightStyle)> {
    let mut cuts: Vec<usize> = a
        .iter()
        .chain(b)
        .flat_map(|(r, _)| [r.start, r.end])
        .collect();
    cuts.sort_unstable();
    cuts.dedup();
    let mut out: Vec<(Range<usize>, HighlightStyle)> = Vec::new();
    for w in cuts.windows(2) {
        let piece = w[0]..w[1];
        let style = a
            .iter()
            .chain(b)
            .filter(|(r, _)| r.start <= piece.start && piece.end <= r.end)
            .map(|(_, s)| *s)
            .reduce(HighlightStyle::highlight);
        if let Some(style) = style {
            out.push((piece, style));
        }
    }
    out
}

/// When an event happens, in words, read in `tz`: "Wednesday, November 18 · 11 AM – 12 PM".
pub(super) fn when_in(e: &Event, tz: chrono_tz::Tz, hour12: bool) -> String {
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

/// A timed event's time in its own zone, where it has one whose clock reads differently from
/// `here` at its start, as Google Calendar adds it: "Wednesday, November 18 · 2 – 3 PM (GMT-5)
/// New York". None for an all-day event, or one in `here` or a zone the same hours as it.
pub(super) fn when_there(e: &Event, here: chrono_tz::Tz, hour12: bool) -> Option<String> {
    let zone = e.time_zone?;
    let EventTime::Timed { start, .. } = e.time else {
        return None;
    };
    let offset = |tz: chrono_tz::Tz| start.with_timezone(&tz).offset().fix();
    if zone == here || offset(zone) == offset(here) {
        return None;
    }
    Some(format!(
        "{} ({}) {}",
        when_in(e, zone, hour12),
        time::gmt_offset(start, zone),
        time::zone_place(zone)
    ))
}

impl Calendar {
    /// When an event happens, in words: "Wednesday, November 18 · 11 AM – 12 PM".
    fn when(&self, e: &Event) -> String {
        when_in(e, self.options.time_zone, self.options.hour12)
    }

    /// The event's time in its own zone, if it reads differently (see [`when_there`]).
    fn when_there(&self, e: &Event) -> Option<String> {
        when_there(e, self.options.time_zone, self.options.hour12)
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

    /// Text as `marked` gives it, with its web addresses underlined: a click on one asks the
    /// host to open it ([`CalendarEvent::OpenUrl`]).
    fn linked(
        &self,
        id: &'static str,
        text: SharedString,
        event: EventRef,
        cx: &Context<Self>,
    ) -> AnyElement {
        let urls = find_urls(&text);
        if urls.is_empty() {
            return self.marked(text, cx).into_any_element();
        }
        let colors = self
            .colors
            .unwrap_or_else(|| crate::theme::CalendarColors::from_theme(cx));
        let found: Vec<(Range<usize>, HighlightStyle)> = self
            .query
            .as_ref()
            .map(|q| q.ranges(&text))
            .unwrap_or_default()
            .into_iter()
            .map(|r| {
                (
                    r,
                    HighlightStyle {
                        background_color: Some(colors.find.opacity(0.38)),
                        ..Default::default()
                    },
                )
            })
            .collect();
        let underline = HighlightStyle {
            underline: Some(UnderlineStyle {
                thickness: px(1.),
                color: None,
                wavy: false,
            }),
            ..Default::default()
        };
        let links: Vec<(Range<usize>, HighlightStyle)> =
            urls.iter().map(|(r, _)| (r.clone(), underline)).collect();
        let styled = StyledText::new(text).with_highlights(merge_highlights(&found, &links));
        let ranges = urls.iter().map(|(r, _)| r.clone()).collect();
        let this = cx.entity().downgrade();
        InteractiveText::new(id, styled)
            .on_click(ranges, move |i, window, cx| {
                let Some((_, url)) = urls.get(i) else { return };
                let (event, url) = (event.clone(), SharedString::from(url.clone()));
                this.update(cx, |c, cx| {
                    cx.emit(CalendarEvent::OpenUrl { event, url });
                    c.focus.focus(window, cx);
                })
                .ok();
            })
            .into_any_element()
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
        // A narrow calendar has no room beside an event: the panel is a sheet there too,
        // unless the host asks for a popover (an iPad's) or a sheet whatever the calendar.
        let sheet = match self.options.panel_style {
            PanelStyle::Auto => touch || self.is_narrow(window),
            PanelStyle::Popover => false,
            PanelStyle::Sheet => true,
        };
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
                .children(self.when_there(&e).map(|there| {
                    div()
                        .text_color(muted)
                        .debug_selector(|| "panel-zone".into())
                        .child(there)
                }))
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
                            .accessibility_label("Change calendar")
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
                div()
                    .debug_selector(|| "panel-place".into())
                    .child(self.linked("panel-place", place.clone(), e.to_ref(), cx))
                    .into_any_element(),
            ));
        }
        if let Some(notes) = &e.notes {
            body = body.child(line(
                "notes",
                IconName::TextAlignStart,
                text_line,
                div()
                    .text_color(c.popover_foreground.opacity(0.9))
                    .debug_selector(|| "panel-notes".into())
                    .child(self.linked("panel-notes", notes.clone(), e.to_ref(), cx))
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
            let (count, answers) = guest_summary(&e.attendees);
            let mut guests = div()
                .flex()
                .flex_col()
                .gap(rems(4. / 15.))
                .child(div().debug_selector(|| "panel-guests".into()).child(count))
                .child(
                    div()
                        .text_color(muted)
                        .debug_selector(|| "panel-answers".into())
                        .child(answers),
                );
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
        let content = super::after_calendar(content);
        Some(if sheet {
            Sheet::new("calendar-panel")
                .accessibility_label(e.title.clone())
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
                .accessibility_label(e.title.clone())
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

#[cfg(test)]
mod tests {
    use super::{find_urls, guest_summary};
    use crate::{Attendee, Response};

    #[test]
    fn the_guest_list_counts_each_answer() {
        let guests = [
            Attendee::new("Ana", Response::Accepted).organizer(true),
            Attendee::new("Ben", Response::Accepted),
            Attendee::new("Cy", Response::Tentative),
            Attendee::new("Di", Response::Declined),
            Attendee::new("Ed", Response::NeedsAction).me(true),
            Attendee::new("Flo", Response::NeedsAction),
        ];
        assert_eq!(
            guest_summary(&guests),
            ("6 guests".into(), "2 yes, 1 maybe, 1 no, 2 awaiting".into())
        );
        // Answers nobody gave are left out.
        assert_eq!(
            guest_summary(&guests[..2]),
            ("2 guests".into(), "2 yes".into())
        );
        assert_eq!(
            guest_summary(&guests[4..5]),
            ("1 guest".into(), "1 awaiting".into())
        );
    }

    fn urls(text: &str) -> Vec<(&str, String)> {
        find_urls(text)
            .into_iter()
            .map(|(r, url)| (&text[r], url))
            .collect()
    }

    #[test]
    fn web_addresses_are_found_without_the_sentence_round_them() {
        assert_eq!(
            urls("Join at https://meet.google.com/abc-defg-hij."),
            [(
                "https://meet.google.com/abc-defg-hij",
                "https://meet.google.com/abc-defg-hij".into()
            )]
        );
        // In brackets, and with brackets of their own.
        assert_eq!(
            urls("Notes (https://en.wikipedia.org/wiki/Rust_(language)), and www.example.com!"),
            [
                (
                    "https://en.wikipedia.org/wiki/Rust_(language)",
                    "https://en.wikipedia.org/wiki/Rust_(language)".into()
                ),
                ("www.example.com", "https://www.example.com".into()),
            ]
        );
        // Several, after text that isn't ASCII, with queries and anchors.
        assert_eq!(
            urls("Café: http://a.example/x?y=1&z=2#top\nZoom <https://zoom.us/j/1>"),
            [
                (
                    "http://a.example/x?y=1&z=2#top",
                    "http://a.example/x?y=1&z=2#top".into()
                ),
                ("https://zoom.us/j/1", "https://zoom.us/j/1".into()),
            ]
        );
        // Not in the middle of a word, and not a bare prefix.
        assert!(urls("xhttps://a.example and https:// and www.").is_empty());
        assert!(urls("Room 4, Building 2").is_empty());
    }
}
