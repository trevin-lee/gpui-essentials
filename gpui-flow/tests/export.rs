//! A flow as a picture: one standalone SVG, and a PNG made from it.

mod common;

use common::*;
use gpui::TestAppContext;
use gpui_flow::*;

/// A whiteboard with one of every kind, an edge of every look, and text that tries to break
/// out of the picture.
fn board(flow: Flow) -> Flow {
    flow.with_nodes(vec![
        Node::new("frame", xy(0., 0.))
            .kind("frame")
            .label("Storage")
            .size(300., 220.),
        Node::new("db", xy(40., 60.))
            .parent("frame")
            .shape(NodeShape::Cylinder)
            .label("Users")
            .color("teal"),
        Node::new("api", xy(500., 60.))
            .shape(NodeShape::Hexagon)
            .label("Auth <service>"),
        Node::new("evil", xy(500., 300.))
            .label("</text><script>alert(1)</script><image href=\"https://evil.example/x.png\"/>"),
        Node::new("t", xy(0., -100.)).kind("text").label("Sign-up"),
        Node::new("pic", xy(800., 0.)).image("https://cdn.example/a.png", 400., 200., "The form"),
        Node::new("card", xy(800., 300.)).link_card("https://example.com/kyoto"),
        Node::new("far", xy(5000., 5000.)).label("Far away"),
    ])
    .with_edges(vec![
        Edge::new("e1", "api", "db")
            .source_handle("left")
            .target_handle("right")
            .label("reads & writes")
            .dash(EdgeDash::Dashed)
            .pencil("blue")
            .arrows(Arrows::Both),
        Edge::new("e2", "api", "far").label("off screen"),
    ])
}

#[gpui::test]
fn the_svg_is_one_standalone_picture_of_everything(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, board);
    let svg = cx.update(|_, cx| flow.read(cx).to_svg(cx));
    assert!(svg.starts_with("<svg ") && svg.ends_with("</svg>"));
    // Every node's words, escaped.
    for text in [
        "Storage",
        "Users",
        "Auth &lt;service&gt;",
        "Sign-up",
        "The form",
        "Far away",
    ] {
        assert!(svg.contains(text), "{text} missing");
    }
    // The hostile label is only text: no markup of its own got through.
    assert!(!svg.contains("<script"));
    assert!(svg.contains("&lt;script&gt;"));
    // Nothing points at another file (the hostile label's `href` is only escaped text).
    assert!(!svg.contains("href=\""));
    assert!(!svg.contains("<image"));
    // The edges: dashed, with a closed arrowhead at each end and the label on a patch.
    assert!(svg.contains(r#"stroke-dasharray="6 4""#));
    assert!(svg.contains("reads &amp; writes"));
    // The one to a node off screen, that the canvas never drew, is there too.
    assert!(svg.contains("off screen"));
}

#[cfg(feature = "png")]
#[gpui::test]
fn the_png_is_the_svg_in_pixels(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, board);
    let svg = cx.update(|_, cx| flow.read(cx).to_svg(cx));
    let png = export::svg_to_png(&svg, 2.).expect("a PNG");
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
    // The size, from the header: twice the picture's.
    let w = u32::from_be_bytes(png[16..20].try_into().unwrap());
    let h = u32::from_be_bytes(png[20..24].try_into().unwrap());
    let size = |name: &str| -> f32 {
        let at = svg.find(&format!(" {name}=\"")).unwrap() + name.len() + 3;
        svg[at..at + svg[at..].find('"').unwrap()].parse().unwrap()
    };
    assert_eq!(w, (size("width") * 2.).ceil() as u32);
    assert_eq!(h, (size("height") * 2.).ceil() as u32);
    assert!(export::svg_to_png("not an svg", 1.).is_err());
}
