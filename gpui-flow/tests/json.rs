//! The stored form: a versioned document with stable ids, that keeps what it doesn't know.

mod common;

use common::*;
use gpui::{AppContext as _, TestAppContext};
use gpui_flow::json::{self, DocLayout, Document};
use gpui_flow::*;
use serde_json::{Value, json};

fn board(flow: Flow) -> Flow {
    flow.with_nodes(vec![
        Node::new("frame", xy(0., 0.))
            .kind("frame")
            .label("Storage")
            .size(400., 300.),
        Node::new("db", xy(40., 60.))
            .shape(gpui_flow::shapes::NodeShape::Cylinder)
            .label("Postgres")
            .color("blue")
            .parent("frame")
            .link("4f2a"),
        Node::new("api", xy(600., 60.)).label("API").link("9c1e"),
    ])
    .with_edges(vec![
        Edge::new("api-db", "api", "db")
            .label("reads")
            .dash(EdgeDash::Dashed),
    ])
}

#[gpui::test]
fn a_flow_saves_and_loads_with_the_same_ids(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, board);
    let text = flow.read_with(cx, |f, _| f.document().to_json());
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["format"], "gpui-flow");
    assert_eq!(v["version"], 1);
    assert_eq!(v["nodes"][1]["parentId"], "frame");
    assert_eq!(v["nodes"][1]["link"], "4f2a");

    let doc = Document::from_json(&text).unwrap();
    assert_eq!(doc.links(), ["4f2a", "9c1e"]);
    let other = cx.new(Flow::new);
    other.update(cx, |f, cx| f.set_document(doc, cx));
    assert_eq!(
        flow.read_with(cx, |f, _| f.to_object().nodes),
        other.read_with(cx, |f, _| f.to_object().nodes)
    );
    // And written again, byte for byte.
    assert_eq!(other.read_with(cx, |f, _| f.document().to_json()), text);
}

#[gpui::test]
fn what_is_selected_is_not_stored_and_survives_a_load(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, board);
    flow.update(cx, |f, cx| {
        f.update_node("api", |n| n.selected = true, cx);
        f.update_edge("api-db", |e| e.selected = true, cx);
    });
    let mut doc = flow.read_with(cx, |f, _| f.document());
    assert!(doc.nodes.iter().all(|n| !n.selected));
    assert!(!doc.to_json().contains("selected"));
    // Someone else's version arrives: api is still there, db is gone.
    doc.nodes.retain(|n| n.id != "db");
    doc.edges.clear();
    flow.update(cx, |f, cx| f.set_document(doc, cx));
    assert_eq!(flow.read_with(cx, |f, _| f.selected_nodes()), ["api"]);
    assert!(flow.read_with(cx, |f, _| f.selected_edges()).is_empty());
}

#[gpui::test]
fn unknown_fields_are_kept_at_every_level(cx: &mut TestAppContext) {
    let text = json!({
        "format": "gpui-flow",
        "version": 1,
        "layout": { "type": "radial", "rings": 3 },
        "comments": [{ "on": "n1", "text": "hm" }],
        "nodes": [{
            "id": "n1", "position": { "x": 1, "y": 2 }, "type": "sticker",
            "data": { "label": "Hi", "mood": "glad" },
            "author": "agent:finance"
        }],
        "edges": [{
            "id": "e1", "source": "n1", "target": "n1",
            "style": { "stroke": "#ff0000", "opacity": 0.5 }, "weight": 2
        }]
    })
    .to_string();
    let doc = Document::from_json(&text).unwrap();
    let (flow, _, cx) = open(cx, |f| f);
    flow.update(cx, |f, cx| f.set_document(doc, cx));
    let back: Value =
        serde_json::from_str(&flow.read_with(cx, |f, _| f.document().to_json())).unwrap();
    assert_eq!(back["layout"], json!({ "type": "radial", "rings": 3 }));
    assert_eq!(back["comments"][0]["text"], "hm");
    assert_eq!(back["nodes"][0]["type"], "sticker");
    assert_eq!(back["nodes"][0]["data"]["mood"], "glad");
    assert_eq!(back["nodes"][0]["author"], "agent:finance");
    assert_eq!(back["edges"][0]["style"]["opacity"], 0.5);
    assert_eq!(back["edges"][0]["weight"], 2);
}

#[test]
fn another_format_or_a_newer_version_is_refused() {
    let doc = |format: &str, version: u32| {
        Document::from_json(
            &json!({ "format": format, "version": version, "nodes": [] }).to_string(),
        )
    };
    assert!(doc("gpui-flow", 1).is_ok());
    assert!(matches!(
        doc("gpui-flow", 2),
        Err(json::Error::Format { version: 2, .. })
    ));
    assert!(matches!(
        doc("gpui-blocks", 1),
        Err(json::Error::Format { .. })
    ));
    assert!(matches!(
        Document::from_json("[1, 2]"),
        Err(json::Error::Json(_))
    ));
    // A bare React Flow object has no format.
    assert!(Document::from_json(r#"{"nodes":[],"edges":[]}"#).is_err());
}

#[test]
fn bad_and_repeated_ids_are_replaced_and_what_pointed_at_them_follows() {
    let long = "x".repeat(200);
    let text = json!({
        "format": "gpui-flow", "version": 1,
        "nodes": [
            { "id": "a b", "position": { "x": 0, "y": 0 } },
            { "id": long, "position": { "x": 0, "y": 0 }, "parentId": "a b" },
            { "id": "c", "position": { "x": 0, "y": 0 } },
            { "id": "c", "position": { "x": 9, "y": 9 } }
        ],
        "edges": [
            { "id": "e", "source": "a b", "target": long },
            { "id": "e", "source": "c", "target": "c" },
            { "id": "", "source": "c", "target": "c" }
        ]
    })
    .to_string();
    let doc = Document::from_json(&text).unwrap();
    let ids: Vec<&str> = doc.nodes.iter().map(|n| n.id.as_ref()).collect();
    assert!(ids.iter().all(|id| json::valid_id(id)), "{ids:?}");
    assert_eq!(ids[2], "c");
    assert_ne!(ids[3], "c");
    // What pointed at the bad ids points at their new ones; at the repeated one, the first.
    assert_eq!(doc.nodes[1].parent.as_deref(), Some(ids[0]));
    assert_eq!(
        (doc.edges[0].source.as_ref(), doc.edges[0].target.as_ref()),
        (ids[0], ids[1])
    );
    assert_eq!(doc.edges[1].source, "c");
    let edge_ids: std::collections::HashSet<&str> =
        doc.edges.iter().map(|e| e.id.as_ref()).collect();
    assert_eq!(edge_ids.len(), 3);
    assert!(edge_ids.contains("e"));
}

#[test]
fn new_ids_are_valid_and_unique() {
    let ids: std::collections::HashSet<String> = (0..10_000).map(|_| json::new_id()).collect();
    assert_eq!(ids.len(), 10_000);
    assert!(ids.iter().all(|id| id.len() == 12 && json::valid_id(id)));
}

#[gpui::test]
fn what_the_flow_adds_gets_an_id_no_one_else_will_make(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, with_three);
    let edge = flow.update(cx, |f, cx| {
        let edge = f
            .connect(
                Connection {
                    source: "b".into(),
                    source_handle: None,
                    target: "c".into(),
                    target_handle: None,
                },
                cx,
            )
            .unwrap();
        f.update_node("c", |n| n.selected = true, cx);
        edge.id
    });
    draw(cx);
    let frame = flow.update(cx, |f, cx| f.frame_selection("Group", cx).unwrap());
    for id in [&edge, &frame] {
        assert_eq!(id.len(), 12, "{id}");
        assert!(json::valid_id(id));
    }
}

#[gpui::test]
fn a_mind_map_stores_its_layout(cx: &mut TestAppContext) {
    let (flow, _, cx) = open(cx, |f| {
        with_three(f).with_options(|o| {
            o.layout = Layout::Tree(TreeLayout {
                direction: TreeDirection::Down,
                ..TreeLayout::default()
            })
        })
    });
    let doc = flow.read_with(cx, |f, _| f.document());
    assert_eq!(
        doc.layout,
        Some(DocLayout::Tree {
            direction: TreeDirection::Down
        })
    );
    let v: Value = serde_json::from_str(&doc.to_json()).unwrap();
    assert_eq!(v["layout"], json!({ "type": "tree", "direction": "down" }));
    // A whiteboard given it becomes a mind map; given a whiteboard, it lays out freely again.
    let other = cx.new(Flow::new);
    other.update(cx, |f, cx| f.set_document(doc, cx));
    assert!(matches!(
        other.read_with(cx, |f, _| f.options.layout),
        Layout::Tree(TreeLayout {
            direction: TreeDirection::Down,
            ..
        })
    ));
    other.update(cx, |f, cx| f.set_document(Document::default(), cx));
    assert_eq!(other.read_with(cx, |f, _| f.options.layout), Layout::Free);
}

#[test]
fn a_picture_and_a_card_keep_what_they_dont_know() {
    let text = json!({
        "format": "gpui-flow", "version": 1,
        "nodes": [
            { "id": "p", "position": { "x": 0, "y": 0 }, "type": "image",
              "media": { "src": "file:1", "blurhash": "LEHV6n" } },
            { "id": "l", "position": { "x": 0, "y": 0 }, "type": "link",
              "card": { "url": "https://example.com", "favicon": "file:2" } }
        ]
    })
    .to_string();
    let back: Value = serde_json::from_str(&Document::from_json(&text).unwrap().to_json()).unwrap();
    assert_eq!(back["nodes"][0]["media"]["blurhash"], "LEHV6n");
    assert_eq!(back["nodes"][1]["card"]["favicon"], "file:2");
}
