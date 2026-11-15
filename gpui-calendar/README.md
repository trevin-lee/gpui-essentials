# gpui-calendar

Day, week, N-day and month calendar views for [GPUI](https://www.gpui.rs) apps built with
[GPUI Kit](https://github.com/longbridge/gpui-kit) 0.9. GPUI Kit has a date picker and a month
picker; this adds the time grid and the views a calendar app needs. You drag empty time to make
an event, drag an event to move it, and drag its edge to change its length, and every change
comes back to your app as a proposal.

It uses GPUI Kit's types throughout (GPUI Kit builds on `gpui-pre`, a snapshot of Zed's GPUI), so
it works in GPUI Kit apps but not with Zed's own `gpui` crate. It follows how FullCalendar,
react-big-calendar and Notion Calendar behave, without their code or their look.

![A week in gpui-calendar: all-day spans, overlapping events side by side, and the line at the current time](docs/week.png)

## What it does

- **Views:** day, week, N days (`CalendarView::Days(n)`) and month. Day and week share a time
  grid. It has a row of day names with today marked, an all-day row (three rows of spans, then
  "+N more", which opens the row), hour lines, and a line at the current time with the time in the
  gutter. The grid opens with the current time a third of the way down. The month shows spans for
  all-day and multi-day events, a dot, time and title for the rest, and "+N more", which opens
  that day.
- **Layout:** timed events that overlap sit side by side. Each group of overlapping events gets as
  many columns as it needs at its busiest. Each event takes the leftmost column free when it
  starts, then widens over the columns to its right that stay free for its whole length; this is
  FullCalendar's and Google Calendar's layout. Events a day or longer, and all-day ones, are spans
  stacked in the lowest free row, longer first. A shorter event that crosses midnight is drawn as
  a piece on each day.
- **Dragging:** drag empty time to make an event, drag an event to move it (to other days too),
  and drag its bottom edge to change its end (an all-day event's right edge, in the all-day row
  and the month). Drags snap to 15 minutes (`snap_minutes`), show a ghost of where the event would
  land with the original faded, and Escape drops them. Near the grid's top or bottom a drag
  scrolls it. Read-only events can be selected and opened but not moved.
- **Clicks:** a click selects, and a double-click opens an event or makes one in empty time.
- **Keys:** the arrows move the selection: a 30-minute slot on the grid, a day in the month, or
  from event to event. With nothing selected, ← and → change the period. `j` or `n` goes to the
  next period and `k` or `p` to the previous one. `t` goes to today, and `d`, `w` and `m` switch
  views. Enter opens the selected event or makes one in the selected slot, Delete asks to delete,
  and Escape drops a drag or the selection.
- **Time zones:** times are shown in a display zone from chrono-tz, which bundles the IANA
  database, so a DST day doesn't depend on the machine's zone files.
- **Theming:** colours per calendar, the week's first day, 12- or 24-hour time, and colours from
  GPUI Kit's theme. It scales with the window's rem, so it follows an app's zoom.

| Overlapping events | The month |
|---|---|
| ![Three days with events side by side in columns](docs/overlap.png) | ![November with spans, dotted rows and "+N more"](docs/month.png) |

## Use it

It needs Rust 1.100 or newer, as GPUI Kit 0.9 does.

```toml
[dependencies]
gpui-kit = "=0.9.1"
gpui-calendar = { git = "https://github.com/trevin-lee/gpui-calendar", rev = "<a commit>" }
```

Pin a commit: until it's on crates.io, any commit can change the API.

```rust,no_run
use chrono::{Duration, Utc};
use gpui_calendar::{Calendar, CalendarEvent, CalendarView, Event};
use gpui_kit::*;

fn main() {
    gpui_kit::application()
        // The header's chevrons are GPUI Kit's icons, so the app needs GPUI Kit's assets.
        .with_assets(gpui_kit::assets::Assets::new(""))
        .run(|cx| {
            gpui_kit::init(cx);
            gpui_calendar::init(cx);
            gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
                let now = Utc::now();
                let calendar = cx.new(|cx| {
                    Calendar::new(cx)
                        .with_options(|o| {
                            o.time_zone = chrono_tz::America::Los_Angeles;
                            o.week_start = chrono::Weekday::Sun;
                        })
                        .with_calendar_color("work", 0x5b8def)
                        .with_view(CalendarView::Week)
                        .with_events(vec![
                            Event::timed("standup", "Standup", now, now + Duration::minutes(15))
                                .calendar("work"),
                        ])
                });
                cx.subscribe(&calendar, |calendar, event: &CalendarEvent, cx| match event {
                    // The app owns the events: it agrees to a move by setting the event again.
                    CalendarEvent::Move { id, time } | CalendarEvent::Resize { id, time } => {
                        calendar.update(cx, |c, cx| {
                            if let Some(mut e) = c.event(id).cloned() {
                                e.time = *time;
                                c.upsert_event(e, cx);
                            }
                        });
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

- **Events:** an `Event` has an id, a title, an `EventTime`, a calendar id or its own colour, a
  location and a read-only flag. An `EventTime` is either two instants or all-day dates. The end
  is exclusive, as iCalendar's DTEND: a one-day event on the 14th ends on the 15th. Your app
  supplies the events for the range it's told about in `RangeChange`.
- **The calendar never changes an event.** Making, moving, resizing and deleting are proposals,
  sent as `CalendarEvent::{Create, Move, Resize, Delete}`. `Select` and `Open` report clicks.
  Your app agrees by calling `set_events` or `upsert_event`.
- **Recurring events:** your app expands them, supplying each occurrence in the range as an
  event of its own.
- **Options** (`CalendarOptions`, through `with_options` or `set_options`): `time_zone`,
  `week_start`, `hour12`, `snap_minutes`, `slot_minutes`, `default_duration`, `hour_height`,
  `scroll_time`, `calendar_colors`, `dim_past` and `all_day_rows`.
- **The header:** the title, Today, previous and next, and Day / Week / Month are GPUI Kit's
  buttons. Turn it off with `.with_header(false)` and drive `go_to`, `next`, `previous` and
  `set_view` from your own.
- **Keys:** the actions (`gpui_calendar::Today`, `SelectLeft` and the rest) work in the
  `Calendar` key context. Bind them to other keys after `gpui_calendar::init`.
- **Placing your own popovers:** `event_bounds(id)` gives where an event was drawn, and
  `point_at(date, minute)` gives where a time is on the grid.

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
cargo run --release --example calendar
cargo run --release --example calendar -- --bench [--events 2000] [--packed]
cargo run --release --features shots --example calendar -- --shot out.png --scene week
```

The example is a week of sample events and a host that agrees to every proposal and says what
happened at the bottom of the window. For pictures and the bench it pretends it's Wednesday 18
November 2026, 10:40 in Los Angeles. Pictures and the bench draw a window that's never shown and
never takes the keyboard. The scenes are `week`, `day`, `month`, `overlap`, `drag-create`,
`drag-move`, `resize`, `dst`, `dst-spring`, `dense` and `month-drag`.

The example draws in Inter, which it loads from `examples/fonts` (three fixed weights, under the
SIL Open Font License; see `examples/fonts/OFL.txt`). The crate itself takes its font from GPUI
Kit's theme.

| Dragging out a new event | Moving an event to another day |
|---|---|
| ![Empty time on Monday dragged out into a ghost](docs/drag-create.png) | ![Design review carried to Tuesday, the original faded](docs/drag-move.png) |
| **Changing an event's end** | **Moving an all-day event in the month** |
| ![Planning's end dragged to 1:15](docs/resize.png) | ![A trip dragged a week later](docs/month-drag.png) |
| **A day** | **A busy week (160 events)** |
| ![Wednesday on its own](docs/day.png) | ![A week packed with overlapping events](docs/dense.png) |

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

Early, and used for one app so far. It's tested on macOS. GPUI also runs on Linux and Windows,
but this crate hasn't been tried there.

Not done yet:
- Recurrence editing (expanding recurring events is the app's job, as above).
- Agenda, year, resource and timeline views.
- Dragging a timed event into the all-day row, or back.
- A scrollbar on the grid.
- A drag that keeps scrolling while the pointer rests at the grid's edge. Today it scrolls only
  while the pointer moves.
- In narrow columns, titles are cut to one line with "…" rather than wrapped.

## License

Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE). The example's Inter fonts are under the
SIL Open Font License 1.1 (`examples/fonts/OFL.txt`).
