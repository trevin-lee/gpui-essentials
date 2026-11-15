//! Questions about a flow's nodes and edges as a graph, and the viewport that shows a region.

use crate::types::{Connection, Edge, EdgeId, Node, NodeId, Rect, Viewport};

/// The nodes with an edge into `node`.
pub fn incomers<'a>(node: &NodeId, nodes: &'a [Node], edges: &[Edge]) -> Vec<&'a Node> {
    nodes
        .iter()
        .filter(|n| edges.iter().any(|e| &e.target == node && e.source == n.id))
        .collect()
}

/// The nodes `node` has an edge to.
pub fn outgoers<'a>(node: &NodeId, nodes: &'a [Node], edges: &[Edge]) -> Vec<&'a Node> {
    nodes
        .iter()
        .filter(|n| edges.iter().any(|e| &e.source == node && e.target == n.id))
        .collect()
}

/// The edges that start or end at any of `nodes`.
pub fn connected_edges<'a>(nodes: &[NodeId], edges: &'a [Edge]) -> Vec<&'a Edge> {
    edges
        .iter()
        .filter(|e| nodes.contains(&e.source) || nodes.contains(&e.target))
        .collect()
}

/// The id a new edge for this connection gets, as React Flow names them.
pub fn edge_id(connection: &Connection) -> EdgeId {
    format!(
        "xy-edge__{}{}-{}{}",
        connection.source,
        connection.source_handle.as_deref().unwrap_or(""),
        connection.target,
        connection.target_handle.as_deref().unwrap_or(""),
    )
    .into()
}

/// The id for a new edge, as [`edge_id`] makes it, but never one of `edges`' own: two
/// connections can run their names together into one id ("a" + "1" and "a1" + "").
pub fn unique_edge_id(connection: &Connection, edges: &[Edge]) -> EdgeId {
    let base = edge_id(connection);
    let taken = |id: &str| edges.iter().any(|e| e.id.as_ref() == id);
    if !taken(&base) {
        return base;
    }
    (1..)
        .map(|n| format!("{base}__{n}"))
        .find(|id| !taken(id))
        .map(EdgeId::from)
        .unwrap_or(base)
}

/// A handle id, with an empty one counted as none, as React Flow counts it.
fn handle(h: &Option<gpui_kit::SharedString>) -> Option<&str> {
    h.as_deref().filter(|h| !h.is_empty())
}

/// Whether an edge already joins the connection's two handles.
pub fn connection_exists(connection: &Connection, edges: &[Edge]) -> bool {
    edges.iter().any(|e| {
        e.source == connection.source
            && e.target == connection.target
            && handle(&e.source_handle) == handle(&connection.source_handle)
            && handle(&e.target_handle) == handle(&connection.target_handle)
    })
}

/// Adds `edge` unless one already joins the same two handles. Says whether it was added.
pub fn add_edge(edge: Edge, edges: &mut Vec<Edge>) -> bool {
    let connection = Connection {
        source: edge.source.clone(),
        source_handle: edge.source_handle.clone(),
        target: edge.target.clone(),
        target_handle: edge.target_handle.clone(),
    };
    if connection_exists(&connection, edges) {
        return false;
    }
    edges.push(edge);
    true
}

/// `v` within `min..=max`, as `f32::clamp` would, but without its panics: a NaN `v` comes out
/// as `min`, and limits the wrong way round or NaN are taken as best they can be.
pub fn clamp(v: f32, min: f32, max: f32) -> f32 {
    v.max(min).min(max)
}

/// The viewport that shows `bounds` whole and centered in a pane of the given size, as React
/// Flow's `getViewportForBounds` places it: `padding` is how much larger than the bounds the
/// shown area is (0.1 is a tenth), split between the two sides.
pub fn viewport_for_bounds(
    bounds: Rect,
    width: f32,
    height: f32,
    min_zoom: f32,
    max_zoom: f32,
    padding: f32,
) -> Viewport {
    let pad = |side: f32| ((side - side / (1. + padding.max(0.))) * 0.5).floor();
    let x_zoom = (width - pad(width) * 2.) / bounds.w.max(1.);
    let y_zoom = (height - pad(height) * 2.) / bounds.h.max(1.);
    let zoom = clamp(x_zoom.min(y_zoom), min_zoom, max_zoom);
    let center = bounds.center();
    Viewport::new(
        width / 2. - center.x * zoom,
        height / 2. - center.y * zoom,
        zoom,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::xy;

    fn sample() -> (Vec<Node>, Vec<Edge>) {
        let nodes = ["a", "b", "c"].map(|id| Node::new(id, xy(0., 0.))).to_vec();
        let edges = vec![
            Edge::new("ab", "a", "b"),
            Edge::new("bc", "b", "c"),
            Edge::new("ac", "a", "c"),
        ];
        (nodes, edges)
    }

    #[test]
    fn neighbours_follow_the_edges_direction() {
        let (nodes, edges) = sample();
        let ids = |v: Vec<&Node>| v.into_iter().map(|n| n.id.to_string()).collect::<Vec<_>>();
        assert_eq!(ids(outgoers(&"a".into(), &nodes, &edges)), ["b", "c"]);
        assert_eq!(ids(incomers(&"c".into(), &nodes, &edges)), ["a", "b"]);
        assert!(incomers(&"a".into(), &nodes, &edges).is_empty());
        assert_eq!(connected_edges(&["b".into()], &edges).len(), 2);
    }

    #[test]
    fn the_same_connection_is_added_once() {
        let (_, mut edges) = sample();
        assert!(!add_edge(Edge::new("again", "a", "b"), &mut edges));
        assert!(add_edge(Edge::new("ba", "b", "a"), &mut edges));
        assert!(add_edge(
            Edge::new("ab2", "a", "b").source_handle("x"),
            &mut edges
        ));
        assert_eq!(edges.len(), 5);
    }

    #[test]
    fn fitting_centers_the_bounds_and_respects_the_zoom_limits() {
        let v = viewport_for_bounds(Rect::new(100., 100., 400., 200.), 1000., 500., 0.5, 2., 0.1);
        // Both directions allow about 2.27, so the limit of 2 holds.
        assert_eq!(v.zoom, 2.);
        assert_eq!(v.to_pane(xy(300., 200.)), xy(500., 250.));
        let far = viewport_for_bounds(Rect::new(0., 0., 10_000., 100.), 1000., 500., 0.5, 2., 0.);
        assert_eq!(far.zoom, 0.5);
    }

    #[test]
    fn padding_is_react_flows() {
        // 1000 wide with 0.1: floor((1000 - 1000 / 1.1) / 2) = 45 a side, so 910 / 1000.
        let v = viewport_for_bounds(Rect::new(0., 0., 1000., 500.), 1000., 500., 0.1, 4., 0.1);
        assert!((v.zoom - 0.91).abs() < 1e-6, "{}", v.zoom);
    }

    #[test]
    fn clamping_never_panics() {
        assert_eq!(clamp(f32::NAN, 0.5, 2.), 0.5);
        assert_eq!(clamp(1., 3., 2.), 2.);
        assert_eq!(clamp(1., f32::NAN, 2.), 1.);
    }

    #[test]
    fn new_edge_ids_never_collide() {
        let a1 = Connection {
            source: "a".into(),
            source_handle: Some("1".into()),
            target: "b".into(),
            target_handle: None,
        };
        let edges = vec![Edge::new(edge_id(&a1), "a", "b").source_handle("1")];
        let other = Connection {
            source: "a1".into(),
            source_handle: None,
            ..a1.clone()
        };
        assert_eq!(edge_id(&other), edges[0].id);
        assert_ne!(unique_edge_id(&other, &edges), edges[0].id);
        // An empty handle id is no handle id.
        let empty = Connection {
            source_handle: Some("".into()),
            ..other.clone()
        };
        let mut edges = vec![Edge::new("x", "a1", "b")];
        assert!(connection_exists(&empty, &edges));
        assert!(!add_edge(
            Edge::new("y", "a1", "b").source_handle(""),
            &mut edges
        ));
    }
}
