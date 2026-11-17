# gpui-calendar

Day, week, N-day, month and agenda calendar views for [GPUI](https://www.gpui.rs) apps built on
[gpui-core](../gpui-core): the time grid and the views a calendar app needs. You drag empty time to make
an event, drag an event to move it, drag its edge to change its length, and press Delete to
delete it, and every change comes back to your app as a proposal. Your app supplies the calendars
(their colours, which are shown, which are read-only) and the events, recurring ones already
expanded, and the crate never touches a network.

It uses GPUI as `gpui-pre` (a snapshot of Zed's GPUI) and takes its colours, font and header
buttons from gpui-core, so it works in apps on the same GPUI but not with Zed's own `gpui` crate.
It follows how FullCalendar, react-big-calendar and Notion Calendar behave, without their code or
their look.

![A week in gpui-calendar: all-day spans, overlapping events side by side, and the line at the current time](docs/week.png)

## What it does

- **Views:** day, week, N days (`CalendarView::Days(n)`), month and agenda
  (`CalendarView::Agenda(n)`, two weeks from the header's button). Day and week share a time
  grid. It has a row of day names with today marked, an all-day row (three rows of spans, then
  "+N more", which opens the row; open or not, it never takes more than 40% of the calendar
  under the day names, so a crowded day can't push the hours away), hour lines, and a line at the current time with the time in the
  gutter. The grid opens with the current time a third of the way down. The month shows spans for
  all-day and multi-day events, a dot, time and title for the rest, and "+N more", which lists
  all that day's events in a popover beside it (a sheet on touch), as Google Calendar does; a
  click on one opens it. `open_more(date)` opens it from your app. A span is one bar across a week's days; where it goes on past the week (into the
  month's next row, or the next period), that end is square with a chevron pointing on.
- **Agenda:** a list of the days that have events (and today), each under a header that stays
  at the top while its day scrolls by. A row has the time (or "All day", "Day 2 of 3",
  "Until 2 AM"), the calendar's colour, the title, the place and every link as a pill. It opens
  at today and scrolls with the wheel or a trackpad. Nothing in it is dragged; a click selects,
  a double-click opens, a click on a day's header opens that day, and a link opens its link. It
  is the layout meant for a phone, and it fits from 320 pixels wide.
- **Layout:** timed events that overlap sit side by side. Each group of overlapping events gets as
  many columns as it needs at its busiest. Each event takes the leftmost column free when it
  starts, then widens over the columns to its right that stay free for its whole length; this is
  FullCalendar's and Google Calendar's layout. No column gets narrower than a readable 36
  pixels (44 points on touch): a group that needs more shows the columns that fit and, in a
  strip at their right, "+N" for the rest, from the first one's start to the last one's end. A
  click (a tap) on it lists them in a popover beside it (a sheet on touch), each with its
  colour, title and time; a click on a row opens that event, and a drag from a row carries the
  event into the grid, held by its start. `hidden_bounds(id)` says where an event's "+N" is,
  `open_hidden(id)` opens its list, and `crowd_events()` gives what's listed. VoiceOver hears
"+N" as "12 more events, 2 to 5 PM", and activating it opens the list. A day too narrow
  for even one such column beside the strip (a phone's week) keeps one column and a narrower
  strip. Events a day or longer, and all-day ones, are spans
  stacked in the lowest free row, longer first. A shorter event that crosses midnight is drawn as
  a piece on each day.
- **Dragging:** drag empty time to make an event, drag an event to move it (to other days too),
  and drag its bottom edge to change its end (an all-day event's right edge, in the all-day row
  and the month). On the grid a move crosses between the all-day row and the hours: an all-day
  event dropped in the hours becomes a timed one starting there, `default_duration` long, and
  a timed one dropped in the all-day row becomes that whole day. Drags snap to 15 minutes (`snap_minutes`), show a ghost of where the event would
  land with the original faded, and Escape drops them. Near the grid's top or bottom a drag
  scrolls it. Read-only events, and events in read-only calendars, can be selected and opened
  but not moved.
- **Links:** an event can link to docs and agents. A chip with room lists them under its text,
  each with a mark (a page for a doc, a ring for an agent) and its label. A smaller chip shows
  the one link's mark by its title, or a chain and a count for several. A click on a link asks
  your app to open it; a drag that starts on one moves the event.
- **Clicks:** a click selects, and a double-click opens an event or makes one in empty time.
- **Keys:** the arrows move the selection: a 30-minute slot on the grid, a day in the month, or
  from event to event; ← and → cross into the next or previous day (and period) in the day,
  week and month views. With nothing selected, ← and → change the period. `j` goes to the next
  period and `k` or `p` to the previous one. `t` goes to today, `g` opens the mini month, and
  `d`, `w`, `m` and `a` switch views. In the agenda the arrows go from event to event down the list. `n` proposes a new event
  at the selected slot, or at the next round half hour today. Enter opens the selected event or
  makes one in the selected slot, Delete or Backspace asks to delete the selected event, and
  Escape drops a drag or the selection.
- **Time zones:** times are shown in a display zone from chrono-tz, which bundles the IANA
  database, so a DST day doesn't depend on the machine's zone files. A second zone
  (`secondary_zone`) adds a column of its hours left of the day and week grids' own, under its
  abbreviation ("IST", or "+11" where the zone has no letters and the column no room). Each hour
  line is turned into an instant and read in the second zone, so half-hour zones read "2:30a"
  and a DST day in either zone shows as a jump. The column stays put during a swipe, with the
  hours.
- **Touch** (`touch`, on by default on iOS and Android): GPUI turns touches into taps, long
  presses and pans, the same on every platform, and the calendar takes them so:
  - A tap selects, and a tap on the selected event opens it.
  - A long press on empty time makes an event of the usual length there (or a day, in the
    all-day row and the month), and a long press on an event picks it up; either then follows
    the finger, and lifting it proposes the change.
  - A finger on the selected event drags it at once, and its handle (a dot at its end) changes
    its end. The end follows the finger's travel, so it doesn't jump to the finger.
  - A pan up and down scrolls, with GPUI's momentum. A swipe sideways carries the days with
    the finger (the hours in the gutter stay), with the next or previous period drawn beside
    them. Past a fifth of the width, or flicked at 500 points a second or more over its last
    80 ms, that period slides in; otherwise it eases back. The easing takes 200 ms by GPUI's
    clock, a frame at a time at whatever rate the display draws, and nothing is drawn at
    rest.
  - Small things (short events, link marks, "+N more", dates) take taps in a 44-point square
    round them, the nearest winning.
  - The header is the plain one, with 44-point controls and no arrows: the swipe changes the
    period.
- **Narrow widths:** below 600 pixels the calendar lays itself out for a phone. The header is a
  compact one, drawn from plain elements: a short title ("Wed, Nov 18", "Nov 2026"), Today, the
  arrows, and Day, Month and List. The gutter narrows, days too narrow for "Mon 16" show the name
  over the number, "+3 more" becomes "+3", and the month's rows drop their times for the
  title. Anywhere, text that can't keep two letters before its "…" is left out rather than cut
  to "L…". `short_title()` gives the short title for a header of your own.
- **The event panel** (`event_panel`, off by default): opening an event (a double click, Enter,
  or a tap on the selected one on touch) shows its details beside it on the Mac, or in a sheet
  on touch and in a narrow calendar: the title, when, the calendar (with Change, listing the
  other calendars it can move to), the place, the notes (web addresses in these two are
  underlined, and a click on one sends `CalendarEvent::OpenUrl` for your app to open it), the
  links (each opens its link), the guests (counted by answer, "4 guests" over "2 yes, 1 maybe,
  1 awaiting", then each with theirs), Yes / Maybe / No for the guest marked `me`, and Edit and
  Delete. `CalendarEvent::Open` is still sent, panel or not. Every action goes to your app as
  an event: `Edit`, `Delete`, `ChangeCalendar` and `Rsvp`, the last three asking "this event,
  following or all?" first for a repeating event. A read-only event's panel has no Edit,
  Delete or Change. `open_panel(id)`, `close_panel()` and `panel_event()` drive it from your
  app; Escape or a click outside closes it, and Delete or Backspace is its Delete.
  `panel_style` overrides where it goes: `PanelStyle::Popover` keeps it beside the event on
  touch too (an iPad's, with 44-point rows and buttons, closed by a tap outside),
  `PanelStyle::Sheet` makes it a sheet everywhere, and `PanelStyle::Auto`, the default, is as
  above.
- **The mini month** (`mini_month`, off by default): gpui-core's `MonthGrid` to jump to a date.
  Where the calendar is wide (its days still at least 600 pixels with the month beside them),
  it's a column at the calendar's left; narrower, or on touch, it's behind the title: a click
  (or `g`) opens it in a popover, or a sheet of 44-point days on touch. It shows the calendar's
  period and keeps in step as that changes, starts weeks on `week_start`, dots the days your
  app's shown events fall on (it never fetches anything), and a pick goes to that date (sending
  `RangeChange`). Escape closes it. Its today is the calendar's: by its clock (or `with_now`) in
  `time_zone`, not the Mac's local date, and it turns over at that zone's midnight.
- **Find:** `set_query(text)` filters the events by text: those whose title, place, notes or
  link labels contain every word, ignoring case and accents ("cafe zurich" finds "Café Zürich").
  The time grid dims the rest, rings what it found and marks the found words; the month and
  the agenda show only what it found. `found()` lists them, and `query()` gives the query back.
  ⌘F (Ctrl-F elsewhere) sends `CalendarEvent::Find` for your app to focus its own search field.
  The calendar takes no typing for find unless your app asks with `type_query`: then typing goes
  into the query (Enter keeps it, Escape clears it), a pill at the bottom says what it's finding
  and how many it found, and each change is sent as `CalendarEvent::QueryChange`.
- **Accessibility:** VoiceOver reads the calendar.
  - **The canvas.** It is a group named for the period ("Calendar, November 2026"). It holds a
    node for each day name, event, "+N more" and hour (on the grid) or day (in the month) on
    screen, each with a role, a label and its bounds.
  - **Event labels.** An event says what, when and where: "Team sync, Tuesday 3 to 4 PM, Work
    calendar". Where they apply, ", repeating", ", read-only" and ", 2 links" follow.
  - **Day labels.** Days say "Wednesday, November 18, today", and hours "Wednesday, November 18,
    9 AM".
  - **Activating a node** does what a click does: it opens an event, selects an hour, or shows a
    day. Selected and read-only are marked.
  - **When it runs.** The nodes are built only while a screen reader is on. The frame is laid
    out without paint first, so they're this frame's, never the last one's.
  - **For your app.** `describe_event(id)`, `accessibility_items()` and `activate(target)` give
    the same to your app and its tests.
  - **The header.** Its controls are labelled buttons ("Previous", "Next", the views, and on a
    phone the title, "November 2026, go to a date").
  - **Tab** goes through the header, the mini month, the days (the arrow keys move in them), and
    an open panel or question, in that order. Where Tab brought the keyboard to the days, a ring
    in the theme's focus colour shows it.
  - **Reduce Motion:** a swipe lands at once instead of easing. This reads `App::reduce_motion()`,
    which GPUI 0.3.12 doesn't set from the system, so set it from your app (see "Putting it in an
    app"). Nothing else in the calendar moves on its own: popovers and sheets appear at once, and
    drags follow the pointer.
- **Theming:** each calendar's colour, the week's first day, 12- or 24-hour time, and colours
  from gpui-core's theme. It scales with the window's rem, so it follows an app's zoom.

| Overlapping events | The month |
|---|---|
| ![Three days with events side by side in columns](docs/overlap.png) | ![November with spans, dotted rows and "+N more"](docs/month.png) |

## Use it

It needs Rust 1.100 or newer. It uses GPUI as `gpui-pre = "=0.3.12"` and takes its theme from
gpui-core, so your app must use the same GPUI, or two would be linked:

```toml
[dependencies]
gpui = { package = "gpui-pre", version = "=0.3.12" }
gpui-core = { git = "https://github.com/trevin-lee/gpui-essentials", rev = "<a commit>" }
gpui-calendar = { git = "https://github.com/trevin-lee/gpui-essentials", rev = "<a commit>" }
```

Call `gpui_core::init(cx)` and then `gpui_calendar::init(cx)` once at startup.

Pin a commit: until it's on crates.io, any commit can change the API.

```rust,no_run
use chrono::{Duration, Utc};
use gpui_calendar::{Calendar, CalendarEvent, CalendarSource, CalendarView, Event};
use gpui::*;

fn main() {
    // GPUI's platform for this OS (the `gpui-pre-platform` crate); on iOS the app's own.
    gpui_platform::application()
        // The header's chevrons are gpui-core's icons, so the app needs its assets.
        .with_assets(gpui_core::Icons)
        .run(|cx| {
            gpui_core::init(cx);
            gpui_calendar::init(cx);
            gpui_core::open_window(WindowOptions::default(), cx, |_, cx| {
                let now = Utc::now();
                let calendar = cx.new(|cx| {
                    Calendar::new(cx)
                        .with_options(|o| {
                            o.time_zone = chrono_tz::America::Los_Angeles;
                            o.week_start = chrono::Weekday::Sun;
                        })
                        .with_calendars(vec![
                            CalendarSource::new("work", "Work", 0x5b8def),
                            CalendarSource::new("holidays", "Holidays", 0x8b8b93).read_only(true),
                        ])
                        .with_view(CalendarView::Week)
                        .with_events(vec![
                            // One occurrence of a recurring event: its own id and its series'.
                            Event::timed("standup@1", "Standup", now, now + Duration::minutes(15))
                                .calendar("work")
                                .series("standup"),
                        ])
                });
                cx.subscribe(&calendar, |calendar, event: &CalendarEvent, cx| match event {
                    // The app owns the events: it agrees to a move by setting the event again.
                    // `event.series` says it's one occurrence of a recurring event.
                    CalendarEvent::Move { event, time, .. }
                    | CalendarEvent::Resize { event, time, .. } => {
                        calendar.update(cx, |c, cx| {
                            if let Some(mut e) = c.event(&event.id).cloned() {
                                e.time = *time;
                                c.upsert_event(e, cx);
                            }
                        });
                    }
                    CalendarEvent::Delete { event, .. } => {
                        calendar.update(cx, |c, cx| c.remove_event(&event.id, cx));
                    }
                    // Fetch the events in this range, then call `set_events`.
                    CalendarEvent::RangeChange { .. } => {}
                    _ => {}
                })
                .detach();
                calendar
            })
            .unwrap();
        });
}
```

## Putting it in an app

- **Calendars:** a `CalendarSource` has an id, a name, a colour and two flags, `visible` and
  `read_only`. Give the list with `with_calendars` or `set_calendars` (say, when someone hides
  one in your sidebar). An event takes its calendar's colour. A hidden calendar's events aren't
  drawn, selected or counted. A read-only calendar's events can be selected and opened but not
  moved, resized or deleted, and new events never go in it. An event whose calendar isn't in
  the list is shown, editable, and gets a palette colour chosen by its calendar id.
- **Events:** an `Event` has an id, a series id for an occurrence of a recurring event, a title,
  an `EventTime`, a calendar id, a location, notes, attendees (`Attendee`: name, email,
  `Response`, organizer, me), a read-only flag of its own and, rarely, a colour of its own. An `EventTime` is either two instants or all-day dates. The end is exclusive, as
  iCalendar's DTEND: a one-day event on the 14th ends on the 15th. Your app supplies the events
  for the range it's told about in `RangeChange`.
- **The calendar never changes an event.** Making, moving, resizing and deleting are proposals,
  sent as `CalendarEvent::{Create, Move, Resize, Delete}`. `Create` says which calendar:
  `default_calendar` if it's shown and can be edited, or else the first that is. The others
  carry an `EventRef` with the event's id, its series and its calendar. `Select` and `Open`
  report clicks.
  Your app agrees by calling `set_events` or `upsert_event`, or `remove_event` for a delete.
- **Links:** `Event::link(EventLink::doc(id, label))` or `EventLink::agent(id, label)`. A
  click on one sends `CalendarEvent::OpenLink { event, link }`; the calendar never fetches or
  opens anything. `link_bounds(id, index)` gives where a link was drawn.
- **Recurring events:** your app expands them, supplying each occurrence in the range as an
  event of its own, with an id of its own and the series' id in `series`. When a move, a resize
  or a delete touches an occurrence, the calendar first asks "This event / This and following
  events / All events", in a small popover where the edit was let go (a sheet with 44-point
  answers on touch), and puts the answer in the edit's `scope` (`EditScope`). The ghost stays
  where it was dropped while it asks; Escape, a click outside or Cancel drops the edit and the
  event stays put. The arrow keys and Enter answer too. An event that doesn't repeat isn't asked
  about: its edits carry `EditScope::This`. A host that can't carry out every answer sets
  `scopes` (say, `&[EditScope::This, EditScope::All]` where its server can't split a series):
  the question offers only those, and with just one it isn't asked and the edit carries that
  one. `is_asking()`, `answer(scope)` and
  `dismiss_question()` let a host drive the question itself. The popover and the sheet are
  gpui-core's `Popover` and `Sheet`, opened without taking the keyboard: it stays with the
  calendar, which handles Escape.
- **Options** (`CalendarOptions`, through `with_options` or `set_options`): `time_zone`,
  `week_start`, `hour12`, `snap_minutes`, `slot_minutes`, `default_duration`, `hour_height`,
  `secondary_zone`, `scroll_time`, `default_calendar`, `dim_past`, `all_day_rows`, `touch`,
  `event_panel`, `panel_style`, `mini_month` and `scopes`.
- **The header:** the title, Today, previous and next, and Day / Week / Month / Agenda are
  gpui-core's buttons. Turn it off with `.with_header(false)` and drive `go_to`, `next`, `previous` and
  `set_view` from your own.
- **Keys:** the actions (`gpui_calendar::Today`, `SelectLeft` and the rest) work in the
  `Calendar` key context. Bind them to other keys after `gpui_calendar::init`.
- **Placing your own popovers:** `event_bounds(id)` gives where an event was drawn, and
  `point_at(date, minute)` gives where a time is on the grid.
- **Reduce Motion:** call `cx.set_reduce_motion(on)` with the system's setting at launch, and
  again when it changes:
  - **macOS:** `NSWorkspace.shared.accessibilityDisplayShouldReduceMotion`, and
    `NSWorkspace.accessibilityDisplayOptionsDidChangeNotification` on its notification center.
  - **iOS:** `UIAccessibility.isReduceMotionEnabled` and
    `UIAccessibility.reduceMotionStatusDidChangeNotification`.

  It's one switch for the whole app: GPUI's own animations honour it too.
- **What VoiceOver can't do yet:**
  - It reaches only what's on screen. It can't scroll the grid by itself, so a person scrolls
    with three fingers, or uses the keys or the header, to bring more hours or days in.
  - An event's links are part of its label, not nodes of their own; the panel lists them as
    buttons.

## Time zones and DST

The grid is the wall clock, with every hour the same height. On a 23-hour day the skipped hour is
shaded and holds nothing. On a 25-hour day the repeated hour's two passes share its rows, and two
events in them sit side by side. The day's name says "23 hours" or "25 hours".

A place on the grid becomes an instant as RFC 5545 and JavaScript's Temporal say: a time in the
repeated hour is its first pass, and a time in the skipped hour moves forward by the gap (2:30
becomes 3:30). Moving an event keeps its real length, so a two-hour event moved across the spring
change ends three wall-clock hours after it starts. The tests cover 23- and 25-hour days, a zone
that skips midnight itself (America/Santiago), and moves across a change.

| The autumn change: 1 AM happens twice | The spring change: 2 AM is skipped |
|---|---|
| ![A 25-hour Sunday with two backups at the two 1:30s side by side](docs/dst.png) | ![A 23-hour Sunday with the skipped hour shaded](docs/dst-spring.png) |

## Trying it

```sh
export CARGO_TARGET_DIR=$TMPDIR/gpui-calendar-target   # or anywhere you like
cargo test
cargo run --release --example calendar [-- --touch]   # --touch: as a phone takes it
cargo run --release --example calendar -- --bench [--events 2000] [--packed]
cargo run --release --features shots --example calendar -- --shot out.png --scene week
```

`cargo test` also draws every view at widths from 320 (the narrowest iPhone) to 2,200 pixels,
with long titles, locations and links, laid out with Core Text and Inter as on screen, and fails
if any text runs out of the area it's drawn in, runs into other text, or is cut to fewer than
two letters and "…". It checks that the header's title gives way before its buttons run out of
the calendar, at the same widths.

The example is a week of sample events and a host that agrees to every proposal and says what
happened at the bottom of the window. For pictures and the bench it pretends it's Wednesday 18
November 2026, 10:40 in Los Angeles. Pictures and the bench draw a window that's never shown and
never takes the keyboard. The scenes are `week`, `day`, `month`, `overlap`, `drag-create`,
`drag-move`, `resize`, `dst`, `dst-spring`, `dense`, `month-drag`, `links`, `agenda` (drawn
720 pixels wide) and, at an iPhone's 390 by 844 and taken by touch, `compact` (with an event
selected, showing its handle), `compact-week`, `compact-agenda`, `compact-swipe` (a finger
halfway through a swipe to the next day), `compact-hold` (a long press on empty time, drawn
down), `compact-zones` (the day with a second zone), `compact-recurring` (the recurring
question as a sheet), `compact-find` (the list found by "CS 162"), `compact-panel` (the
design review's panel as a sheet) and `compact-mini` (the mini month open behind the title); at
the Mac's size, `zones` is the week with India's hours beside Los Angeles', `recurring` asks
the recurring question in a popover, `find` finds "lunch" while the calendar takes typing,
`panel` opens the design review's panel beside it, `mini` has the mini month beside the
week, `crowd` is the busy week with Thursday's "+N" open, `all-day` crowds Wednesday's
all-day row and has a visit go on into next week, `all-day-month` is the same month, and
`all-day-drag` carries Mom's birthday into Friday's hours. Only `mini` and `compact-mini` are
drawn with the mini month, so the other pictures stay
comparable. `tests/touch.rs` drives the calendar with real touch events on the Mac: taps, long
presses, drags, swipes and pans.

The example draws in Inter, which it loads from `examples/fonts` (three fixed weights, under the
SIL Open Font License; see `examples/fonts/OFL.txt`). The crate itself takes its font from
gpui-core's theme.

| Dragging out a new event | Moving an event to another day |
|---|---|
| ![Empty time on Monday dragged out into a ghost](docs/drag-create.png) | ![Design review carried to Tuesday, the original faded](docs/drag-move.png) |
| **Changing an event's end** | **Moving an all-day event in the month** |
| ![Planning's end dragged to 1:15](docs/resize.png) | ![A trip dragged a week later](docs/month-drag.png) |
| **A crowded all-day row, and a span going on** | **The span across the month's rows** |
| ![Wednesday's all-day row with "+6 more", and Parents visiting with a chevron at its end](docs/all-day.png) | ![Parents visiting from the 21st into the next row, a chevron at each break](docs/all-day-month.png) |
| **An all-day event dragged into the hours** | |
| ![Mom's birthday carried into Friday at 2 PM, an hour long](docs/all-day-drag.png) | |
| **A day** | **A busy week (160 events)** |
| ![Wednesday on its own](docs/day.png) | ![A week packed with overlapping events, readable columns and "+N" for the rest](docs/dense.png) |
| **What didn't fit, listed** | |
| ![Thursday's "+12" open beside it, twelve events with their times](docs/crowd.png) | |
| **Links to docs and agents** | **The agenda** |
| ![Design review listing its notes; shorter events with a link's mark or a count](docs/links.png) | ![Tuesday's events listed with their times, colours and links](docs/agenda.png) |
| **A day on a phone** | **The agenda on a phone** |
| ![Wednesday at 390 pixels wide, with the compact header](docs/compact.png) | ![Tuesday's list at 390 pixels wide](docs/compact-agenda.png) |
| **A swipe under way** | **A long press on empty time, drawn down** |
| ![Wednesday carried left by a finger, Thursday coming in beside it](docs/compact-swipe.png) | ![A new event from 5:30 to 7:15 under a finger](docs/compact-hold.png) |
| **A second zone, half an hour off** | **On a phone** |
| ![The week with India's hours beside Los Angeles'](docs/zones.png) | ![Wednesday at 390 pixels wide with a second column of hours](docs/compact-zones.png) |
| **A repeating event moved: which ones?** | **The same question on a phone** |
| ![Tuesday's standup dragged half an hour later, a popover asking which occurrences](docs/recurring.png) | ![A sheet asking which occurrences, with Cancel](docs/compact-recurring.png) |
| **Finding "lunch", taking typing** | **Finding "CS 162" on a phone's list** |
| ![The week with the lunches ringed and marked, the rest dimmed, and a Find pill](docs/find.png) | ![The list showing only CS 162's events](docs/compact-find.png) |
| **An event's panel, beside it** | **The panel as a sheet on a phone** |
| ![The design review's time, calendar, place, notes, link, guests, Going? and Edit and Delete](docs/panel.png) | ![The same details in a sheet at 390 pixels wide](docs/compact-panel.png) |
| **The mini month beside the week** | **Behind the title on a phone** |
| ![The week with November's mini month at its left, days with events dotted](docs/mini.png) | ![November's month in a sheet at 390 pixels wide](docs/compact-mini.png) |

## Performance

Measured with `cargo run --release --example calendar -- --bench` on an Apple M4 Pro. This is the
CPU time to lay out and paint a frame, from drawing a hidden window directly. The GPU's part and
presenting the frame aren't counted.

| 2,000 events over a year (41 in the week on screen) | median | p95 |
|---|---|---|
| Scrolling the grid | 0.27 ms | 0.32 ms |
| Dragging an event across the week | 0.25 ms | 0.36 ms |
| Changing the week every frame (a fresh layout each time) | 0.63 ms | 0.81 ms |
| The month | 0.53 ms | 0.86 ms |

With all 2,000 events in the week on screen (`--packed`), scrolling takes 1.9 ms and dragging
2.0 ms at the median.

Layout runs only when the events, the period or the options change, never on every frame. It
covers only the events in range, found by binary search, and painting skips what isn't scrolled
into view. Nothing draws while nothing changes, apart from the line at the current time, which
moves once a minute.

## Status

Early, and used for one app so far. It's tested on macOS, and CI builds it for the iOS
simulator (`aarch64-apple-ios-sim`): the crate takes no platform, and on iOS the app links
`gpui-pre-ios` itself. GPUI also runs on Linux and Windows, but this crate hasn't been tried
there.

Not done yet:
- Editing a recurrence rule itself (expanding recurring events, and applying a scoped edit to
  the series, is the app's job, as above).
- A context menu (Delete is the key for now).
- Year, resource and timeline views.
- A scrollbar on the grid.
- A drag that keeps scrolling while the pointer rests at the grid's edge. Today it scrolls only
  while the pointer moves.
- In narrow columns, titles are cut to one line with "…" rather than wrapped.

## License

Apache-2.0. See [LICENSE](../LICENSE) and [NOTICE](../NOTICE). The example's Inter fonts are under the
SIL Open Font License 1.1 (`examples/fonts/OFL.txt`).
