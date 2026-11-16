//! Mind maps: a flow's nodes as a tree, read from its edges, and laid out automatically.
//!
//! A node's parent in the tree is the source of the first edge into it, so a mind map is an
//! ordinary flow: it saves, loads and draws as one. Roots stay where they're put; everything
//! under them is placed from the nodes' sizes, beside or below their parent, children in the
//! order of their edges.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use crate::types::{Edge, Node, NodeId, Rect, Side, XY, xy};

/// Which way a tree grows from its root.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TreeDirection {
    /// Children to the right of their parent, as an outline reads.
    #[default]
    Right,
    /// Children below their parent, as an org chart.
    Down,
    /// The root's children shared out to both sides of it, the first half to the right: a mind
    /// map as XMind and MindNode draw one.
    Both,
}

/// How a flow lays its nodes out.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Layout {
    /// Where they're put, as in React Flow.
    #[default]
    Free,
    /// As a tree read from the edges: a mind map.
    Tree(TreeLayout),
}

/// How a tree is laid out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TreeLayout {
    pub direction: TreeDirection,
    /// Between neighbouring subtrees, in flow units.
    pub sibling_gap: f32,
    /// Between a parent and its children, in flow units.
    pub level_gap: f32,
    /// How long nodes take to move to a new place (after a node is added, collapsed or
    /// moved); zero moves them at once.
    pub motion: Duration,
}

impl Default for TreeLayout {
    fn default() -> Self {
        Self {
            direction: TreeDirection::Right,
            sibling_gap: 14.,
            level_gap: 56.,
            motion: Duration::from_millis(180),
        }
    }
}

impl TreeLayout {
    pub fn direction(mut self, direction: TreeDirection) -> Self {
        self.direction = direction;
        self
    }
}

/// The tree a flow's edges make.
#[derive(Clone, Debug, Default)]
pub struct Tree {
    /// Each node's parent: the source of the first edge into it that doesn't close a loop.
    pub parent: HashMap<NodeId, NodeId>,
    /// Each node's children, in the order of their edges.
    pub children: HashMap<NodeId, Vec<NodeId>>,
    /// Nodes with no parent, in the flow's order.
    pub roots: Vec<NodeId>,
}

impl Tree {
    /// The tree of these nodes (hidden ones left out) and edges.
    pub fn new(nodes: &[Node], edges: &[Edge]) -> Self {
        let present: HashSet<&NodeId> = nodes.iter().filter(|n| !n.hidden).map(|n| &n.id).collect();
        let mut tree = Tree::default();
        for edge in edges.iter().filter(|e| !e.hidden) {
            let (s, t) = (&edge.source, &edge.target);
            if s == t
                || !present.contains(s)
                || !present.contains(t)
                || tree.parent.contains_key(t)
                || tree.is_ancestor(t, s)
            {
                continue;
            }
            tree.parent.insert(t.clone(), s.clone());
            tree.children.entry(s.clone()).or_default().push(t.clone());
        }
        tree.roots = nodes
            .iter()
            .filter(|n| !n.hidden && !tree.parent.contains_key(&n.id))
            .map(|n| n.id.clone())
            .collect();
        tree
    }

    /// Whether `a` is `b` or above it.
    pub fn is_ancestor(&self, a: &NodeId, b: &NodeId) -> bool {
        let mut at = Some(b);
        let mut steps = 0;
        while let Some(n) = at {
            if n == a {
                return true;
            }
            steps += 1;
            if steps > self.parent.len() + 1 {
                return false;
            }
            at = self.parent.get(n);
        }
        false
    }

    pub fn children_of(&self, id: &NodeId) -> &[NodeId] {
        self.children.get(id).map_or(&[], Vec::as_slice)
    }

    /// Everything under `id`, nearest first.
    pub fn descendants(&self, id: &NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut next = vec![id.clone()];
        while let Some(n) = next.pop() {
            for c in self.children_of(&n) {
                out.push(c.clone());
                next.push(c.clone());
            }
        }
        out
    }

    /// How deep `id` is: 0 for a root.
    pub fn depth(&self, id: &NodeId) -> usize {
        let mut depth = 0;
        let mut at = self.parent.get(id);
        while let Some(p) = at {
            depth += 1;
            if depth > self.parent.len() {
                break;
            }
            at = self.parent.get(p);
        }
        depth
    }
}

/// Where layout put a node, and which side of it its children leave from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub position: XY,
    /// The side its children are on; for a root of a tree that grows both ways, the side of
    /// its first child.
    pub out: Side,
}

/// Lays the trees under `roots` out, each root where it is.
///
/// `size` gives a node's size; `folded` says whether a node's children are hidden (collapsed),
/// so it's laid out as a leaf.
pub fn layout(
    tree: &Tree,
    roots: &[(NodeId, XY)],
    size: &dyn Fn(&NodeId) -> (f32, f32),
    folded: &dyn Fn(&NodeId) -> bool,
    options: &TreeLayout,
) -> HashMap<NodeId, Placed> {
    let mut out = HashMap::new();
    let lay = Lay {
        tree,
        size,
        folded,
        options,
    };
    for (root, at) in roots {
        let kids: Vec<NodeId> = if folded(root) {
            Vec::new()
        } else {
            tree.children_of(root).to_vec()
        };
        let (w, h) = size(root);
        match options.direction {
            TreeDirection::Right => {
                out.insert(
                    root.clone(),
                    Placed {
                        position: *at,
                        out: Side::Right,
                    },
                );
                lay.children(&kids, *at, (w, h), Side::Right, &mut out);
            }
            TreeDirection::Down => {
                out.insert(
                    root.clone(),
                    Placed {
                        position: *at,
                        out: Side::Bottom,
                    },
                );
                lay.children(&kids, *at, (w, h), Side::Bottom, &mut out);
            }
            TreeDirection::Both => {
                let half = kids.len().div_ceil(2);
                let (right, left) = kids.split_at(half);
                out.insert(
                    root.clone(),
                    Placed {
                        position: *at,
                        out: Side::Right,
                    },
                );
                lay.children(right, *at, (w, h), Side::Right, &mut out);
                lay.children(left, *at, (w, h), Side::Left, &mut out);
            }
        }
    }
    out
}

struct Lay<'a> {
    tree: &'a Tree,
    size: &'a dyn Fn(&NodeId) -> (f32, f32),
    folded: &'a dyn Fn(&NodeId) -> bool,
    options: &'a TreeLayout,
}

impl Lay<'_> {
    /// How much room across the way the tree grows a subtree takes: its node's, or its
    /// children's together, whichever is more.
    fn extent(&self, id: &NodeId, side: Side) -> f32 {
        let (w, h) = (self.size)(id);
        let own = if across_is_y(side) { h } else { w };
        let kids = self.kids(id);
        if kids.is_empty() {
            return own;
        }
        let block: f32 = kids.iter().map(|k| self.extent(k, side)).sum::<f32>()
            + self.options.sibling_gap * (kids.len() - 1) as f32;
        own.max(block)
    }

    fn kids(&self, id: &NodeId) -> Vec<NodeId> {
        if (self.folded)(id) {
            Vec::new()
        } else {
            self.tree.children_of(id).to_vec()
        }
    }

    /// Places `kids` on `side` of a parent at `at` of `size`, centred on it across.
    fn children(
        &self,
        kids: &[NodeId],
        at: XY,
        size: (f32, f32),
        side: Side,
        out: &mut HashMap<NodeId, Placed>,
    ) {
        if kids.is_empty() {
            return;
        }
        let extents: Vec<f32> = kids.iter().map(|k| self.extent(k, side)).collect();
        let block =
            extents.iter().sum::<f32>() + self.options.sibling_gap * (kids.len() - 1) as f32;
        let y_across = across_is_y(side);
        let center = if y_across {
            at.y + size.1 / 2.
        } else {
            at.x + size.0 / 2.
        };
        let mut start = center - block / 2.;
        for (kid, extent) in kids.iter().zip(extents) {
            let (w, h) = (self.size)(kid);
            // Centred within its subtree's band.
            let across = start + (extent - if y_across { h } else { w }) / 2.;
            let gap = self.options.level_gap;
            let position = match side {
                Side::Right => xy(at.x + size.0 + gap, across),
                Side::Left => xy(at.x - gap - w, across),
                Side::Bottom => xy(across, at.y + size.1 + gap),
                Side::Top => xy(across, at.y - gap - h),
            };
            out.insert(
                kid.clone(),
                Placed {
                    position,
                    out: side,
                },
            );
            self.children(&self.kids(kid), position, (w, h), side, out);
            start += extent + self.options.sibling_gap;
        }
    }
}

/// Whether children on this side stack along y.
fn across_is_y(side: Side) -> bool {
    matches!(side, Side::Left | Side::Right)
}

/// The node nearest `from` in a direction (`dir` a unit step such as (1, 0)), among `rects`,
/// favouring ones straight ahead: what an arrow key moves the selection to.
pub fn nearest_in_direction(from: &Rect, dir: XY, rects: &[(NodeId, Rect)]) -> Option<NodeId> {
    let c = from.center();
    rects
        .iter()
        .filter_map(|(id, r)| {
            let d = r.center() - c;
            let ahead = d.x * dir.x + d.y * dir.y;
            if ahead <= 1. {
                return None;
            }
            let aside = (d.x * dir.y - d.y * dir.x).abs();
            Some((ahead + aside * 2., id))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, id)| id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(edges: &[(&str, &str)]) -> (Vec<Node>, Vec<Edge>) {
        let mut ids: Vec<&str> = Vec::new();
        for (s, t) in edges {
            for id in [s, t] {
                if !ids.contains(id) {
                    ids.push(id);
                }
            }
        }
        let nodes = ids.iter().map(|id| Node::new(*id, xy(0., 0.))).collect();
        let edges = edges
            .iter()
            .map(|(s, t)| Edge::new(format!("{s}-{t}"), *s, *t))
            .collect();
        (nodes, edges)
    }

    fn at(placed: &HashMap<NodeId, Placed>, id: &str) -> XY {
        placed[&NodeId::from(id)].position
    }

    #[test]
    fn the_tree_follows_the_first_edge_into_each_node_and_ignores_loops() {
        let (nodes, mut edges) = map(&[("r", "a"), ("r", "b"), ("a", "c"), ("b", "c")]);
        edges.push(Edge::new("c-r", "c", "r"));
        let tree = Tree::new(&nodes, &edges);
        assert_eq!(tree.roots, ["r"]);
        assert_eq!(tree.parent[&NodeId::from("c")], "a");
        assert_eq!(tree.children_of(&"r".into()), ["a", "b"]);
        assert!(tree.is_ancestor(&"r".into(), &"c".into()));
        assert_eq!(tree.depth(&"c".into()), 2);
        assert_eq!(tree.descendants(&"a".into()), ["c"]);
    }

    #[test]
    fn children_stack_beside_their_parent_and_centre_on_it() {
        let (nodes, edges) = map(&[("r", "a"), ("r", "b"), ("a", "c"), ("a", "d")]);
        let tree = Tree::new(&nodes, &edges);
        let options = TreeLayout {
            sibling_gap: 10.,
            level_gap: 50.,
            ..Default::default()
        };
        let placed = layout(
            &tree,
            &[("r".into(), xy(0., 0.))],
            &|_| (100., 40.),
            &|_| false,
            &options,
        );
        // a's subtree is c and d: 40 + 10 + 40 = 90 tall; b's is 40; together 140, centred on
        // r's middle (20): from -50 to 90.
        assert_eq!(at(&placed, "a"), xy(150., -25.));
        assert_eq!(at(&placed, "b"), xy(150., 50.));
        assert_eq!(at(&placed, "c"), xy(300., -50.));
        assert_eq!(at(&placed, "d"), xy(300., 0.));
        // Collapsed, a is a leaf, and b moves up.
        let placed = layout(
            &tree,
            &[("r".into(), xy(0., 0.))],
            &|_| (100., 40.),
            &|id| id == "a",
            &options,
        );
        assert_eq!(at(&placed, "a"), xy(150., -25.));
        assert_eq!(at(&placed, "b"), xy(150., 25.));
        assert!(!placed.contains_key(&NodeId::from("c")));
    }

    #[test]
    fn a_mind_map_shares_the_roots_children_between_its_sides() {
        let (nodes, edges) = map(&[("r", "a"), ("r", "b"), ("r", "c")]);
        let tree = Tree::new(&nodes, &edges);
        let options = TreeLayout {
            direction: TreeDirection::Both,
            sibling_gap: 10.,
            level_gap: 50.,
            ..Default::default()
        };
        let placed = layout(
            &tree,
            &[("r".into(), xy(0., 0.))],
            &|_| (100., 40.),
            &|_| false,
            &options,
        );
        assert_eq!(at(&placed, "a").x, 150.);
        assert_eq!(at(&placed, "b").x, 150.);
        assert_eq!(at(&placed, "c"), xy(-150., 0.));
        assert_eq!(placed[&NodeId::from("c")].out, Side::Left);
    }

    #[test]
    fn down_puts_children_below() {
        let (nodes, edges) = map(&[("r", "a"), ("r", "b")]);
        let tree = Tree::new(&nodes, &edges);
        let options = TreeLayout {
            direction: TreeDirection::Down,
            sibling_gap: 10.,
            level_gap: 50.,
            ..Default::default()
        };
        let placed = layout(
            &tree,
            &[("r".into(), xy(0., 0.))],
            &|_| (100., 40.),
            &|_| false,
            &options,
        );
        assert_eq!(at(&placed, "a"), xy(-55., 90.));
        assert_eq!(at(&placed, "b"), xy(55., 90.));
    }

    #[test]
    fn arrows_go_to_the_nearest_node_ahead() {
        let from = Rect::new(0., 0., 100., 40.);
        let rects = vec![
            ("up".into(), Rect::new(0., -100., 100., 40.)),
            ("right".into(), Rect::new(200., 10., 100., 40.)),
            ("far right".into(), Rect::new(400., 0., 100., 40.)),
            ("down right".into(), Rect::new(150., 200., 100., 40.)),
        ];
        assert_eq!(
            nearest_in_direction(&from, xy(1., 0.), &rects).as_deref(),
            Some("right")
        );
        assert_eq!(
            nearest_in_direction(&from, xy(0., -1.), &rects).as_deref(),
            Some("up")
        );
        assert_eq!(nearest_in_direction(&from, xy(-1., 0.), &rects), None);
    }
}
