//! Where events go: side by side columns for timed events that overlap on a day, and rows
//! (lanes) for spans of days in the all-day row and the month view, with what doesn't fit
//! counted for "+N more". Pure functions over numbers, so they're tested on their own.

/// A timed event's place among the events it overlaps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Column {
    /// The column it starts in, from the left.
    pub column: usize,
    /// How many columns it covers: it widens to the right over columns that are free for its
    /// whole length.
    pub span: usize,
    /// How many columns its group of overlapping events has.
    pub columns: usize,
}

impl Column {
    /// Its left edge as a share of the day's width.
    pub fn left(&self) -> f32 {
        self.column as f32 / self.columns as f32
    }

    /// Its width as a share of the day's width.
    pub fn width(&self) -> f32 {
        self.span as f32 / self.columns as f32
    }
}

fn overlaps(a: (f32, f32), b: (f32, f32)) -> bool {
    a.0 < b.1 && b.0 < a.1
}

/// Lays out one day's timed events, each `(start, end)` in any unit. Events that overlap,
/// directly or through others, form a group; the group is split into as many columns as it
/// needs at its busiest, each event going in the leftmost column free when it starts (earlier
/// starts first, longer first on a tie). Then each event widens to the right while the next
/// column is free for its whole length. This is the layout FullCalendar and Google Calendar
/// use. The result is in the input's order.
pub fn columns(intervals: &[(f32, f32)]) -> Vec<Column> {
    let n = intervals.len();
    // An event with no length still takes a sliver, so it gets a place.
    let iv: Vec<(f32, f32)> = intervals
        .iter()
        .map(|&(s, e)| (s, e.max(s + 1e-3)))
        .collect();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| {
        iv[a]
            .0
            .total_cmp(&iv[b].0)
            .then(iv[b].1.total_cmp(&iv[a].1))
            .then(a.cmp(&b))
    });
    let mut out = vec![
        Column {
            column: 0,
            span: 1,
            columns: 1
        };
        n
    ];
    let mut column_of = vec![0usize; n];
    let mut i = 0;
    while i < n {
        // The group: everything until a start at or after the latest end so far.
        let mut group_end = iv[order[i]].1;
        let mut j = i + 1;
        while j < n && iv[order[j]].0 < group_end {
            group_end = group_end.max(iv[order[j]].1);
            j += 1;
        }
        let group = &order[i..j];
        let mut ends: Vec<f32> = Vec::new();
        let mut members: Vec<Vec<usize>> = Vec::new();
        for &e in group {
            let at = ends.iter().position(|&end| end <= iv[e].0);
            let c = match at {
                Some(c) => c,
                None => {
                    ends.push(f32::MIN);
                    members.push(Vec::new());
                    ends.len() - 1
                }
            };
            ends[c] = iv[e].1;
            members[c].push(e);
            column_of[e] = c;
        }
        let k = ends.len();
        for &e in group {
            let c = column_of[e];
            let mut span = 1;
            while c + span < k && members[c + span].iter().all(|&o| !overlaps(iv[e], iv[o])) {
                span += 1;
            }
            out[e] = Column {
                column: c,
                span,
                columns: k,
            };
        }
        i = j;
    }
    out
}

/// How a group of overlapping events fits a day `width` wide when no column should be
/// narrower than `min` (both in the same unit).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Squeeze {
    /// How many of the group's columns show, from the left.
    pub shown: usize,
    /// How wide the shown columns are together.
    pub events: f32,
    /// How wide the strip at their right for "+N" is: none when every column shows.
    pub chip: f32,
}

impl Squeeze {
    /// Whether some of the group's columns don't show.
    pub fn hides(&self, columns: usize) -> bool {
        self.shown < columns
    }

    /// Where a column shows, as `(left, width)` from the day's left edge, or none if it's one
    /// of those that don't.
    pub fn place(&self, c: Column) -> Option<(f32, f32)> {
        if c.column >= self.shown {
            return None;
        }
        let w = self.events / self.shown as f32;
        let span = c.span.min(self.shown - c.column);
        Some((w * c.column as f32, w * span as f32))
    }
}

/// Fits a group of `columns` side-by-side columns into `width`, none narrower than `min`: all
/// of them where they fit; otherwise as many as fit beside a strip for "+N", which is `min`
/// wide where there's room and gives way down to `strip_min` before the one column left does.
/// A day narrower than `min` and a strip shows one column, as wide as the strip leaves it.
pub fn squeeze(columns: usize, width: f32, min: f32, strip_min: f32) -> Squeeze {
    let columns = columns.max(1);
    // A lone column takes the day however narrow it is: there's nothing to hide.
    if columns == 1 || columns as f32 * min <= width {
        return Squeeze {
            shown: columns,
            events: width,
            chip: 0.,
        };
    }
    let strip = (width - min).clamp(strip_min.min(width / 2.), min);
    let events = width - strip;
    Squeeze {
        shown: ((events / min).floor() as usize).clamp(1, columns - 1),
        events,
        chip: strip,
    }
}

/// Gives each span of days, `(first, last)` with the last inclusive, the lowest row that's free
/// on all its days, in the order given (so the caller decides who gets the top rows: longer
/// spans first, usually).
pub fn lanes(spans: &[(i32, i32)]) -> Vec<usize> {
    let mut rows: Vec<Vec<(i32, i32)>> = Vec::new();
    spans
        .iter()
        .map(|&(first, last)| {
            let (first, last) = (first.min(last), first.max(last));
            let free = |row: &Vec<(i32, i32)>| row.iter().all(|&(a, b)| last < a || first > b);
            let at = match rows.iter().position(free) {
                Some(at) => at,
                None => {
                    rows.push(Vec::new());
                    rows.len() - 1
                }
            };
            rows[at].push((first, last));
            at
        })
        .collect()
}

/// How a day in a row of days fits its events into `max_rows`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DayFit {
    /// Events in rows below this one are hidden on this day.
    pub shown_rows: usize,
    /// How many of the day's events that hides: the "+N more".
    pub hidden: usize,
}

/// For each of `days` days, how many rows show and how many events are left over, given the
/// spans and the rows [`lanes`] gave them. A day whose events all fit in `max_rows` shows them
/// all; a day that needs more shows one row fewer and gives the last to "+N more".
pub fn fit(spans: &[(i32, i32)], lanes: &[usize], days: usize, max_rows: usize) -> Vec<DayFit> {
    (0..days as i32)
        .map(|day| {
            let on: Vec<usize> = spans
                .iter()
                .zip(lanes)
                .filter(|((a, b), _)| *a <= day && day <= *b)
                .map(|(_, &lane)| lane)
                .collect();
            let needed = on.iter().map(|l| l + 1).max().unwrap_or(0);
            if needed <= max_rows {
                DayFit {
                    shown_rows: max_rows,
                    hidden: 0,
                }
            } else {
                let shown_rows = max_rows.saturating_sub(1);
                DayFit {
                    shown_rows,
                    hidden: on.iter().filter(|&&l| l >= shown_rows).count(),
                }
            }
        })
        .collect()
}

/// The runs of consecutive days on which a span in row `lane` shows, given each day's fit.
pub fn visible_runs(first: i32, last: i32, lane: usize, fits: &[DayFit]) -> Vec<(i32, i32)> {
    let mut runs = Vec::new();
    let mut open: Option<i32> = None;
    for day in first..=last {
        let shows = fits.get(day as usize).is_some_and(|f| lane < f.shown_rows);
        match (shows, open) {
            (true, None) => open = Some(day),
            (false, Some(a)) => {
                runs.push((a, day - 1));
                open = None;
            }
            _ => {}
        }
    }
    if let Some(a) = open {
        runs.push((a, last));
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(column: usize, span: usize, columns: usize) -> Column {
        Column {
            column,
            span,
            columns,
        }
    }

    #[test]
    fn events_that_dont_overlap_take_the_whole_day() {
        let out = columns(&[(0., 60.), (60., 120.), (180., 200.)]);
        assert_eq!(out, vec![c(0, 1, 1), c(0, 1, 1), c(0, 1, 1)]);
    }

    #[test]
    fn two_overlapping_events_share_the_day() {
        let out = columns(&[(0., 60.), (30., 90.)]);
        assert_eq!(out, vec![c(0, 1, 2), c(1, 1, 2)]);
        assert_eq!(out[1].left(), 0.5);
        assert_eq!(out[1].width(), 0.5);
    }

    #[test]
    fn a_column_is_reused_once_its_event_ends() {
        // A long one beside two short ones in a row: two columns, not three.
        let out = columns(&[(0., 120.), (0., 60.), (60., 120.)]);
        assert_eq!(out, vec![c(0, 1, 2), c(1, 1, 2), c(1, 1, 2)]);
    }

    #[test]
    fn events_widen_into_free_columns() {
        // A 9–12 block; 9–10 and 9–10 beside it fill three columns; 11–12 has columns 1 and 2
        // free and takes both.
        let out = columns(&[(540., 720.), (540., 600.), (540., 600.), (660., 720.)]);
        assert_eq!(out[0], c(0, 1, 3));
        assert_eq!(out[1], c(1, 1, 3));
        assert_eq!(out[2], c(2, 1, 3));
        assert_eq!(out[3], c(1, 2, 3));
    }

    #[test]
    fn groups_are_laid_out_apart() {
        // Two overlapping in the morning, one alone in the afternoon at full width.
        let out = columns(&[(540., 600.), (570., 630.), (840., 900.)]);
        assert_eq!(out[2], c(0, 1, 1));
        assert_eq!(out[0].columns, 2);
    }

    #[test]
    fn a_chain_of_overlaps_is_one_group() {
        // a overlaps b, b overlaps c, a and c don't: still one group of two columns, and c
        // goes back to the first.
        let out = columns(&[(0., 60.), (30., 90.), (60., 120.)]);
        assert_eq!(out, vec![c(0, 1, 2), c(1, 1, 2), c(0, 1, 2)]);
    }

    #[test]
    fn longer_first_on_the_same_start_and_input_order_kept() {
        let out = columns(&[(0., 30.), (0., 90.)]);
        assert_eq!(out[1].column, 0);
        assert_eq!(out[0].column, 1);
    }

    #[test]
    fn zero_length_events_get_a_place() {
        let out = columns(&[(60., 60.), (60., 60.)]);
        assert_eq!(out[0].columns, 2);
    }

    #[test]
    fn many_events_at_once() {
        let iv: Vec<(f32, f32)> = (0..50).map(|_| (0., 60.)).collect();
        let out = columns(&iv);
        assert!(out.iter().all(|p| p.columns == 50 && p.span == 1));
        let mut cols: Vec<usize> = out.iter().map(|p| p.column).collect();
        cols.sort();
        assert_eq!(cols, (0..50).collect::<Vec<_>>());
    }

    #[test]
    fn spans_stack_in_the_lowest_free_row() {
        // Mon–Wed, Tue, Thu–Fri, Wed–Thu: the last fits under Tuesday.
        let out = lanes(&[(0, 2), (1, 1), (3, 4), (2, 3)]);
        assert_eq!(out, vec![0, 1, 0, 1]);
        // Mon–Wed, Tue–Thu, Wed: three rows.
        assert_eq!(lanes(&[(0, 2), (1, 3), (2, 2)]), vec![0, 1, 2]);
    }

    #[test]
    fn days_that_overflow_show_a_row_fewer() {
        // Day 0 has four events, day 1 two; three rows fit.
        let spans = [(0, 1), (0, 0), (0, 1), (0, 0)];
        let l = lanes(&spans);
        assert_eq!(l, vec![0, 1, 2, 3]);
        let fits = fit(&spans, &l, 2, 3);
        assert_eq!(
            fits[0],
            DayFit {
                shown_rows: 2,
                hidden: 2
            }
        );
        // Day 1 uses rows 0 and 2, which fit in three.
        assert_eq!(
            fits[1],
            DayFit {
                shown_rows: 3,
                hidden: 0
            }
        );
        // The span in row 2 hides on day 0 and shows on day 1.
        assert_eq!(visible_runs(0, 1, 2, &fits), vec![(1, 1)]);
        assert_eq!(visible_runs(0, 1, 0, &fits), vec![(0, 1)]);
    }

    #[test]
    fn a_span_can_show_in_pieces() {
        let fits = [
            DayFit {
                shown_rows: 3,
                hidden: 0,
            },
            DayFit {
                shown_rows: 1,
                hidden: 4,
            },
            DayFit {
                shown_rows: 3,
                hidden: 0,
            },
        ];
        assert_eq!(visible_runs(0, 2, 1, &fits), vec![(0, 0), (2, 2)]);
        assert_eq!(visible_runs(0, 2, 0, &fits), vec![(0, 2)]);
    }

    #[test]
    fn no_rows_at_all_hides_everything() {
        let spans = [(0, 0)];
        let fits = fit(&spans, &lanes(&spans), 1, 0);
        assert_eq!(
            fits[0],
            DayFit {
                shown_rows: 0,
                hidden: 1
            }
        );
    }

    #[test]
    fn a_group_that_fits_shows_every_column() {
        let q = squeeze(3, 150., 36., 16.);
        assert_eq!(
            q,
            Squeeze {
                shown: 3,
                events: 150.,
                chip: 0.
            }
        );
        assert!(!q.hides(3));
        assert_eq!(q.place(c(1, 2, 3)), Some((50., 100.)));
    }

    #[test]
    fn a_group_too_wide_shows_the_columns_that_fit_and_a_strip() {
        // Eight columns in 150: a 36 strip, and three of 38 beside it.
        let q = squeeze(8, 150., 36., 16.);
        assert_eq!(q.shown, 3);
        assert_eq!(q.chip, 36.);
        assert_eq!(q.events, 114.);
        assert!(q.hides(8));
        assert!(q.events / q.shown as f32 >= 36.);
        assert_eq!(q.place(c(0, 1, 8)), Some((0., 38.)));
        // A span reaching past the shown columns stops at their edge.
        assert_eq!(q.place(c(2, 4, 8)), Some((76., 38.)));
        assert_eq!(q.place(c(3, 1, 8)), None);
        // Never fewer than one, and the strip gives way before a column does.
        let q = squeeze(4, 60., 36., 16.);
        assert_eq!((q.shown, q.events, q.chip), (1, 36., 24.));
    }

    #[test]
    fn a_day_narrower_than_a_column_and_a_strip_keeps_one_column_and_a_narrow_strip() {
        // A phone's week: 43 wide, 44 apiece. One column, and the strip at its narrowest.
        let q = squeeze(2, 43., 44., 16.);
        assert_eq!(
            q,
            Squeeze {
                shown: 1,
                events: 27.,
                chip: 16.
            }
        );
        assert!(q.hides(2));
        assert_eq!(q.place(c(0, 1, 2)), Some((0., 27.)));
        assert_eq!(q.place(c(1, 1, 2)), None);
        // A lone event takes the day, however narrow.
        assert_eq!(squeeze(1, 20., 44., 16.).shown, 1);
        assert_eq!(squeeze(1, 20., 44., 16.).chip, 0.);
        // Narrower still, the two halve it.
        let q = squeeze(3, 20., 44., 16.);
        assert_eq!((q.shown, q.events, q.chip), (1, 10., 10.));
    }
}
