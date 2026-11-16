//! No node's text runs out of its box, at any zoom. The built-in kinds, and nodes of a fixed
//! size, are given the worst text there is (a word with nowhere to break, a 300-character
//! address, Japanese, Arabic and emoji) and laid out with Core Text, as on screen. Any glyph
//! drawn past its node's box fails the test with where it is.

#![cfg(target_os = "macos")]

use std::sync::Arc;

use gpui::{
    Bounds, Focusable as _, InteractiveElement as _, IntoElement as _, ParentElement as _, Pixels,
    Styled as _, TestAppContext, TestDispatcher, VisualTestContext, div,
};
use gpui_flow::*;

/// One word, with nowhere to break.
const WORD: &str = "Pneumonoultramicroscopicsilicovolcanoconiosis-Antidisestablishmentarianism-Floccinaucinihilipilification";

fn worst() -> String {
    format!(
        "{WORD} https://example.com/apply/{} 職務経歴書と志望動機を提出してください يرجى إرسال الخطة 🎓📚✅🚀 1234567890123456789012345678901234567890",
        "x".repeat(300)
    )
}

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

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.refresh();
        let _ = window.draw(cx);
    });
    cx.run_until_parked();
}

/// Every glyph drawn past the box of the node it's in (with a pixel's slack for a glyph's
/// padding), and how many glyphs there were.
fn overflows(flow: &gpui::Entity<Flow>, cx: &mut VisualTestContext) -> (Vec<String>, usize) {
    draw(cx);
    // Where each edge's label may be: centred on its point, as wide as a label wraps at, and
    // a few lines tall (a little more, for the padding round it).
    let labels: Vec<Bounds<Pixels>> = flow.read_with(cx, |f, _| {
        let zoom = f.viewport().zoom;
        f.edges()
            .iter()
            .filter_map(|e| f.edge_label_point(&e.id))
            .map(|at| {
                let c = f.flow_to_screen(at);
                let (w, h) = ((180. + 12.) * zoom, (4. * 14. + 6.) * zoom);
                Bounds::new(
                    gpui::point(c.x - gpui::px(w / 2.), c.y - gpui::px(h / 2.)),
                    gpui::size(gpui::px(w), gpui::px(h)),
                )
            })
            .collect()
    });
    let boxes: Vec<(String, Bounds<Pixels>)> = flow.read_with(cx, |f, _| {
        f.nodes()
            .iter()
            .filter_map(|n| {
                let r = f.node_rect(&n.id)?;
                let a = f.flow_to_screen(xy(r.x, r.y));
                let b = f.flow_to_screen(xy(r.right(), r.bottom()));
                Some((n.id.to_string(), Bounds::from_corners(a, b)))
            })
            .collect()
    });
    cx.update(|window, _| {
        let scale = window.scale_factor();
        let slack = 2. * scale;
        let scene = window.rendered_scene();
        let sprites = scene
            .monochrome_sprites
            .iter()
            .map(|s| s.bounds)
            .chain(scene.subpixel_sprites.iter().map(|s| s.bounds))
            .chain(scene.polychrome_sprites.iter().map(|s| s.bounds));
        let mut out = Vec::new();
        let mut count = 0;
        for b in sprites {
            count += 1;
            let (l, t) = (b.origin.x.as_f32(), b.origin.y.as_f32());
            let (r, btm) = (l + b.size.width.as_f32(), t + b.size.height.as_f32());
            // The node whose box holds the glyph's middle is the one it belongs to.
            let (mx, my) = ((l + r) / 2. / scale, (t + btm) / 2. / scale);
            let owner = boxes.iter().find(|(_, n)| {
                let (x, y) = (f32::from(n.origin.x), f32::from(n.origin.y));
                mx >= x
                    && mx <= x + f32::from(n.size.width)
                    && my >= y
                    && my <= y + f32::from(n.size.height)
            });
            let inside = owner.is_some_and(|(_, n)| {
                let (x, y) = (f32::from(n.origin.x) * scale, f32::from(n.origin.y) * scale);
                let (w, h) = (
                    f32::from(n.size.width) * scale,
                    f32::from(n.size.height) * scale,
                );
                l >= x - slack && r <= x + w + slack && t >= y - slack && btm <= y + h + slack
            });
            // An edge's label may lie over a node; inside its own box, it's where it should be.
            let in_label = labels.iter().any(|lb| {
                let (x, y) = (
                    f32::from(lb.origin.x) * scale,
                    f32::from(lb.origin.y) * scale,
                );
                let (w, h) = (
                    f32::from(lb.size.width) * scale,
                    f32::from(lb.size.height) * scale,
                );
                l >= x - slack && r <= x + w + slack && t >= y - slack && btm <= y + h + slack
            });
            if !inside && !in_label {
                out.push(format!(
                    "glyph at x {:.0}–{:.0}, y {:.0}–{:.0} {}",
                    l / scale,
                    r / scale,
                    t / scale,
                    btm / scale,
                    owner.map_or("outside every node".into(), |(id, n)| format!(
                        "in {id} {n:?}"
                    )),
                ));
            }
        }
        (out, count)
    })
}

/// Edges between nodes far apart, labelled with the worst text, of every look.
fn worst_edges() -> Vec<Edge> {
    let text = worst();
    vec![
        Edge::new("e1", "default", "sized").label(text.clone()),
        Edge::new("e2", "input", "topic")
            .label(WORD)
            .pencil("red")
            .dash(EdgeDash::Dotted),
        Edge::new("e3", "group", "linked")
            .label(text)
            .kind(EdgeKind::SmoothStep)
            .dash(EdgeDash::Dashed),
    ]
}

fn worst_nodes() -> Vec<Node> {
    let text = worst();
    vec![
        Node::new("default", xy(0., 0.)).label(text.clone()),
        Node::new("input", xy(200., 0.)).kind("input").label(WORD),
        Node::new("sized", xy(400., 0.))
            .label(text.clone())
            .size(120., 60.),
        Node::new("group", xy(0., 300.))
            .kind("group")
            .label(text.clone())
            .size(220., 120.),
        Node::new("topic", xy(300., 300.))
            .kind("topic")
            .label(text.clone()),
        Node::new("linked", xy(300., 700.))
            .kind("topic")
            .label(WORD)
            .link("doc:1"),
        Node::new("styled", xy(700., 300.))
            .label(text.clone())
            .color("blue")
            .icon("database")
            .emoji("🎓")
            .text_align(LabelAlign::Right),
    ]
    .into_iter()
    // A picture with no loader, its alt text in its place, and link cards filled in with the
    // worst a page could give.
    .chain(
        [
            Node::new("pic", xy(0., 1400.))
                .image("x", 1600., 900., text.clone())
                .label(text.clone()),
            Node::new("card", xy(400., 1400.))
                .link_card(format!("https://example.com/{}", "x".repeat(300))),
            Node::new("filled", xy(800., 1400.)).link_card("https://example.com"),
        ]
        .map(|mut n| {
            if let Some(card) = n.card.as_mut().filter(|_| n.id == "filled") {
                card.title = Some(text.clone().into());
                card.description = Some(text.clone().into());
                card.site = Some(WORD.into());
            }
            n
        }),
    )
    // Headings with no box, and a drawing.
    .chain([
        Node::new("heading", xy(1200., 1400.))
            .kind("text")
            .label(text.clone()),
        Node::new("small heading", xy(1600., 1400.))
            .kind("text")
            .label(WORD)
            .color("teal")
            .icon("type")
            .size(120., 40.),
    ])
    // Frames, titled with the worst text, colored and not.
    .chain([
        Node::new("frame", xy(0., 1100.))
            .kind("frame")
            .label(text.clone())
            .icon("folder")
            .emoji("📁")
            .size(300., 140.),
        Node::new("small frame", xy(400., 1100.))
            .kind("frame")
            .label(WORD)
            .color("purple")
            .size(130., 80.),
    ])
    // Every whiteboard shape, at its own size and at a small fixed one.
    .chain(NodeShape::ALL.iter().enumerate().flat_map(|(i, shape)| {
        let at = xy(900. + (i % 3) as f32 * 260., (i / 3) as f32 * 260.);
        [
            Node::new(format!("{shape:?}"), at)
                .shape(*shape)
                .label(text.clone())
                .color("green")
                .icon("mail")
                .emoji("🚀"),
            Node::new(format!("small {shape:?}"), at + xy(0., 150.))
                .shape(*shape)
                .label(WORD)
                .text_align(LabelAlign::Left)
                .size(90., 50.),
        ]
    }))
    .collect()
}

#[test]
fn every_node_keeps_its_text_in_its_box() {
    let mut found = Vec::new();
    for zoom in [0.5, 1., 2.] {
        let mut cx = app();
        let (flow, cx) = cx.add_window_view(move |_, cx| {
            Flow::new(cx)
                .with_nodes(worst_nodes())
                .with_edges(worst_edges())
                .with_options(|o| {
                    o.min_zoom = 0.25;
                    o.max_zoom = 4.;
                    // Every node built whole, at every zoom.
                    o.simplify_below_zoom = 0.;
                    o.only_render_visible = false;
                })
                .with_viewport(Viewport::new(10., 10., zoom))
        });
        cx.update(|window, cx| flow.read(cx).focus_handle(cx).focus(window, cx));
        draw(cx);
        let (bad, glyphs) = overflows(&flow, cx);
        assert!(
            glyphs > 100,
            "zoom {zoom}: only {glyphs} glyphs drawn: no text system?"
        );
        found.extend(bad.into_iter().map(|b| format!("zoom {zoom}: {b}")));
    }
    assert!(
        found.is_empty(),
        "{} glyphs out of their node:\n{}",
        found.len(),
        found.join("\n")
    );
}

/// The test itself: a node kind that lets its text run on is caught.
#[test]
fn text_that_runs_on_is_caught() {
    let mut cx = app();
    let (flow, cx) = cx.add_window_view(move |_, cx| {
        Flow::new(cx)
            .with_nodes(vec![Node::new("n", xy(0., 0.)).kind("runs-on").label(WORD)])
            .with_node_kind("runs-on", |node, _, _, _, _| {
                div()
                    .id("runs-on")
                    .w(units(100.))
                    .child(div().whitespace_nowrap().child(node.label_text()))
                    .into_any_element()
            })
            .with_viewport(Viewport::new(10., 10., 1.))
    });
    draw(cx);
    let (bad, _) = overflows(&flow, cx);
    assert!(!bad.is_empty());
}
