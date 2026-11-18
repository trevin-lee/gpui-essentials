//! What a drag source costs a frame: a window of 500 small elements drawn as plain divs, as
//! `dnd` elements with no source, as `dnd` elements each with its own source, and as `dnd`
//! elements sharing one keyed source, each frame timed. Ignored by default:
//! `cargo test --release -p gpui-dnd --test source_cost -- --ignored --nocapture`.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, Render, Styled as _,
    TestAppContext, Window, div, px, size,
};
use gpui_dnd::*;

const N: usize = 500;

#[derive(Clone)]
#[allow(dead_code)]
struct Card {
    id: String,
    lane: String,
}

struct Grid {
    kind: Rc<Cell<u8>>,
}

impl Render for Grid {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let kind = self.kind.get();
        // Built once a frame for every cell, as a view would.
        let shared = DragSource::keyed(|key: &usize, _| Card {
            id: format!("row-{key}"),
            lane: "lane".into(),
        })
        .keyboard(false)
        .on_begin(|_, _, _| {})
        .on_end(|_, _, _| {})
        .shared();
        let cells = (0..N).map(move |i| match kind {
            0 => div()
                .id(("cell", i))
                .w(px(30.))
                .h(px(20.))
                .into_any_element(),
            1 => dnd(("cell", i)).w(px(30.)).h(px(20.)).into_any_element(),
            3 => dnd(("cell", i))
                .w(px(30.))
                .h(px(20.))
                .shared_source(&shared, i)
                .into_any_element(),
            _ => dnd(("cell", i))
                .w(px(30.))
                .h(px(20.))
                .source(
                    DragSource::item(Card {
                        id: format!("row-{i}"),
                        lane: "lane".into(),
                    })
                    .keyboard(false)
                    .on_begin(|_, _, _| {})
                    .on_end(|_, _, _| {}),
                )
                .into_any_element(),
        });
        DndProvider::new(div().size_full().flex().flex_wrap().children(cells))
    }
}

#[gpui::test]
#[ignore]
fn what_a_source_costs_a_frame(cx: &mut TestAppContext) {
    cx.update(gpui_core::init);
    let kind = Rc::new(Cell::new(0));
    let k = kind.clone();
    let (_, vcx) = cx.add_window_view(move |_, _| Grid { kind: k });
    vcx.simulate_resize(size(px(1200.), px(800.)));
    for (k, name) in [
        (0u8, "plain div"),
        (1, "dnd, no source"),
        (2, "dnd, its own source"),
        (3, "dnd, a shared source"),
    ] {
        kind.set(k);
        let mut times = Vec::new();
        for _ in 0..200 {
            let start = Instant::now();
            vcx.update(|window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            });
            times.push(start.elapsed());
        }
        times.sort();
        let ms = |d: Duration| d.as_secs_f64() * 1e3;
        println!(
            "{name}: median {:.3} ms a frame for {N} ({:.2} us each)",
            ms(times[times.len() / 2]),
            ms(times[times.len() / 2]) * 1000. / N as f64
        );
    }
}
