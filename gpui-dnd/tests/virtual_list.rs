//! VirtualSortable: of 10,000 rows only those on screen are drawn; a drag reorders them, the
//! rows on screen parting for it; auto-scroll carries a drag to rows that weren't drawn and it
//! lands right; the keyboard moves the focus and then the row through rows not yet drawn, the
//! list scrolling ahead; it shares items with a Sortable of its group; nothing runs at rest; and
//! (ignored, run with --release) how long a frame of a drag takes at 10,000 rows.

mod common;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use common::*;
use gpui::{
    InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Point, Styled as _,
    TestAppContext, UniformListScrollHandle, div, point, px,
};
use gpui_dnd::*;

const ROW: f32 = 40.;
const N: u32 = 10_000;

type Keys = Rc<RefCell<Rc<[u32]>>>;
type Moves = Rc<RefCell<Vec<String>>>;

struct Board {
    keys: Keys,
    side: Rc<RefCell<Vec<u32>>>,
    moves: Moves,
    drawn: Rc<Cell<usize>>,
    scroll: UniformListScrollHandle,
}

fn board(cx: &mut TestAppContext, n: u32, side: Vec<u32>) -> (Opened<'_>, Board) {
    let keys: Keys = Rc::new(RefCell::new((0..n).collect::<Vec<_>>().into()));
    let side = Rc::new(RefCell::new(side));
    let moves: Moves = Rc::default();
    let drawn = Rc::new(Cell::new(0));
    let scroll = UniformListScrollHandle::new();
    let (k, s, m, d, h) = (
        keys.clone(),
        side.clone(),
        moves.clone(),
        drawn.clone(),
        scroll.clone(),
    );
    let o = open_in_root(cx, move |_, _| {
        let apply = {
            let (k, s, m) = (k.clone(), s.clone(), m.clone());
            Rc::new(move |mv: &SortableMove<u32>| {
                m.borrow_mut().push(format!(
                    "move {} {}:{} -> {}:{}",
                    mv.key, mv.from.0, mv.from.1, mv.to.0, mv.to.1
                ));
                let take = |list: &gpui::ElementId, at: usize| -> u32 {
                    if list.to_string() == "big" {
                        let mut v = k.borrow().to_vec();
                        let key = v.remove(at);
                        *k.borrow_mut() = v.into();
                        key
                    } else {
                        s.borrow_mut().remove(at)
                    }
                };
                let key = take(&mv.from.0, mv.from.1);
                if mv.to.0.to_string() == "big" {
                    let mut v = k.borrow().to_vec();
                    v.insert(mv.to.1, key);
                    *k.borrow_mut() = v.into();
                } else {
                    s.borrow_mut().insert(mv.to.1, key);
                }
            })
        };
        let (a1, a2) = (apply.clone(), apply);
        let d = d.clone();
        div()
            .p(px(20.))
            .flex()
            .gap(px(40.))
            .child(
                VirtualSortable::new("big", k.borrow().clone(), &h)
                    .group("g")
                    .label("Big")
                    .item_label(|k: &u32| format!("Row {k}").into())
                    .item(move |k: &u32, _, _, _| {
                        d.set(d.get() + 1);
                        div()
                            .debug_selector({
                                let k = *k;
                                move || format!("row-{k}")
                            })
                            .h(px(ROW))
                            .w_full()
                            .child(format!("Row {k}"))
                            .into_any_element()
                    })
                    .on_move(move |mv, _, _| a1(mv))
                    .w(px(300.))
                    .h(px(400.)),
            )
            .child(
                Sortable::new("small", s.borrow().clone())
                    .group("g")
                    .label("Small")
                    .item(|k: &u32, _, _, _| {
                        div()
                            .debug_selector({
                                let k = *k;
                                move || format!("side-{k}")
                            })
                            .h(px(ROW))
                            .w(px(200.))
                            .child(format!("Side {k}"))
                            .into_any_element()
                    })
                    .on_move(move |mv, _, _| a2(mv)),
            )
            .into_any_element()
    });
    (
        o,
        Board {
            keys,
            side,
            moves,
            drawn,
            scroll,
        },
    )
}

fn frames(o: &mut Opened, n: usize) {
    for _ in 0..n {
        o.wait(16);
        o.frame();
    }
}

fn row_at(o: &mut Opened, k: u32, frac: f32) -> Point<Pixels> {
    let b = o.bounds(&format!("row-{k}"));
    point(b.center().x, b.top() + b.size.height * frac)
}

#[gpui::test]
fn only_the_rows_on_screen_are_drawn(cx: &mut TestAppContext) {
    let (mut o, b) = board(cx, N, vec![]);
    o.draw();
    b.drawn.set(0);
    o.draw();
    let drawn = b.drawn.get();
    assert!(drawn <= 12, "{drawn} rows drawn of {N}");
    assert!(o.shown("row-0") && !o.shown("row-50"));
}

#[gpui::test]
fn a_drag_reorders_and_the_rows_on_screen_part(cx: &mut TestAppContext) {
    let (mut o, b) = board(cx, N, vec![]);
    let (t1, t2) = (o.bounds("row-1").top(), o.bounds("row-2").top());
    let from = o.center("row-0");
    let to = row_at(&mut o, 3, 0.25);
    o.hold(from, to);
    frames(&mut o, 12);
    assert_eq!(o.bounds("row-1").top(), t1 - px(ROW), "1 slid up");
    assert_eq!(o.bounds("row-2").top(), t2 - px(ROW));
    o.up(to);
    assert_eq!(*b.moves.borrow(), ["move 0 big:0 -> big:2"]);
    assert_eq!(b.keys.borrow()[..4], [1, 2, 0, 3]);
}

#[gpui::test]
fn auto_scroll_carries_a_drag_to_rows_not_drawn_and_it_lands_right(cx: &mut TestAppContext) {
    let (mut o, b) = board(cx, N, vec![]);
    let from = o.center("row-2");
    let list = o.bounds("row-0");
    let bottom = point(list.center().x, list.top() + px(400. - 6.));
    o.hold(from, bottom);
    frames(&mut o, 40);
    let offset = -b.scroll.0.borrow().base_handle.offset().y;
    assert!(
        offset > px(ROW * 10.),
        "scrolled past the rows first drawn: {offset:?}"
    );
    // Back off the edge to stop, then let go: where depends only on the offset and the row
    // height, though those rows were never drawn before.
    let at = point(bottom.x, bottom.y - px(100.));
    o.drag_to(at);
    let offset = -b.scroll.0.borrow().base_handle.offset().y;
    let content = at.y - list.top() + offset;
    // Rows whose middles are above the pointer, the dragged one (2) left out.
    let expect = ((f32::from(content) - ROW / 2.) / ROW).ceil() as usize - 1;
    o.up(at);
    assert_eq!(*b.moves.borrow(), [format!("move 2 big:2 -> big:{expect}")]);
}

#[gpui::test]
fn the_keyboard_moves_through_rows_not_yet_drawn(cx: &mut TestAppContext) {
    let (mut o, b) = board(cx, N, vec![]);
    o.keys("tab down down down space");
    assert_eq!(
        o.said().as_deref(),
        Some("Picked up Row 3. Arrow keys move it, Space drops it, Escape cancels.")
    );
    for _ in 0..25 {
        o.keys("down");
    }
    assert_eq!(o.said().as_deref(), Some("Over Big, position 29 of 10000."));
    assert!(
        b.scroll.0.borrow().base_handle.offset().y < px(-ROW * 15.),
        "the list scrolled ahead of it"
    );
    o.keys("up space");
    assert_eq!(*b.moves.borrow(), ["move 3 big:3 -> big:27"]);
    assert_eq!(b.keys.borrow()[27], 3);
    // The focus followed it: Space picks Row 3 up again.
    o.draw();
    o.keys("space");
    assert_eq!(
        o.said().as_deref(),
        Some("Picked up Row 3. Arrow keys move it, Space drops it, Escape cancels.")
    );
    o.keys("escape");
}

#[gpui::test]
fn it_shares_items_with_a_sortable_of_its_group(cx: &mut TestAppContext) {
    let (mut o, b) = board(cx, 50, vec![100, 101]);
    let from = o.center("row-1");
    let to = o.bounds("side-101");
    let to = point(to.center().x, to.top() + px(4.));
    o.drag(from, to);
    assert_eq!(*b.moves.borrow(), ["move 1 big:1 -> small:1"]);
    assert_eq!(*b.side.borrow(), [100, 1, 101]);
    // And back, into the long list.
    o.draw();
    let from = o.center("side-100");
    let to = row_at(&mut o, 4, 0.75);
    o.drag(from, to);
    assert_eq!(b.moves.borrow()[1], "move 100 small:0 -> big:4");
    assert_eq!(b.keys.borrow()[4], 100);
}

#[gpui::test]
fn nothing_runs_at_rest_or_while_a_drag_holds_still(cx: &mut TestAppContext) {
    let (mut o, _b) = board(cx, N, vec![]);
    o.frame();
    assert_eq!(o.frames_wanted(), 0);
    let from = o.center("row-0");
    let to = row_at(&mut o, 4, 0.5);
    o.hold(from, to);
    frames(&mut o, 12);
    o.frame();
    assert_eq!(o.frames_wanted(), 0);
}

/// How long the window takes to draw a frame of a drag over 10,000 rows: a move a frame, the
/// rows parting, and auto-scroll at the edge. `cargo test --release -p gpui-dnd --test
/// virtual_list -- --ignored --nocapture`.
#[gpui::test]
#[ignore]
fn frame_time_of_a_drag_at_10000_rows(cx: &mut TestAppContext) {
    let (mut o, _b) = board(cx, N, vec![]);
    // Release builds draw no debug selectors: the list is at (20, 20), 300 wide, rows 40 tall.
    let list = gpui::Bounds::new(point(px(20.), px(20.)), gpui::size(px(300.), px(400.)));
    let from = point(px(170.), px(40.));
    let to = point(px(170.), px(160.));
    o.hold(from, to);
    let mut times = Vec::new();
    for i in 0..300 {
        let y = list.top() + px(40. + (i % 280) as f32);
        let start = Instant::now();
        o.vcx.simulate_mouse_move(
            point(list.center().x, y),
            gpui::MouseButton::Left,
            gpui::Modifiers::none(),
        );
        o.vcx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        times.push(start.elapsed());
    }
    // At the bottom edge: auto-scroll's frames.
    o.drag_to(point(list.center().x, list.top() + px(396.)));
    let mut scroll = Vec::new();
    for _ in 0..300 {
        o.vcx.executor().advance_clock(Duration::from_millis(8));
        let start = Instant::now();
        o.vcx.update(|window, cx| {
            window.simulate_next_frame(cx);
            window.draw(cx).clear(cx);
        });
        scroll.push(start.elapsed());
    }
    let report = |name: &str, mut t: Vec<Duration>| {
        t.sort();
        let ms = |d: Duration| d.as_secs_f64() * 1000.;
        println!(
            "{name}: median {:.3} ms, p95 {:.3} ms, max {:.3} ms over {} frames",
            ms(t[t.len() / 2]),
            ms(t[t.len() * 95 / 100]),
            ms(t[t.len() - 1]),
            t.len()
        );
    };
    report("drag move", times);
    report("auto-scroll", scroll);
}
