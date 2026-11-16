//! How long a frame of a large whiteboard takes to lay out and paint: 5,000 nodes and 5,000
//! edges (labelled boxes, whiteboard shapes, frames holding some of them, ink strokes, and
//! edges of every kind, a tenth labelled), panned, zoomed out to show all of it, dragged, and
//! edited. Offscreen, with Core Text laying the text out as on screen, so it runs anywhere,
//! CI included. The window's own frame (the GPU's part) isn't counted: this is the flow's work.
//!
//! cargo bench -p gpui-flow --bench scale [-- --nodes 5000]

#[cfg(not(target_os = "macos"))]
fn main() {
    println!("the scale benchmark lays text out with Core Text: macOS only");
}

#[cfg(target_os = "macos")]
fn main() {
    mac::run();
}

#[cfg(target_os = "macos")]
mod mac {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use gpui::{Modifiers, MouseButton, TestAppContext, TestDispatcher, VisualTestContext};
    use gpui_flow::ink::{InkPoint, InkStroke, InkTool};
    use gpui_flow::shapes::NodeShape;
    use gpui_flow::*;

    fn app() -> TestAppContext {
        let cx = TestAppContext::build_with_text_system(
            TestDispatcher::new(0),
            None,
            Arc::new(gpui_apple::AppleTextSystem::new()),
        );
        cx.update(|cx| {
            gpui_core::init(cx);
            gpui_flow::init(cx);
        });
        cx
    }

    /// A whiteboard of `count` nodes and as many edges: rows of mixed nodes, every tenth row
    /// in a frame, an ink stroke every fiftieth node, each joined to the next and to the one
    /// below.
    fn board(count: usize) -> (Vec<Node>, Vec<Edge>) {
        let columns = (count as f32).sqrt().ceil() as usize;
        let shapes = NodeShape::ALL;
        let colors = ["red", "blue", "green", "purple", "orange", "teal"];
        let mut nodes = Vec::with_capacity(count + count / 10);
        let rows = count.div_ceil(columns);
        for row in (0..rows).step_by(10) {
            nodes.push(
                Node::new(format!("f{row}"), xy(-20., row as f32 * 140. - 40.))
                    .kind("frame")
                    .label(format!("Rows {row} to {}", row + 9))
                    .size(columns as f32 * 220. + 20., 10. * 140. + 20.),
            );
        }
        for i in 0..count {
            let (row, col) = (i / columns, i % columns);
            let at = xy(col as f32 * 220., row as f32 * 140.);
            let frame = format!("f{}", row / 10 * 10);
            let local = at - xy(-20., (row / 10 * 10) as f32 * 140. - 40.);
            let node = match i % 50 {
                0 => {
                    let s = InkStroke::new(
                        InkTool::Ball,
                        0x2040_80ff,
                        3.,
                        (0..60)
                            .map(|k| {
                                let t = k as f32 / 10.;
                                InkPoint::at(at.x + t * 25., at.y + t.sin() * 20., t, 3.)
                            })
                            .collect(),
                    );
                    let mut n = Node::ink(format!("n{i}"), &s);
                    n.position = n.position - xy(-20., (row / 10 * 10) as f32 * 140. - 40.);
                    n.parent = Some(frame.into());
                    n
                }
                k if k % 3 == 0 => Node::new(format!("n{i}"), local)
                    .shape(shapes[i % shapes.len()])
                    .label(format!("Shape {i}"))
                    .color(colors[i % colors.len()])
                    .parent(frame),
                _ => Node::new(format!("n{i}"), local)
                    .label(format!("Node {i}, a step of the plan"))
                    .parent(frame),
            };
            nodes.push(node);
        }
        let kinds = [
            EdgeKind::Bezier,
            EdgeKind::SmoothStep,
            EdgeKind::Step,
            EdgeKind::Straight,
        ];
        let mut edges = Vec::with_capacity(count);
        for i in 1..count {
            if edges.len() >= count {
                break;
            }
            let mut e = Edge::new(format!("e{i}"), format!("n{}", i - 1), format!("n{i}"))
                .kind(kinds[i % kinds.len()].clone());
            if i % 10 == 0 {
                e = e.label(format!("step {i}"));
            }
            edges.push(e);
        }
        (nodes, edges)
    }

    fn draw(cx: &mut VisualTestContext) -> Duration {
        cx.run_until_parked();
        let t = Instant::now();
        cx.update(|window, cx| {
            window.refresh();
            let _ = window.draw(cx);
        });
        let spent = t.elapsed();
        cx.run_until_parked();
        spent
    }

    /// Ten frames to settle, then 120 measured, each after a step. With `--only <name>`, only
    /// that scenario, for 3,000 frames (to profile it).
    fn scenario(
        cx: &mut VisualTestContext,
        name: &str,
        step: &mut dyn FnMut(usize, &mut VisualTestContext),
    ) {
        let only = std::env::args().skip_while(|a| a != "--only").nth(1);
        if only.as_deref().is_some_and(|o| o != name) {
            return;
        }
        let frames = if only.is_some() { 3000 } else { 120 };
        for i in 0..10 {
            step(i, cx);
            draw(cx);
        }
        let ms: Vec<f64> = (10..10 + frames)
            .map(|i| {
                step(i, cx);
                draw(cx).as_secs_f64() * 1e3
            })
            .collect();
        report(name, ms);
    }

    fn report(name: &str, mut ms: Vec<f64>) {
        ms.sort_by(f64::total_cmp);
        let at = |q: f64| ms[((ms.len() - 1) as f64 * q) as usize];
        println!(
            "{name:<22} median {:6.2} ms  p95 {:6.2} ms  max {:6.2} ms  ({} frames)",
            at(0.5),
            at(0.95),
            at(1.),
            ms.len()
        );
    }

    pub fn run() {
        let count = std::env::args()
            .skip_while(|a| a != "--nodes")
            .nth(1)
            .and_then(|n| n.parse().ok())
            .unwrap_or(5000);
        let mut app = app();
        let (nodes, edges) = board(count);
        println!(
            "{} nodes ({count} and their frames), {} edges",
            nodes.len(),
            edges.len()
        );
        let built = Instant::now();
        let (flow, cx) = app.add_window_view(move |_, cx| {
            Flow::new(cx)
                .with_nodes(nodes)
                .with_edges(edges)
                .with_viewport(Viewport::new(40., 60., 1.))
                // As far out as a whiteboard goes, to see all of it.
                .with_options(|o| o.min_zoom = 0.02)
        });
        let first = draw(cx);
        println!(
            "first frame            {:6.2} ms (built in {:.2} ms)",
            first.as_secs_f64() * 1e3,
            (built.elapsed() - first).as_secs_f64() * 1e3
        );
        let fl = flow.clone();
        scenario(cx, "still", &mut |_, _| {});
        scenario(cx, "pan at 100%", &mut |i, cx| {
            let t = i as f32 / 20.;
            fl.update(cx, |f, cx| {
                f.set_viewport(
                    Viewport::new(40. - 600. * t.sin(), 60. - 400. * (t * 1.3).cos(), 1.),
                    cx,
                )
            });
        });
        scenario(cx, "pan at 40%", &mut |i, cx| {
            let t = i as f32 / 20.;
            fl.update(cx, |f, cx| {
                f.set_viewport(
                    Viewport::new(40. - 600. * t.sin(), 60. - 400. * (t * 1.3).cos(), 0.4),
                    cx,
                )
            });
        });
        // Zoomed right out, the whole board on screen.
        // (From where the nodes are: most have never been on screen, so aren't measured.)
        let fit = flow.read_with(cx, |f, _| {
            let (mut w, mut h) = (1f32, 1f32);
            for n in f.nodes() {
                let at = f.absolute_position(&n.id).unwrap_or_default();
                w = w.max(at.x + 200.);
                h = h.max(at.y + 100.);
            }
            (1200. / w).min(800. / h)
        });
        println!("whole board at zoom    {fit:.3}");
        scenario(cx, "whole board", &mut |i, cx| {
            let z = fit * (1. + 0.05 * (i as f32 / 10.).sin());
            fl.update(cx, |f, cx| f.set_viewport(Viewport::new(10., 10., z), cx));
        });
        // Dragging a node about.
        fl.update(cx, |f, cx| f.set_viewport(Viewport::new(40., 60., 1.), cx));
        draw(cx);
        let at = flow.read_with(cx, |f, _| {
            let r = f.node_rect("n1").unwrap();
            f.flow_to_screen(xy(r.x + r.w / 2., r.y + r.h / 2.))
        });
        cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::none());
        scenario(cx, "drag a node", &mut |i, cx| {
            let t = i as f32 / 10.;
            let to = gpui::point(
                at.x + gpui::px(40. * t.sin()),
                at.y + gpui::px(30. * t.cos()),
            );
            cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::none());
        });
        cx.simulate_mouse_up(at, MouseButton::Left, Modifiers::none());
        draw(cx);
        // An edit a frame: a label changed, sent as an edit when it settles.
        scenario(cx, "edit a label", &mut |i, cx| {
            fl.update(cx, |f, cx| {
                f.update_node("n2", |n| n.data["label"] = format!("Edit {i}").into(), cx);
            });
        });
        // The pointer moving over the board, pressing nothing.
        scenario(cx, "hover", &mut |i, cx| {
            let to = gpui::point(gpui::px(200. + (i % 40) as f32 * 15.), gpui::px(300.));
            cx.simulate_mouse_move(to, None, Modifiers::none());
        });
    }
}
