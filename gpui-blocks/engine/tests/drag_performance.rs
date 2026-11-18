//! How long a frame of a block drag takes on a long page: a block dragged up and down past 300
//! others, each step a mouse move and a drawn frame. Ignored by default; run it in release:
//! `cargo test --release -p gpui-blocks-engine --test drag_performance -- --ignored --nocapture`.

use std::time::{Duration, Instant};

use gpui::{
    AppContext as _, Context, Entity, IntoElement, Modifiers, MouseButton, ParentElement as _,
    Render, Styled as _, TestAppContext, Window, div, point, px,
};
use markdown_editor::{BlockEditor, BlockEditorState};

struct Root(Entity<BlockEditorState>);

impl Render for Root {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(700.))
            .h(px(900.))
            .child(BlockEditor::new(&self.0))
    }
}

#[gpui::test]
#[ignore]
fn frame_time_of_a_block_drag_on_a_long_page(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_core::init(cx);
        markdown_editor::init(cx);
    });
    let source: Vec<String> = (0..300)
        .map(|i| format!("Paragraph {i}, with a line of words to lay out."))
        .collect();
    let source = source.join("\n\n");
    let (root, cx) = cx.add_window_view(move |window, cx| {
        Root(cx.new(|cx| BlockEditorState::new(window, cx).with_source(&source)))
    });
    let editor = root.read_with(cx, |root, _| root.0.clone());
    let draw = |cx: &mut gpui::VisualTestContext| {
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
    };
    draw(cx);
    draw(cx);
    let blocks = editor.read_with(cx, |e, _| e.top_level_blocks());
    let (block, b) = blocks[4];
    let start = point(b.left() + px(2.), b.center().y);
    editor.update(cx, |e, _| e.begin_block_drag(block, start));
    cx.simulate_mouse_move(
        start + point(px(0.), px(10.)),
        Some(MouseButton::Left),
        Modifiers::none(),
    );
    draw(cx);
    assert!(
        editor.read_with(cx, |e, _| e.is_dragging_block()),
        "the drag began"
    );
    let (top, bottom) = (blocks[0].1.center().y, blocks[14].1.center().y);
    let mut times: Vec<Duration> = Vec::new();
    for i in 0..300 {
        let t = (i % 150) as f32 / 150.;
        let at = point(start.x, top + (bottom - top) * t);
        let begin = Instant::now();
        cx.simulate_mouse_move(at, Some(MouseButton::Left), Modifiers::none());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        times.push(begin.elapsed());
    }
    cx.simulate_mouse_up(start, MouseButton::Left, Modifiers::none());
    times.sort();
    let ms = |d: Duration| d.as_secs_f64() * 1e3;
    println!(
        "a frame of a block drag on a long page: median {:.3} ms, p95 {:.3} ms, max {:.3} ms over {}",
        ms(times[times.len() / 2]),
        ms(times[times.len() * 95 / 100]),
        ms(times[times.len() - 1]),
        times.len()
    );
}
