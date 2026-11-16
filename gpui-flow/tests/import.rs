//! Flows from elsewhere, loaded into a canvas and drawn: Mermaid flowcharts and subcanvas
//! whiteboards.

mod common;

use common::*;
use gpui::TestAppContext;
use gpui_flow::*;

const CHART: &str = "flowchart TD
  subgraph pay [Payments]
    A[Charge card] --> B{Approved?}
  end
  B -->|yes| C[(Ledger)]
  B -.->|no| D([Tell the customer])
  C ==> E{{Reconcile}}
";

#[gpui::test]
fn a_mermaid_flowchart_draws_whole(cx: &mut TestAppContext) {
    let object = import::from_mermaid(CHART).unwrap();
    let (flow, _, cx) = open(cx, |f| f);
    flow.update(cx, |f, cx| {
        f.restore(object, cx);
        f.fit_view(cx);
    });
    draw(cx);
    draw(cx);
    flow.read_with(cx, |f, _| {
        for n in f.nodes() {
            assert!(f.node_rect(&n.id).is_some(), "{} not measured", n.id);
        }
        for e in f.edges() {
            assert!(f.edge_line(&e.id).is_some(), "{} not drawn", e.id);
        }
        // The subgraph's frame holds its nodes.
        let frame = f.node_rect("pay").unwrap();
        for id in ["A", "B"] {
            assert!(
                frame.encloses(&f.node_rect(id).unwrap()),
                "{id} outside the frame"
            );
        }
        assert!(!frame.encloses(&f.node_rect("C").unwrap()));
        // Top to bottom: each layer below the one before.
        let y = |id: &str| f.node_rect(id).unwrap().y;
        assert!(y("A") < y("B") && y("B") < y("C") && y("C") < y("E"));
    });
}

const BOARD: &str = r#"{
    "nodes": [
        { "id": "g", "kind": "group", "title": "Storage", "x": 0, "y": 0, "width": 420, "height": 300 },
        { "id": "db", "kind": "plain", "title": "Users", "shape": "cylinder", "x": 30, "y": 60, "group_id": "g" },
        { "id": "api", "kind": "plain", "title": "API", "shape": "hexagon", "x": 600, "y": 80, "color": "green" },
        { "id": "t", "kind": "text", "title": "Sign-up", "x": 0, "y": -80, "width": 240 },
        { "id": "pic", "kind": "media", "title": "The form", "x": 600, "y": 300, "width": 320, "height": 206,
          "media": { "type": "image", "width": 1600, "height": 900, "alt": "The sign-up form", "url": "https://subcanvas.example/x.png" } },
        { "id": "ink", "kind": "drawing", "x": 300, "y": 400, "width": 120, "height": 60, "points": "0,0 0.5,1 1,0.2" }
    ],
    "edges": [
        { "id": "e", "source": "api", "target": "db", "label": "reads", "direction": "forward", "shape": "step" }
    ]
}"#;

#[gpui::test]
fn a_subcanvas_whiteboard_draws_whole(cx: &mut TestAppContext) {
    let object = import::from_subcanvas(BOARD).unwrap();
    let (flow, _, cx) = open(cx, |f| f);
    flow.update(cx, |f, cx| {
        f.restore(object, cx);
        f.fit_view(cx);
    });
    draw(cx);
    draw(cx);
    flow.read_with(cx, |f, _| {
        for id in ["g", "db", "api", "t", "pic", "ink"] {
            assert!(f.node_rect(id).is_some(), "{id} not measured");
        }
        // The cylinder is inside its frame, and the edge from the API reaches it.
        assert!(
            f.node_rect("g")
                .unwrap()
                .encloses(&f.node_rect("db").unwrap())
        );
        let line = f.edge_line("e").expect("the edge is drawn");
        let (api, db) = (f.node_rect("api").unwrap(), f.node_rect("db").unwrap());
        // From the API's left side to the database's right: they face each other.
        assert!((line[0].x - api.x).abs() < 1.);
        assert!((line[line.len() - 1].x - db.right()).abs() < 1.);
        // The drawing takes the size it was saved at.
        let ink = f.node_rect("ink").unwrap();
        assert_eq!((ink.w, ink.h), (120., 60.));
    });
}

#[test]
fn drawing_points_are_read_defensively() {
    assert_eq!(parse_points("0,0 1,1"), [xy(0., 0.), xy(1., 1.)]);
    // Out of the box is brought back in; what isn't a pair is skipped.
    assert_eq!(
        parse_points("2,-1 nonsense 0.5 0.5,NaN 0.5,0.5"),
        [xy(1., 0.), xy(0.5, 0.5)]
    );
    assert_eq!(parse_points(&"0.1,0.1 ".repeat(5000)).len(), 4000);
}
