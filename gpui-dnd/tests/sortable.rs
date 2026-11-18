//! Sortable lists: a drag reorders a list, the items part to show where it'd land and slide
//! there, and the host hears the move; lists of a group take each other's items, others don't;
//! `can_move` forbids; the keyboard moves the focus, picks up and moves one place at a time,
//! across to the next list too, saying where it is; a line placeholder; horizontal lists;
//! empty lists; and nothing runs once the items have settled.

mod common;

use std::cell::RefCell;
use std::rc::Rc;

use common::*;
use gpui::{
    Axis, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, Point, Styled as _,
    TestAppContext, div, point, px,
};
use gpui_dnd::*;

type Lists = Rc<RefCell<Vec<Vec<u32>>>>;
type Moves = Rc<RefCell<Vec<String>>>;

struct Setup {
    group: Option<&'static str>,
    axis: Axis,
    placeholder: Placeholder,
    forbid: Option<u32>,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            group: Some("board"),
            axis: Axis::Vertical,
            placeholder: Placeholder::Gap,
            forbid: None,
        }
    }
}

fn card(n: u32, axis: Axis) -> gpui::AnyElement {
    let d = div()
        .debug_selector(move || format!("card-{n}"))
        .child(format!("Card {n}"));
    match axis {
        Axis::Vertical => d.w(px(160.)).h(px(40.)),
        Axis::Horizontal => d.w(px(60.)).h(px(40.)),
    }
    .into_any_element()
}

/// Lists side by side ("list-0", "list-1", …), each moving its keys as the moves say.
fn open_lists(
    cx: &mut TestAppContext,
    start: Vec<Vec<u32>>,
    setup: Setup,
) -> (Opened<'_>, Lists, Moves) {
    let lists: Lists = Rc::new(RefCell::new(start));
    let moves: Moves = Rc::default();
    let (l, m) = (lists.clone(), moves.clone());
    let setup = Rc::new(setup);
    let o = open_in_root(cx, move |_, _| {
        let mut row = div().p(px(20.)).flex().gap(px(40.));
        let count = l.borrow().len();
        for i in 0..count {
            let keys = l.borrow()[i].clone();
            let (l, m, s) = (l.clone(), m.clone(), setup.clone());
            let axis = s.axis;
            let mut list = Sortable::new(format!("list-{i}"), keys)
                .axis(axis)
                .placeholder(s.placeholder.clone())
                .label(format!("List {i}"))
                .item_label(|k: &u32| format!("Card {k}").into())
                .item(move |k: &u32, _, _, _| card(*k, axis))
                .empty(|_, _| {
                    div()
                        .debug_selector(|| "empty".into())
                        .w(px(160.))
                        .h(px(40.))
                        .child("Nothing here")
                        .into_any_element()
                })
                .on_move(move |mv: &SortableMove<u32>, _, _| {
                    m.borrow_mut().push(format!(
                        "move {} {}:{} -> {}:{}",
                        mv.key, mv.from.0, mv.from.1, mv.to.0, mv.to.1
                    ));
                    let parse = |id: &gpui::ElementId| {
                        id.to_string()
                            .rsplit('-')
                            .next()
                            .unwrap()
                            .parse::<usize>()
                            .unwrap()
                    };
                    let (f, t) = (parse(&mv.from.0), parse(&mv.to.0));
                    let mut lists = l.borrow_mut();
                    let key = lists[f].remove(mv.from.1);
                    lists[t].insert(mv.to.1, key);
                });
            if let Some(g) = s.group {
                list = list.group(g);
            }
            if let Some(forbid) = s.forbid {
                list = list.can_move(move |mv, _| mv.key != forbid);
            }
            row = row.child(list);
        }
        row.into_any_element()
    });
    (o, lists, moves)
}

fn settle(o: &mut Opened) {
    for _ in 0..12 {
        o.wait(16);
        o.frame();
    }
}

fn top(o: &mut Opened, n: u32) -> Pixels {
    o.bounds(&format!("card-{n}")).top()
}

/// A point in card `n`, `frac` of the way along it.
fn in_card(o: &mut Opened, n: u32, frac: f32) -> Point<Pixels> {
    let b = o.bounds(&format!("card-{n}"));
    point(b.center().x, b.top() + b.size.height * frac)
}

#[gpui::test]
fn a_drag_reorders_the_list_and_the_items_part_to_show_where(cx: &mut TestAppContext) {
    let (mut o, lists, moves) = open_lists(cx, vec![vec![1, 2, 3, 4, 5]], Setup::default());
    let tops: Vec<Pixels> = (1..=5).map(|n| top(&mut o, n)).collect();
    let from = o.center("card-1");
    // Down past the middles of 2 and 3: it'd land after 3.
    let to = in_card(&mut o, 3, 0.75);
    o.hold(from, to);
    settle(&mut o);
    assert_eq!(top(&mut o, 2), tops[0], "2 slid up into 1's place");
    assert_eq!(top(&mut o, 3), tops[1], "and 3 into 2's");
    assert_eq!(top(&mut o, 4), tops[3], "4 stays");
    o.up(to);
    assert_eq!(*moves.borrow(), ["move 1 list-0:0 -> list-0:2"]);
    assert_eq!(lists.borrow()[0], [2, 3, 1, 4, 5]);
    o.draw();
    assert_ne!(
        top(&mut o, 1),
        tops[2],
        "1 starts where its preview was let go"
    );
    settle(&mut o);
    assert_eq!(top(&mut o, 1), tops[2], "and settles in its new place");
    assert_eq!(top(&mut o, 2), tops[0]);
    o.frame();
    assert_eq!(o.frames_wanted(), 0, "nothing runs once it's settled");
}

#[gpui::test]
fn a_drop_in_place_moves_nothing(cx: &mut TestAppContext) {
    let (mut o, lists, moves) = open_lists(cx, vec![vec![1, 2, 3]], Setup::default());
    let from = o.center("card-2");
    o.drag(from, from + point(px(5.), px(6.)));
    assert!(moves.borrow().is_empty());
    assert_eq!(lists.borrow()[0], [1, 2, 3]);
}

#[gpui::test]
fn an_item_moves_to_another_list_of_its_group(cx: &mut TestAppContext) {
    let (mut o, lists, moves) = open_lists(cx, vec![vec![1, 2, 3], vec![10, 11]], Setup::default());
    let (t2, t10, t11) = (top(&mut o, 2), top(&mut o, 10), top(&mut o, 11));
    let from = o.center("card-2");
    let to = in_card(&mut o, 11, 0.25);
    o.hold(from, to);
    settle(&mut o);
    assert_eq!(top(&mut o, 3), t2, "the list it left closes up");
    assert_eq!(top(&mut o, 10), t10);
    assert!(
        top(&mut o, 11) > t11,
        "the list it'd go to opens a gap before 11"
    );
    o.up(to);
    assert_eq!(*moves.borrow(), ["move 2 list-0:1 -> list-1:1"]);
    assert_eq!(*lists.borrow(), [vec![1, 3], vec![10, 2, 11]]);
}

#[gpui::test]
fn lists_without_a_shared_group_dont_take_each_others_items(cx: &mut TestAppContext) {
    let setup = Setup {
        group: None,
        ..Setup::default()
    };
    let (mut o, lists, moves) = open_lists(cx, vec![vec![1, 2], vec![10, 11]], setup);
    let t11 = top(&mut o, 11);
    let from = o.center("card-1");
    let to = in_card(&mut o, 11, 0.25);
    o.hold(from, to);
    settle(&mut o);
    assert_eq!(top(&mut o, 11), t11, "no gap opens");
    o.up(to);
    o.wait(300);
    assert!(moves.borrow().is_empty());
    assert_eq!(*lists.borrow(), [vec![1, 2], vec![10, 11]]);
}

#[gpui::test]
fn can_move_forbids_a_move(cx: &mut TestAppContext) {
    let setup = Setup {
        forbid: Some(1),
        ..Setup::default()
    };
    let (mut o, _lists, moves) = open_lists(cx, vec![vec![1, 2, 3]], setup);
    let t2 = top(&mut o, 2);
    let from = o.center("card-1");
    let to = in_card(&mut o, 3, 0.75);
    o.hold(from, to);
    settle(&mut o);
    assert_eq!(top(&mut o, 2), t2, "no gap");
    o.up(to);
    assert!(moves.borrow().is_empty());
}

#[gpui::test]
fn the_keyboard_moves_the_focus_then_the_item_one_place_at_a_time(cx: &mut TestAppContext) {
    let (mut o, lists, moves) = open_lists(cx, vec![vec![1, 2, 3, 4]], Setup::default());
    // The list is one Tab stop; Down moves the focus to the second item.
    o.keys("tab down space");
    assert_eq!(
        o.said().as_deref(),
        Some("Picked up Card 2. Arrow keys move it, Space drops it, Escape cancels.")
    );
    o.keys("down");
    assert_eq!(o.said().as_deref(), Some("Over List 0, position 3 of 4."));
    o.keys("down");
    assert_eq!(o.said().as_deref(), Some("Over List 0, position 4 of 4."));
    o.keys("up");
    assert_eq!(o.said().as_deref(), Some("Over List 0, position 3 of 4."));
    o.keys("space");
    assert_eq!(*moves.borrow(), ["move 2 list-0:1 -> list-0:2"]);
    assert_eq!(lists.borrow()[0], [1, 3, 2, 4]);
    assert_eq!(
        o.said().as_deref(),
        Some("Dropped Card 2 on List 0, position 3 of 4.")
    );
    // The focus followed it: Space picks Card 2 up again.
    o.keys("space");
    assert_eq!(
        o.said().as_deref(),
        Some("Picked up Card 2. Arrow keys move it, Space drops it, Escape cancels.")
    );
    o.keys("escape");
}

#[gpui::test]
fn across_the_axis_the_keyboard_moves_to_the_next_list(cx: &mut TestAppContext) {
    let (mut o, lists, moves) =
        open_lists(cx, vec![vec![1, 2, 3], vec![10, 11, 12]], Setup::default());
    o.keys("tab down space right");
    let said = o.said().unwrap();
    assert!(said.starts_with("Over List 1, position "), "{said}");
    o.keys("space");
    assert_eq!(moves.borrow().len(), 1);
    assert!(moves.borrow()[0].starts_with("move 2 list-0:1 -> list-1:"));
    assert_eq!(lists.borrow()[0], [1, 3]);
    assert_eq!(lists.borrow()[1].len(), 4);
}

#[gpui::test]
fn a_line_placeholder_marks_the_place_and_nothing_moves(cx: &mut TestAppContext) {
    let setup = Setup {
        placeholder: Placeholder::Line,
        ..Setup::default()
    };
    let (mut o, _lists, moves) = open_lists(cx, vec![vec![1, 2, 3, 4]], setup);
    let (t2, t3) = (top(&mut o, 2), top(&mut o, 3));
    let b3 = o.bounds("card-3");
    let from = o.center("card-1");
    let to = in_card(&mut o, 3, 0.75);
    o.hold(from, to);
    settle(&mut o);
    assert_eq!((top(&mut o, 2), top(&mut o, 3)), (t2, t3), "nothing moves");
    let line = o.bounds("dnd-sortable-line");
    assert!(
        line.top() > b3.bottom() && line.top() < top(&mut o, 4),
        "the line is between 3 and 4: {line:?}"
    );
    o.up(to);
    assert_eq!(*moves.borrow(), ["move 1 list-0:0 -> list-0:2"]);
}

#[gpui::test]
fn a_horizontal_list_reorders_along_its_row(cx: &mut TestAppContext) {
    let setup = Setup {
        axis: Axis::Horizontal,
        ..Setup::default()
    };
    let (mut o, lists, moves) = open_lists(cx, vec![vec![1, 2, 3]], setup);
    let from = o.center("card-3");
    let b1 = o.bounds("card-1");
    let to = point(b1.left() + px(10.), b1.center().y);
    o.drag(from, to);
    assert_eq!(*moves.borrow(), ["move 3 list-0:2 -> list-0:0"]);
    assert_eq!(lists.borrow()[0], [3, 1, 2]);
}

#[gpui::test]
fn an_empty_list_shows_its_empty_state_and_takes_an_item(cx: &mut TestAppContext) {
    let (mut o, lists, moves) = open_lists(cx, vec![vec![1, 2], vec![]], Setup::default());
    assert!(o.shown("empty"));
    let from = o.center("card-1");
    let to = o.center("empty");
    o.drag(from, to);
    assert_eq!(*moves.borrow(), ["move 1 list-0:0 -> list-1:0"]);
    assert_eq!(*lists.borrow(), [vec![2], vec![1]]);
    o.draw();
    assert!(!o.shown("empty"));
}

#[gpui::test]
fn nothing_runs_while_a_drag_over_a_list_holds_still(cx: &mut TestAppContext) {
    let (mut o, _lists, _moves) = open_lists(cx, vec![vec![1, 2, 3, 4]], Setup::default());
    let from = o.center("card-1");
    let to = in_card(&mut o, 3, 0.75);
    o.hold(from, to);
    settle(&mut o);
    o.frame();
    assert_eq!(
        o.frames_wanted(),
        0,
        "the slide is over and nothing else moves"
    );
}

#[gpui::test]
fn a_list_just_drawn_asks_for_no_frames(cx: &mut TestAppContext) {
    let (mut o, _lists, _moves) = open_lists(cx, vec![vec![1, 2, 3]], Setup::default());
    o.frame();
    assert_eq!(o.frames_wanted(), 0, "nothing slides at rest");
}
